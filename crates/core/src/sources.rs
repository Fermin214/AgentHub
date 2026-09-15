//! Inspected Skill sources and saved repository sources.
use crate::{model::Source, store::Store};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

pub fn dispatch(store: &Store, method: &str, args: &Value) -> Result<Value> {
    ensure(store)?;
    match method {
        "repositories.list" | "repositories.save" | "repositories.remove" => {
            repositories(store, method, args)
        }
        "sources.inspect" => inspect(store, args),
        "sources.begin" => crate::source_control::begin(store.data_dir()),
        "sources.release" => {
            remove_inspection(
                store.data_dir(),
                args["inspectionId"].as_str().context("缺少来源检查标识")?,
            )?;
            store.conn.execute(
                "DELETE FROM source_inspections WHERE id=?1",
                [args["inspectionId"].as_str().unwrap()],
            )?;
            Ok(json!({"ok":true}))
        }
        _ => bail!("unknown source method"),
    }
}

// Saved repository sources contain no live checkout. Inspecting refreshes the
// candidate list; forgetting a source never removes installed Skills or files.
fn repositories(store: &Store, method: &str, args: &Value) -> Result<Value> {
    store.conn.execute_batch("CREATE TABLE IF NOT EXISTS repositories(id TEXT PRIMARY KEY,source_json TEXT NOT NULL,updated_at TEXT NOT NULL)")?;
    store
        .conn
        .execute_batch("CREATE TABLE IF NOT EXISTS hidden_repositories(id TEXT PRIMARY KEY)")?;
    if method == "repositories.save" {
        let manifest = inspected(
            store,
            args["inspectionId"].as_str().context("请先检查仓库")?,
        )?;
        let mut source = source_identity(&serde_json::from_value(manifest["source"].clone())?)?;
        if source.kind != "git" {
            bail!("仓库列表只保存 Git 来源");
        }
        source.subpath = None;
        let id = format!("{:x}", Sha256::digest(serde_json::to_vec(&source)?));
        store
            .conn
            .execute("DELETE FROM hidden_repositories WHERE id=?1", [&id])?;
        store.conn.execute("INSERT INTO repositories(id,source_json,updated_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at",rusqlite::params![id,serde_json::to_string(&source)?,chrono::Utc::now().to_rfc3339()])?;
    } else if method == "repositories.remove" {
        let id = args["id"].as_str().context("缺少仓库标识")?;
        store.conn.execute(
            "INSERT OR IGNORE INTO hidden_repositories(id) VALUES(?1)",
            [id],
        )?;
        store
            .conn
            .execute("DELETE FROM repositories WHERE id=?1", [id])?;
    }
    let mut statement = store.conn.prepare(
        "SELECT id,source_json,updated_at FROM repositories ORDER BY updated_at DESC,id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (id, source, updated_at) = row?;
        items.push(json!({"id":id,"source":serde_json::from_str::<Source>(&source)?,"updatedAt":updated_at}));
    }
    for skill in store.list_skills()? {
        if skill.source.kind != "git" {
            continue;
        }
        let mut source = source_identity(&skill.source)?;
        source.subpath = None;
        let id = format!("{:x}", Sha256::digest(serde_json::to_vec(&source)?));
        let hidden: bool = store.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM hidden_repositories WHERE id=?1)",
            [&id],
            |r| r.get(0),
        )?;
        if !hidden && !items.iter().any(|item| item["id"] == id) {
            items
                .push(json!({"id":id,"source":source,"derived":true,"updatedAt":skill.updated_at}));
        }
    }
    Ok(json!({"repositories":items}))
}

pub(crate) fn inspection_root(data: &Path, id: &str) -> Result<std::path::PathBuf> {
    let id = uuid::Uuid::parse_str(id).context("来源检查标识无效，请重新检查来源")?;
    Ok(data.join("source-inspections").join(id.to_string()))
}
/// Return only a validated, unexpired inspection created by this data directory.
pub(crate) fn inspected(store: &Store, id: &str) -> Result<Value> {
    let root = inspection_root(store.data_dir(), id)?;
    crate::safe_files::safe_directory(&root)?;
    ensure(store)?;
    let raw: String = store
        .conn
        .query_row(
            "SELECT payload_json FROM source_inspections WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .context("来源检查已过期，请重新检查")?;
    if raw.len() > 1024 * 1024 {
        bail!("来源检查记录无效");
    }
    let value: Value = serde_json::from_str(&raw)?;
    let age =
        chrono::Utc::now().timestamp() - value["createdAt"].as_i64().context("来源检查时间无效")?;
    if !(0..86400).contains(&age) {
        bail!("来源检查已过期，请重新检查");
    }
    if value["digest"].as_str() != Some(source_digest(&root.join("source"))?.as_str()) {
        bail!("来源快照已变化，请重新检查");
    }
    Ok(value)
}
fn source_digest(root: &Path) -> Result<String> {
    crate::safe_files::digest(root)
}
fn remove_inspection(data: &Path, id: &str) -> Result<()> {
    let root = inspection_root(data, id)?;
    if root.exists() {
        crate::safe_files::remove_tree(&root)?;
    }
    Ok(())
}
fn inspect(store: &Store, args: &Value) -> Result<Value> {
    let control = crate::source_control::Guard::enter(store.data_dir(), args)?;
    crate::source_control::checkpoint()?;
    let data = store.data_dir();
    let cache_root = data.join("source-inspections");
    if let Ok(entries) = fs::read_dir(&cache_root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if uuid::Uuid::parse_str(&name).is_ok()
                && entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.elapsed().ok())
                    .is_some_and(|age| age > Duration::from_secs(86400))
            {
                // An incomplete or expired inspection is disposable; never traverse a link.
                if remove_inspection(data, &name).is_ok() {
                    store
                        .conn
                        .execute("DELETE FROM source_inspections WHERE id=?1", [&name])?;
                }
            }
        }
    }
    let source = args.get("source").context("请提供来源地址")?;
    let normalized = crate::native::normalize_source(source)?;
    let parsed: Source = serde_json::from_value(normalized.clone())?;
    crate::sources::validate_source(&parsed)?;
    let id = uuid::Uuid::new_v4().to_string();
    let root = inspection_root(data, &id)?;
    let proxy = crate::network::proxy_url(store)?;
    let result = (|| -> Result<Value> {
        let mut result =
            crate::native::inspect_source_with_proxy(&normalized, &root, Some(&proxy))?;
        crate::source_control::stage("fingerprinting", 0)?;
        let manifest = json!({"source":result["source"],"candidates":result["candidates"],"digest":source_digest(&root.join("source"))?,"createdAt":chrono::Utc::now().timestamp()});
        annotate_library(store, &mut result)?;
        save_inspection(store, &id, &manifest)?;
        result["inspectionId"] = json!(id);
        result["cached"] = json!(false);
        Ok(result)
    })();
    let completion = control.seal();
    control.detach(); // Cleanup must not itself be interrupted by cancellation.
    let result = result.and_then(|value| completion.map(|_| value));
    if result.is_err() {
        if let Err(cleanup) = remove_inspection(data, &id) {
            return Err(crate::errors::coded(
                "SOURCE_CLEANUP_FAILED",
                json!({}),
                format!("{}; cleanup failed: {cleanup:#}", result.unwrap_err()),
            ));
        }
        store
            .conn
            .execute("DELETE FROM source_inspections WHERE id=?1", [&id])?;
    }
    result
}

pub fn validate_source(source: &Source) -> Result<()> {
    if source.locator.len() > 8192 {
        bail!("来源地址过长");
    }
    if !matches!(
        source.kind.as_str(),
        "git" | "zip" | "https-zip" | "local" | "unknown"
    ) {
        bail!("来源类型不受支持");
    }
    if source.locator.contains("://") {
        let url = reqwest::Url::parse(&source.locator)?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("来源地址不能包含登录信息、查询参数或片段");
        }
    }
    Ok(())
}

/// Compare source identities without fetching content. A checkout revision and
/// a Skill subdirectory are part of the identity, even within one repository.
pub fn source_identity(source: &Source) -> Result<Source> {
    validate_source(source)?;
    let mut normalized = source.clone();
    normalized.locator = normalized.locator.trim().to_string();
    normalized.subpath = normalized.subpath.filter(|subpath| !subpath.is_empty());
    if normalized.kind == "https-zip" {
        normalized.kind = "zip".into();
    }
    if normalized.kind == "local" && Path::new(&normalized.locator).is_absolute() {
        normalized.locator = crate::protection::path_key(Path::new(&normalized.locator));
    } else if normalized.kind == "git" {
        let parts: Vec<_> = normalized.locator.split('/').collect();
        if parts.len() == 2
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            })
        {
            normalized.locator = format!("https://github.com/{}", normalized.locator);
        }
        if let Some(mut url) = reqwest::Url::parse(&normalized.locator)
            .ok()
            .filter(|url| matches!(url.scheme(), "https" | "http"))
        {
            let mut path = url
                .path()
                .trim_end_matches('/')
                .trim_end_matches(".git")
                .to_string();
            if url.host_str() == Some("github.com") {
                path = path.to_lowercase();
            }
            url.set_path(&path);
            normalized.locator = url.to_string();
        } else {
            normalized.locator = normalized.locator.trim_end_matches('/').to_string();
        }
    }
    Ok(normalized)
}
/// One candidate's own source within an inspected source. A Skill subdirectory is
/// part of the identity, so two Skills from one repository stay distinguishable.
pub(crate) fn candidate_source(source: &Source, subpath: &str) -> Result<Source> {
    let mut source = source.clone();
    source.subpath = if subpath.is_empty() {
        None
    } else {
        Some(subpath.to_string())
    };
    let normalized = crate::native::normalize_source(&json!(source))?;
    Ok(serde_json::from_value(normalized)?)
}
/// Report which inspected candidates are already saved in the Skill library.
/// Library membership says nothing about Agent install locations; the caller
/// reads those from the recorded locations separately.
fn annotate_library(store: &Store, result: &mut Value) -> Result<()> {
    let source: Source = serde_json::from_value(result["source"].clone())?;
    let library: Vec<(String, Source)> = store
        .list_skills()?
        .into_iter()
        .filter_map(|item| {
            source_identity(&item.source)
                .ok()
                .map(|identity| (item.id, identity))
        })
        .collect();
    for candidate in result["candidates"].as_array_mut().into_iter().flatten() {
        let Ok(identity) = candidate_source(&source, candidate["subpath"].as_str().unwrap_or(""))
            .and_then(|candidate| source_identity(&candidate))
        else {
            continue;
        };
        if let Some((id, _)) = library.iter().find(|(_, saved)| *saved == identity) {
            candidate["skillId"] = json!(id);
        }
    }
    Ok(())
}
/// A selected source is always resolved from a bounded, previously inspected snapshot.
pub(crate) fn selected(
    store: &Store,
    id: &str,
    subpath: &str,
) -> Result<(Source, std::path::PathBuf, Value)> {
    let manifest = inspected(store, id)?;
    let candidate = manifest["candidates"]
        .as_array()
        .context("来源内容无效")?
        .iter()
        .find(|item| item["subpath"].as_str() == Some(subpath))
        .context("请选择已检查的 Skill")?
        .clone();
    let source = candidate_source(
        &serde_json::from_value(manifest["source"].clone())?,
        subpath,
    )?;
    let path = inspection_root(store.data_dir(), id)?
        .join("source")
        .join(subpath);
    crate::safe_files::safe_directory(&path)?;
    if !path.join("SKILL.md").is_file() {
        bail!("Skill 内容已失效");
    }
    Ok((source, path, candidate))
}

fn ensure(store: &Store) -> Result<()> {
    store.conn.execute_batch("CREATE TABLE IF NOT EXISTS source_inspections(id TEXT PRIMARY KEY,payload_json TEXT NOT NULL)")?;
    Ok(())
}
fn save_inspection(store: &Store, id: &str, value: &Value) -> Result<()> {
    ensure(store)?;
    store.conn.execute(
        "INSERT OR REPLACE INTO source_inspections(id,payload_json) VALUES(?1,?2)",
        rusqlite::params![id, serde_json::to_string(value)?],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_repository_survives_snapshot_release_and_removal_only_forgets_its_record() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("data")).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let root = inspection_root(store.data_dir(), &id).unwrap();
        fs::create_dir_all(root.join("source/skills/one")).unwrap();
        fs::write(root.join("source/skills/one/SKILL.md"), "# One").unwrap();
        let manifest = json!({"source":{"kind":"git","locator":"https://github.com/example/skills.git","subpath":"skills/one"},"candidates":[{"name":"One","subpath":"skills/one"}],"digest":source_digest(&root.join("source")).unwrap(),"createdAt":chrono::Utc::now().timestamp()});
        save_inspection(&store, &id, &manifest).unwrap();
        let saved = dispatch(&store, "repositories.save", &json!({"inspectionId":id})).unwrap();
        assert_eq!(saved["repositories"].as_array().unwrap().len(), 1);
        assert_eq!(
            saved["repositories"][0]["source"]["locator"],
            "https://github.com/example/skills"
        );
        assert!(saved["repositories"][0]["source"]["subpath"].is_null());
        dispatch(&store, "sources.release", &json!({"inspectionId":id})).unwrap();
        assert!(!root.exists());
        assert_eq!(
            dispatch(&store, "repositories.list", &json!({})).unwrap(),
            saved
        );
        let library = store.data_dir().join("library/skills/unrelated");
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("SKILL.md"), "Keep").unwrap();
        let removed = dispatch(
            &store,
            "repositories.remove",
            &json!({"id":saved["repositories"][0]["id"]}),
        )
        .unwrap();
        assert_eq!(removed["repositories"], json!([]));
        assert!(library.join("SKILL.md").is_file());
    }
}
