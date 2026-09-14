//! Configurable Skill destinations. These settings never launch an Agent.
use crate::{protection, store::Store};
use anyhow::{bail, Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub name: String,
    pub global_path: String,
    pub project_path: String,
    pub enabled: bool,
}
fn ensure(store: &Store) -> Result<()> {
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS agent_targets(id TEXT PRIMARY KEY,payload_json TEXT NOT NULL);",
    )?;
    Ok(())
}
fn builtin(id: &str) -> bool {
    crate::agents::known(id)
}
/// Only destinations explicitly saved by the user, without inferred home defaults.
pub fn configured(store: &Store) -> Result<Vec<Target>> {
    ensure(store)?;
    let mut statement = store
        .conn
        .prepare("SELECT payload_json FROM agent_targets ORDER BY id")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let targets: Result<Vec<Target>> = rows
        .map(|row| Ok(serde_json::from_str::<Target>(&row?)?))
        .collect();
    Ok(targets?
        .into_iter()
        .filter(|t| builtin(&t.id))
        .collect())
}
/// The fixed Agent table with saved paths applied on top of home defaults.
pub fn list(store: &Store) -> Result<Vec<Target>> {
    ensure(store)?;
    let settings = store.get_settings()?;
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    let mut result = Vec::new();
    for definition in crate::agents::DEFINITIONS {
        let (id, name, project) = (definition.id, definition.name, definition.project);
        let default_global = settings
            .scan_roots
            .iter()
            .find(|r| {
                r.agent == id
                    && r.scope == "global"
                    && Path::new(&r.path)
                        .file_name()
                        .is_some_and(|n| n == "skills")
            })
            .map(|r| r.path.clone())
            .or_else(|| {
                user.as_ref().map(|p| {
                    crate::agents::global_path(definition, p)
                        .to_string_lossy()
                        .into_owned()
                })
            })
            .unwrap_or_default();
        result.push(Target {
            id: id.into(),
            name: name.into(),
            global_path: default_global,
            project_path: project.into(),
            enabled: true,
        });
    }
    for target in configured(store)? {
        if let Some(existing) = result.iter_mut().find(|t| t.id == target.id) {
            *existing = target;
        }
    }
    result.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(result)
}
pub(crate) fn plain_path(path: &Path) -> Result<()> {
    let mut current = Some(path);
    while let Some(p) = current {
        match fs::symlink_metadata(p) {
            Ok(meta) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        bail!("目录经过链接或重解析点，请选择实际目录");
                    }
                }
                if meta.file_type().is_symlink() {
                    bail!("目录经过链接，请选择实际目录");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        current = p.parent();
    }
    Ok(())
}
fn relative_path(value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Ok(());
    }
    let path = Path::new(value);
    if path.is_absolute()
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
        || value.contains(['\0', ':'])
    {
        bail!("项目 Skill 目录必须是项目内的相对目录");
    }
    Ok(())
}
fn validate(target: &Target) -> Result<()> {
    if target.name.trim().is_empty() || target.name.len() > 100 {
        bail!("请填写 1 到 100 个字符的 Agent 名称");
    }
    if !builtin(&target.id) {
        bail!("仅支持内置 Agent");
    }
    let path = Path::new(&target.global_path);
    if !path.is_absolute()
        || path.parent().is_none()
        || path.file_name().is_none()
        || target.global_path.contains('\0')
    {
        bail!("请选择完整的全局 Skill 目录，不能使用磁盘根目录");
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!("全局目录不能包含上级路径");
    }
    if path.exists() && !path.is_dir() {
        bail!("全局 Skill 路径必须是目录");
    }
    // Saving a Skill root only stores configuration. A standard user root may
    // contain protected system Skills; validate a potential child rather than
    // treating this as a request to replace the whole root.
    if path.file_name().is_some_and(|name| name == "skills") && !protection::is_host_managed(path) {
        protection::ensure_mutable(&path.join("atb-target-validation"))?;
    } else {
        protection::ensure_mutable(path)?;
    }
    plain_path(path)?;
    relative_path(&target.project_path)?;
    Ok(())
}
pub fn validate_skill_target(
    store: &Store,
    agent: &str,
    scope: &str,
    project: Option<&Path>,
    target: &Path,
) -> Result<()> {
    let config = list(store)?
        .into_iter()
        .find(|t| t.id == agent)
        .context("Agent 目标尚未配置")?;
    if !config.enabled {
        bail!("此 Agent 已停止接收新的 Skill 安装");
    }
    let root = match scope {
        "global" => PathBuf::from(&config.global_path),
        "project" => {
            if config.project_path.is_empty() {
                bail!("此 Agent 未配置项目级 Skill 目录");
            }
            let project = project.context("项目安装需要项目目录")?;
            if !project.is_absolute() || !project.is_dir() {
                bail!("请选择实际存在的项目目录");
            }
            project.join(&config.project_path)
        }
        _ => bail!("Agent 安装仅支持全局或项目范围"),
    };
    if !target.is_absolute() {
        bail!("安装目标必须是完整目录路径");
    }
    plain_path(target)?;
    protection::ensure_mutable(target)?;
    let root_key = protection::path_key(&root);
    let target_key = protection::path_key(target);
    if !target_key.starts_with(&(root_key.trim_end_matches('/').to_string() + "/")) {
        bail!("安装位置必须在所选 Agent 的 Skill 目录内");
    }
    if target
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        bail!("安装位置不能包含上级路径");
    }
    Ok(())
}
pub fn dispatch(store: &Store, method: &str, args: &Value) -> Result<Value> {
    ensure(store)?;
    match method {
        "targets.list" => {
            let config = crate::adapters::ExecConfig::from_settings(&serde_json::to_value(
                store.get_settings()?,
            )?);
            let targets = list(store)?
                .into_iter()
                .map(|target| {
                    let mut value = serde_json::to_value(&target)?;
                    value["available"] = json!(crate::agents::available(&config, &target.id));
                    Ok(value)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({"targets":targets}))
        }
        "targets.save" => {
            let mut value = args.get("target").cloned().context("请提供 Agent 配置")?;
            let id = value["id"]
                .as_str()
                .filter(|id| builtin(id))
                .context("请选择内置 Agent")?
                .to_string();
            value["id"] = json!(id);
            if value.get("enabled").is_none() {
                value["enabled"] = json!(true);
            }
            if value.get("projectPath").is_none() {
                value["projectPath"] = json!("");
            }
            let target: Target = serde_json::from_value(value).context("Agent 配置字段不完整")?;
            validate(&target)?;
            store.conn.execute("INSERT INTO agent_targets(id,payload_json) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload_json=excluded.payload_json",params![target.id,serde_json::to_string(&target)?])?;
            Ok(json!({"target":target,"targets":list(store)?}))
        }
        _ => bail!("unknown target method"),
    }
}
