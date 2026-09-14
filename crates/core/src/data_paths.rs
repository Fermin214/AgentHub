//! Relocate only registered application-owned paths. External paths are opaque.
use crate::{library, model::Skill, protection, safe_files, skill_changes, store::Store};
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use std::path::{Component, Path, PathBuf};

const ROOT_KEY: &str = "ownedDataRoot";

fn same(a: &Path, b: &Path) -> bool {
    protection::path_key(a) == protection::path_key(b)
}

fn recorded_root(store: &Store) -> Result<Option<PathBuf>> {
    let raw: Option<String> = store
        .conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key=?1",
            [ROOT_KEY],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|raw| Ok(serde_json::from_str(&raw)?)).transpose()
}

pub(crate) fn guard_root(store: &Store) -> Result<()> {
    if !store.data_dir().as_os_str().is_empty()
        && recorded_root(store)?.is_some_and(|root| !same(&root, store.data_dir()))
    {
        bail!("数据根尚未完成迁移，请先恢复数据目录");
    }
    Ok(())
}

fn plain_absolute(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        bail!("数据根迁移记录中的路径无效");
    }
    Ok(())
}

fn library_suffix(store: &Store, skill: &Skill) -> Result<PathBuf> {
    uuid::Uuid::parse_str(&skill.id)?;
    let name: Option<String> = store
        .conn
        .query_row(
            "SELECT directory_name FROM skill_library_paths WHERE skill_id=?1",
            [&skill.id],
            |r| r.get(0),
        )
        .optional()?;
    let name = name.context("Skill 库目录未登记")?;
    if name.is_empty()
        || name.contains(['/', '\\', ':', '\0'])
        || !Path::new(&name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        bail!("Skill 库目录名无效");
    }
    Ok(Path::new("library").join("skills").join(name))
}

fn infer_root(path: &Path, suffix: &Path) -> Result<PathBuf> {
    plain_absolute(path)?;
    let mut root = path;
    for _ in suffix.components() {
        root = root.parent().context("数据根路径缺失")?;
    }
    if !same(path, &root.join(suffix)) {
        bail!("自有数据路径与登记目录不匹配：{}", path.display());
    }
    Ok(root.to_path_buf())
}

fn invalidate_table(store: &Store, table: &str) -> Result<()> {
    let exists: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?;
    if exists {
        store.conn.execute(&format!("DELETE FROM {table}"), [])?;
    }
    Ok(())
}

/// Call before file-exchange recovery. A failed relocation must also prevent
/// recovery from following paths recorded at the previous data root.
pub(crate) fn prepare(store: &Store) -> Result<()> {
    if store.data_dir().as_os_str().is_empty() {
        return Ok(());
    }
    if recorded_root(store)?.is_some_and(|root| same(&root, store.data_dir())) {
        return Ok(());
    }
    let _lock = safe_files::lock_wait(store)?;
    // Another request may have completed the move while this one was waiting.
    if recorded_root(store)?.is_some_and(|root| same(&root, store.data_dir())) {
        return Ok(());
    }
    library::schema(store)?;
    skill_changes::schema(store)?;
    let tx = store.conn.unchecked_transaction()?;
    let mut skills = store.list_skills()?;
    skills.extend(skill_changes::backup_skills(store)?);
    let backups: Vec<(String, String)> = {
        let mut statement = store.conn.prepare("SELECT id,path FROM backups")?;
        let rows = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows
    };
    let old = recorded_root(store)?.unwrap_or_else(|| store.data_dir().to_path_buf());
    plain_absolute(&old)?;
    for skill in &skills {
        let root = infer_root(Path::new(&skill.path), &library_suffix(store, skill)?)?;
        if !same(&root, &old) {
            bail!("自有数据路径与登记数据根不匹配：{}", skill.path);
        }
    }
    for (id, path) in &backups {
        uuid::Uuid::parse_str(id)?;
        let root = infer_root(Path::new(path), &Path::new("backups").join(id))?;
        if !same(&root, &old) {
            bail!("备份路径与登记数据根不匹配：{path}");
        }
    }
    if !same(&old, store.data_dir()) {
        let pending: bool = store.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM skill_change_plans WHERE state IN ('executing','committed'))",
            [], |r| r.get(0),
        )?;
        if pending {
            bail!("数据目录已移动，但仍有未完成的 Skill 文件变更。已保留原计划和文件；请将数据目录移回 {}，完成恢复后再移动。", old.display());
        }
        // Verify the moved backup bytes before changing any of their metadata.
        // A missing or damaged backup remains untouched and enters restricted mode.
        for (id, _) in &backups {
            let raw: String = store
                .conn
                .query_row(
                    "SELECT payload_json FROM skill_backup_manifests WHERE backup_id=?1",
                    [id],
                    |r| r.get(0),
                )
                .context("数据根迁移发现缺少清单的备份")?;
            let manifest: serde_json::Value = serde_json::from_str(&raw)?;
            if manifest["id"].as_str() != Some(id.as_str())
                || manifest["rootDigest"].as_str()
                    != Some(
                        safe_files::digest(&store.data_dir().join("backups").join(id))?.as_str(),
                    )
            {
                bail!("数据根迁移发现备份缺失或内容变化：{id}");
            }
        }
        let mut moved_ids = std::collections::BTreeSet::new();
        for skill in skills {
            if !moved_ids.insert(skill.id.clone()) {
                continue;
            }
            let suffix = library_suffix(store, &skill)?;
            let from = old.join(&suffix);
            let to = store.data_dir().join(&suffix);
            safe_files::safe_directory(&to)?;
            if let Some(mut current) = store.get_skill(&skill.id)? {
                current.path = to.to_string_lossy().into();
                store.save_skill(&current)?;
            }
            // This typed helper updates only Skill records and locations whose
            // role is library; deployment/source/project paths are untouched.
            skill_changes::move_library_references(store, &skill.id, &from, &to)?;
        }
        for (id, _) in backups {
            store.conn.execute(
                "UPDATE backups SET path=?1 WHERE id=?2",
                params![
                    store.data_dir().join("backups").join(&id).to_string_lossy(),
                    id
                ],
            )?;
        }
        store.conn.execute(
            "UPDATE skill_change_plans SET state='expired' WHERE state='ready'",
            [],
        )?;
        for table in ["skill_checks", "skill_update_status", "source_inspections"] {
            invalidate_table(store, table)?;
        }
    }
    store.conn.execute(
        "INSERT OR REPLACE INTO app_settings(key,value_json) VALUES(?1,?2)",
        params![ROOT_KEY, serde_json::to_string(store.data_dir())?],
    )?;
    tx.commit()?;
    Ok(())
}
