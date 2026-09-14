//! Current desktop domain models. No compatibility records for retired products.
use serde::{Deserialize, Serialize};

fn default_string() -> String {
    String::new()
}

fn default_vec<T>() -> Vec<T> {
    Vec::new()
}

/// A prompt stored in the local library.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    #[serde(default = "default_string")]
    pub id: String,
    #[serde(default = "default_string")]
    pub title: String,
    #[serde(default = "default_string")]
    pub body: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default = "default_string")]
    pub category: String,
    #[serde(default = "default_vec")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default = "default_string")]
    pub created_at: String,
    #[serde(default = "default_string")]
    pub updated_at: String,
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            body: String::new(),
            purpose: String::new(),
            category: String::new(),
            tags: Vec::new(),
            favorite: false,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

/// A user-visible upstream/source binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde(default = "default_unknown")]
    pub kind: String,
    #[serde(default = "default_string")]
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

fn default_unknown() -> String {
    "unknown".to_string()
}

impl Default for Source {
    fn default() -> Self {
        Self {
            kind: "unknown".to_string(),
            locator: String::new(),
            subpath: None,
            revision: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanRoot {
    #[serde(default = "default_string")]
    pub id: String,
    pub agent: String,
    pub path: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Executables {
    #[serde(default)]
    pub hermes: String,
    #[serde(default)]
    pub zcode: String,
    #[serde(default = "default_string")]
    pub codex: String,
    #[serde(default = "default_string")]
    pub claude: String,
    #[serde(default = "default_string")]
    pub dsh: String,
}

impl Default for Executables {
    fn default() -> Self {
        Self {
            hermes: String::new(),
            zcode: String::new(),
            codex: String::new(),
            claude: String::new(),
            dsh: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_vec")]
    pub scan_roots: Vec<ScanRoot>,
    #[serde(default)]
    pub executables: Executables,
    /// UI display language. Empty means the interface follows the system locale.
    /// A `String` so an unknown value from a newer build stays readable.
    #[serde(default)]
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan_roots: Vec::new(),
            executables: Executables::default(),
            language: String::new(),
        }
    }
}

/// One copy owned by the Skill library; deployment records link by ID.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub path: String,
    #[serde(default)]
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_digest: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// A physical Skill location discovered for one Agent. No duplicate component model.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillDeployment {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub agent: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub path: String,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub ignored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_digest: Option<String>,
}

/// A local folder; archival only changes this record.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalProject {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub git_trusted: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}
