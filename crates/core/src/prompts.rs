//! Prompt persistence and text/metadata export.
use crate::{model::Prompt, store::Store};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::collections::HashSet;
use uuid::Uuid;

impl Store {
    pub fn list_prompts(&self) -> Result<Vec<Prompt>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, body, category, tags_json, favorite, created_at, updated_at, purpose
             FROM prompts ORDER BY favorite DESC, updated_at DESC, id ASC",
        )?;
        let rows = stmt.query_map([], prompt_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("read prompts")
    }

    pub fn get_prompt(&self, id: &str) -> Result<Option<Prompt>> {
        self.conn
            .query_row(
                "SELECT id, title, body, category, tags_json, favorite, created_at, updated_at, purpose
                 FROM prompts WHERE id = ?1",
                [id],
                prompt_from_row,
            )
            .optional()
            .context("read prompt")
    }

    pub fn save_prompt(&mut self, input: &Prompt) -> Result<Prompt> {
        let mut prompt = input.clone();
        prompt.id = prompt.id.trim().to_string();
        if prompt.id.is_empty() {
            prompt.id = Uuid::new_v4().to_string();
        }
        prompt.title = prompt.title.trim().to_string();
        if prompt.title.is_empty() {
            bail!("prompt title must not be empty");
        }
        prompt.category = prompt.category.trim().to_string();
        prompt.purpose = prompt.purpose.trim().to_string();
        prompt.tags = normalize_tags(&prompt.tags);

        if prompt.created_at.trim().is_empty() {
            prompt.created_at = self
                .get_prompt(&prompt.id)?
                .map(|old| old.created_at)
                .unwrap_or_else(now_string);
        }
        if prompt.updated_at.trim().is_empty() {
            prompt.updated_at = now_string();
        }
        validate_timestamp(&prompt.created_at, "createdAt")?;
        validate_timestamp(&prompt.updated_at, "updatedAt")?;

        let old = self.get_prompt(&prompt.id)?;
        let transaction = self.conn.unchecked_transaction()?;
        insert_prompt(&self.conn, &prompt)?;
        if old.as_ref().is_none_or(|old| {
            old.title != prompt.title || old.body != prompt.body || old.purpose != prompt.purpose
        }) {
            self.record_activity(
                "prompt",
                &format!(
                    "{} Prompt · {}",
                    if old.is_some() { "修改" } else { "添加" },
                    prompt.title
                ),
                &[],
            )?;
        }
        transaction.commit()?;
        Ok(prompt)
    }

    pub fn delete_prompt(&mut self, id: &str) -> Result<bool> {
        let old = self.get_prompt(id)?;
        let transaction = self.conn.unchecked_transaction()?;
        let changed = self
            .conn
            .execute("DELETE FROM prompts WHERE id = ?1", [id])?;
        if let Some(old) = old.filter(|_| changed > 0) {
            self.record_activity("prompt", &format!("删除 Prompt · {}", old.title), &[])?;
        }
        transaction.commit()?;
        Ok(changed > 0)
    }

    pub fn export_prompts(&self, format: &str) -> Result<(String, String)> {
        let prompts = self.list_prompts()?;
        match format.to_ascii_lowercase().as_str() {
            "json" => Ok((
                serde_json::to_string_pretty(&prompts).context("serialize prompt export")?,
                "prompts.json".to_string(),
            )),
            "markdown" | "md" => Ok((export_markdown(&prompts)?, "prompts.md".to_string())),
            other => bail!("unsupported prompt export format {}", other),
        }
    }
}

fn now_string() -> String {
    Utc::now().to_rfc3339()
}

fn validate_timestamp(value: &str, field: &str) -> Result<()> {
    DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .with_context(|| format!("{} must be an RFC3339 timestamp", field))
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty() && seen.insert(tag.to_ascii_lowercase()))
        .collect()
}

fn insert_prompt(conn: &Connection, prompt: &Prompt) -> Result<()> {
    conn.execute(
        "INSERT INTO prompts
            (id, title, body, category, tags_json, favorite, created_at, updated_at, purpose)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
             title = excluded.title,
             body = excluded.body,
             category = excluded.category,
             tags_json = excluded.tags_json,
             favorite = excluded.favorite,
             created_at = excluded.created_at,
             updated_at = excluded.updated_at,
             purpose = excluded.purpose",
        params![
            prompt.id,
            prompt.title,
            prompt.body,
            prompt.category,
            serde_json::to_string(&prompt.tags)?,
            if prompt.favorite { 1 } else { 0 },
            prompt.created_at,
            prompt.updated_at,
            prompt.purpose,
        ],
    )?;
    Ok(())
}

fn prompt_from_row(row: &Row<'_>) -> rusqlite::Result<Prompt> {
    let tags_json: String = row.get(4)?;
    let tags = serde_json::from_str(&tags_json).unwrap_or_default();
    Ok(Prompt {
        id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        purpose: row.get(8)?,
        category: row.get(3)?,
        tags,
        favorite: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptFrontmatter {
    #[serde(skip_serializing_if = "String::is_empty")]
    purpose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body_length: Option<usize>,
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    favorite: bool,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

fn export_markdown(prompts: &[Prompt]) -> Result<String> {
    let mut output = String::new();
    for (index, prompt) in prompts.iter().enumerate() {
        let frontmatter = PromptFrontmatter {
            purpose: prompt.purpose.clone(),
            body_length: Some(prompt.body.len()),
            id: prompt.id.clone(),
            title: prompt.title.clone(),
            category: prompt.category.clone(),
            tags: prompt.tags.clone(),
            favorite: prompt.favorite,
            created_at: prompt.created_at.clone(),
            updated_at: prompt.updated_at.clone(),
        };
        output.push_str("---\n");
        let yaml = serde_yaml::to_string(&frontmatter).context("serialize Markdown frontmatter")?;
        output.push_str(&yaml);
        output.push_str("---\n");
        output.push_str(&prompt.body);
        if !prompt.body.ends_with('\n') {
            output.push('\n');
        }
        if index + 1 < prompts.len() {
            output.push('\n');
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prompt(id: &str, title: &str, body: &str) -> Prompt {
        Prompt {
            id: id.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            purpose: "解释问题".to_string(),
            category: "test".to_string(),
            tags: vec!["one".to_string(), "two".to_string()],
            favorite: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:01Z".to_string(),
        }
    }

    #[test]
    fn prompt_crud_round_trip() {
        let mut store = Store::open_in_memory().unwrap();
        let saved = store.save_prompt(&prompt("p1", "Title", "Body")).unwrap();
        assert_eq!(store.get_prompt("p1").unwrap(), Some(saved.clone()));
        assert_eq!(store.list_prompts().unwrap(), vec![saved]);
        assert!(store.delete_prompt("p1").unwrap());
        assert!(!store.delete_prompt("p1").unwrap());
    }

    #[test]
    fn exports_preserve_body_and_metadata() {
        let mut store = Store::open_in_memory().unwrap();
        let item = prompt("saved", "标题", "  中文正文\r\n\n");
        store.save_prompt(&item).unwrap();
        let (json, filename) = store.export_prompts("json").unwrap();
        assert_eq!(filename, "prompts.json");
        assert_eq!(
            serde_json::from_str::<Vec<Prompt>>(&json).unwrap(),
            vec![item.clone()]
        );
        let (markdown, _) = store.export_prompts("markdown").unwrap();
        assert!(markdown.contains(&item.body));
        assert!(markdown.contains("title: 标题"));
        assert!(markdown.contains("favorite: true"));
        assert!(markdown.contains("purpose: 解释问题"));
    }
}
