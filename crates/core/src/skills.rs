//! Skill library content, discovered Agent locations, and immutable source comparisons.
use crate::{
    model::{ScanRoot, Skill, SkillDeployment, Source},
    native, protection, safe_files, scan, sources,
    store::Store,
    targets,
};
use anyhow::{bail, Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

const MAX_TEXT: usize = 1024 * 1024;

pub fn dispatch(store: &Store, method: &str, args: &Value) -> Result<Value> {
    match method {
        "skills.scan" => {
            let _lock = crate::recovery::lock(store)?;
            scan_existing(store, args)
        }
        "skills.add" => {
            let _lock = crate::recovery::lock(store)?;
            add(store, args)
        }
        "skills.import" => {
            let _lock = crate::recovery::lock(store)?;
            import(store, args)
        }
        "skills.ignore" => {
            let _lock = crate::recovery::lock(store)?;
            ignore(store, args)
        }
        "skills.metadata.save" => {
            let _lock = crate::recovery::lock(store)?;
            metadata(store, args)
        }
        "skills.bindSource" => {
            let _lock = crate::recovery::lock(store)?;
            bind_source(store, args)
        }
        "skills.unbindSource" => {
            let _lock = crate::recovery::lock(store)?;
            let mut item = skill(store, required(args, "skillId")?)?;
            let had_source = item.source.kind != "unknown";
            item.source = Source {
                kind: "unknown".into(),
                locator: String::new(),
                subpath: None,
                revision: None,
            };
            item.source_digest = None;
            item.updated_at = now();
            let transaction = store.conn.unchecked_transaction()?;
            store.save_skill(&item)?;
            clear_status(store, &item.id)?;
            if had_source {
                store.record_activity("skill", &format!("删除 Skill 来源 · {}", item.name), &[])?;
            }
            transaction.commit()?;
            Ok(json!({"skill":item}))
        }
        "skills.check" => check_options(
            store,
            required(args, "skillId")?,
            args["batchId"].as_str(),
            args["useCached"].as_bool() == Some(true),
        ),
        "skills.checkBatch.start" => crate::check_jobs::begin(store),
        "skills.checkBatch.finish" => crate::check_jobs::end(store, required(args, "batchId")?),
        "skills.diff" => diff(store, args),
        "skills.files" => list_files(store, args),
        "skills.read" => read_file(store, args),
        _ => bail!("未知 Skill 操作"),
    }
}
fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("缺少 {key}"))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn skill(store: &Store, id: &str) -> Result<Skill> {
    store.get_skill(id)?.context("Skill 不存在")
}
fn lexical(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    let value = value
        .strip_prefix("//?/")
        .unwrap_or(&value)
        .trim_end_matches('/')
        .to_string();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}
fn same_location(a: &SkillDeployment, b: &SkillDeployment) -> bool {
    a.agent == b.agent
        && a.scope == b.scope
        && a.profile.as_deref().unwrap_or("") == b.profile.as_deref().unwrap_or("")
        && protection::path_key(Path::new(&a.path)) == protection::path_key(Path::new(&b.path))
}
pub fn deployment_values(store: &Store) -> Result<Vec<Value>> {
    store
        .list_deployments()?
        .into_iter()
        .map(|deployment| {
            let mut value = json!(deployment);
            value["present"] = json!(Path::new(&deployment.path).join("SKILL.md").is_file());
            value["pathKey"] = json!(protection::path_key(Path::new(&deployment.path)));
            Ok(value)
        })
        .collect()
}
fn roots(store: &Store, args: &Value) -> Result<Vec<ScanRoot>> {
    if let Some(value) = args.get("scanRoots") {
        return Ok(serde_json::from_value(value.clone())?);
    }
    let settings = store.get_settings()?;
    let fresh = !store.settings_saved()?;
    let mut roots = if settings.scan_roots.is_empty() && fresh {
        scan::default_scan_roots()
            .into_iter()
            .filter(|r| Path::new(&r.path).is_dir())
            .collect()
    } else {
        settings.scan_roots
    };
    let configured = targets::configured(store)?;
    let all_targets = targets::list(store)?;
    for target in all_targets.iter().filter(|t| t.enabled) {
        if (fresh || configured.iter().any(|c| c.id == target.id))
            && Path::new(&target.global_path).is_dir()
        {
            roots.push(ScanRoot {
                id: String::new(),
                agent: target.id.clone(),
                path: target.global_path.clone(),
                scope: "global".into(),
                profile: None,
            });
        }
        for project in store.list_projects()?.iter().filter(|p| !p.archived) {
            let mut paths = vec![target.project_path.as_str()];
            if target.id == "codex" {
                paths.push(".codex/skills");
            }
            if target.id == "hermes" {
                paths.extend([".hermes/skills", ".agents/skills"]);
            }
            for relative in paths.into_iter().filter(|p| !p.is_empty()) {
                let path = Path::new(&project.path).join(relative);
                if path.is_dir() {
                    roots.push(ScanRoot {
                        id: String::new(),
                        agent: target.id.clone(),
                        path: path.to_string_lossy().into(),
                        scope: "project".into(),
                        profile: None,
                    });
                }
            }
        }
    }
    for deployment in store
        .list_deployments()?
        .into_iter()
        .filter(|d| d.owner == "agenthub")
    {
        if let Some(parent) = Path::new(&deployment.path).parent().filter(|p| p.is_dir()) {
            roots.push(ScanRoot {
                id: String::new(),
                agent: deployment.agent,
                scope: deployment.scope,
                profile: deployment.profile,
                path: parent.to_string_lossy().into(),
            });
        }
    }
    let disabled: HashSet<_> = all_targets
        .iter()
        .filter(|t| !t.enabled)
        .map(|t| t.id.as_str())
        .collect();
    let mut seen = HashSet::new();
    roots.retain(|r| {
        !disabled.contains(r.agent.as_str())
            && !protection::is_host_managed(Path::new(&r.path))
            && seen.insert((
                r.agent.clone(),
                r.scope.clone(),
                r.profile.clone(),
                lexical(Path::new(&r.path)),
            ))
    });
    Ok(roots)
}
fn scan_existing(store: &Store, args: &Value) -> Result<Value> {
    let roots = roots(store, args)?;
    let report = scan::scan_roots(&roots);
    let existing = store.list_deployments()?;
    let mut known = existing.clone();
    let projects = store.list_projects()?;
    let mut observed_ids = HashSet::new();
    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<()> {
        for mut observed in report.deployments {
            if let Some(saved) = known.iter().find(|d| same_location(d, &observed)) {
                observed.id = saved.id.clone();
                observed.path = saved.path.clone();
                observed.skill_id = saved.skill_id.clone();
                observed.project_id = saved.project_id.clone();
                observed.ignored = saved.ignored;
                observed.baseline_digest = saved.baseline_digest.clone();
                if saved.source.kind != "unknown" {
                    observed.source = saved.source.clone();
                }
                if saved.owner != "unknown" {
                    observed.owner = saved.owner.clone();
                }
                if saved.owner == "agenthub" {
                    observed.status = "installed".into();
                }
            }
            if observed.scope == "project" && observed.project_id.is_none() {
                observed.project_id = projects
                    .iter()
                    .filter(|p| Path::new(&observed.path).starts_with(&p.path))
                    .max_by_key(|p| p.path.len())
                    .map(|p| p.id.clone());
            }
            observed_ids.insert(observed.id.clone());
            store.save_deployment(&observed)?;
            known.retain(|d| d.id != observed.id);
            known.push(observed);
        }
        for mut saved in existing {
            if observed_ids.contains(&saved.id) {
                continue;
            }
            if report.successful_roots.iter().any(|r| {
                r.agent == saved.agent
                    && r.scope == saved.scope
                    && r.profile.as_deref().unwrap_or("") == saved.profile.as_deref().unwrap_or("")
                    && lexical(Path::new(&saved.path))
                        .starts_with(&(lexical(Path::new(&r.path)) + "/"))
            }) && !Path::new(&saved.path).join("SKILL.md").is_file()
            {
                saved.status = "missing".into();
                store.save_deployment(&saved)?;
            }
        }
        Ok(())
    })();
    finish_transaction(store, result)?;
    Ok(
        json!({"deployments":deployment_values(store)?,"warnings":report.warnings,"completedRoots":report.successful_roots.len(),"totalRoots":roots.len()}),
    )
}
fn finish_transaction<T>(store: &Store, result: Result<T>) -> Result<T> {
    match result.and_then(|value| {
        store.conn.execute_batch("COMMIT")?;
        Ok(value)
    }) {
        Ok(value) => Ok(value),
        Err(error) => {
            let _ = store.conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}
fn add_copy(
    store: &Store,
    path: &Path,
    name: &str,
    description: &str,
    source: Source,
) -> Result<Skill> {
    safe_files::safe_directory(path)?;
    if !path.join("SKILL.md").is_file() {
        bail!("来源不是 Skill 目录");
    }
    let digest = safe_files::digest(path)?;
    let content = content_digest(path)?;
    if let Some(mut existing) = store
        .list_skills()?
        .into_iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
    {
        if content_digest(Path::new(&existing.path))? != content {
            bail!("同名 Skill 已在库中且内容不同，请明确选择更新来源后查看变化");
        }
        if existing.source.kind != "unknown"
            && source.kind != "unknown"
            && sources::source_identity(&existing.source)? != sources::source_identity(&source)?
        {
            bail!("同名 Skill 的来源不同，无法自动认定为同一个 Skill");
        }
        if existing.source.kind == "unknown" && source.kind != "unknown" {
            existing.source = source;
            existing.source_digest = Some(content);
            existing.updated_at = now();
            store.save_skill(&existing)?;
        }
        return Ok(existing);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let destination = crate::library::allocate(store, &id, name)?;
    let timestamp = now();
    let item = Skill {
        id,
        name: name.to_string(),
        description: description.to_string(),
        path: destination.to_string_lossy().into(),
        source,
        source_digest: Some(content),
        tags: Vec::new(),
        favorite: false,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };
    let stage = store
        .data_dir()
        .join("staging")
        .join(uuid::Uuid::new_v4().to_string());
    let mut moved = false;
    let result = (|| -> Result<()> {
        safe_files::copy_tree(path, &stage)?;
        if safe_files::digest(path)? != digest || safe_files::digest(&stage)? != digest {
            bail!("复制期间 Skill 内容已变化，请重试");
        }
        safe_files::safe_directory(&destination)?;
        if std::fs::symlink_metadata(&destination).is_ok() {
            bail!("Skill 库目录已被占用，请重试");
        }
        std::fs::create_dir_all(destination.parent().context("Skill 库目录缺失")?)?;
        std::fs::rename(&stage, &destination)?;
        moved = true;
        store.save_skill(&item)
    })();
    let _ = safe_files::remove_tree(&stage);
    if let Err(error) = result {
        if moved && safe_files::digest(&destination).ok().as_deref() == Some(&digest) {
            let _ = safe_files::remove_tree(&destination);
        }
        let _ = store.conn.execute(
            "DELETE FROM skill_library_paths WHERE skill_id=?1",
            [&item.id],
        );
        return Err(error);
    }
    Ok(item)
}
fn add(store: &Store, args: &Value) -> Result<Value> {
    let (source, path, candidate) = sources::selected(
        store,
        required(args, "inspectionId")?,
        args["subpath"].as_str().unwrap_or(""),
    )?;
    let item = add_copy(
        store,
        &path,
        candidate["name"].as_str().context("Skill 名称缺失")?,
        candidate["description"].as_str().unwrap_or(""),
        source,
    )?;
    Ok(json!({"skill":item}))
}
fn import(store: &Store, args: &Value) -> Result<Value> {
    let ids: Vec<String> = serde_json::from_value(args["deploymentIds"].clone())?;
    if ids.is_empty() {
        bail!("请选择要导入的 Skill");
    }
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for id in ids.into_iter().filter(|id| seen.insert(id.clone())) {
        let result = (|| -> Result<Skill> {
            let mut deployment = store.get_deployment(&id)?.context("安装位置不存在")?;
            protection::ensure_independent_skill(Path::new(&deployment.path))?;
            if deployment.owner == "host" {
                bail!("宿主管理的 Skill 不能独立导入");
            }
            let item = add_copy(
                store,
                Path::new(&deployment.path),
                &deployment.name,
                &deployment.description,
                deployment.source.clone(),
            )?;
            deployment.skill_id = Some(item.id.clone());
            deployment.ignored = false;
            deployment.baseline_digest = Some(content_digest(Path::new(&deployment.path))?);
            store.save_deployment(&deployment)?;
            Ok(item)
        })();
        match result {
            Ok(item) => results.push(json!({"deploymentId":id,"status":"succeeded","skill":item})),
            Err(error) => results
                .push(json!({"deploymentId":id,"status":"failed","message":format!("{error:#}")})),
        }
    }
    Ok(json!({"results":results,"originalFilesChanged":false}))
}
fn ignore(store: &Store, args: &Value) -> Result<Value> {
    let ids: Vec<String> = serde_json::from_value(args["deploymentIds"].clone())?;
    let ignored = args["ignored"].as_bool().context("缺少 ignored")?;
    let mut rows = Vec::new();
    for id in ids {
        let mut row = store.get_deployment(&id)?.context("安装位置不存在")?;
        if row.skill_id.is_some() {
            bail!("已导入的 Skill 请在库中管理");
        }
        row.ignored = ignored;
        rows.push(row);
    }
    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    finish_transaction(
        store,
        (|| -> Result<()> {
            for row in rows {
                store.save_deployment(&row)?;
            }
            Ok(())
        })(),
    )?;
    Ok(json!({"deployments":deployment_values(store)?}))
}
fn metadata(store: &Store, args: &Value) -> Result<Value> {
    let ids: Vec<String> = serde_json::from_value(args["ids"].clone())?;
    let tags: Option<Vec<String>> = args
        .get("tags")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?;
    let favorite: Option<bool> = args
        .get("favorite")
        .map(|v| v.as_bool().context("favorite 必须为布尔值"))
        .transpose()?;
    if tags
        .as_ref()
        .is_some_and(|tags| tags.len() > 50 || tags.iter().any(|t| t.len() > 128))
    {
        bail!("标签过多或过长");
    }
    let mut items = Vec::new();
    for id in ids {
        let mut item = skill(store, &id)?;
        if let Some(tags) = &tags {
            let mut seen = HashSet::new();
            item.tags = tags
                .iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty() && seen.insert(t.clone()))
                .collect();
        }
        if let Some(favorite) = favorite {
            item.favorite = favorite;
        }
        item.updated_at = now();
        items.push(item);
    }
    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    finish_transaction(
        store,
        (|| -> Result<()> {
            for item in &items {
                store.save_skill(item)?;
            }
            Ok(())
        })(),
    )?;
    Ok(json!({"skills":items}))
}
fn bind_source(store: &Store, args: &Value) -> Result<Value> {
    let mut item = skill(store, required(args, "skillId")?)?;
    let (source, path, _) = sources::selected(
        store,
        required(args, "inspectionId")?,
        args["subpath"].as_str().unwrap_or(""),
    )?;
    let changed = item.source != source;
    item.source = source;
    item.source_digest = Some(content_digest(&path)?);
    item.updated_at = now();
    let transaction = store.conn.unchecked_transaction()?;
    store.save_skill(&item)?;
    if changed {
        store.record_activity(
            "skill",
            &format!("设置 Skill 来源 · {}", item.name),
            &[item.source.locator.clone()],
        )?;
    }
    transaction.commit()?;
    Ok(json!({"skill":item}))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckedLocation {
    pub id: String,
    pub path: String,
    pub label: String,
    pub agents: Vec<String>,
    pub before_digest: String,
    pub differences: Vec<Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckedUpdate {
    pub id: String,
    pub skill_id: String,
    pub created_at: String,
    pub source_path: PathBuf,
    pub source_digest: String,
    pub skill_json: Value,
    pub deployments_json: Value,
    pub locations: Vec<CheckedLocation>,
}
fn check_table(store: &Store) -> Result<()> {
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS skill_checks(id TEXT PRIMARY KEY,payload_json TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS skill_update_status(skill_id TEXT PRIMARY KEY REFERENCES skills(id) ON DELETE CASCADE,payload_json TEXT NOT NULL);",
    )?;
    Ok(())
}
fn linked(store: &Store, id: &str) -> Result<Vec<SkillDeployment>> {
    let mut rows: Vec<_> = store
        .list_deployments()?
        .into_iter()
        .filter(|d| d.skill_id.as_deref() == Some(id))
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rows)
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedStatus {
    result: Value,
    skill: Skill,
    deployments: Vec<SkillDeployment>,
}
fn saved_statuses(store: &Store) -> Result<Vec<SavedStatus>> {
    check_table(store)?;
    let mut statement = store
        .conn
        .prepare("SELECT payload_json FROM skill_update_status ORDER BY skill_id")?;
    let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
    rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
}
fn save_status(store: &Store, item: &Skill, result: &Value) -> Result<()> {
    check_table(store)?;
    let saved = SavedStatus {
        result: result.clone(),
        skill: item.clone(),
        deployments: linked(store, &item.id)?,
    };
    store.conn.execute("INSERT INTO skill_update_status(skill_id,payload_json) VALUES(?1,?2) ON CONFLICT(skill_id) DO UPDATE SET payload_json=excluded.payload_json",rusqlite::params![item.id,serde_json::to_string(&saved)?])?;
    Ok(())
}
fn clear_status(store: &Store, id: &str) -> Result<()> {
    check_table(store)?;
    store
        .conn
        .execute("DELETE FROM skill_update_status WHERE skill_id=?1", [id])?;
    Ok(())
}
fn same_source(a: &Skill, b: &Skill) -> bool {
    match (
        sources::source_identity(&a.source),
        sources::source_identity(&b.source),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
pub(crate) fn statuses(store: &Store) -> Result<Vec<Value>> {
    let mut results = Vec::new();
    for saved in saved_statuses(store)? {
        let Some(item) = store.get_skill(&saved.skill.id)? else {
            continue;
        };
        if item.source.kind == "unknown" || !same_source(&item, &saved.skill) {
            continue;
        }
        let mut result = saved.result;
        let age = result["checkedAt"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| chrono::Utc::now().signed_duration_since(t).num_seconds());
        result["needsRefresh"] = json!(
            json!(item) != json!(saved.skill)
                || json!(linked(store, &item.id)?) != json!(saved.deployments)
                || age.is_none_or(|a| !(0..86400).contains(&a))
                || result["checkId"]
                    .as_str()
                    .is_some_and(|id| check_root(store, id).map_or(true, |p| !p.exists()))
        );
        results.push(result);
    }
    Ok(results)
}
/// Cached bytes may be reused to compare against changed local contents, but
/// their source identity, ownership, lifetime and digest still have to match.
fn cached_source(store: &Store, item: &Skill) -> Result<Option<(PathBuf, String)>> {
    let Some(saved) = saved_statuses(store)?
        .into_iter()
        .find(|s| s.skill.id == item.id && same_source(item, &s.skill))
    else {
        return Ok(None);
    };
    let Some(id) = saved.result["checkId"].as_str() else {
        return Ok(None);
    };
    let root = check_root(store, id)?;
    let raw: Option<String> = store
        .conn
        .query_row(
            "SELECT payload_json FROM skill_checks WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let checked: CheckedUpdate = serde_json::from_str(&raw)?;
    let age = chrono::Utc::now()
        .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&checked.created_at)?)
        .num_seconds();
    if checked.id != id
        || checked.skill_id != item.id
        || !(0..86400).contains(&age)
        || !checked.source_path.starts_with(root.join("source"))
        || safe_files::digest(&checked.source_path)? != checked.source_digest
    {
        return Ok(None);
    }
    Ok(Some((checked.source_path, checked.created_at)))
}
fn check_root(store: &Store, id: &str) -> Result<PathBuf> {
    Ok(store
        .data_dir()
        .join("skill-checks")
        .join(uuid::Uuid::parse_str(id)?.to_string()))
}
pub fn checked_update(store: &Store, id: &str) -> Result<CheckedUpdate> {
    let root = check_root(store, id)?;
    check_table(store)?;
    let raw: String = store
        .conn
        .query_row(
            "SELECT payload_json FROM skill_checks WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .context("更新检查已失效，请重新检查")?;
    let checked: CheckedUpdate = serde_json::from_str(&raw)?;
    let age = chrono::Utc::now()
        .signed_duration_since(chrono::DateTime::parse_from_rfc3339(&checked.created_at)?)
        .num_seconds();
    if !(0..86400).contains(&age) || checked.id != id {
        bail!("更新检查已过期");
    }
    if !checked.source_path.starts_with(root.join("source")) {
        bail!("来源快照路径无效");
    }
    safe_files::safe_directory(&checked.source_path)?;
    if safe_files::digest(&checked.source_path)? != checked.source_digest {
        bail!("来源快照已改变，请重新检查");
    }
    if json!(skill(store, &checked.skill_id)?) != checked.skill_json
        || json!(linked(store, &checked.skill_id)?) != checked.deployments_json
    {
        bail!("Skill 来源或安装记录已改变，请重新检查");
    }
    for location in &checked.locations {
        if safe_files::digest(Path::new(&location.path))? != location.before_digest {
            bail!("本地 Skill 内容已改变，请重新检查");
        }
    }
    Ok(checked)
}
fn file_bytes(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    if !root.exists() {
        return Ok(files);
    }
    safe_files::safe_directory(root)?;
    native::inspect_tree(root)?;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !matches!(e.file_name().to_str(), Some(".git" | "__pycache__")))
    {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().is_none_or(|e| e != "pyc") {
            files.insert(
                entry
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read(entry.path())?,
            );
        }
    }
    Ok(files)
}
/// Published Skill content uses the same file set as the visible differences.
/// Full directory fingerprints remain separate and guard copying, stale plans,
/// source snapshots, and rollback, including ignored metadata files.
pub fn content_digest(root: &Path) -> Result<String> {
    safe_files::safe_directory(root)?;
    if !root.exists() {
        return Ok("absent".into());
    }
    Ok(content_digest_of(&file_bytes(root)?))
}
fn content_digest_of(files: &BTreeMap<String, Vec<u8>>) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for (path, bytes) in files {
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}
fn is_text(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|s| !s.contains('\0'))
}
fn differences(old: &BTreeMap<String, Vec<u8>>, new: &BTreeMap<String, Vec<u8>>) -> Vec<Value> {
    old.keys().chain(new.keys()).collect::<BTreeSet<_>>().into_iter().filter_map(|path|{
        let before=old.get(path);let after=new.get(path);if before==after{return None;}
        let binary=before.into_iter().chain(after).any(|bytes|!is_text(bytes));let large=before.into_iter().chain(after).any(|bytes|bytes.len()>MAX_TEXT);
        Some(json!({"path":path,"change":if before.is_none(){"added"}else if after.is_none(){"deleted"}else{"modified"},"oldSize":before.map(Vec::len),"newSize":after.map(Vec::len),"binary":binary,"previewable":!binary&&!large}))
    }).collect()
}
fn compare_status(current: &str, latest: &str, baseline: Option<&str>) -> &'static str {
    if current == latest {
        "current"
    } else if let Some(base) = baseline {
        match (latest != base, current != base) {
            (true, true) => "diverged",
            (true, false) => "available",
            (false, _) => "modified",
        }
    } else {
        "different"
    }
}
pub fn check(store: &Store, id: &str) -> Result<Value> {
    check_options(store, id, None, false)
}
pub(crate) fn check_all(store: &Store) -> Result<Vec<Value>> {
    let ids: Vec<_> = store.list_skills()?.into_iter().map(|s| s.id).collect();
    let session = crate::check_jobs::begin(store)?;
    let batch_id = session["batchId"].as_str().context("批量检查标识缺失")?;
    let index = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    let data = store.data_dir();
    std::thread::scope(|scope| {
        for _ in 0..ids.len().min(3) {
            scope.spawn(|| loop {
                let i = index.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(id) = ids.get(i) else {
                    break;
                };
                let result = Store::open(data)
                    .and_then(|worker| check_options(&worker, id, Some(batch_id), false))
                    .unwrap_or_else(
                        |e| json!({"skillId":id,"status":"failed","message":format!("{e:#}")}),
                    );
                results.lock().unwrap().push((i, result));
            });
        }
    });
    crate::check_jobs::end(store, batch_id)?;
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|r| r.0);
    Ok(results.into_iter().map(|r| r.1).collect())
}
fn check_options(
    store: &Store,
    id: &str,
    batch_id: Option<&str>,
    use_cached: bool,
) -> Result<Value> {
    let batch = batch_id
        .map(|id| crate::check_jobs::batch(store, id))
        .transpose()?;
    crate::check_jobs::run(store, id, || {
        check_inner(store, id, batch.as_deref(), use_cached)
    })
}
fn check_inner(
    store: &Store,
    id: &str,
    batch: Option<&crate::check_jobs::Batch>,
    use_cached: bool,
) -> Result<Value> {
    let (item, cache) = {
        let _lock = crate::recovery::lock_wait(store)?;
        let item = skill(store, id)?;
        expire_checks(store)?;
        crate::check_jobs::cleanup(store)?;
        let cache = if use_cached {
            cached_source(store, &item).ok().flatten()
        } else {
            None
        };
        (item, cache)
    };
    if item.source.kind == "unknown" || item.source.locator.is_empty() {
        return Ok(
            json!({"skillId":id,"name":item.name,"status":"unsupported","message":"请先设置来源","checkedAt":now(),"locations":[]}),
        );
    }
    let check_id = uuid::Uuid::new_v4().to_string();
    let root = check_root(store, &check_id)?;
    let result = (|| -> Result<Value> {
        let proxy = crate::network::proxy_url(store)?;
        let checked_at = cache.as_ref().map(|(_, time)| time.clone());
        let source_path = if let Some((source, _)) = cache {
            let path = root.join("source");
            safe_files::copy_tree(&source, &path)?;
            path
        } else if let Some(batch) = batch {
            batch.stage(&json!(item.source), &proxy, &root)?
        } else {
            native::stage(&json!({"source":item.source,"networkProxy":proxy}), &root)?
        };
        let _lock = crate::recovery::lock_wait(store)?;
        let current = skill(store, id)?;
        if !same_source(&current, &item) || current.path != item.path {
            bail!("检查期间来源或库目录已改变，请重新检查");
        }
        compare_source(
            store,
            &current,
            source_path,
            &check_id,
            checked_at.as_deref(),
        )
    })();
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let _ = safe_files::remove_tree(&root);
            let result = json!({"skillId":id,"name":item.name,"status":"failed","message":format!("{error:#}"),"checkedAt":now(),"locations":[]});
            let _lock = crate::recovery::lock_wait(store)?;
            if let Some(current) = store
                .get_skill(id)?
                .filter(|s| same_source(s, &item) && s.path == item.path)
            {
                save_status(store, &current, &result)?;
            }
            Ok(result)
        }
    }
}
fn compare_source(
    store: &Store,
    item: &Skill,
    source_path: PathBuf,
    check_id: &str,
    checked_at: Option<&str>,
) -> Result<Value> {
    let id = &item.id;
    let source_digest = safe_files::digest(&source_path)?;
    let upstream = file_bytes(&source_path)?;
    let latest_content = content_digest_of(&upstream);
    let deployments = linked(store, id)?;
    let mut grouped: BTreeMap<String, (String, String, Vec<String>, Option<String>)> =
        BTreeMap::new();
    grouped.insert(
        protection::path_key(Path::new(&item.path)),
        (
            "library".into(),
            item.path.clone(),
            Vec::new(),
            item.source_digest.clone(),
        ),
    );
    for deployment in &deployments {
        let value = grouped
            .entry(protection::path_key(Path::new(&deployment.path)))
            .or_insert_with(|| {
                (
                    deployment.id.clone(),
                    deployment.path.clone(),
                    Vec::new(),
                    deployment.baseline_digest.clone(),
                )
            });
        if !value.2.contains(&deployment.agent) {
            value.2.push(deployment.agent.clone());
        }
    }
    let mut locations = Vec::new();
    let mut descriptions = Vec::new();
    for (_, (location_id, path, mut agents, baseline)) in grouped {
        agents.sort();
        let before_digest = safe_files::digest(Path::new(&path))?;
        let current_files = file_bytes(Path::new(&path))?;
        let current_content = if before_digest == "absent" {
            "absent".into()
        } else {
            content_digest_of(&current_files)
        };
        let changes = differences(&current_files, &upstream);
        let label = if location_id == "library" {
            "Skill 库".into()
        } else {
            agents.join(" / ")
        };
        let status = compare_status(&current_content, &latest_content, baseline.as_deref());
        descriptions.push(json!({"id":location_id,"path":path,"label":label,"agents":agents,"exists":Path::new(&path).join("SKILL.md").is_file(),"differences":changes,"status":status}));
        locations.push(CheckedLocation {
            id: location_id,
            path,
            label,
            agents,
            before_digest,
            differences: changes,
        });
    }
    let status = ["diverged", "modified", "different", "available"]
        .into_iter()
        .find(|s| descriptions.iter().any(|l| l["status"] == *s))
        .unwrap_or("current");
    let checked = CheckedUpdate {
        id: check_id.to_owned(),
        skill_id: id.into(),
        created_at: checked_at.map(str::to_owned).unwrap_or_else(now),
        source_path,
        source_digest,
        skill_json: json!(item),
        deployments_json: json!(deployments),
        locations,
    };
    check_table(store)?;
    let transaction = store.conn.unchecked_transaction()?;
    store.conn.execute(
        "INSERT INTO skill_checks(id,payload_json) VALUES(?1,?2)",
        rusqlite::params![check_id, serde_json::to_string(&checked)?],
    )?;
    let result = json!({"skillId":id,"name":item.name,"status":status,"message":match status {"current"=>"所有位置与来源一致","available"=>"有更新可用","modified"=>"本地内容有修改，可在更新窗口比较","diverged"=>"来源与本地均有变化，请先比较",_=>"本地内容与来源不同，可在更新窗口比较"},"checkId":check_id,"checkedAt":checked.created_at,"locations":descriptions});
    save_status(store, item, &result)?;
    transaction.commit()?;
    Ok(result)
}
/// Reconcile known source bytes after a file mutation, including partial updates.
/// This never downloads a source and never claims unchecked locations are current.
pub(crate) fn refresh_after_change(store: &Store, force: bool) -> Result<()> {
    let _lock = crate::recovery::lock_wait(store)?;
    for saved in saved_statuses(store)? {
        let Some(item) = store.get_skill(&saved.skill.id)? else {
            continue;
        };
        if !force
            && json!(item) == json!(saved.skill)
            && json!(linked(store, &item.id)?) == json!(saved.deployments)
        {
            continue;
        }
        let Some((source, checked_at)) = cached_source(store, &item).ok().flatten() else {
            clear_status(store, &item.id)?;
            continue;
        };
        let check_id = uuid::Uuid::new_v4().to_string();
        let root = check_root(store, &check_id)?;
        let path = root.join("source");
        if let Err(_error) = (|| -> Result<()> {
            safe_files::copy_tree(&source, &path)?;
            compare_source(store, &item, path, &check_id, Some(&checked_at))?;
            Ok(())
        })() {
            let _ = safe_files::remove_tree(&root);
            clear_status(store, &item.id)?;
        }
    }
    Ok(())
}

fn expire_checks(store: &Store) -> Result<()> {
    check_table(store)?;
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
    let ids: Vec<String> = {
        let mut statement = store.conn.prepare(
            "SELECT id FROM skill_checks WHERE json_extract(payload_json,'$.createdAt') < ?1",
        )?;
        let rows = statement.query_map([cutoff], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for id in ids {
        if let Ok(root) = check_root(store, &id) {
            if safe_files::remove_tree(&root).is_ok() {
                store
                    .conn
                    .execute("DELETE FROM skill_checks WHERE id=?1", [&id])?;
            }
        }
    }
    Ok(())
}
fn safe_child(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains(['\\', ':', '\0'])
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("文件必须位于 Skill 目录内");
    }
    safe_files::safe_directory(root)?;
    let child = root.join(path);
    safe_files::safe_directory(child.parent().context("文件父目录缺失")?)?;
    if let Ok(metadata) = fs::symlink_metadata(&child) {
        if metadata.file_type().is_symlink() {
            bail!("不能读取链接文件");
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                bail!("不能读取重解析文件");
            }
        }
    }
    Ok(child)
}
fn text(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_file() || fs::metadata(path)?.len() > MAX_TEXT as u64 {
        bail!("文件超过预览大小限制或不是普通文件");
    }
    let bytes = fs::read(path)?;
    if !is_text(&bytes) {
        bail!("此文件不是可预览文本");
    }
    Ok(Some(String::from_utf8(bytes)?))
}
fn diff(store: &Store, args: &Value) -> Result<Value> {
    let checked = checked_update(store, required(args, "checkId")?)?;
    let location = checked
        .locations
        .iter()
        .find(|l| Some(l.id.as_str()) == args["locationId"].as_str())
        .context("检查位置不存在")?;
    let relative = required(args, "path")?;
    if !location.differences.iter().any(|d| d["path"] == relative) {
        bail!("该文件不在本次变化列表中");
    }
    Ok(
        json!({"path":relative,"oldText":text(&safe_child(Path::new(&location.path),relative)?)?,"newText":text(&safe_child(&checked.source_path,relative)?)?}),
    )
}
fn content_root(store: &Store, args: &Value) -> Result<PathBuf> {
    let item = skill(store, required(args, "skillId")?)?;
    if let Some(id) = args["deploymentId"].as_str() {
        let deployment = store.get_deployment(id)?.context("安装位置不存在")?;
        if deployment.skill_id.as_deref() != Some(&item.id) {
            bail!("安装位置不属于此 Skill");
        }
        Ok(PathBuf::from(deployment.path))
    } else {
        Ok(PathBuf::from(item.path))
    }
}
fn list_files(store: &Store, args: &Value) -> Result<Value> {
    let root = content_root(store, args)?;
    let files=file_bytes(&root)?.into_iter().map(|(path,bytes)|json!({"path":path,"size":bytes.len(),"previewable":bytes.len()<=MAX_TEXT&&is_text(&bytes)})).collect::<Vec<_>>();
    Ok(json!({"files":files}))
}
fn read_file(store: &Store, args: &Value) -> Result<Value> {
    let root = content_root(store, args)?;
    let relative = args["path"].as_str().unwrap_or("SKILL.md");
    Ok(
        json!({"path":relative,"content":text(&safe_child(&root,relative)?)?.context("文件不存在")?}),
    )
}
