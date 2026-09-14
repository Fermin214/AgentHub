//! Fault isolation for startup maintenance. Journals remain the authority for writes.
use crate::{skill_changes, store::Store};
use anyhow::Result;
use serde_json::{json, Value};

pub(crate) fn prepare(store: &Store) -> Result<Value> {
    let attempt = crate::data_paths::prepare(store).and_then(|_| skill_changes::recover(store));
    match attempt {
        Ok(()) => Ok(json!({"status":"ready","issues":[]})),
        Err(error) => diagnostics(store, &format!("{error:#}")),
    }
}

/// Recheck under the shared write lock: startup's observation may be stale by
/// the time another request has committed an interrupted intent.
pub(crate) fn guard_journals(store: &Store) -> Result<()> {
    crate::data_paths::guard_root(store).map_err(|error| {
        crate::errors::coded("RECOVERY_REQUIRED", json!({}), format!("{error:#}"))
    })?;
    skill_changes::schema(store)?;
    let pending: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM skill_change_plans WHERE state IN ('executing','committed'))",
        [],
        |row| row.get(0),
    )?;
    if pending {
        return Err(crate::errors::coded(
            "RECOVERY_REQUIRED",
            json!({}),
            "存在未完成的 Skill 文件变更，请先完成恢复",
        ));
    }
    Ok(())
}

pub(crate) fn lock(store: &Store) -> Result<std::fs::File> {
    let lock = crate::safe_files::lock(store)?;
    guard_journals(store)?;
    Ok(lock)
}

pub(crate) fn lock_wait(store: &Store) -> Result<std::fs::File> {
    let lock = crate::safe_files::lock_wait(store)?;
    guard_journals(store)?;
    Ok(lock)
}

fn diagnostics(store: &Store, detail: &str) -> Result<Value> {
    skill_changes::schema(store)?;
    let mut statement = store.conn.prepare("SELECT id,state,payload_json FROM skill_change_plans WHERE state IN ('executing','committed') ORDER BY id")?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut issues = Vec::new();
    for row in rows {
        let (id, state, raw) = row?;
        // Read only for display. Malformed records never become executable plans.
        let record: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let paths: Vec<&str> = record["locations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|location| location["path"].as_str())
            .collect();
        issues.push(json!({"id":id,"state":state,"paths":paths,"recordValid":!record.is_null()}));
    }
    if issues.is_empty() {
        issues.push(json!({"state":"maintenance","paths":[store.data_dir()]}));
    }
    Ok(json!({"status":"restricted","code":"RECOVERY_REQUIRED","detail":detail,"issues":issues}))
}

pub(crate) fn guard(method: &str, state: &Value) -> Result<()> {
    if state["status"] != "restricted" {
        return Ok(());
    }
    // Unknown scope (including corrupt plans) must not admit a new file writer.
    // Prompt and settings data are independent of Skill file transactions.
    if matches!(
        method,
        "snapshot"
            | "prompts.save"
            | "prompts.delete"
            | "prompts.export"
            | "settings.save"
            | "backups.list"
            | "targets.list"
            | "network.get"
            | "network.save"
            | "maintenance.get"
            | "maintenance.preview"
            | "repositories.list"
            | "bookmarks.list"
            | "git.probe"
            | "links.open"
            | "skills.files"
            | "skills.read"
    ) {
        return Ok(());
    }
    Err(crate::errors::coded(
        "RECOVERY_REQUIRED",
        serde_json::json!({"issues":state["issues"]}),
        state["detail"]
            .as_str()
            .unwrap_or("Skill recovery required"),
    ))
}
