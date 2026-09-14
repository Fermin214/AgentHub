//! Readable library directories with stable record IDs and reserved names.
use crate::{model::Skill, protection, safe_files, store::Store};
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(crate) fn schema(store: &Store) -> Result<()> {
    store.conn.execute_batch("CREATE TABLE IF NOT EXISTS skill_library_paths(skill_id TEXT PRIMARY KEY,directory_name TEXT NOT NULL UNIQUE COLLATE NOCASE);")?;
    Ok(())
}

fn root(store: &Store) -> PathBuf {
    store.data_dir().join("library/skills")
}
fn child(store: &Store, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.contains(['/', '\\', ':', '\0'])
        || !Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        bail!("Skill 库目录名无效");
    }
    let path = root(store).join(name);
    safe_files::safe_directory(&path)?;
    Ok(path)
}
fn reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_uppercase();
    ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|n| {
                n.chars().count() == 1 && n.chars().all(|c| c.is_ascii_digit() || "¹²³".contains(c))
            })
        })
}
pub(crate) fn directory_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .take(60)
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches([' ', '.']).to_string();
    if cleaned.is_empty() {
        "skill".into()
    } else if reserved(&cleaned) {
        format!("skill-{cleaned}")
    } else {
        cleaned
    }
}
fn registered(store: &Store, id: &str) -> Result<Option<String>> {
    schema(store)?;
    Ok(store
        .conn
        .query_row(
            "SELECT directory_name FROM skill_library_paths WHERE skill_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?)
}
/// Call while holding the Skill write lock. Reservations also protect deleted
/// Skills that remain restorable from a backup.
pub(crate) fn allocate(store: &Store, id: &str, name: &str) -> Result<PathBuf> {
    let id = uuid::Uuid::parse_str(id)?.to_string();
    if let Some(name) = registered(store, &id)? {
        return child(store, &name);
    }
    let base = directory_name(name);
    for name in [
        base.clone(),
        format!("{base}--{}", &id[..8]),
        format!("{base}--{id}"),
    ] {
        let path = child(store, &name)?;
        let used:bool=store.conn.query_row("SELECT EXISTS(SELECT 1 FROM skill_library_paths WHERE directory_name=?1 COLLATE NOCASE)",[&name],|r|r.get(0))?;
        let recorded = store
            .list_skills()?
            .iter()
            .any(|s| protection::path_key(Path::new(&s.path)) == protection::path_key(&path));
        if used || recorded || fs::symlink_metadata(&path).is_ok() {
            continue;
        }
        store.conn.execute(
            "INSERT INTO skill_library_paths(skill_id,directory_name) VALUES(?1,?2)",
            params![id, name],
        )?;
        return Ok(path);
    }
    bail!("Skill 名称对应的目录已被占用，请检查 Skill 库目录")
}
pub(crate) fn validate(store: &Store, skill: &Skill) -> Result<PathBuf> {
    uuid::Uuid::parse_str(&skill.id).context("Skill 标识无效")?;
    let name = registered(store, &skill.id)?.context("Skill 库目录未登记")?;
    let expected = child(store, &name)?;
    if protection::path_key(Path::new(&skill.path)) != protection::path_key(&expected) {
        bail!("Skill 目录不属于当前库");
    }
    Ok(expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_remain_readable_and_do_not_overwrite_collisions() {
        assert_eq!(directory_name(" json-canvas "), "json-canvas");
        assert_eq!(directory_name("测试/中文:Skill?. "), "测试-中文-Skill-");
        for name in ["CON", "nul.txt", "LPT1", "COM²"] {
            assert!(directory_name(name).starts_with("skill-"));
        }
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let first = allocate(&store, &uuid::Uuid::new_v4().to_string(), "same/name").unwrap();
        fs::create_dir_all(&first).unwrap();
        fs::write(first.join("keep"), "untouched").unwrap();
        let other = allocate(&store, &uuid::Uuid::new_v4().to_string(), "same:name").unwrap();
        assert!(other
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("same-name--"));
        assert_eq!(fs::read_to_string(first.join("keep")).unwrap(), "untouched");
    }
}
