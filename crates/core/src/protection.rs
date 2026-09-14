use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

pub fn resolved(path: &Path) -> PathBuf {
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if parent != path {
            return resolved(parent).join(name);
        }
    }
    path.to_path_buf()
}
pub fn path_key(path: &Path) -> String {
    let raw = resolved(path).to_string_lossy().replace('\\', "/");
    let raw = raw
        .trim_start_matches("//?/")
        .trim_end_matches('/')
        .to_string();
    if cfg!(windows) {
        raw.to_lowercase()
    } else {
        raw
    }
}
fn protected_spelling(path: &Path) -> bool {
    let value = format!(
        "/{}/",
        path.to_string_lossy()
            .replace('\\', "/")
            .to_lowercase()
            .trim_matches('/')
    );
    [
        "/.codex/skills/.system/",
        "/.codex/plugins/",
        "/.claude/plugins/",
        "/.dsh/plugins/",
        "/.deepseek/plugins/",
        "/.hermes/plugins/",
        "/.codex/runtime/",
        "/.codex/runtimes/",
        "/.codex/dependencies/",
        // Synced and retired Claude Skills are owned by the host.
        "/.claude/skills/synced/",
        "/.claude/skills/.trash/",
    ]
    .iter()
    .any(|part| value.contains(part))
}
pub fn is_host_managed(path: &Path) -> bool {
    if bundled_skill(path)
        || hermes_origin(path).is_some_and(|(kind, _)| kind == "hermes-official-optional")
    {
        return true;
    }
    if protected_spelling(path) || protected_spelling(&resolved(path)) {
        return true;
    }
    let homes = [
        std::env::var_os("CODEX_HOME").map(PathBuf::from),
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|p| PathBuf::from(p).join(".codex")),
    ];
    let key = path_key(path);
    homes.into_iter().flatten().any(|home| {
        [
            "skills/.system",
            "plugins",
            "runtime",
            "runtimes",
            "dependencies",
        ]
        .iter()
        .any(|child| {
            let root = path_key(&home.join(child));
            key == root || key.starts_with(&(root.clone() + "/"))
        })
    })
}
/// Classify Hermes resources from local provenance, never from the Agent name alone.
pub fn hermes_origin(path: &Path) -> Option<(&'static str, &'static str)> {
    if bundled_skill(path) {
        return Some(("hermes-bundled", "Hermes 内置清单或随包 skills 目录"));
    }
    let actual = resolved(path);
    for root in actual
        .ancestors()
        .take(12)
        .filter(|p| p.file_name().is_some_and(|n| n == "skills"))
    {
        let home = root.parent()?;
        let hub = root.join(".hub/lock.json");
        let context = root.join(".bundled_manifest").is_file()
            || home.join("hermes-agent").is_dir()
            || hub.is_file();
        if !context {
            continue;
        }
        if let Ok(meta) = std::fs::symlink_metadata(&hub) {
            if meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 1024 * 1024 {
                if let Ok(value) = std::fs::read(&hub).and_then(|bytes| {
                    serde_json::from_slice::<serde_json::Value>(&bytes)
                        .map_err(std::io::Error::other)
                }) {
                    if let Some(installed) = value["installed"].as_object() {
                        for item in installed.values() {
                            let Some(relative) = item["install_path"].as_str() else {
                                continue;
                            };
                            let relative = Path::new(relative);
                            if relative.is_absolute()
                                || !relative
                                    .components()
                                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                            {
                                continue;
                            }
                            let owned = root.join(relative);
                            let key = path_key(&owned);
                            let current = path_key(&actual);
                            if item["source"] == "official"
                                && item["identifier"]
                                    .as_str()
                                    .is_some_and(|id| id.starts_with("official/"))
                                && owned.join("SKILL.md").is_file()
                                && (current == key || current.starts_with(&(key + "/")))
                            {
                                return Some((
                                    "hermes-official-optional",
                                    "Hermes .hub/lock.json 官方可选包安装记录",
                                ));
                            }
                        }
                    }
                }
            }
        }
        for directory in actual.ancestors().take_while(|p| *p != root) {
            let skill = directory.join("SKILL.md");
            let Ok(meta) = std::fs::metadata(&skill) else {
                continue;
            };
            if !meta.is_file() || meta.len() > 1024 * 1024 {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(skill) else {
                continue;
            };
            let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
            let Some(front) = text
                .strip_prefix("---\n")
                .and_then(|s| s.split_once("\n---").map(|(v, _)| v))
            else {
                continue;
            };
            let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(front) else {
                continue;
            };
            if value["metadata"]["hermes"]["created_by"].as_str() == Some("agent") {
                return Some((
                    "hermes-generated",
                    "SKILL.md 声明 metadata.hermes.created_by: agent",
                ));
            }
            if value["metadata"]["author"]
                .as_str()
                .or_else(|| value["author"].as_str())
                .is_some_and(|author| author.replace(' ', "").eq_ignore_ascii_case("HermesAgent"))
            {
                return Some((
                    "hermes-unconfirmed",
                    "仅有 Hermes Agent 作者声明，未找到内置或官方可选包记录",
                ));
            }
        }
    }
    None
}
// Hermes records bundled Skill directory names in `skills/.bundled_manifest`
// as `name:hash`. It may place them inside category directories. Keep those
// resources read-only even when discovered through a user-configured target.
fn bundled_skill(path: &Path) -> bool {
    let actual = resolved(path);
    for root in actual.ancestors().take(12) {
        if root.file_name().is_none_or(|name| name != "skills") {
            continue;
        }
        let manifest = root.join(".bundled_manifest");
        let Ok(metadata) = std::fs::symlink_metadata(&manifest) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        for directory in actual.ancestors().take_while(|p| *p != root) {
            if let (Some(home), Ok(relative)) = (root.parent(), directory.strip_prefix(root)) {
                if home
                    .join("hermes-agent/skills")
                    .join(relative)
                    .join("SKILL.md")
                    .is_file()
                {
                    return true;
                }
            }
            let Some(name) = directory.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if text.lines().any(|line| {
                line.split_once(':').is_some_and(|(n, hash)| {
                    n == name && !hash.is_empty() && hash.chars().all(|c| c.is_ascii_hexdigit())
                })
            }) {
                return true;
            }
        }
    }
    false
}
/// Plugin manifests identify ownership even before the first inventory scan.
pub fn is_plugin_content(path: &Path) -> bool {
    [path.to_path_buf(), resolved(path)].iter().any(|path| {
        path.ancestors().any(|parent| {
            [
                ".claude-plugin",
                ".codex-plugin",
                ".deepseek-plugin",
                ".dsh-plugin",
            ]
            .iter()
            .any(|marker| parent.join(marker).is_dir())
                || parent.join("plugin.json").is_file()
                || parent.join("plugin.yaml").is_file()
        })
    })
}

pub fn ensure_independent_skill(path: &Path) -> Result<()> {
    ensure_mutable(path)?;
    if is_plugin_content(path) {
        bail!("plugin-provided content is managed by its parent plugin and cannot be managed independently");
    }
    Ok(())
}

pub fn ensure_mutable(path: &Path) -> Result<()> {
    let key = path_key(path);
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|p| PathBuf::from(p).join(".codex"))
        });
    let ancestor = home.is_some_and(|home| {
        [
            "skills/.system",
            "plugins",
            "runtime",
            "runtimes",
            "dependencies",
        ]
        .iter()
        .any(|child| path_key(&home.join(child)).starts_with(&(key.clone() + "/")))
    });
    let spelling = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let lexical_ancestor = [
        "/.codex",
        "/.codex/skills",
        "/.codex/plugins",
        "/.claude",
        "/.claude/skills",
        "/.claude/plugins",
    ]
    .iter()
    .any(|suffix| spelling.trim_end_matches('/').ends_with(suffix));
    let system_roots = [
        "SystemRoot",
        "WINDIR",
        "ProgramFiles",
        "ProgramW6432",
        "ProgramFiles(x86)",
    ]
    .iter()
    .filter_map(std::env::var_os)
    .map(PathBuf::from)
    .chain([
        PathBuf::from("C:/Windows"),
        PathBuf::from("C:/Program Files"),
        PathBuf::from("C:/Program Files (x86)"),
    ]);
    let system = system_roots.into_iter().any(|root| {
        let root = path_key(&root);
        key == root || key.starts_with(&(root + "/"))
    });
    if is_host_managed(path) || ancestor || lexical_ancestor || system {
        bail!(
            "blocked: host-managed resource is read-only: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_code_host_state_is_read_only_but_user_skills_stay_manageable() {
        // The agenthub's whole purpose for this Agent is managing Skills under
        // `.claude/skills`, so that path itself must remain writable.
        for manageable in [
            r"C:\Users\example\.claude\skills\writing-check",
            r"C:\Users\example\.claude\skills\writing-check\SKILL.md",
            r"C:\Projects\demo\.claude\skills\project-skill",
        ] {
            assert!(
                !is_host_managed(Path::new(manageable)),
                "{manageable} must stay manageable"
            );
            assert!(
                ensure_mutable(Path::new(manageable)).is_ok(),
                "{manageable} must stay mutable"
            );
        }
        // Claude Code re-syncs, retires, and caches these itself.
        for protected in [
            r"C:\Users\example\.claude\skills\synced\from-claude-ai",
            r"C:\Users\example\.claude\skills\.trash\retired",
            r"C:\Users\example\.claude\plugins\synced\some-plugin",
            r"C:\Users\example\.claude\plugins\.trash\old-plugin",
            r"C:\Users\example\.claude\plugins\marketplaces\claude-plugins-official",
            r"C:\Users\example\.claude\plugins\cache\claude-plugins-official\agent-sdk-dev\3deb821cb71c",
            r"C:\Users\example\.claude\plugins\data\some-plugin",
        ] {
            assert!(
                is_host_managed(Path::new(protected)),
                "{protected} must be host-managed"
            );
            assert!(
                ensure_mutable(Path::new(protected)).is_err(),
                "{protected} must be read-only"
            );
        }
        // The container directories themselves are never operated on directly.
        for container in [
            r"C:\Users\example\.claude",
            r"C:\Users\example\.claude\skills",
            r"C:\Users\example\.claude\plugins",
        ] {
            assert!(ensure_mutable(Path::new(container)).is_err(), "{container}");
        }
    }

    #[test]
    fn hermes_optional_generated_and_unconfirmed_have_distinct_provenance() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("skills");
        std::fs::create_dir_all(root.join(".hub")).unwrap();
        for name in ["creative/sketch", "generated", "petdex", "personal"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
            std::fs::write(root.join(name).join("SKILL.md"), "# Skill").unwrap();
        }
        std::fs::write(root.join(".hub/lock.json"),r#"{"installed":{"sketch":{"source":"official","identifier":"official/creative/sketch","install_path":"creative/sketch"},"bad":{"source":"official","identifier":"official/bad","install_path":"../personal"}}}"#).unwrap();
        std::fs::write(
            root.join("generated/SKILL.md"),
            "---\nmetadata:\n  hermes:\n    created_by: agent\n---\n# Generated",
        )
        .unwrap();
        std::fs::write(
            root.join("petdex/SKILL.md"),
            "---\nmetadata:\n  author: Hermes Agent\n---\n# Petdex",
        )
        .unwrap();
        assert_eq!(
            hermes_origin(&root.join("creative/sketch")).unwrap().0,
            "hermes-official-optional"
        );
        assert!(is_host_managed(&root.join("creative/sketch")));
        assert_eq!(
            hermes_origin(&root.join("generated")).unwrap().0,
            "hermes-generated"
        );
        assert_eq!(
            hermes_origin(&root.join("petdex")).unwrap().0,
            "hermes-unconfirmed"
        );
        assert!(!is_host_managed(&root.join("generated")));
        assert!(hermes_origin(&root.join("personal")).is_none());
    }
    #[test]
    fn hermes_bundled_skills_are_protected_but_user_skills_remain_manageable() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("skills");
        for relative in ["category/bundled", "category/new-bundled", "personal/mine"] {
            std::fs::create_dir_all(root.join(relative)).unwrap();
            std::fs::write(root.join(relative).join("SKILL.md"), "# Skill").unwrap();
        }
        std::fs::write(root.join(".bundled_manifest"), "bundled:abc123\n").unwrap();
        let installed = temp.path().join("hermes-agent/skills/category/new-bundled");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(installed.join("SKILL.md"), "# Skill").unwrap();
        assert!(is_host_managed(&root.join("category/bundled")));
        assert!(ensure_mutable(&root.join("category/bundled/SKILL.md")).is_err());
        assert!(is_host_managed(&root.join("category/new-bundled")));
        assert!(!is_host_managed(&root.join("personal/mine")));
        assert!(ensure_mutable(&root.join("personal/mine")).is_ok());
    }
}
