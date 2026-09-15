//! Shared desktop domains and internal test transport.
pub mod adapters;
pub mod agents;
pub mod bookmarks;
mod check_jobs;
mod data_paths;
pub mod errors;
mod library;
pub mod maintenance;
pub mod model;
pub mod native;
pub mod network;
pub mod projects;
pub mod prompts;
pub mod protection;
mod recovery;
pub mod safe_files;
pub mod scan;
pub mod skill_changes;
pub mod skills;
mod source_control;
pub mod sources;
pub mod store;
pub mod targets;
use anyhow::{anyhow, bail, Context, Result};
use model::{Prompt, Settings};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use store::Store;
pub fn dispatch(data_dir: &Path, method: &str, args: Value) -> Result<Value> {
    let mut result = dispatch_inner(data_dir, method, args)?;
    if matches!(
        method,
        "skills.install" | "skills.remove" | "skills.update" | "skills.delete" | "backups.restore"
    ) {
        let store = Store::open(data_dir)?;
        if result["status"] == "succeeded" {
            if let Err(error) = skills::refresh_after_change(&store, method == "backups.restore") {
                result["statusRefresh"] = json!({"status":"deferred","message":error.to_string()});
            }
        }
        if maintenance::max_backups(&store)?.is_some() {
            result["retention"] = match maintenance::apply_retention(&store) {
                Ok(value) => value,
                Err(error) => json!({"status":"deferred","message":error.to_string()}),
            };
        }
    }
    Ok(result)
}
fn dispatch_inner(data_dir: &Path, method: &str, args: Value) -> Result<Value> {
    // These only observe/signal existing in-memory work, even during recovery.
    match method {
        "sources.status" => return source_control::control(data_dir, &args, false),
        "sources.cancel" => return source_control::control(data_dir, &args, true),
        _ => {}
    }
    let mut store = Store::open(data_dir)?;
    let recovery = recovery::prepare(&store)?;
    recovery::guard(method, &recovery)?;
    match method {
        "snapshot" => {
            let mut snapshot = store.snapshot()?;
            snapshot["recovery"] = recovery;
            Ok(snapshot)
        }
        "links.open" => {
            let url = required_string(&args, "url")?;
            let parsed = reqwest::Url::parse(&url)?;
            if !["https", "http"].contains(&parsed.scheme())
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                bail!("只能打开不含登录凭据的 HTTP(S) 链接");
            }
            #[cfg(windows)]
            {
                std::process::Command::new("explorer.exe")
                    .arg(parsed.as_str())
                    .spawn()
                    .context("无法打开默认浏览器")?;
                Ok(json!({"ok":true}))
            }
            #[cfg(not(windows))]
            bail!("打开链接仅支持 Windows 桌面");
        }
        "bookmarks.list" | "bookmarks.save" | "bookmarks.delete" | "bookmarks.sync" => {
            bookmarks::dispatch(&store, method, &args)
        }
        "projects.save" | "projects.delete" | "projects.archive" | "projects.status"
        | "projects.inspect" | "projects.check" | "projects.commits" | "projects.trust" => {
            projects::dispatch(&store, method, &args)
        }
        "skills.scan"
        | "skills.add"
        | "skills.import"
        | "skills.ignore"
        | "skills.metadata.save"
        | "skills.bindSource"
        | "skills.unbindSource"
        | "skills.check"
        | "skills.checkBatch.start"
        | "skills.checkBatch.finish"
        | "skills.diff"
        | "skills.files"
        | "skills.read" => skills::dispatch(&store, method, &args),
        "skills.install.preview"
        | "skills.remove.preview"
        | "skills.update.preview"
        | "skills.delete.preview"
        | "skills.install"
        | "skills.remove"
        | "skills.update"
        | "skills.delete"
        | "skills.cancel"
        | "backups.list"
        | "backups.restore" => skill_changes::dispatch(&store, method, &args),
        "sources.inspect"
        | "sources.begin"
        | "sources.release"
        | "repositories.list"
        | "repositories.save"
        | "repositories.remove" => sources::dispatch(&store, method, &args),
        "targets.list" | "targets.save" => targets::dispatch(&store, method, &args),
        "network.get" | "network.save" => network::dispatch(&store, method, &args),
        "maintenance.get" | "maintenance.save" | "maintenance.preview" => {
            maintenance::dispatch(&store, method, &args)
        }
        "prompts.save" => {
            let value = args.get("prompt").cloned().unwrap_or_else(|| args.clone());
            let prompt: Prompt = serde_json::from_value(value).context("invalid prompt payload")?;
            Ok(serde_json::to_value(store.save_prompt(&prompt)?)?)
        }
        "prompts.delete" => {
            let id = required_string(&args, "id")?;
            store.delete_prompt(&id)?;
            Ok(json!({ "ok": true }))
        }
        "prompts.export" => {
            let format = required_string(&args, "format")?;
            let (content, filename) = store.export_prompts(&format)?;
            Ok(json!({ "content": content, "filename": filename }))
        }

        "settings.save" => {
            let value = args
                .get("settings")
                .cloned()
                .unwrap_or_else(|| args.clone());
            let settings: Settings =
                serde_json::from_value(value).context("invalid settings payload")?;
            Ok(serde_json::to_value(store.save_settings(&settings)?)?)
        }

        "git.probe" => Ok(adapters::git_status()),
        "updates.check" => {
            let work = (|| -> Result<Value> {
                let mut updates = skills::check_all(&store)?;
                updates.extend(projects::check_all(&store)?);
                Ok(json!({"updates":updates}))
            })();
            // The attempt time is recorded either way so a failing check is
            // throttled by the configured interval instead of retrying at once.
            let _ = maintenance::record_attempt(&store);
            work
        }
        _ => bail!("unknown core method {}", method),
    }
}
fn required_string(args: &Value, key: &str) -> Result<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{} is required", key))
}
pub fn installed_root(executable: &Path) -> Option<PathBuf> {
    let root = executable.parent()?;
    root.join("data/runtime/agenthub-layout.json")
        .is_file()
        .then(|| root.to_path_buf())
}
pub fn default_data_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("AGENTHUB_DATA_DIR") {
        return Some(PathBuf::from(path));
    }
    if let Some(root) = std::env::current_exe()
        .ok()
        .and_then(|exe| installed_root(&exe))
    {
        return Some(root.join("data"));
    }
    if cfg!(debug_assertions) {
        return Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../output/development/data"),
        );
    }
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| {
                    PathBuf::from(home).join(if cfg!(windows) {
                        "AppData/Local"
                    } else {
                        ".local/share"
                    })
                })
        })?;
    Some(local.join("AgentHub").join("data"))
}
