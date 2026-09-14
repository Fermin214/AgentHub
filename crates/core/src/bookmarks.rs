//! Offline repository bookmarks, independent of installed content and sources.
use crate::store::Store;
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

fn address_key(raw: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(raw.trim())?;
    url.set_fragment(None);
    url.set_query(None);
    let _ = url.set_scheme("https");
    let path = url
        .path()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_owned();
    url.set_path(&if url.host_str() == Some("github.com") {
        path.to_lowercase()
    } else {
        path
    });
    Ok(url.to_string())
}

pub fn dispatch(store: &Store, method: &str, args: &Value) -> Result<Value> {
    store.conn.execute_batch("CREATE TABLE IF NOT EXISTS repository_bookmarks(id TEXT PRIMARY KEY NOT NULL, payload_json TEXT NOT NULL)")?;
    match method {
        "bookmarks.sync" => {
            let transaction = store.conn.unchecked_transaction()?;
            for project in store.list_projects()? {
                sync_project(store, &project)?;
            }
            transaction.commit()?;
            dispatch(store, "bookmarks.list", &json!({}))
        }
        "bookmarks.list" => {
            let mut statement = store
                .conn
                .prepare("SELECT payload_json FROM repository_bookmarks ORDER BY rowid DESC")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            let mut items = Vec::new();
            for row in rows {
                let item: Value = serde_json::from_str(&row?)?;
                let query = args["query"].as_str().unwrap_or("").trim().to_lowercase();
                let status = args["status"].as_str().unwrap_or("all");
                let tag = args["tag"].as_str().unwrap_or("");
                if (status == "all" || item["status"] == status)
                    && (tag.is_empty()
                        || item["tags"]
                            .as_array()
                            .is_some_and(|tags| tags.iter().any(|t| t.as_str() == Some(tag))))
                    && (["name", "url", "notes"].iter().any(|key| {
                        item[*key]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)
                    }) || item["tags"].as_array().is_some_and(|tags| {
                        tags.iter()
                            .any(|t| t.as_str().unwrap_or("").to_lowercase().contains(&query))
                    }))
                {
                    items.push(item);
                }
            }
            Ok(json!({"items":items}))
        }
        "bookmarks.save" => {
            let transaction = store.conn.unchecked_transaction()?;
            let input = args.get("bookmark").unwrap_or(args);
            let url = input["url"].as_str().unwrap_or("").trim();
            let parsed = reqwest::Url::parse(url).map_err(|_| anyhow!("请填写有效的仓库链接"))?;
            if !["http", "https"].contains(&parsed.scheme())
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                bail!("仓库链接必须是 HTTP(S) 地址，且不能包含登录凭据");
            }
            let supplied_id = input["id"].as_str().filter(|id| !id.is_empty());
            let key = address_key(url)?;
            {
                let mut statement = store
                    .conn
                    .prepare("SELECT id,payload_json FROM repository_bookmarks")?;
                let rows = statement
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
                for row in rows {
                    let (id, raw) = row?;
                    let item: Value = serde_json::from_str(&raw)?;
                    if supplied_id != Some(id.as_str())
                        && address_key(item["url"].as_str().unwrap_or(""))? == key
                    {
                        bail!("这个仓库已在收藏中，请编辑已有记录");
                    }
                }
            }
            let old: Option<Value> = if let Some(id) = supplied_id {
                let raw: String = store
                    .conn
                    .query_row(
                        "SELECT payload_json FROM repository_bookmarks WHERE id=?1",
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| anyhow!("收藏记录不存在"))?;
                Some(serde_json::from_str(&raw)?)
            } else {
                None
            };
            let id = supplied_id
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let status = input["status"].as_str().unwrap_or("interested");
            if !["interested", "in_use", "uninstalled"].contains(&status) {
                bail!("未知的收藏状态");
            }
            let name = input["name"].as_str().unwrap_or("").trim();
            let inferred = parsed
                .path()
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".git");
            let name = if name.is_empty() {
                if inferred.is_empty() {
                    parsed.host_str().unwrap_or("仓库")
                } else {
                    inferred
                }
            } else {
                name
            };
            let notes = input["notes"].as_str().unwrap_or("");
            let raw_tags = input
                .get("tags")
                .or_else(|| old.as_ref().and_then(|v| v.get("tags")));
            let tags: Vec<String> = raw_tags
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?
                .unwrap_or_default();
            if tags.len() > 50 || tags.iter().any(|t| t.len() > 128) {
                bail!("标签过多或过长");
            }
            let mut seen = std::collections::HashSet::new();
            let tags: Vec<String> = tags
                .into_iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty() && seen.insert(t.clone()))
                .collect();
            if url.len() > 4096 || name.len() > 512 || notes.len() > 20000 {
                bail!("收藏内容过长");
            }
            let inferred_project = if input["projectId"].is_null() && old.is_none() {
                let matches: Vec<_> = store
                    .list_projects()?
                    .into_iter()
                    .filter(|p| {
                        crate::projects::bookmark_url(p)
                            .and_then(|url| address_key(&url).ok())
                            .as_deref()
                            == Some(&key)
                    })
                    .collect();
                if matches.len() == 1 {
                    Some(matches[0].id.clone())
                } else {
                    None
                }
            } else {
                None
            };
            let project_id = input["projectId"]
                .as_str()
                .filter(|v| !v.is_empty())
                .or(inferred_project.as_deref());
            if project_id != old.as_ref().and_then(|v| v["projectId"].as_str()) {
                if let Some(id) = project_id {
                    if store.get_project(id)?.is_none() {
                        bail!("关联的本地项目不存在");
                    }
                }
            }
            let now = chrono::Utc::now().to_rfc3339();
            let mut archived = input["archived"]
                .as_bool()
                .unwrap_or_else(|| old.as_ref().is_some_and(|v| v["archived"] == true));
            if let Some(project_id) = project_id {
                if let Some(mut project) = store.get_project(project_id)? {
                    // Changing archive state is a shared action. Establishing a link adopts the project's state.
                    let changed = old.as_ref().is_some_and(|v| {
                        v["archived"].as_bool().unwrap_or(false) != archived
                            && v["projectId"] == project_id
                    });
                    if changed {
                        project.archived = archived;
                        project.updated_at = now.clone();
                        store.save_project(&project)?;
                    } else {
                        archived = project.archived;
                    }
                    sync_archive(store, project_id, archived)?;
                }
            }
            let status = if project_id.is_some() {
                "in_use"
            } else {
                status
            };
            let item = json!({"id":id,"name":name,"url":url,"notes":notes,"tags":tags,"status":status,"projectId":project_id,"archived":archived,"createdAt":old.as_ref().and_then(|v|v["createdAt"].as_str()).unwrap_or(&now),"updatedAt":now});
            store.conn.execute(
                "INSERT OR REPLACE INTO repository_bookmarks(id,payload_json) VALUES(?1,?2)",
                params![id, item.to_string()],
            )?;
            let action = match old.as_ref() {
                None => Some("添加仓库收藏"),
                Some(old) if old["archived"].as_bool().unwrap_or(false) != archived => {
                    Some(if archived {
                        "归档收藏及关联项目"
                    } else {
                        "取消归档收藏及关联项目"
                    })
                }
                Some(old) if old["projectId"] != item["projectId"] => {
                    Some(if project_id.is_some() {
                        "关联本机项目"
                    } else {
                        "解除本机项目关联"
                    })
                }
                Some(old) if old["url"] != item["url"] => Some("修改仓库地址"),
                _ => None,
            };
            if let Some(action) = action {
                store.record_activity(
                    "bookmark",
                    &format!("{action} · {name}"),
                    &[url.to_owned()],
                )?;
            }
            transaction.commit()?;
            Ok(item)
        }
        "bookmarks.delete" => {
            let transaction = store.conn.unchecked_transaction()?;
            let id = args["id"]
                .as_str()
                .ok_or_else(|| anyhow!("id is required"))?;
            store.conn.execute_batch("CREATE TABLE IF NOT EXISTS suppressed_project_bookmarks(project_id TEXT PRIMARY KEY)")?;
            let items = dispatch(store, "bookmarks.list", &json!({}))?;
            if let Some(project) = items["items"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["id"] == id))
                .and_then(|item| item["projectId"].as_str())
            {
                store.conn.execute(
                    "INSERT OR IGNORE INTO suppressed_project_bookmarks(project_id) VALUES(?1)",
                    [project],
                )?;
            }
            let removed = store
                .conn
                .execute("DELETE FROM repository_bookmarks WHERE id=?1", [id])?;
            if removed > 0 {
                if let Some(item) = items["items"]
                    .as_array()
                    .and_then(|items| items.iter().find(|item| item["id"] == id))
                {
                    store.record_activity(
                        "bookmark",
                        &format!("删除仓库收藏 · {}", item["name"].as_str().unwrap_or("")),
                        &[item["url"].as_str().unwrap_or("").to_owned()],
                    )?;
                }
            }
            transaction.commit()?;
            Ok(json!({"ok":true,"removed":removed>0}))
        }
        _ => bail!("unknown bookmark method"),
    }
}

pub(crate) fn sync_archive(store: &Store, project_id: &str, archived: bool) -> Result<()> {
    let items = dispatch(store, "bookmarks.list", &json!({}))?;
    for mut item in items["items"].as_array().cloned().unwrap_or_default() {
        if item["projectId"] == project_id
            && (item["archived"] != archived || item["status"] != "in_use")
        {
            item["archived"] = json!(archived);
            item["status"] = json!("in_use");
            item["updatedAt"] = json!(chrono::Utc::now().to_rfc3339());
            store.conn.execute(
                "UPDATE repository_bookmarks SET payload_json=?1 WHERE id=?2",
                params![item.to_string(), item["id"].as_str()],
            )?;
        }
    }
    Ok(())
}

pub(crate) fn sync_project(store: &Store, project: &crate::model::LocalProject) -> Result<()> {
    sync_archive(store, &project.id, project.archived)?;
    let items = dispatch(store, "bookmarks.list", &json!({}))?;
    let items = items["items"].as_array().cloned().unwrap_or_default();
    if items.iter().any(|item| item["projectId"] == project.id) {
        return Ok(());
    }
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS suppressed_project_bookmarks(project_id TEXT PRIMARY KEY)",
    )?;
    let suppressed: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM suppressed_project_bookmarks WHERE project_id=?1)",
        [&project.id],
        |r| r.get(0),
    )?;
    if suppressed {
        return Ok(());
    }
    let Some(url) = crate::projects::bookmark_url(project) else {
        return Ok(());
    };
    let key = address_key(&url)?;
    let found = items.into_iter().find(|item| {
        address_key(item["url"].as_str().unwrap_or(""))
            .ok()
            .as_deref()
            == Some(&key)
    });
    let now = chrono::Utc::now().to_rfc3339();
    let mut item = if let Some(item) = found {
        if item["projectId"]
            .as_str()
            .is_some_and(|id| id != project.id)
        {
            return Ok(());
        }
        item
    } else {
        json!({"id":uuid::Uuid::new_v4().to_string(),"name":project.name,"url":url,"notes":"","tags":[],"status":"in_use","createdAt":now})
    };
    item["projectId"] = json!(project.id);
    item["archived"] = json!(project.archived);
    item["updatedAt"] = json!(now);
    item["status"] = json!("in_use");
    store.conn.execute(
        "INSERT OR REPLACE INTO repository_bookmarks(id,payload_json) VALUES(?1,?2)",
        params![item["id"].as_str(), item.to_string()],
    )?;
    Ok(())
}

pub(crate) fn delete_project_bookmarks(store: &Store, id: &str) -> Result<()> {
    let items = dispatch(store, "bookmarks.list", &json!({}))?;
    for item in items["items"].as_array().cloned().unwrap_or_default() {
        if item["projectId"] == id {
            store.conn.execute(
                "DELETE FROM repository_bookmarks WHERE id=?1",
                [item["id"].as_str()],
            )?;
        }
    }
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS suppressed_project_bookmarks(project_id TEXT PRIMARY KEY)",
    )?;
    store.conn.execute(
        "DELETE FROM suppressed_project_bookmarks WHERE project_id=?1",
        [id],
    )?;
    Ok(())
}
