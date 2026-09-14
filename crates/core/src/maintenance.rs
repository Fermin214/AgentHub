//! Optional in-app update-check preferences and backup retention. No scheduler or updater is launched here.
use crate::store::Store;
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

fn ensure(store: &Store) -> Result<()> {
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS maintenance_preferences (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            automatic_checks INTEGER NOT NULL DEFAULT 0,
            interval_hours INTEGER NOT NULL DEFAULT 24,
            last_attempt_at TEXT
        );
        INSERT OR IGNORE INTO maintenance_preferences(singleton) VALUES(1);
        CREATE TABLE IF NOT EXISTS maintenance_options(key TEXT PRIMARY KEY,value_json TEXT NOT NULL);",
    )?;
    Ok(())
}

fn settings(store: &Store) -> Result<Value> {
    ensure(store)?;
    let (enabled, interval, attempted): (bool, i64, Option<String>) =
        store.conn.query_row(
            "SELECT automatic_checks,interval_hours,last_attempt_at FROM maintenance_preferences WHERE singleton=1",
            [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))
        )?;
    let mut result = json!({"automaticChecks": enabled, "intervalHours": interval,"retainUpdateBackup":retain_update_backup(store)?,"maxBackups":max_backups(store)?});
    if let Some(value) = attempted {
        result["lastAttemptAt"] = json!(value);
    }
    let retention: Option<String> = store
        .conn
        .query_row(
            "SELECT value_json FROM maintenance_options WHERE key='lastRetention'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(raw) = retention {
        result["lastRetention"] = serde_json::from_str(&raw)?;
    }
    Ok(result)
}

pub fn dispatch(store: &Store, method: &str, args: &Value) -> Result<Value> {
    ensure(store)?;
    match method {
        "maintenance.get" => settings(store),
        "maintenance.preview" => retention_preview(store, parse_max(&args["maxBackups"])?),
        "maintenance.save" => {
            let limit = args.get("maxBackups").map(parse_max).transpose()?;
            let _lock = if limit.is_some() {
                Some(retention_lock(store)?)
            } else {
                None
            };
            let previous = settings(store)?;
            let enabled = args
                .get("automaticChecks")
                .unwrap_or(&previous["automaticChecks"])
                .as_bool()
                .context("automaticChecks must be a boolean")?;
            let interval = args
                .get("intervalHours")
                .unwrap_or(&previous["intervalHours"])
                .as_i64()
                .context("intervalHours must be an integer")?;
            let retain = args
                .get("retainUpdateBackup")
                .map(|v| v.as_bool().context("retainUpdateBackup must be a boolean"))
                .transpose()?;
            if !(1..=168).contains(&interval) {
                bail!("intervalHours must be between 1 and 168");
            }
            let tx = store.conn.unchecked_transaction()?;
            if let Some(retain) = retain {
                tx.execute("INSERT OR REPLACE INTO maintenance_options(key,value_json) VALUES('retainUpdateBackup',?1)",[serde_json::to_string(&retain)?])?;
            }
            if let Some(limit) = limit {
                tx.execute("INSERT OR REPLACE INTO maintenance_options(key,value_json) VALUES('maxBackups',?1)",[serde_json::to_string(&limit)?])?;
            }
            tx.execute("UPDATE maintenance_preferences SET automatic_checks=?1,interval_hours=?2 WHERE singleton=1", params![enabled,interval])?;
            tx.commit()?;
            let mut result = settings(store)?;
            if limit.is_some() {
                result["retention"] = prune_locked(store)?;
            }
            Ok(result)
        }
        _ => bail!("Unknown maintenance method"),
    }
}
fn parse_max(value: &Value) -> Result<Option<usize>> {
    if value.is_null() {
        return Ok(None);
    }
    let count = value
        .as_u64()
        .filter(|n| *n > 0)
        .context("备份数量必须是正整数，或选择不限")?;
    Ok(Some(usize::try_from(count).context("备份数量过大")?))
}
pub(crate) fn max_backups(store: &Store) -> Result<Option<usize>> {
    ensure(store)?;
    let raw: Option<String> = store
        .conn
        .query_row(
            "SELECT value_json FROM maintenance_options WHERE key='maxBackups'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| parse_max(&serde_json::from_str::<Value>(&s)?))
        .transpose()
        .map(Option::flatten)
}
fn retention_lock(store: &Store) -> Result<std::fs::File> {
    crate::recovery::lock(store)
}
fn excess(store: &Store, limit: Option<usize>) -> Result<Vec<(String, String)>> {
    crate::skill_changes::ensure_schema(&store.conn)?;
    let Some(limit) = limit else {
        return Ok(Vec::new());
    };
    let mut query = store
        .conn
        .prepare("SELECT id,path FROM backups ORDER BY created_at DESC,id DESC")?;
    let rows = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().skip(limit).rev().collect())
}
fn retention_preview(store: &Store, limit: Option<usize>) -> Result<Value> {
    let rows = excess(store, limit)?;
    Ok(json!({"maxBackups":limit,"pruneCount":rows.len(),"protectedCount":0}))
}
fn prune_locked(store: &Store) -> Result<Value> {
    let rows = excess(store, max_backups(store)?)?;
    let mut removed = Vec::new();
    let mut failures = Vec::new();
    for (id, path) in rows {
        let work = (|| -> Result<()> {
            let valid_id = uuid::Uuid::parse_str(&id).is_ok();
            if !valid_id {
                bail!("备份标识无效，未删除");
            }
            let root = store.data_dir().join("backups").join(&id);
            crate::safe_files::safe_directory(&root)?;
            let actual = crate::protection::path_key(std::path::Path::new(&path));
            if actual != crate::protection::path_key(&root)
                && actual != crate::protection::path_key(&root.join("payload"))
            {
                bail!("备份位置与记录不符，未删除");
            }
            if root.exists() {
                crate::native::inspect_tree(&root)?;
                std::fs::remove_dir_all(&root)?;
            }
            let tx = store.conn.unchecked_transaction()?;
            tx.execute(
                "DELETE FROM skill_backup_manifests WHERE backup_id=?1",
                [&id],
            )?;
            tx.execute("DELETE FROM backups WHERE id=?1", [&id])?;
            tx.commit()?;
            Ok(())
        })();
        match work {
            Ok(()) => removed.push(id),
            Err(error) => failures.push(json!({"id":id,"message":error.to_string()})),
        }
    }
    let result = json!({"status":if failures.is_empty(){"succeeded"}else{"partial"},"removedCount":removed.len(),"removedIds":removed,"failures":failures,"checkedAt":chrono::Utc::now().to_rfc3339()});
    store.conn.execute(
        "INSERT OR REPLACE INTO maintenance_options(key,value_json) VALUES('lastRetention',?1)",
        [serde_json::to_string(&result)?],
    )?;
    Ok(result)
}
pub(crate) fn apply_retention(store: &Store) -> Result<Value> {
    if max_backups(store)?.is_none() {
        return Ok(json!({"status":"succeeded","removedCount":0,"failures":[]}));
    }
    let _lock = retention_lock(store)?;
    prune_locked(store)
}

pub(crate) fn retain_update_backup(store: &Store) -> Result<bool> {
    use rusqlite::OptionalExtension;
    ensure(store)?;
    let value: Option<String> = store
        .conn
        .query_row(
            "SELECT value_json FROM maintenance_options WHERE key='retainUpdateBackup'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(value
        .map(|v| serde_json::from_str(&v))
        .transpose()?
        .unwrap_or(true))
}

/// Record that an update check ran. The timestamp is the only check history the
/// product keeps: the app uses it to throttle automatic checks.
pub fn record_attempt(store: &Store) -> Result<()> {
    ensure(store)?;
    store.conn.execute(
        "UPDATE maintenance_preferences SET last_attempt_at=?1 WHERE singleton=1",
        [chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}
