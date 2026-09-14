//! Interrupted file exchanges and committed temporary-file cleanup.
use super::{key, now, owned_dir, schema, validate_locations, Location, Plan};
use crate::{safe_files, store::Store};
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;
fn temporary_paths(plan: &Plan, index: usize, location: &Location) -> Result<(PathBuf, PathBuf)> {
    let parent = Path::new(&location.path)
        .parent()
        .context("位置没有父目录")?;
    let old = parent.join(format!(".atb-{}-{index}-old", plan.id));
    let incoming = parent.join(format!(".atb-{}-{index}-new", plan.id));
    safe_files::safe_directory(&old)?;
    safe_files::safe_directory(&incoming)?;
    Ok((old, incoming))
}
fn cleanup_committed(store: &Store, plan: &Plan) -> Result<()> {
    // Never examine or restore the live destination after COMMIT: subsequent
    // legitimate edits belong to the user. Only our known temporary roots remain.
    for (index, location) in plan.locations.iter().enumerate() {
        let (old, incoming) = temporary_paths(plan, index, location)?;
        if old.exists() && safe_files::digest(&old)? != location.before_digest {
            bail!("保留的原文件已变化，请检查 {}", old.display());
        }
        validate_incoming(plan, location, &incoming)?;
    }
    for (index, location) in plan.locations.iter().enumerate() {
        let (old, incoming) = temporary_paths(plan, index, location)?;
        safe_files::remove_tree(&old)?;
        safe_files::remove_tree(&incoming)?;
    }
    safe_files::remove_tree(&owned_dir(store, "skill-change-plans", &plan.id)?)?;
    store.conn.execute(
        "UPDATE skill_change_plans SET state='done' WHERE id=?1 AND state='committed'",
        [&plan.id],
    )?;
    Ok(())
}
fn validate_incoming(plan: &Plan, location: &Location, incoming: &Path) -> Result<()> {
    if incoming.exists() {
        let written = plan
            .recovery_digests
            .get(&key(&location.path))
            .context("中断计划缺少写入摘要")?;
        if &safe_files::digest(incoming)? != written {
            bail!("中断后暂存文件已变化，请检查 {}", incoming.display());
        }
    }
    Ok(())
}
pub(super) fn finish_committed(store: &Store, plan: &Plan) -> Vec<String> {
    cleanup_committed(store, plan)
        .err()
        .map(|error| vec![format!("变更已提交；临时文件尚未清理：{error:#}")])
        .unwrap_or_default()
}
pub(super) fn mark_rolled_back(store: &Store, plan: &Plan, reason: &str) -> Result<()> {
    let count = store.conn.execute(
        "UPDATE skill_change_plans SET state='failed' WHERE id=?1 AND state='executing'",
        [&plan.id],
    )?;
    if count > 0 {
        store.record_change(&json!({"id":Uuid::new_v4().to_string(),"action":plan.action,"status":"failed","summary":format!("变更已还原：{reason}"),"createdAt":now()}))?;
        safe_files::remove_tree(&owned_dir(store, "skill-change-plans", &plan.id)?)?;
    }
    Ok(())
}
/// Recover only interrupted Skill exchanges, before scanning or serving new writes.
/// The trusted SQLite intent supplies every path and digest; no disk journal is trusted.
pub fn recover(store: &Store) -> Result<()> {
    schema(store)?;
    let count: i64 = store.conn.query_row(
        "SELECT COUNT(*) FROM skill_change_plans WHERE state IN ('executing','committed')",
        [],
        |row| row.get(0),
    )?;
    if count == 0 {
        return Ok(());
    }
    let _lock = safe_files::lock(store)?;
    crate::data_paths::guard_root(store)?;
    let rows: Vec<(String, String)> = {
        let mut statement=store.conn.prepare("SELECT payload_json,state FROM skill_change_plans WHERE state IN ('executing','committed') ORDER BY id")?;
        let values = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
    };
    for (raw, state) in rows {
        let plan: Plan = serde_json::from_str(&raw)?;
        Uuid::parse_str(&plan.id)?;
        if state == "committed" {
            cleanup_committed(store, &plan)?;
            continue;
        }
        validate_locations(store, &plan.locations, plan.skill.as_ref())?;
        // Preflight all positions so unexpected user content is never discarded.
        for (index, location) in plan.locations.iter().enumerate() {
            let (old, incoming) = temporary_paths(&plan, index, location)?;
            validate_incoming(&plan, location, &incoming)?;
            let current = safe_files::digest(Path::new(&location.path))?;
            let written = plan
                .recovery_digests
                .get(&key(&location.path))
                .context("中断计划缺少写入摘要")?;
            if old.exists() {
                if safe_files::digest(&old)? != location.before_digest
                    || current != "absent"
                        && &current != written
                        && current != location.before_digest
                {
                    bail!("中断后文件已变化；原文件保留于 {}，请先检查", old.display());
                }
            } else if current != location.before_digest
                && !(location.before_digest == "absent" && &current == written)
            {
                bail!("中断后位置已变化，不能自动恢复 {}", location.path);
            }
        }
        for (index, location) in plan.locations.iter().enumerate().rev() {
            let (old, incoming) = temporary_paths(&plan, index, location)?;
            if old.exists() {
                safe_files::remove_tree(Path::new(&location.path))?;
                fs::rename(&old, &location.path)?;
            } else if location.before_digest == "absent" {
                safe_files::remove_tree(Path::new(&location.path))?;
            }
            safe_files::remove_tree(&incoming)?;
        }
        if let Some(id) = &plan.recovery_backup_id {
            safe_files::remove_tree(&owned_dir(store, "backups", id)?)?;
        }
        mark_rolled_back(store, &plan, "检测到中断，已恢复变更前的文件")?;
    }
    Ok(())
}
