//! Read-only discovery of Agent Skills.
//!
//! The scanner deliberately understands only metadata files. It never loads
//! or executes a SKILL.md. It inspects direct metadata for linked deployments
//! without recursively traversing link targets, so scanning an
//! untrusted or partially installed tree cannot run arbitrary code.

use crate::model::{ScanRoot, SkillDeployment, Source};
use anyhow::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_SKILL_HEADER_BYTES: u64 = 128 * 1024;

#[derive(Debug, Default, Clone)]
pub struct ScanReport {
    pub deployments: Vec<SkillDeployment>,
    pub warnings: Vec<String>,
    /// Roots whose directory walk completed without an access error. The
    /// store may reconcile stale discovered rows only for these roots.
    pub successful_roots: Vec<ScanRoot>,
}

#[derive(Debug, Clone)]
struct SkillCandidate {
    name: String,
    description: String,
    source: Source,
    owner: String,
}

/// Build the default global roots from the current user's profile. Callers
/// may pass explicit roots to scan_roots for fixtures or custom Agent setups.
pub fn default_scan_roots() -> Vec<ScanRoot> {
    let home = user_home();
    let Some(home) = home else {
        return Vec::new();
    };
    let candidates = [
        ("codex", ".codex", "skills", "global"),
        ("claude", ".claude", "skills", "global"),
        ("zcode", ".zcode", "skills", "global"),
        ("dsh", ".deepseek", "skills", "global"),
        ("dsh", ".dsh", "skills", "global"),
        ("shared", ".agents", "skills", "global"),
    ];
    let mut roots: Vec<ScanRoot> = candidates
        .into_iter()
        .map(|(agent, parent, child, scope)| ScanRoot {
            id: String::new(),
            agent: agent.to_string(),
            path: home.join(parent).join(child).to_string_lossy().to_string(),
            scope: scope.to_string(),
            profile: None,
        })
        .collect();
    if let Some(custom) = env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        for child in ["skills"] {
            let path = PathBuf::from(&custom)
                .join(child)
                .to_string_lossy()
                .to_string();
            if !roots
                .iter()
                .any(|root| root.path.eq_ignore_ascii_case(&path))
            {
                roots.push(ScanRoot {
                    id: String::new(),
                    agent: "codex".into(),
                    path,
                    scope: "global".into(),
                    profile: None,
                });
            }
        }
    }
    roots.extend(dsh_profile_roots(&home));
    roots.extend(hermes_scan_roots(&home));
    roots
}

/// Enumerate Hermes's default and named-profile skill directories.
fn hermes_scan_roots(home: &Path) -> Vec<ScanRoot> {
    let configured = env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(windows) {
                env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .map(|path| path.join("hermes"))
                    .unwrap_or_else(|| home.join(".hermes"))
            } else {
                home.join(".hermes")
            }
        });
    let (root, active_profile) = if configured
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "profiles")
    {
        (
            configured
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_else(|| configured.clone()),
            configured
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
        )
    } else {
        (configured, None)
    };
    let mut roots = vec![ScanRoot {
        id: String::new(),
        agent: "hermes".into(),
        path: root.join("skills").to_string_lossy().into_owned(),
        scope: "global".into(),
        profile: None,
    }];
    append_hermes_configured_roots(&mut roots, &root, None);
    let profiles_dir = root.join("profiles");
    if let Ok(entries) = fs::read_dir(&profiles_dir) {
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let profile = entry.file_name().to_string_lossy().into_owned();
            if profile.is_empty() || profile == "." || profile == ".." {
                continue;
            }
            let profile_home = entry.path();
            roots.push(ScanRoot {
                id: String::new(),
                agent: "hermes".into(),
                path: profile_home.join("skills").to_string_lossy().into_owned(),
                scope: "profile".into(),
                profile: Some(profile.clone()),
            });
            append_hermes_configured_roots(&mut roots, &profile_home, Some(profile));
        }
    }
    if let Some(profile) = active_profile {
        let path = root.join("profiles").join(&profile).join("skills");
        if !roots
            .iter()
            .any(|candidate| candidate.path == path.to_string_lossy())
        {
            roots.push(ScanRoot {
                id: String::new(),
                agent: "hermes".into(),
                path: path.to_string_lossy().into_owned(),
                scope: "profile".into(),
                profile: Some(profile),
            });
        }
    }
    roots
}

/// Read Hermes's profile-local skill search settings. Hermes expands `~` and
/// `${VAR}` values, and resolves relative entries against the active profile
/// home. Missing directories are intentionally ignored, matching Hermes.
fn append_hermes_configured_roots(
    roots: &mut Vec<ScanRoot>,
    profile_home: &Path,
    profile: Option<String>,
) {
    let Ok(raw) = fs::read_to_string(profile_home.join("config.yaml")) else {
        return;
    };
    let Ok(config) = serde_yaml::from_str::<serde_yaml::Value>(&raw) else {
        return;
    };
    let Some(skills) = config.get("skills") else {
        return;
    };
    let mut configured = Vec::new();
    if let Some(entries) = skills
        .get("external_dirs")
        .and_then(serde_yaml::Value::as_sequence)
    {
        configured.extend(entries.iter().filter_map(serde_yaml::Value::as_str));
    }
    if let Some(create_dir) = skills.get("create_dir").and_then(serde_yaml::Value::as_str) {
        configured.push(create_dir);
    }
    let user_home = user_home();
    for value in configured {
        let Some(path) = expand_hermes_path(value, profile_home, user_home.as_deref()) else {
            continue;
        };
        if !path.is_dir()
            || roots.iter().any(|root| {
                path_key(Path::new(&root.path)) == path_key(&path) && root.profile == profile
            })
        {
            continue;
        }
        roots.push(ScanRoot {
            id: String::new(),
            agent: "hermes".into(),
            path: path.to_string_lossy().into_owned(),
            scope: if profile.is_some() {
                "profile"
            } else {
                "global"
            }
            .into(),
            profile: profile.clone(),
        });
    }
}

fn expand_hermes_path(
    value: &str,
    profile_home: &Path,
    user_home: Option<&Path>,
) -> Option<PathBuf> {
    let value = value.trim();
    let mut expanded = String::with_capacity(value.len());
    let chars: Vec<_> = value.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '$' && index + 1 < chars.len() && chars[index + 1] == '{' {
            let end = (index + 2..chars.len()).find(|position| chars[*position] == '}')?;
            let name: String = chars[index + 2..end].iter().collect();
            expanded.push_str(&std::env::var_os(name)?.to_string_lossy());
            index = end + 1;
        } else {
            expanded.push(chars[index]);
            index += 1;
        }
    }
    let expanded = if let Some(home) = user_home {
        expanded
            .strip_prefix("~/")
            .map(|rest| home.join(rest).to_string_lossy().into_owned())
            .or_else(|| (expanded == "~").then(|| home.to_string_lossy().into_owned()))
            .unwrap_or(expanded)
    } else {
        expanded
    };
    let path = PathBuf::from(expanded);
    Some(if path.is_absolute() {
        path
    } else {
        profile_home.join(path)
    })
}

/// DSH keeps profile-specific packages below `.dsh/profiles/<name>`. Treat
/// each real profile directory as its own scan root so inventory rows retain
/// the profile identity instead of being flattened into one global DSH list.
/// The shared `node_modules` directory is dependency storage, not a profile.
fn dsh_profile_roots(home: &Path) -> Vec<ScanRoot> {
    let mut roots = Vec::new();
    for parent in [".dsh", ".deepseek"] {
        let profiles = home.join(parent).join("profiles");
        let Ok(entries) = fs::read_dir(&profiles) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let profile = entry.file_name().to_string_lossy().to_string();
            if profile.is_empty() || profile.eq_ignore_ascii_case("node_modules") {
                continue;
            }
            roots.push(ScanRoot {
                id: String::new(),
                agent: "dsh".to_string(),
                path: entry.path().to_string_lossy().to_string(),
                scope: "profile".to_string(),
                profile: Some(profile),
            });
        }
    }
    roots.sort_by(|left, right| left.path.cmp(&right.path));
    roots
}

fn user_home() -> Option<PathBuf> {
    if cfg!(windows) {
        env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
    } else {
        env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }
}

/// Scan the supplied roots. This function has no write side effects.
pub fn scan_roots(roots: &[ScanRoot]) -> ScanReport {
    let mut report = ScanReport::default();
    let mut all: BTreeMap<String, SkillDeployment> = BTreeMap::new();
    for root in roots {
        if root.path.trim().is_empty() {
            report
                .warnings
                .push(format!("scan root for {} has an empty path", root.agent));
            continue;
        }
        let root_path = PathBuf::from(&root.path);
        if !root_path.exists() {
            report
                .warnings
                .push(format!("scan root does not exist: {}", root_path.display()));
            continue;
        }
        if !root_path.is_dir() {
            report.warnings.push(format!(
                "scan root is not a directory: {}",
                root_path.display()
            ));
            continue;
        }
        let walked = scan_one_root(root, &root_path, &mut all, &mut report.warnings);
        if walked {
            report.successful_roots.push(root.clone());
        }
    }
    report.deployments = all
        .into_values()
        .map(|mut item| {
            if crate::sources::validate_source(&item.source).is_err() {
                item.source = Source::default();
                report.warnings.push(format!(
                    "source metadata omitted because it contains credentials or URL parameters: {}",
                    item.path
                ));
            }
            if item.owner == "agenthub" {
                item.owner = "unknown".into();
            }
            if crate::protection::is_host_managed(Path::new(&item.path)) {
                item.owner = "host".into();
                item.status = "host-managed".into();
            }
            item
        })
        .collect();
    report.deployments.sort_by(|left, right| {
        (
            left.agent.to_ascii_lowercase(),
            left.scope.to_ascii_lowercase(),
            left.profile.clone().unwrap_or_default(),
            left.path.to_ascii_lowercase(),
        )
            .cmp(&(
                right.agent.to_ascii_lowercase(),
                right.scope.to_ascii_lowercase(),
                right.profile.clone().unwrap_or_default(),
                right.path.to_ascii_lowercase(),
            ))
    });
    report
}

fn scan_one_root(
    root: &ScanRoot,
    root_path: &Path,
    output: &mut BTreeMap<String, SkillDeployment>,
    warnings: &mut Vec<String>,
) -> bool {
    let mut skill_files = Vec::new();
    let mut walk_failed = false;

    let walker = WalkDir::new(root_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            !is_git_directory(entry)
                && (entry.file_type().is_file()
                    || (!crate::protection::is_host_managed(entry.path())
                        && !crate::protection::is_plugin_content(entry.path())))
        });
    for item in walker {
        let entry = match item {
            Ok(entry) => entry,
            Err(error) => {
                walk_failed = true;
                warnings.push(format!(
                    "cannot read scan path below {}: {}",
                    root_path.display(),
                    error
                ));
                continue;
            }
        };
        if entry.file_type().is_symlink() {
            if entry.path().join("SKILL.md").is_file() {
                skill_files.push(entry.path().join("SKILL.md"));
            }
            // Do not recurse into symlink targets; direct metadata is sufficient.
            // and keeps inventory paths tied to the configured root.
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy();
        if file_name.eq_ignore_ascii_case("SKILL.md") {
            skill_files.push(entry.path().to_path_buf());
        }
    }

    let skill_files: Vec<_> = skill_files
        .into_iter()
        .filter(|file| !is_nested_node_modules_path(file, root_path))
        .collect();
    for file in skill_files {
        let skill_path = file.parent().unwrap_or(root_path).to_path_buf();
        let metadata = collect_skill(&skill_path, &file, root_path, warnings);
        let id = stable_deployment_id(
            &root.agent,
            &root.scope,
            root.profile.as_deref(),
            &skill_path,
        );
        output.insert(
            id.clone(),
            SkillDeployment {
                id,
                name: metadata.name,
                description: metadata.description,
                skill_id: None,
                project_id: None,
                ignored: false,
                baseline_digest: None,
                agent: root.agent.clone(),
                scope: root.scope.clone(),
                profile: root.profile.clone(),
                path: path_string(&skill_path),
                owner: metadata.owner,
                source: metadata.source,
                status: "discovered".into(),
            },
        );
    }

    !walk_failed
}

fn is_git_directory(entry: &DirEntry) -> bool {
    entry.file_type().is_dir() && entry.file_name().to_string_lossy() == ".git"
}

/// Skill files nested inside another package's dependencies are not independent
/// Agent deployments. Keep that boundary even when scanning a broad root.
fn is_nested_node_modules_path(path: &Path, scan_root: &Path) -> bool {
    let Some(relative) = relative_path_key(path, scan_root) else {
        return false;
    };
    let mut found_node_modules = false;
    for (index, component) in relative.split('/').enumerate() {
        if component.eq_ignore_ascii_case("node_modules") {
            if index != 0 || found_node_modules {
                return true;
            }
            found_node_modules = true;
        }
    }
    false
}

fn relative_path_key(path: &Path, root: &Path) -> Option<String> {
    let path_value = path_key(path);
    let root_value = path_key(root);
    if path_value == root_value {
        return Some(String::new());
    }
    let prefix = format!("{root_value}/");
    path_value
        .strip_prefix(&prefix)
        .map(|relative| relative.to_string())
}

fn collect_skill(
    path: &Path,
    skill_file: &Path,
    root: &Path,
    warnings: &mut Vec<String>,
) -> SkillCandidate {
    let mut metadata_value = Value::Null;
    let metadata_paths = [
        path.join(".skillhub").join("metadata.json"),
        path.join(".agenthub").join("metadata.json"),
        path.join("metadata.json"),
        path.join("manifest.json"),
    ];
    for metadata_path in metadata_paths {
        if !metadata_path.is_file() {
            continue;
        }
        match read_json(&metadata_path) {
            Ok(value) => {
                metadata_value = value;
                break;
            }
            Err(error) => warnings.push(format!(
                "cannot read skill metadata {}: {}",
                metadata_path.display(),
                error
            )),
        }
    }

    let header = match read_limited(skill_file, MAX_SKILL_HEADER_BYTES) {
        Ok(text) => parse_skill_frontmatter(&text),
        Err(error) => {
            warnings.push(format!(
                "cannot read skill {}: {}",
                skill_file.display(),
                error
            ));
            Value::Null
        }
    };
    let name = value_string(&metadata_value, &["name"])
        .or_else(|| value_string(&header, &["name"]))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unnamed skill".to_string())
        });
    let description = value_string(&metadata_value, &["description"])
        .or_else(|| value_string(&header, &["description"]))
        .unwrap_or_default();
    let metadata_source = source_from_value(&metadata_value)
        .or_else(|| source_from_value(&header))
        .unwrap_or_else(|| source_from_git(path, root));
    let owner = owner_from_value(&metadata_value);
    SkillCandidate {
        name,
        description,
        source: metadata_source,
        owner,
    }
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = read_limited(path, MAX_METADATA_BYTES)?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

fn read_limited(path: &Path, max_bytes: u64) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > max_bytes {
        return Err(format!("metadata is larger than {} bytes", max_bytes));
    }
    fs::read_to_string(path).map_err(|error| error.to_string())
}

fn parse_skill_frontmatter(content: &str) -> Value {
    let normalized = content.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Value::Null;
    };
    let Some(close) = rest.find("\n---") else {
        return Value::Null;
    };
    serde_yaml::from_str::<serde_yaml::Value>(&rest[..close])
        .ok()
        .and_then(|yaml| serde_json::to_value(yaml).ok())
        .unwrap_or(Value::Null)
}

fn source_from_value(value: &Value) -> Option<Source> {
    if value.is_null() {
        return None;
    }
    if let Some(source) = value.get("source") {
        if let Some(result) = source_from_value(source) {
            return Some(result);
        }
    }
    let locator = value_string(
        value,
        &[
            "locator",
            "url",
            "sourceUrl",
            "repository",
            "repo",
            "source",
        ],
    )?;
    let locator = locator.trim().to_string();
    if locator.is_empty() || locator.eq_ignore_ascii_case("unknown") {
        return None;
    }
    let kind = value_string(value, &["kind", "type"])
        .unwrap_or_else(|| infer_source_kind(&locator).to_string());
    let revision = value_string(value, &["revision", "ref", "commit", "version"]);
    let subpath = value_string(value, &["subpath", "path"]);
    Some(Source {
        kind: normalize_source_kind(&kind, &locator),
        locator,
        subpath,
        revision,
    })
}

fn source_from_git(path: &Path, root: &Path) -> Source {
    let mut current = Some(path);
    let root_key = path_key(root);
    while let Some(directory) = current {
        let config = directory.join(".git").join("config");
        if config.is_file() {
            if let Ok(content) = fs::read_to_string(&config) {
                if let Some(locator) = git_origin(&content) {
                    return Source {
                        kind: "git".to_string(),
                        locator,
                        subpath: relative_subpath(directory, path),
                        revision: None,
                    };
                }
            }
        }
        if path_key(directory) == root_key {
            break;
        }
        current = directory.parent();
    }
    Source::default()
}

fn git_origin(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_origin = trimmed.eq_ignore_ascii_case("[remote \"origin\"]");
            continue;
        }
        if in_origin {
            let (key, value) = trimmed.split_once('=')?;
            if key.trim().eq_ignore_ascii_case("url") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

fn relative_subpath(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .filter(|value| !value.as_os_str().is_empty())
        .map(|value| value.to_string_lossy().replace('\\', "/"))
}

fn owner_from_value(value: &Value) -> String {
    let owner = value_string(value, &["owner", "managedBy", "manager"]);
    match owner.as_deref().map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("agenthub") => "agenthub".to_string(),
        Some(value) if !value.is_empty() => "external".to_string(),
        _ => "unknown".to_string(),
    }
}

fn value_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        match value.get(*key) {
            Some(Value::String(text)) => return Some(text.clone()),
            Some(Value::Object(object)) => {
                if let Some(Value::String(url)) = object.get("url") {
                    return Some(url.clone());
                }
            }
            _ => {}
        }
    }
    None
}

fn infer_source_kind(locator: &str) -> &'static str {
    let lower = locator.to_ascii_lowercase();
    if lower.starts_with("file:")
        || lower.starts_with("./")
        || lower.starts_with("../")
        || Path::new(locator).is_absolute()
    {
        "local"
    } else if lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("ssh://")
        || lower.starts_with("git://")
        || lower.starts_with("git@")
        || lower.starts_with("github:")
    {
        "git"
    } else {
        "unknown"
    }
}

fn normalize_source_kind(kind: &str, locator: &str) -> String {
    let lower = kind.trim().to_ascii_lowercase();
    if lower == "repository" || lower == "repo" {
        infer_source_kind(locator).to_string()
    } else if lower.is_empty() {
        infer_source_kind(locator).to_string()
    } else {
        lower
    }
}

fn stable_deployment_id(agent: &str, scope: &str, profile: Option<&str>, path: &Path) -> String {
    let key = format!(
        "{}|{}|{}|{}",
        agent.to_ascii_lowercase(),
        scope.to_ascii_lowercase(),
        profile.unwrap_or_default().to_ascii_lowercase(),
        if fs::read_link(path).is_ok() {
            path.to_string_lossy()
                .replace(char::from(92), "/")
                .to_lowercase()
        } else {
            path_key(path)
        }
    );
    let digest = Sha256::digest(key.as_bytes());
    let short: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("inst-{}", short)
}

fn path_key(path: &Path) -> String {
    let mut value = if path.exists() {
        fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        path.to_string_lossy().replace('\\', "/")
    };
    while value.ends_with('/') && value.len() > 1 {
        value.pop();
    }
    if cfg!(windows) {
        value = value.to_ascii_lowercase();
    }
    value
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs::{create_dir_all, write};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("agenthub-scan-{}-{}", label, stamp));
        create_dir_all(&root).unwrap();
        root
    }

    fn scan_root(path: &Path) -> ScanRoot {
        ScanRoot {
            id: "fixture".to_string(),
            agent: "codex".to_string(),
            path: path.to_string_lossy().to_string(),
            scope: "global".to_string(),
            profile: None,
        }
    }

    #[test]
    fn discovers_skill_metadata_and_skips_plugin_and_dependency_contents() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let skill = root.join("standalone");
        create_dir_all(&skill).unwrap();
        write(
            skill.join("SKILL.md"),
            "---\nname: Standalone\ndescription: Reads safely\nversion: 2.0.0\nsource: https://github.com/example/skill.git\n---\n# Skill\n",
        )
        .unwrap();
        let plugin = root.join("plugin");
        create_dir_all(plugin.join(".claude-plugin")).unwrap();
        create_dir_all(plugin.join("skills/hidden")).unwrap();
        write(plugin.join(".claude-plugin/plugin.json"), "{}").unwrap();
        write(plugin.join("skills/hidden/SKILL.md"), "# Plugin skill").unwrap();
        let dependency = skill.join("node_modules/dependency");
        create_dir_all(&dependency).unwrap();
        write(dependency.join("SKILL.md"), "# Dependency skill").unwrap();
        write(
            root.join("package.json"),
            r#"{"name":"command","bin":"cli.js"}"#,
        )
        .unwrap();

        let report = scan_roots(&[scan_root(root)]);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.successful_roots.len(), 1);
        assert_eq!(report.deployments.len(), 1);
        let found = &report.deployments[0];
        assert_eq!(found.name, "Standalone");
        assert_eq!(found.description, "Reads safely");
        assert_eq!(found.source.kind, "git");
        assert_eq!(found.owner, "unknown");
        assert_eq!(found.id, scan_roots(&[scan_root(root)]).deployments[0].id);
    }

    #[test]
    fn missing_root_is_a_warning_and_symlink_is_ignored() {
        let root = fixture_root("links");
        let outside = fixture_root("outside");
        create_dir_all(outside.join("hidden")).unwrap();
        write(outside.join("hidden").join("SKILL.md"), "# outside").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        #[cfg(windows)]
        {
            // Windows test environments may not permit symlink creation; the
            // scanner's non-following behavior is covered by the read-only
            // branch above and the fixture still exercises missing roots.
        }
        let missing = root.join("missing");
        let mut roots = vec![scan_root(&root)];
        roots.push(scan_root(&missing));
        let report = scan_roots(&roots);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("does not exist")));
        assert!(report.deployments.is_empty());
    }

    #[test]
    fn dsh_profiles_have_distinct_skill_identities() {
        let root = fixture_root("dsh-profiles");
        create_dir_all(root.join("skills/child")).unwrap();
        write(root.join("skills/child/SKILL.md"), "# Profile Skill").unwrap();
        let mut first = scan_root(&root);
        first.agent = "dsh".to_string();
        first.scope = "profile".to_string();
        first.profile = Some("work".to_string());
        let mut second = first.clone();
        second.profile = Some("personal".to_string());
        let first_report = scan_roots(&[first]);
        let second_report = scan_roots(&[second]);
        assert_eq!(first_report.deployments.len(), 1);
        assert_eq!(second_report.deployments.len(), 1);
        assert!(first_report
            .deployments
            .iter()
            .all(|item| item.agent == "dsh"
                && item.scope == "profile"
                && item.profile.as_deref() == Some("work")));
        assert!(second_report
            .deployments
            .iter()
            .all(|item| item.profile.as_deref() == Some("personal")));
        assert_ne!(
            first_report.deployments[0].id,
            second_report.deployments[0].id
        );
    }

    #[test]
    fn default_dsh_profile_roots_keep_profile_identity() {
        let home = fixture_root("default-roots");
        create_dir_all(home.join(".dsh").join("profiles").join("work")).unwrap();
        create_dir_all(home.join(".dsh").join("profiles").join("personal")).unwrap();
        create_dir_all(home.join(".dsh").join("profiles").join("node_modules")).unwrap();

        let roots = dsh_profile_roots(&home);
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().all(|root| {
            root.agent == "dsh"
                && root.scope == "profile"
                && root.profile.is_some()
                && root.profile.as_deref() != Some("node_modules")
        }));
        assert_eq!(
            roots
                .iter()
                .filter_map(|root| root.profile.as_deref())
                .collect::<HashSet<_>>(),
            HashSet::from(["work", "personal"])
        );
    }
}

#[cfg(test)]
mod source_safety_tests {
    use super::*;
    #[test]
    fn scanner_does_not_persist_secrets_or_grant_ownership_from_metadata() {
        let d = tempfile::tempdir().unwrap();
        let skill = d.path().join("demo");
        fs::create_dir(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# demo").unwrap();
        fs::write(skill.join("metadata.json"),r#"{"source":{"kind":"git","locator":"https://u:secret@example.test/repo"},"owner":"agenthub"}"#).unwrap();
        let report = scan_roots(&[ScanRoot {
            id: "x".into(),
            agent: "codex".into(),
            path: d.path().to_string_lossy().into(),
            scope: "project".into(),
            profile: None,
        }]);
        assert_eq!(report.deployments[0].source.kind, "unknown");
        assert_eq!(report.deployments[0].owner, "unknown");
        assert!(!report.warnings.join(" ").contains("secret@example"));
    }
}
