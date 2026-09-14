//! Reviewed changes to Skill files. Plans contain domain locations, never executable commands.
use crate::{
    model::{Skill, SkillDeployment},
    native, protection, safe_files,
    store::Store,
    targets,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;
mod recovery;
pub use recovery::recover;
use recovery::{finish_committed, mark_rolled_back};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    id: String,
    path: String,
    label: String,
    agents: Vec<String>,
    role: String,
    before_digest: String,
    differences: Vec<Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallTarget {
    agent: String,
    scope: String,
    project_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    id: String,
    action: String,
    skill: Option<Skill>,
    name: String,
    created_at: String,
    state_digest: String,
    source_path: Option<String>,
    source_digest: Option<String>,
    locations: Vec<Location>,
    install_targets: Vec<InstallTarget>,
    retain_backup: bool,
    can_execute: bool,
    blocked_reason: Option<String>,
    recovery_digests: BTreeMap<String, String>,
    recovery_backup_id: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Records {
    skill: Option<Skill>,
    deployments: Vec<SkillDeployment>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedLocation {
    location: Location,
    digest: String,
    slot: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    id: String,
    root_digest: String,
    locations: Vec<SavedLocation>,
    before: Records,
    after: Records,
}
struct Swap {
    path: PathBuf,
    old: PathBuf,
    incoming: PathBuf,
    moved_old: bool,
    placed_new: bool,
}

pub(crate) fn schema(store: &Store) -> Result<()> {
    ensure_schema(&store.conn)
}
pub(crate) fn backup_skills(store: &Store) -> Result<Vec<Skill>> {
    schema(store)?;
    let mut statement = store
        .conn
        .prepare("SELECT payload_json FROM skill_backup_manifests")?;
    let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
    let mut skills = Vec::new();
    for row in rows {
        // A damaged backup must not prevent opening the app or managing other
        // backups. Its restore operation still rejects the invalid manifest.
        let Ok(manifest) = serde_json::from_str::<BackupManifest>(&row?) else {
            continue;
        };
        skills.extend(manifest.before.skill);
        skills.extend(manifest.after.skill);
    }
    Ok(skills)
}
pub(crate) fn move_library_references(
    store: &Store,
    id: &str,
    old: &Path,
    new: &Path,
) -> Result<()> {
    schema(store)?;
    let rows: Vec<(String, String)> = {
        let mut statement = store
            .conn
            .prepare("SELECT backup_id,payload_json FROM skill_backup_manifests")?;
        let rows = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows
    };
    let old_key = protection::path_key(old);
    let new_path = new.to_string_lossy().to_string();
    for (backup_id, raw) in rows {
        let Ok(mut manifest) = serde_json::from_str::<BackupManifest>(&raw) else {
            continue;
        };
        let belongs = manifest
            .before
            .skill
            .as_ref()
            .or(manifest.after.skill.as_ref())
            .is_some_and(|s| s.id == id);
        if !belongs {
            continue;
        }
        for skill in [&mut manifest.before.skill, &mut manifest.after.skill]
            .into_iter()
            .flatten()
        {
            if skill.id == id && key(&skill.path) == old_key {
                skill.path = new_path.clone();
            }
        }
        for location in &mut manifest.locations {
            if location.location.role == "library" && key(&location.location.path) == old_key {
                location.location.path = new_path.clone();
            }
        }
        store.conn.execute(
            "UPDATE skill_backup_manifests SET payload_json=?1 WHERE backup_id=?2",
            params![serde_json::to_string(&manifest)?, backup_id],
        )?;
        store.conn.execute(
            "UPDATE backups SET original_path=?1 WHERE id=?2 AND original_path=?3",
            params![new_path, backup_id, old.to_string_lossy()],
        )?;
    }
    Ok(())
}
pub(crate) fn ensure_schema(connection: &rusqlite::Connection) -> Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS skill_change_plans(id TEXT PRIMARY KEY,payload_json TEXT NOT NULL,state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS backups(id TEXT PRIMARY KEY,name TEXT NOT NULL,path TEXT NOT NULL,created_at TEXT NOT NULL,original_path TEXT NOT NULL); CREATE TABLE IF NOT EXISTS skill_backup_manifests(backup_id TEXT PRIMARY KEY,payload_json TEXT NOT NULL);")?;
    Ok(())
}
fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .with_context(|| format!("缺少 {key}"))
}
fn key(path: &str) -> String {
    protection::path_key(Path::new(path))
}
fn now() -> String {
    Utc::now().to_rfc3339()
}
fn unexpired(value: &str) -> Result<()> {
    let age = Utc::now().signed_duration_since(DateTime::parse_from_rfc3339(value)?);
    if age.num_seconds() < -60 || age.num_hours() >= 24 {
        bail!("计划已过期，请重新检查");
    }
    Ok(())
}
fn owned_dir(store: &Store, category: &str, id: &str) -> Result<PathBuf> {
    Uuid::parse_str(id).context("变更标识无效")?;
    let path = store.data_dir().join(category).join(id);
    safe_files::safe_directory(&path)?;
    Ok(path)
}
fn state_digest(store: &Store) -> Result<String> {
    let state = json!({"skills":store.list_skills()?,"deployments":store.list_deployments()?,"projects":store.list_projects()?,"settings":store.get_settings()?,"targets":targets::list(store)?});
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&state)?)))
}
fn library_path(store: &Store, skill: &Skill) -> Result<PathBuf> {
    crate::library::validate(store, skill)
}
fn valid_name(name: &str) -> Result<()> {
    let path = Path::new(name);
    if name.is_empty()
        || name.len() > 120
        || name.contains(['/', '\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        || name.ends_with(['.', ' '])
        || path.components().count() != 1
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        bail!("Skill 名称不能用作安装目录，请先修改名称");
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
    {
        bail!("Skill 名称是系统保留的目录名");
    }
    Ok(())
}
fn validate_location(store: &Store, location: &Location, skill: Option<&Skill>) -> Result<()> {
    let path = Path::new(&location.path);
    safe_files::safe_directory(path)?;
    protection::ensure_mutable(path)?;
    if path.components().count() < 3 || path.file_name().is_none() {
        bail!("不能操作根目录");
    }
    if location.role == "library" {
        let expected = library_path(store, skill.context("库位置缺少 Skill")?)?;
        if key(&location.path) != protection::path_key(&expected) {
            bail!("库位置不匹配");
        }
    } else {
        let current_key = key(&location.path);
        let data_key = protection::path_key(store.data_dir());
        if current_key == data_key
            || current_key.starts_with(&(data_key.clone() + "/"))
            || data_key.starts_with(&(current_key.clone() + "/"))
        {
            bail!("Agent 安装位置不能覆盖应用数据");
        }
        let mut roots: Vec<String> = store
            .list_projects()?
            .into_iter()
            .map(|project| project.path)
            .collect();
        roots.extend(
            targets::list(store)?
                .into_iter()
                .filter(|target| !target.global_path.is_empty())
                .map(|target| target.global_path),
        );
        for root in roots {
            let root_key = key(&root);
            if root_key == current_key || root_key.starts_with(&(current_key.clone() + "/")) {
                bail!("不能覆盖整个项目或 Agent 目录");
            }
        }
        if store
            .list_deployments()?
            .iter()
            .any(|item| key(&item.path) == current_key && item.owner == "host")
        {
            bail!("宿主管理的 Skill 为只读");
        }
    }
    Ok(())
}
fn validate_locations(store: &Store, locations: &[Location], skill: Option<&Skill>) -> Result<()> {
    let mut keys = Vec::new();
    for location in locations {
        validate_location(store, location, skill)?;
        let current = key(&location.path);
        if keys.iter().any(|old: &String| {
            old == &current
                || old.starts_with(&(current.clone() + "/"))
                || current.starts_with(&(old.clone() + "/"))
        }) {
            bail!("变更位置重复或相互嵌套");
        }
        keys.push(current);
    }
    Ok(())
}
fn agents_at(store: &Store, path: &str) -> Result<Vec<String>> {
    Ok(store
        .list_deployments()?
        .into_iter()
        .filter(|item| key(&item.path) == key(path))
        .map(|item| item.agent)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}
fn location(store: &Store, path: String, role: &str, source: Option<&Path>) -> Result<Location> {
    let before_digest = safe_files::digest(Path::new(&path))?;
    let differences = if let Some(source) = source {
        native::differences(source, Some(Path::new(&path)))?
            .as_array()
            .context("文件比较无效")?
            .clone()
    } else if before_digest != "absent" {
        vec![json!({"path":".","change":"removed"})]
    } else {
        vec![]
    };
    Ok(Location {
        id: format!("location:{:x}", Sha256::digest(key(&path))),
        agents: agents_at(store, &path)?,
        path,
        label: if role == "library" {
            "Skill 库"
        } else {
            "Agent 安装位置"
        }
        .into(),
        role: role.into(),
        before_digest,
        differences,
    })
}
fn public_plan(plan: &Plan) -> Value {
    json!({"id":plan.id,"skillId":plan.skill.as_ref().map(|skill| &skill.id),"action":plan.action,"summary":format!("{}：{}", action_label(&plan.action),plan.name),"canExecute":plan.can_execute,"blockedReason":plan.blocked_reason,"createdAt":plan.created_at,"locations":plan.locations.iter().map(|location|json!({"id":location.id,"path":location.path,"label":location.label,"agents":location.agents,"exists":location.before_digest!="absent","differences":location.differences})).collect::<Vec<_>>()})
}
fn action_label(action: &str) -> &str {
    match action {
        "install" => "安装 Skill",
        "remove" => "移除安装位置",
        "update" => "更新 Skill",
        "delete" => "删除 Skill",
        _ => "恢复 Skill",
    }
}

pub fn dispatch(store: &Store, method: &str, args: &Value) -> Result<Value> {
    schema(store)?;
    if method == "backups.list" {
        return list_backups(store);
    }
    recover(store)?;
    let _lock = crate::recovery::lock(store)?;
    match method {
        "skills.install.preview" => preview(store, "install", args),
        "skills.remove.preview" => preview(store, "remove", args),
        "skills.update.preview" => preview(store, "update", args),
        "skills.delete.preview" => preview(store, "delete", args),
        "skills.install" => execute(store, "install", args),
        "skills.remove" => execute(store, "remove", args),
        "skills.update" => execute(store, "update", args),
        "skills.delete" => execute(store, "delete", args),
        "skills.cancel" => cancel(store, args),
        "backups.restore" => restore(store, args),
        _ => bail!("未知 Skill 变更操作"),
    }
}

fn preview(store: &Store, action: &str, args: &Value) -> Result<Value> {
    let id = Uuid::new_v4().to_string();
    let work = owned_dir(store, "skill-change-plans", &id)?;
    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<Value> {
        let skill = if action == "remove" {
            let deployment = store
                .get_deployment(required(args, "deploymentId")?)?
                .context("安装记录不存在")?;
            deployment
                .skill_id
                .as_deref()
                .map(|id| store.get_skill(id))
                .transpose()?
                .flatten()
        } else {
            Some(
                store
                    .get_skill(required(args, "skillId")?)?
                    .context("Skill 不存在")?,
            )
        };
        let mut plan = Plan {
            id: id.clone(),
            action: action.into(),
            name: skill
                .as_ref()
                .map(|skill| skill.name.clone())
                .unwrap_or_default(),
            skill,
            created_at: now(),
            state_digest: String::new(),
            source_path: None,
            source_digest: None,
            locations: vec![],
            install_targets: vec![],
            retain_backup: args["retainBackup"].as_bool().unwrap_or(true),
            can_execute: true,
            blocked_reason: None,
            recovery_digests: BTreeMap::new(),
            recovery_backup_id: None,
        };
        let all_deployments = store.list_deployments()?;
        match action {
            "install" => {
                let skill = plan.skill.as_ref().unwrap();
                let source = library_path(store, skill)?;
                if !source.join("SKILL.md").is_file() {
                    bail!("Skill 库文件缺失");
                }
                valid_name(&skill.name)?;
                let target_id = required(args, "targetId")?;
                let configs = targets::list(store)?;
                let config = configs
                    .iter()
                    .find(|target| target.id == target_id)
                    .context("Agent 未配置")?;
                if !config.enabled {
                    bail!("此 Agent 未启用");
                }
                let project_id = args["projectId"].as_str().map(String::from);
                let project = project_id
                    .as_deref()
                    .map(|id| store.get_project(id))
                    .transpose()?
                    .flatten();
                if project_id.is_some() && project.is_none() {
                    bail!("项目不存在");
                }
                if project.as_ref().is_some_and(|project| project.archived) {
                    bail!("项目已归档");
                }
                let scope = if project.is_some() {
                    "project"
                } else {
                    "global"
                };
                let root = if let Some(project) = &project {
                    Path::new(&project.path).join(&config.project_path)
                } else {
                    PathBuf::from(&config.global_path)
                };
                let destination = root.join(&skill.name);
                targets::validate_skill_target(
                    store,
                    target_id,
                    scope,
                    project.as_ref().map(|project| Path::new(&project.path)),
                    &destination,
                )?;
                let mut target = location(
                    store,
                    destination.to_string_lossy().into(),
                    "deployment",
                    Some(&source),
                )?;
                for candidate in configs.iter().filter(|target| target.enabled) {
                    let candidate_root = if let Some(project) = &project {
                        if candidate.project_path.is_empty() {
                            continue;
                        }
                        Path::new(&project.path).join(&candidate.project_path)
                    } else {
                        PathBuf::from(&candidate.global_path)
                    };
                    if protection::path_key(&candidate_root.join(&skill.name))
                        == protection::path_key(&destination)
                    {
                        plan.install_targets.push(InstallTarget {
                            agent: candidate.id.clone(),
                            scope: scope.into(),
                            project_id: project_id.clone(),
                        });
                        target.agents.push(candidate.id.clone());
                    }
                }
                target.agents.sort();
                target.agents.dedup();
                let digest = safe_files::digest(&source)?;
                let target_content_digest = crate::skills::content_digest(Path::new(&target.path))?;
                let known_unchanged = all_deployments.iter().any(|item| {
                    key(&item.path) == key(&target.path)
                        && item.skill_id.as_deref() == Some(&skill.id)
                        && item.baseline_digest.as_deref() == Some(&target_content_digest)
                });
                if target.before_digest != "absent"
                    && target.before_digest != digest
                    && !known_unchanged
                    && args["replaceModified"].as_bool() != Some(true)
                {
                    plan.can_execute = false;
                    plan.blocked_reason = Some("安装位置含有其他内容，确认覆盖后重新预览".into());
                }
                plan.source_path = Some(source.to_string_lossy().into());
                plan.source_digest = Some(digest);
                plan.locations.push(target);
            }
            "remove" => {
                let deployment = store
                    .get_deployment(required(args, "deploymentId")?)?
                    .context("安装记录不存在")?;
                plan.name = deployment.name.clone();
                plan.locations
                    .push(location(store, deployment.path, "deployment", None)?);
                plan.retain_backup = true;
            }
            "delete" => {
                let skill = plan.skill.as_ref().unwrap();
                library_path(store, skill)?;
                plan.locations
                    .push(location(store, skill.path.clone(), "library", None)?);
                if args["removeDeployments"].as_bool() == Some(true) {
                    for item in all_deployments
                        .iter()
                        .filter(|item| item.skill_id.as_deref() == Some(&skill.id))
                    {
                        if !plan
                            .locations
                            .iter()
                            .any(|location| key(&location.path) == key(&item.path))
                        {
                            plan.locations.push(location(
                                store,
                                item.path.clone(),
                                "deployment",
                                None,
                            )?);
                        }
                    }
                }
                plan.retain_backup = true;
            }
            "update" => {
                let skill = plan.skill.as_ref().unwrap();
                let checked = crate::skills::checked_update(store, required(args, "checkId")?)?;
                if checked.skill_id != skill.id {
                    bail!("检查记录不属于此 Skill");
                }
                let selected: Option<Vec<String>> = args
                    .get("locationIds")
                    .map(|value| serde_json::from_value(value.clone()))
                    .transpose()?;
                if selected.as_ref().is_some_and(|ids| ids.is_empty()) {
                    bail!("请选择更新位置");
                }
                if selected.as_ref().is_some_and(|ids| {
                    ids.iter()
                        .any(|id| !checked.locations.iter().any(|location| &location.id == id))
                }) {
                    bail!("更新位置不属于该检查");
                }
                for item in checked.locations {
                    if selected.as_ref().is_some_and(|ids| !ids.contains(&item.id)) {
                        continue;
                    }
                    let library = key(&item.path) == key(&skill.path);
                    if !library
                        && !all_deployments.iter().any(|deployment| {
                            deployment.skill_id.as_deref() == Some(&skill.id)
                                && key(&deployment.path) == key(&item.path)
                        })
                    {
                        bail!("更新位置未关联此 Skill");
                    }
                    if plan
                        .locations
                        .iter()
                        .any(|location| key(&location.path) == key(&item.path))
                    {
                        continue;
                    }
                    plan.locations.push(Location {
                        id: item.id,
                        path: item.path,
                        label: item.label,
                        agents: item.agents,
                        role: if library { "library" } else { "deployment" }.into(),
                        before_digest: item.before_digest,
                        differences: item.differences,
                    });
                }
                if plan.locations.is_empty() {
                    bail!("没有可更新位置");
                }
                plan.source_path = Some(checked.source_path.to_string_lossy().into());
                plan.source_digest = Some(checked.source_digest);
            }
            _ => unreachable!(),
        }
        validate_locations(store, &plan.locations, plan.skill.as_ref())?;
        plan.state_digest = state_digest(store)?;
        if let Some(source) = &plan.source_path {
            fs::create_dir_all(&work)?;
            safe_files::copy_tree(Path::new(source), &work.join("content"))?;
            if Some(safe_files::digest(&work.join("content"))?) != plan.source_digest {
                bail!("来源在预览期间发生变化");
            }
        }
        store.conn.execute(
            "INSERT INTO skill_change_plans VALUES(?1,?2,'ready')",
            params![plan.id, serde_json::to_string(&plan)?],
        )?;
        store.conn.execute_batch("COMMIT")?;
        Ok(public_plan(&plan))
    })();
    if result.is_err() {
        let _ = store.conn.execute_batch("ROLLBACK");
        let _ = safe_files::remove_tree(&work);
    }
    result
}

fn capture_records(store: &Store, plan: &Plan, extra_ids: &[String]) -> Result<Records> {
    let skill = if plan
        .locations
        .iter()
        .any(|location| location.role == "library")
    {
        plan.skill
            .as_ref()
            .map(|skill| store.get_skill(&skill.id))
            .transpose()?
            .flatten()
    } else {
        None
    };
    let deployments = store
        .list_deployments()?
        .into_iter()
        .filter(|item| {
            extra_ids.contains(&item.id)
                || plan.locations.iter().any(|location| {
                    location.role == "deployment" && key(&location.path) == key(&item.path)
                })
                || plan.action == "delete"
                    && item.skill_id.as_deref()
                        == plan.skill.as_ref().map(|skill| skill.id.as_str())
                    && item.skill_id.is_some()
        })
        .collect();
    Ok(Records { skill, deployments })
}
fn prepare_backup(store: &Store, plan: &Plan, before: Records) -> Result<Option<BackupManifest>> {
    if !plan.retain_backup
        || plan
            .locations
            .iter()
            .all(|location| location.before_digest == "absent")
    {
        return Ok(None);
    }
    let id = Uuid::new_v4().to_string();
    let root = owned_dir(store, "backups", &id)?;
    let result = (|| -> Result<BackupManifest> {
        fs::create_dir_all(&root)?;
        let mut locations = vec![];
        for (index, location) in plan.locations.iter().enumerate() {
            let slot = index.to_string();
            if location.before_digest != "absent" {
                safe_files::copy_tree(Path::new(&location.path), &root.join(&slot))?;
            }
            let digest = safe_files::digest(&root.join(&slot))?;
            if digest != location.before_digest {
                bail!("备份期间文件发生变化");
            }
            locations.push(SavedLocation {
                location: location.clone(),
                digest,
                slot,
            });
        }
        Ok(BackupManifest {
            id: id.clone(),
            root_digest: safe_files::digest(&root)?,
            locations,
            before,
            after: Records::default(),
        })
    })();
    if result.is_err() {
        let _ = safe_files::remove_tree(&root);
    }
    result.map(Some)
}
fn save_backup(store: &Store, plan: &Plan, backup: &BackupManifest) -> Result<()> {
    let root = owned_dir(store, "backups", &backup.id)?;
    store.conn.execute(
        "INSERT INTO backups(id,name,path,created_at,original_path) VALUES(?1,?2,?3,?4,?5)",
        params![
            backup.id,
            format!("{} · {}", action_label(&plan.action), plan.name),
            root.to_string_lossy(),
            now(),
            backup.locations[0].location.path
        ],
    )?;
    store.conn.execute(
        "INSERT INTO skill_backup_manifests VALUES(?1,?2)",
        params![backup.id, serde_json::to_string(backup)?],
    )?;
    Ok(())
}
fn apply_files(
    store: &Store,
    plan: &Plan,
    sources: &BTreeMap<String, PathBuf>,
    swaps: &mut Vec<Swap>,
) -> Result<()> {
    for (index, location) in plan.locations.iter().enumerate() {
        validate_location(store, location, plan.skill.as_ref())?;
        let path = PathBuf::from(&location.path);
        let parent = path.parent().context("位置没有父目录")?;
        safe_files::safe_directory(parent)?;
        fs::create_dir_all(parent)?;
        let old = parent.join(format!(".atb-{}-{index}-old", plan.id));
        let incoming = parent.join(format!(".atb-{}-{index}-new", plan.id));
        safe_files::safe_directory(&old)?;
        safe_files::safe_directory(&incoming)?;
        if old.exists() || incoming.exists() {
            bail!("此变更存在未清理的文件，请先检查保留目录");
        }
        swaps.push(Swap {
            path: path.clone(),
            old: old.clone(),
            incoming: incoming.clone(),
            moved_old: false,
            placed_new: false,
        });
        if let Some(source) = sources.get(&key(&location.path)) {
            safe_files::copy_tree(source, &incoming)?;
        }
        if safe_files::digest(&path)? != location.before_digest {
            bail!("文件在执行前发生变化，请重新预览");
        }
        if path.exists() {
            fs::rename(&path, &old).with_context(|| format!("保留原目录 {}", path.display()))?;
            swaps.last_mut().unwrap().moved_old = true;
            if safe_files::digest(&old)? != location.before_digest {
                bail!("文件在移动期间发生变化");
            }
        }
        if incoming.exists() {
            fs::rename(&incoming, &path)?;
            swaps.last_mut().unwrap().placed_new = true;
        }
        if let Some(source) = sources.get(&key(&location.path)) {
            if safe_files::digest(&path)? != safe_files::digest(source)? {
                bail!("写入后的内容校验失败");
            }
        }
    }
    Ok(())
}
fn rollback_files(swaps: &mut [Swap]) -> Result<()> {
    let mut errors = vec![];
    for swap in swaps.iter_mut().rev() {
        let result = (|| -> Result<()> {
            if swap.placed_new {
                safe_files::safe_directory(&swap.path)?;
                fs::rename(&swap.path, &swap.incoming)?;
                swap.placed_new = false;
            }
            if swap.moved_old {
                safe_files::safe_directory(&swap.old)?;
                fs::rename(&swap.old, &swap.path)?;
                swap.moved_old = false;
            }
            safe_files::remove_tree(&swap.incoming)?;
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(format!(
                "{}：{error:#}；原文件保留于 {}",
                swap.path.display(),
                swap.old.display()
            ));
        }
    }
    if !errors.is_empty() {
        bail!("回滚未完整完成：{}", errors.join("；"));
    }
    Ok(())
}
fn mutate_records(store: &Store, plan: &Plan) -> Result<()> {
    let content_digest = plan
        .source_path
        .as_deref()
        .map(|path| crate::skills::content_digest(Path::new(path)))
        .transpose()?;
    let mut deployments = store.list_deployments()?;
    match plan.action.as_str() {
        "install" => {
            let skill = plan.skill.as_ref().context("安装缺少 Skill")?;
            let location = &plan.locations[0];
            for target in &plan.install_targets {
                if !deployments.iter().any(|item| {
                    key(&item.path) == key(&location.path)
                        && item.agent == target.agent
                        && item.scope == target.scope
                        && item.project_id == target.project_id
                }) {
                    deployments.push(SkillDeployment {
                        id: Uuid::new_v4().to_string(),
                        skill_id: Some(skill.id.clone()),
                        name: skill.name.clone(),
                        description: skill.description.clone(),
                        agent: target.agent.clone(),
                        scope: target.scope.clone(),
                        profile: None,
                        project_id: target.project_id.clone(),
                        path: location.path.clone(),
                        source: skill.source.clone(),
                        owner: "agenthub".into(),
                        status: "present".into(),
                        ignored: false,
                        baseline_digest: None,
                    });
                }
            }
            for item in deployments
                .iter_mut()
                .filter(|item| key(&item.path) == key(&location.path))
            {
                item.skill_id = Some(skill.id.clone());
                item.source = skill.source.clone();
                item.name = skill.name.clone();
                item.description = skill.description.clone();
                item.baseline_digest = content_digest.clone();
                item.status = "present".into();
                item.owner = "agenthub".into();
                item.ignored = false;
                store.save_deployment(item)?;
            }
        }
        "remove" => {
            for item in deployments.iter().filter(|item| {
                plan.locations
                    .iter()
                    .any(|location| key(&location.path) == key(&item.path))
            }) {
                store.delete_deployment(&item.id)?;
            }
        }
        "update" => {
            let skill = plan.skill.as_ref().context("更新缺少 Skill")?;
            if plan
                .locations
                .iter()
                .any(|location| location.role == "library")
            {
                let mut current = skill.clone();
                if let Some(description) = native::skill_description(Path::new(&current.path)) {
                    current.description = description;
                }
                current.source_digest = content_digest.clone();
                current.updated_at = now();
                store.save_skill(&current)?;
            }
            for item in deployments.iter_mut().filter(|item| {
                plan.locations.iter().any(|location| {
                    location.role == "deployment" && key(&location.path) == key(&item.path)
                })
            }) {
                if let Some(description) = native::skill_description(Path::new(&item.path)) {
                    item.description = description;
                }
                item.baseline_digest = content_digest.clone();
                item.source = skill.source.clone();
                item.status = "present".into();
                store.save_deployment(item)?;
            }
        }
        "delete" => {
            let skill = plan.skill.as_ref().context("删除缺少 Skill")?;
            for item in &mut deployments {
                if plan.locations.iter().any(|location| {
                    location.role == "deployment" && key(&location.path) == key(&item.path)
                }) {
                    store.delete_deployment(&item.id)?;
                } else if item.skill_id.as_deref() == Some(&skill.id) {
                    item.skill_id = None;
                    store.save_deployment(item)?;
                }
            }
            store.delete_skill(&skill.id)?;
        }
        _ => bail!("计划动作无效"),
    }
    Ok(())
}
fn execute(store: &Store, action: &str, args: &Value) -> Result<Value> {
    if args["confirmed"].as_bool() != Some(true) {
        bail!("请先确认变更");
    }
    let id = required(args, "planId")?;
    let work = owned_dir(store, "skill-change-plans", id)?;
    let raw: Option<(String, String)> = store
        .conn
        .query_row(
            "SELECT payload_json,state FROM skill_change_plans WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (raw, state) = raw.context("计划不存在或已取消")?;
    if state != "ready" {
        bail!("计划已处理，请重新预览");
    }
    let mut plan: Plan = serde_json::from_str(&raw)?;
    if plan.id != id || plan.action != action {
        bail!("计划动作不匹配");
    }
    if !plan.can_execute {
        bail!(
            "{}",
            plan.blocked_reason.as_deref().unwrap_or("计划不能执行")
        );
    }
    unexpired(&plan.created_at)?;
    let mut swaps = vec![];
    let mut backup = None;
    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<Value> {
        if state_digest(store)? != plan.state_digest {
            bail!("配置或记录已变化，请重新预览");
        }
        validate_locations(store, &plan.locations, plan.skill.as_ref())?;
        for location in &plan.locations {
            if safe_files::digest(Path::new(&location.path))? != location.before_digest {
                bail!("文件已变化，请重新预览");
            }
        }
        let mut sources = BTreeMap::new();
        if let Some(source) = &plan.source_path {
            if Some(safe_files::digest(Path::new(source))?) != plan.source_digest
                || Some(safe_files::digest(&work.join("content"))?) != plan.source_digest
            {
                bail!("来源或计划内容已变化，请重新预览");
            }
            for location in &plan.locations {
                sources.insert(key(&location.path), work.join("content"));
            }
        }
        let before = capture_records(store, &plan, &[])?;
        let before_ids: Vec<_> = before
            .deployments
            .iter()
            .map(|item| item.id.clone())
            .collect();
        backup = prepare_backup(store, &plan, before)?;
        let digests = plan
            .locations
            .iter()
            .map(|location| {
                (
                    key(&location.path),
                    plan.source_digest
                        .clone()
                        .unwrap_or_else(|| "absent".into()),
                )
            })
            .collect();
        persist_executing(
            store,
            &mut plan,
            backup.as_ref().map(|backup| backup.id.clone()),
            digests,
        )?;
        apply_files(store, &plan, &sources, &mut swaps)?;
        verify_written(
            &plan,
            &swaps,
            &plan
                .locations
                .iter()
                .map(|location| {
                    (
                        key(&location.path),
                        plan.source_digest
                            .clone()
                            .unwrap_or_else(|| "absent".into()),
                    )
                })
                .collect(),
        )?;
        if let Some(source) = &plan.source_path {
            if Some(safe_files::digest(Path::new(source))?) != plan.source_digest {
                bail!("来源在执行期间发生变化");
            }
        }
        mutate_records(store, &plan)?;
        if let Some(backup) = &mut backup {
            backup.after = capture_records(store, &plan, &before_ids)?;
            save_backup(store, &plan, backup)?;
        }
        let result = json!({"id":Uuid::new_v4().to_string(),"status":"succeeded","action":action,"skillId":plan.skill.as_ref().map(|skill|&skill.id),"summary":format!("{}完成：{}",action_label(action),plan.name),"locations":plan.locations.iter().map(|location|&location.path).collect::<Vec<_>>(),"backupId":backup.as_ref().map(|backup|&backup.id),"createdAt":now()});
        store.record_change(&result)?;
        store.conn.execute(
            "UPDATE skill_change_plans SET state='committed' WHERE id=?1",
            [id],
        )?;
        store.conn.execute_batch("COMMIT")?;
        Ok(result)
    })();
    match result {
        Ok(mut result) => {
            let warnings = finish_committed(store, &plan);
            if !warnings.is_empty() {
                result["warnings"] = json!(warnings);
                let _ = store.record_change(&result);
            }
            Ok(result)
        }
        Err(error) => {
            let rollback = rollback_files(&mut swaps);
            let _ = store.conn.execute_batch("ROLLBACK");
            if rollback.is_ok() {
                if let Some(backup) = &backup {
                    let _ = safe_files::remove_tree(&owned_dir(store, "backups", &backup.id)?);
                }
            }
            if let Err(rollback_error) = rollback {
                bail!("变更失败：{error:#}。{rollback_error:#}");
            }
            mark_rolled_back(store, &plan, &format!("{error:#}"))?;
            Err(error)
        }
    }
}
fn cancel(store: &Store, args: &Value) -> Result<Value> {
    let id = required(args, "planId")?;
    let path = owned_dir(store, "skill-change-plans", id)?;
    let count = store.conn.execute(
        "DELETE FROM skill_change_plans WHERE id=?1 AND state='ready'",
        [id],
    )?;
    if count > 0 {
        safe_files::remove_tree(&path)?;
    }
    Ok(json!({"ok":true,"cancelled":count>0}))
}
fn list_backups(store: &Store) -> Result<Value> {
    let mut statement=store.conn.prepare("SELECT id,name,path,created_at,original_path FROM backups ORDER BY created_at DESC,id DESC")?;
    let rows=statement.query_map([],|row|Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"path":row.get::<_,String>(2)?,"createdAt":row.get::<_,String>(3)?,"originalPath":row.get::<_,String>(4)?})))?;
    Ok(json!({"backups":rows.collect::<rusqlite::Result<Vec<_>>>()?}))
}

fn verify_written(plan: &Plan, swaps: &[Swap], expected: &BTreeMap<String, String>) -> Result<()> {
    for (location, swap) in plan.locations.iter().zip(swaps) {
        if safe_files::digest(Path::new(&location.path))?
            != *expected.get(&key(&location.path)).context("缺少写入摘要")?
        {
            bail!("写入内容发生变化，正在回滚");
        }
        if swap.moved_old && safe_files::digest(&swap.old)? != location.before_digest {
            bail!("原文件在执行期间发生变化，正在回滚");
        }
    }
    Ok(())
}
fn validate_restore_records(store: &Store, backup: &BackupManifest) -> Result<()> {
    if let Some(skill) = &backup.before.skill {
        if store
            .get_skill(&skill.id)?
            .is_some_and(|current| key(&current.path) != key(&skill.path))
        {
            bail!("Skill 位置已改变，不能恢复该备份");
        }
    }
    let current = store.list_deployments()?;
    for expected in &backup.after.deployments {
        if current
            .iter()
            .find(|item| item.id == expected.id)
            .is_some_and(|item| {
                key(&item.path) != key(&expected.path)
                    || item.agent != expected.agent
                    || item.scope != expected.scope
                    || item.skill_id != expected.skill_id
            })
        {
            bail!("备份涉及的安装关联已改变");
        }
    }
    for item in &current {
        if backup.locations.iter().any(|saved| {
            saved.location.role == "deployment" && key(&saved.location.path) == key(&item.path)
        }) && !backup
            .after
            .deployments
            .iter()
            .any(|expected| expected.id == item.id)
        {
            bail!("恢复位置已被新的安装记录使用");
        }
    }
    Ok(())
}
fn restore_records(store: &Store, backup: &BackupManifest) -> Result<()> {
    if let Some(skill) = &backup.before.skill {
        store.save_skill(skill)?;
    }
    let ids: BTreeSet<_> = backup
        .before
        .deployments
        .iter()
        .chain(&backup.after.deployments)
        .map(|item| item.id.clone())
        .collect();
    for current in store.list_deployments()? {
        if ids.contains(&current.id)
            || backup.locations.iter().any(|saved| {
                saved.location.role == "deployment"
                    && key(&saved.location.path) == key(&current.path)
            })
        {
            store.delete_deployment(&current.id)?;
        }
    }
    for original in &backup.before.deployments {
        let mut deployment = original.clone();
        if let Some(skill_id) = &deployment.skill_id {
            if store.get_skill(skill_id)?.is_none() {
                deployment.skill_id = None;
            }
        }
        deployment.status = if Path::new(&deployment.path).join("SKILL.md").is_file() {
            "present"
        } else {
            "missing"
        }
        .into();
        store.save_deployment(&deployment)?;
    }
    Ok(())
}
fn restore(store: &Store, args: &Value) -> Result<Value> {
    if args["confirmed"].as_bool() != Some(true) {
        bail!("请先确认恢复");
    }
    let id = required(args, "id")?;
    let root = owned_dir(store, "backups", id)?;
    let raw:Option<(String,String)>=store.conn.query_row("SELECT b.path,m.payload_json FROM backups b JOIN skill_backup_manifests m ON m.backup_id=b.id WHERE b.id=?1",[id],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    let (saved_path, raw) = raw.context("备份不存在或缺少可信清单")?;
    if key(&saved_path) != protection::path_key(&root) {
        bail!("备份目录不属于当前数据库");
    }
    let backup: BackupManifest = serde_json::from_str(&raw)?;
    if backup.id != id || backup.locations.is_empty() {
        bail!("备份清单无效");
    }
    if safe_files::digest(&root)? != backup.root_digest {
        bail!("备份内容已被修改，拒绝恢复");
    }
    let mut plan = Plan {
        id: Uuid::new_v4().to_string(),
        action: "restore".into(),
        skill: backup
            .before
            .skill
            .clone()
            .or_else(|| backup.after.skill.clone()),
        name: backup
            .before
            .skill
            .as_ref()
            .map(|skill| skill.name.clone())
            .unwrap_or_else(|| "安装位置".into()),
        created_at: now(),
        state_digest: String::new(),
        source_path: None,
        source_digest: None,
        locations: vec![],
        install_targets: vec![],
        retain_backup: true,
        can_execute: true,
        blocked_reason: None,
        recovery_digests: BTreeMap::new(),
        recovery_backup_id: None,
    };
    let mut sources = BTreeMap::new();
    let mut expected = BTreeMap::new();
    for (index, saved) in backup.locations.iter().enumerate() {
        if saved.slot != index.to_string() {
            bail!("备份槽位无效");
        }
        let source = root.join(&saved.slot);
        if safe_files::digest(&source)? != saved.digest {
            bail!("备份文件摘要不匹配");
        }
        let mut location = location(
            store,
            saved.location.path.clone(),
            &saved.location.role,
            if saved.digest == "absent" {
                None
            } else {
                Some(&source)
            },
        )?;
        location.id = saved.location.id.clone();
        if saved.digest != "absent" {
            sources.insert(key(&location.path), source);
        }
        expected.insert(key(&location.path), saved.digest.clone());
        plan.locations.push(location);
    }
    validate_locations(store, &plan.locations, plan.skill.as_ref())?;
    validate_restore_records(store, &backup)?;
    let mut swaps = vec![];
    let mut redo = None;
    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<Value> {
        validate_restore_records(store, &backup)?;
        let ids: Vec<_> = backup
            .before
            .deployments
            .iter()
            .chain(&backup.after.deployments)
            .map(|item| item.id.clone())
            .collect();
        let before = capture_records(store, &plan, &ids)?;
        plan.state_digest = state_digest(store)?;
        redo = prepare_backup(store, &plan, before)?;
        persist_executing(
            store,
            &mut plan,
            redo.as_ref().map(|redo| redo.id.clone()),
            expected.clone(),
        )?;
        apply_files(store, &plan, &sources, &mut swaps)?;
        verify_written(&plan, &swaps, &expected)?;
        if safe_files::digest(&root)? != backup.root_digest {
            bail!("备份在恢复期间发生变化");
        }
        restore_records(store, &backup)?;
        if let Some(redo) = &mut redo {
            redo.after = capture_records(store, &plan, &ids)?;
            save_backup(store, &plan, redo)?;
        }
        let result = json!({"id":Uuid::new_v4().to_string(),"status":"succeeded","action":"restore","summary":"已恢复 Skill 备份","restoredBackupId":id,"backupId":redo.as_ref().map(|redo|&redo.id),"createdAt":now()});
        store.record_change(&result)?;
        store.conn.execute(
            "UPDATE skill_change_plans SET state='committed' WHERE id=?1",
            [&plan.id],
        )?;
        store.conn.execute_batch("COMMIT")?;
        Ok(result)
    })();
    match result {
        Ok(mut result) => {
            let warnings = finish_committed(store, &plan);
            if !warnings.is_empty() {
                result["warnings"] = json!(warnings);
                let _ = store.record_change(&result);
            }
            Ok(result)
        }
        Err(error) => {
            let rollback = rollback_files(&mut swaps);
            let _ = store.conn.execute_batch("ROLLBACK");
            if rollback.is_ok() {
                if let Some(redo) = &redo {
                    let _ = safe_files::remove_tree(&owned_dir(store, "backups", &redo.id)?);
                }
            }
            if let Err(rollback_error) = rollback {
                bail!("恢复失败：{error:#}。{rollback_error:#}");
            }
            mark_rolled_back(store, &plan, &format!("{error:#}"))?;
            Err(error)
        }
    }
}

/// Persist the exact Skill locations before any rename. A process exit rolls back
/// only the following domain transaction; this committed intent remains recoverable.
fn persist_executing(
    store: &Store,
    plan: &mut Plan,
    backup_id: Option<String>,
    digests: BTreeMap<String, String>,
) -> Result<()> {
    plan.recovery_backup_id = backup_id;
    plan.recovery_digests = digests;
    store.conn.execute("INSERT INTO skill_change_plans(id,payload_json,state) VALUES(?1,?2,'executing') ON CONFLICT(id) DO UPDATE SET payload_json=excluded.payload_json,state='executing'",params![plan.id,serde_json::to_string(&plan)?])?;
    store.conn.execute_batch("COMMIT")?;
    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    if state_digest(store)? != plan.state_digest {
        bail!("配置或记录在执行前变化，请重新预览");
    }
    validate_locations(store, &plan.locations, plan.skill.as_ref())?;
    for location in &plan.locations {
        if safe_files::digest(Path::new(&location.path))? != location.before_digest {
            bail!("文件在执行前变化，请重新预览");
        }
    }
    Ok(())
}
