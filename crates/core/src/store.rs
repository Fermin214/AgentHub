//! Persistence for the current desktop domains. No old database migration path.
use crate::model::{LocalProject, ScanRoot, Settings, Skill, SkillDeployment};
use crate::scan::default_scan_roots;
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

const DATABASE_FILE: &str = "agenthub.sqlite3";
pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) data_dir: PathBuf,
}
impl Store {
    pub fn open(data_dir: &Path) -> Result<Self> {
        if data_dir.as_os_str().is_empty() {
            bail!("数据目录不能为空");
        }
        let data_dir = std::path::absolute(data_dir)?;
        crate::safe_files::safe_directory(&data_dir)?;
        fs::create_dir_all(&data_dir)?;
        let database = data_dir.join(DATABASE_FILE);
        if let Ok(metadata) = fs::symlink_metadata(&database) {
            if metadata.file_type().is_symlink() {
                bail!("数据库不能是符号链接");
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    bail!("数据库不能是重解析点");
                }
            }
        }
        let store = Self {
            conn: Connection::open(data_dir.join(DATABASE_FILE))?,
            data_dir: data_dir.to_path_buf(),
        };
        store.initialize()?;
        Ok(store)
    }
    pub fn open_in_memory() -> Result<Self> {
        let store = Self {
            conn: Connection::open_in_memory()?,
            data_dir: PathBuf::new(),
        };
        store.initialize()?;
        Ok(store)
    }
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
    pub fn database_path(&self) -> PathBuf {
        if self.data_dir.as_os_str().is_empty() {
            PathBuf::from(":memory:")
        } else {
            self.data_dir.join(DATABASE_FILE)
        }
    }
    fn initialize(&self) -> Result<()> {
        self.conn.busy_timeout(std::time::Duration::from_secs(5))?;
        self.conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        // Concurrent first opens can race while switching the journal mode.
        // SQLite may return BUSY here without invoking its busy handler. Retry
        // this idempotent statement after its temporary locks have been released.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match self.conn.execute_batch("PRAGMA journal_mode=WAL;") {
                Ok(()) => break,
                Err(error)
                    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(error) => return Err(error).context("initialize database journal mode"),
            }
        }
        self.conn.execute_batch("
            CREATE TABLE IF NOT EXISTS prompts(id TEXT PRIMARY KEY NOT NULL,title TEXT NOT NULL,body TEXT NOT NULL,category TEXT NOT NULL DEFAULT '',tags_json TEXT NOT NULL DEFAULT '[]',favorite INTEGER NOT NULL DEFAULT 0,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,purpose TEXT NOT NULL DEFAULT '');
            CREATE INDEX IF NOT EXISTS prompts_updated_at_idx ON prompts(updated_at DESC);
            CREATE TABLE IF NOT EXISTS skills(id TEXT PRIMARY KEY NOT NULL,payload_json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS skill_deployments(id TEXT PRIMARY KEY NOT NULL,payload_json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS local_projects(id TEXT PRIMARY KEY NOT NULL,payload_json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS app_settings(key TEXT PRIMARY KEY,value_json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS skill_change_results(id TEXT PRIMARY KEY,created_at TEXT NOT NULL,payload_json TEXT NOT NULL);")?;
        Ok(())
    }
    pub fn get_settings(&self) -> Result<Settings> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value_json FROM app_settings WHERE key = 'settings'",
                [],
                |row| row.get(0),
            )
            .optional()
            .context("read settings")?;
        match raw {
            Some(raw) => {
                let settings = serde_json::from_str(&raw).context("decode settings")?;
                normalize_settings(settings)
            }
            None => {
                let mut settings = Settings::default();
                settings.scan_roots = default_scan_roots()
                    .into_iter()
                    .filter(|root| Path::new(&root.path).exists())
                    .collect();
                normalize_settings(settings)
            }
        }
    }

    /// Distinguish an untouched database from an explicitly saved empty
    /// configuration. An empty scanRoots list is a valid choice in the UI
    /// and must not silently re-enable default profile roots.
    pub fn settings_saved(&self) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT 1 FROM app_settings WHERE key = 'settings'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|value| value.is_some())
            .context("check settings state")
    }

    pub fn save_settings(&mut self, input: &Settings) -> Result<Settings> {
        let settings = normalize_settings(input.clone())?;
        let payload = serde_json::to_string(&settings).context("serialize settings")?;
        self.conn.execute(
            "INSERT INTO app_settings(key, value_json) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            [payload],
        )?;
        Ok(settings)
    }

    fn list_records<T: DeserializeOwned>(&self, table: &str) -> Result<Vec<T>> {
        let mut statement = self
            .conn
            .prepare(&format!("SELECT payload_json FROM {table} ORDER BY rowid"))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    fn get_record<T: DeserializeOwned>(&self, table: &str, id: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                &format!("SELECT payload_json FROM {table} WHERE id=?1"),
                [id],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|r| Ok(serde_json::from_str(&r)?)).transpose()
    }
    fn save_record<T: Serialize>(&self, table: &str, id: &str, item: &T) -> Result<()> {
        if id.trim().is_empty() || id.len() > 256 {
            bail!("记录标识无效");
        }
        self.conn.execute(&format!("INSERT INTO {table}(id,payload_json) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload_json=excluded.payload_json"),params![id,serde_json::to_string(item)?])?;
        Ok(())
    }
    pub fn list_skills(&self) -> Result<Vec<Skill>> {
        self.list_records("skills")
    }
    pub fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
        self.get_record("skills", id)
    }
    pub fn save_skill(&self, item: &Skill) -> Result<()> {
        if item.name.trim().is_empty() || !Path::new(&item.path).is_absolute() {
            bail!("Skill 名称或目录无效");
        }
        let key = crate::protection::path_key(Path::new(&item.path));
        if self.list_skills()?.iter().any(|s| {
            s.id != item.id
                && (s.name.eq_ignore_ascii_case(&item.name)
                    || crate::protection::path_key(Path::new(&s.path)) == key)
        }) {
            bail!("Skill 库中已有同名或同位置的记录");
        }
        self.save_record("skills", &item.id, item)
    }
    pub fn delete_skill(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM skills WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn list_deployments(&self) -> Result<Vec<SkillDeployment>> {
        self.list_records("skill_deployments")
    }
    pub fn get_deployment(&self, id: &str) -> Result<Option<SkillDeployment>> {
        self.get_record("skill_deployments", id)
    }
    pub fn save_deployment(&self, item: &SkillDeployment) -> Result<()> {
        if item.name.trim().is_empty()
            || item.agent.trim().is_empty()
            || !Path::new(&item.path).is_absolute()
        {
            bail!("Skill 安装位置无效");
        }
        if let Some(id) = &item.skill_id {
            if self.get_skill(id)?.is_none() {
                bail!("关联的 Skill 不存在");
            }
        }
        let key = crate::protection::path_key(Path::new(&item.path));
        if self.list_deployments()?.iter().any(|d| {
            d.id != item.id
                && d.agent == item.agent
                && d.scope == item.scope
                && d.profile.as_deref().unwrap_or("") == item.profile.as_deref().unwrap_or("")
                && crate::protection::path_key(Path::new(&d.path)) == key
        }) {
            bail!("Agent 已记录这个安装位置");
        }
        self.save_record("skill_deployments", &item.id, item)
    }
    pub fn delete_deployment(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM skill_deployments WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn list_projects(&self) -> Result<Vec<LocalProject>> {
        self.list_records("local_projects")
    }
    pub fn get_project(&self, id: &str) -> Result<Option<LocalProject>> {
        self.get_record("local_projects", id)
    }
    pub fn save_project(&self, item: &LocalProject) -> Result<()> {
        if !Path::new(&item.path).is_absolute() {
            bail!("项目目录必须是绝对路径");
        }
        let key = crate::protection::path_key(Path::new(&item.path));
        if self
            .list_projects()?
            .iter()
            .any(|p| p.id != item.id && crate::protection::path_key(Path::new(&p.path)) == key)
        {
            bail!("这个本机项目已经添加");
        }
        self.save_record("local_projects", &item.id, item)
    }
    pub fn delete_project(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM local_projects WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn record_change(&self, result: &Value) -> Result<()> {
        let id = result["id"].as_str().context("变更结果缺少标识")?;
        self.conn.execute("INSERT OR REPLACE INTO skill_change_results(id,created_at,payload_json) VALUES(?1,?2,?3)",params![id,chrono::Utc::now().to_rfc3339(),serde_json::to_string(result)?])?;
        self.conn.execute("DELETE FROM skill_change_results WHERE id NOT IN (SELECT id FROM skill_change_results ORDER BY created_at DESC LIMIT 100)",[])?;
        Ok(())
    }
    pub(crate) fn record_activity(
        &self,
        domain: &str,
        summary: &str,
        locations: &[String],
    ) -> Result<()> {
        self.record_change(&json!({"id":uuid::Uuid::new_v4().to_string(),"domain":domain,"status":"succeeded","summary":summary,"createdAt":chrono::Utc::now().to_rfc3339(),"locations":locations}))
    }
    pub fn list_operations(&self) -> Result<Vec<Value>> {
        let mut statement = self.conn.prepare(
            "SELECT payload_json FROM skill_change_results ORDER BY created_at DESC LIMIT 100",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn data_scope(&self) -> String {
        short_hash(&crate::protection::path_key(&self.data_dir))
    }
    pub fn snapshot(&self) -> Result<Value> {
        let mut skills = self.list_skills()?;
        for skill in &mut skills {
            if let Some(description) = crate::native::skill_description(Path::new(&skill.path)) {
                skill.description = description;
            }
        }
        let deployments = self
            .list_deployments()?
            .iter()
            .map(|d| {
                let mut value = serde_json::to_value(d)?;
                value["present"] = json!(Path::new(&d.path).join("SKILL.md").is_file());
                value["pathKey"] = json!(crate::protection::path_key(Path::new(&d.path)));
                Ok(value)
            })
            .collect::<Result<Vec<Value>>>()?;
        Ok(
            json!({"dataDir":self.data_dir(),"dataScope":self.data_scope(),"prompts":self.list_prompts()?,"skills":skills,"skillUpdates":crate::skills::statuses(self)?,"deployments":deployments,"projects":self.list_projects()?,"settings":self.get_settings()?,"operations":self.list_operations()?}),
        )
    }
}

fn normalize_settings(mut settings: Settings) -> Result<Settings> {
    let mut roots = Vec::with_capacity(settings.scan_roots.len());
    let mut identities = HashSet::new();
    for mut root in settings.scan_roots {
        root.path = root.path.trim().to_string();
        if root.path.is_empty() {
            bail!("scan root path must not be empty");
        }
        root.agent = root.agent.trim().to_ascii_lowercase();
        root.scope = root.scope.trim().to_ascii_lowercase();
        if root.agent.is_empty() || root.scope.is_empty() {
            bail!("scan root agent and scope must not be empty");
        }
        root.profile = root
            .profile
            .map(|profile| profile.trim().to_string())
            .filter(|profile| !profile.is_empty());
        if root.id.trim().is_empty() {
            root.id = stable_scan_root_id(&root);
        }
        let identity = format!(
            "{}|{}|{}|{}",
            root.agent,
            root.scope,
            root.profile.as_deref().unwrap_or_default(),
            crate::protection::path_key(Path::new(&root.path)),
        );
        if identities.insert(identity) {
            roots.push(root);
        }
    }
    settings.scan_roots = roots;
    // An unsupported language follows the system locale.
    settings.language = match settings.language.trim().to_ascii_lowercase().as_str() {
        "zh" => "zh".to_string(),
        "en" => "en".to_string(),
        _ => String::new(),
    };
    Ok(settings)
}

fn stable_scan_root_id(root: &ScanRoot) -> String {
    let identity = format!(
        "{}|{}|{}|{}",
        root.agent,
        root.scope,
        root.profile.as_deref().unwrap_or_default(),
        crate::protection::path_key(Path::new(&root.path)),
    );
    format!("root-{}", short_hash(&identity))
}

fn short_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(value.as_bytes());
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    #[test]
    fn concurrent_first_opens_initialize_one_usable_database() {
        let temp = tempfile::tempdir().unwrap();
        for round in 0..10 {
            let data = temp.path().join(round.to_string());
            let barrier = Barrier::new(8);
            std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for n in 0..8 {
                    let data = &data;
                    let barrier = &barrier;
                    handles.push(scope.spawn(move || {
                        barrier.wait();
                        let store = Store::open(data).unwrap();
                        store
                            .conn
                            .execute(
                                "INSERT INTO app_settings(key,value_json) VALUES(?1,'true')",
                                [format!("worker-{n}")],
                            )
                            .unwrap();
                    }));
                }
                for handle in handles {
                    handle.join().unwrap();
                }
            });
            let store = Store::open(&data).unwrap();
            let count: i64 = store
                .conn
                .query_row("SELECT count(*) FROM app_settings", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 8);
            let mode: String = store
                .conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
        }
    }
}
