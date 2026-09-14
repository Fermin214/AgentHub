use agenthub_core::{
    model::{LocalProject, SkillDeployment, Source},
    scan, skills,
    store::Store,
    targets,
};
use serde_json::{json, Value};
use std::{ffi::OsString, fs, path::Path};

fn skill(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::write(path.join("SKILL.md"), "---\nname: Fixture\n---\nSkill\n").unwrap();
}
fn empty_store(data: &Path) -> Store {
    let mut store = Store::open(data).unwrap();
    let mut settings = store.get_settings().unwrap();
    settings.scan_roots.clear();
    store.save_settings(&settings).unwrap();
    store
}
fn run(store: &Store, args: Value) -> Value {
    skills::dispatch(store, "skills.scan", &args).unwrap()
}
fn has(value: &Value, agent: &str, path: &Path) -> bool {
    value["deployments"].as_array().unwrap().iter().any(|row| {
        row["agent"] == agent
            && agenthub_core::protection::path_key(Path::new(row["path"].as_str().unwrap()))
                == agenthub_core::protection::path_key(path)
    })
}
fn target(store: &Store, id: &str, path: &Path, enabled: bool) {
    targets::dispatch(store,"targets.save",&json!({"target":{"id":id,"name":id,"globalPath":path,"projectPath":format!(".{id}/skills"),"enabled":enabled}})).unwrap();
}

#[test]
fn saved_empty_roots_and_explicit_empty_scan_do_not_expand_user_directories() {
    let temp = tempfile::tempdir().unwrap();
    let store = empty_store(&temp.path().join("data"));
    targets::list(&store).unwrap();
    let scanned = run(&store, json!({}));
    assert_eq!(scanned["totalRoots"], 0);
    assert_eq!(scanned["deployments"], json!([]));
    let configured = temp.path().join("codex/skills");
    skill(&configured.join("fixture"));
    target(&store, "codex", &configured, true);
    assert_eq!(run(&store, json!({"scanRoots":[]}))["totalRoots"], 0);
}

#[test]
fn disabled_targets_do_not_return_as_new_discoveries() {
    let temp = tempfile::tempdir().unwrap();
    let store = empty_store(&temp.path().join("data"));
    let codex = temp.path().join("codex/skills");
    let claude = temp.path().join("claude/skills");
    skill(&codex.join("before"));
    skill(&claude.join("disabled"));
    target(&store, "codex", &codex, true);
    target(&store, "claude", &claude, false);
    let found = run(&store, json!({}));
    assert!(has(&found, "codex", &codex.join("before")));
    assert!(!has(&found, "claude", &claude.join("disabled")));
    assert!(claude.join("disabled/SKILL.md").is_file());
}

#[test]
fn projects_discover_codex_and_hermes_shared_location_without_generic_shared_identity() {
    let temp = tempfile::tempdir().unwrap();
    let store = empty_store(&temp.path().join("data"));
    let project = temp.path().join("project");
    let shared = project.join(".agents/skills/fixture");
    skill(&shared);
    let legacy = project.join(".codex/skills/legacy");
    skill(&legacy);
    store
        .save_project(&LocalProject {
            id: "project".into(),
            name: "Project".into(),
            path: project.to_string_lossy().into(),
            ..Default::default()
        })
        .unwrap();
    let scanned = run(&store, json!({}));
    assert!(has(&scanned, "codex", &shared));
    assert!(has(&scanned, "hermes", &shared));
    assert!(has(&scanned, "codex", &legacy));
    assert!(!scanned["deployments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["agent"] == "shared"));
    assert!(scanned["deployments"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["projectId"] == "project"));
}

#[test]
fn repeated_scans_keep_ids_sources_and_distinct_profile_records() {
    let temp = tempfile::tempdir().unwrap();
    let store = empty_store(&temp.path().join("data"));
    let root = temp.path().join("skills");
    skill(&root.join("fixture"));
    let source = Source {
        kind: "git".into(),
        locator: "https://github.com/example/confirmed.git".into(),
        ..Default::default()
    };
    for profile in [None, Some("work"), Some("personal")] {
        store
            .save_deployment(&SkillDeployment {
                id: format!("stored-{}", profile.unwrap_or("global")),
                name: "Fixture".into(),
                agent: "codex".into(),
                scope: if profile.is_some() {
                    "profile"
                } else {
                    "global"
                }
                .into(),
                profile: profile.map(String::from),
                path: root.join("fixture").to_string_lossy().into(),
                source: source.clone(),
                owner: "agenthub".into(),
                ignored: true,
                baseline_digest: Some("confirmed-baseline".into()),
                ..Default::default()
            })
            .unwrap();
    }
    fs::write(root.join("fixture/metadata.json"),r#"{"source":{"kind":"git","locator":"https://github.com/example/unconfirmed.git"},"owner":"unknown"}"#).unwrap();
    for _ in 0..2 {
        let scanned = run(&store, json!({}));
        let rows = scanned["deployments"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "{scanned}");
        for row in rows {
            assert!(row["id"].as_str().unwrap().starts_with("stored-"));
            assert_eq!(row["source"], json!(source));
            assert_eq!(row["owner"], "agenthub");
            assert_eq!(row["baselineDigest"], "confirmed-baseline");
            assert_eq!(row["ignored"], true);
        }
    }
    fs::remove_dir_all(&root).unwrap();
    let scanned = run(
        &store,
        json!({"scanRoots":[{"agent":"codex","scope":"global","path":root}]}),
    );
    assert_eq!(scanned["completedRoots"], 0);
    assert_eq!(scanned["deployments"].as_array().unwrap().len(), 3);
    assert!(scanned["deployments"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["status"] == "installed" && row["present"] == false));
}

#[test]
fn host_and_plugin_skills_remain_unmodified_and_are_not_imported_independently() {
    let temp = tempfile::tempdir().unwrap();
    let store = empty_store(&temp.path().join("data"));
    let root = temp.path().join("root");
    let plugin = root.join("plugin");
    let plugin_skill = plugin.join("skills/fixture");
    skill(&plugin_skill);
    fs::create_dir(plugin.join(".claude-plugin")).unwrap();
    fs::write(plugin.join(".claude-plugin/plugin.json"), "{}").unwrap();
    let host = root.join(".codex/plugins/fixture");
    skill(&host);
    let scanned = run(
        &store,
        json!({"scanRoots":[{"agent":"codex","scope":"global","path":root}]}),
    );
    assert_eq!(scanned["deployments"], json!([]));
    for (id, path) in [("plugin", &plugin_skill), ("host", &host)] {
        store
            .save_deployment(&SkillDeployment {
                id: id.into(),
                name: "Fixture".into(),
                agent: "codex".into(),
                scope: "global".into(),
                path: path.to_string_lossy().into(),
                owner: "external".into(),
                ..Default::default()
            })
            .unwrap();
    }
    let imported = skills::dispatch(
        &store,
        "skills.import",
        &json!({"deploymentIds":["plugin","host"]}),
    )
    .unwrap();
    assert!(imported["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["status"] == "failed"));
    assert!(store.list_skills().unwrap().is_empty());
    assert!(plugin_skill.join("SKILL.md").is_file());
    assert!(host.join("SKILL.md").is_file());
}

#[test]
fn agent_display_order_uses_names_and_custom_targets_are_not_created() {
    let temp = tempfile::tempdir().unwrap();
    let store = empty_store(&temp.path().join("data"));
    let all = targets::list(&store).unwrap();
    let names: Vec<_> = all.iter().map(|t| t.name.to_lowercase()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert!(targets::dispatch(&store,"targets.save",&json!({"target":{"id":format!("custom-{}",uuid::Uuid::new_v4()),"name":"Custom","globalPath":temp.path().join("skills"),"projectPath":".agents/skills","enabled":true}})).is_err());
}

#[test]
fn git_metadata_subpath_is_relative_to_repository_not_the_broader_scan_root() {
    let temp = tempfile::tempdir().unwrap();
    let store = empty_store(&temp.path().join("data"));
    let root = temp.path().join("skills");
    let repository = root.join("repository");
    skill(&repository.join("skills/fixture"));
    fs::create_dir(repository.join(".git")).unwrap();
    fs::write(
        repository.join(".git/config"),
        "[remote \"origin\"]\nurl = https://github.com/example/skills.git\n",
    )
    .unwrap();
    let scanned = run(
        &store,
        json!({"scanRoots":[{"agent":"codex","scope":"global","path":root}]}),
    );
    assert_eq!(scanned["deployments"][0]["source"]["kind"], "git");
    assert_eq!(
        scanned["deployments"][0]["source"]["subpath"],
        "skills/fixture"
    );
}

struct EnvGuard(Vec<(&'static str, Option<OsString>)>);
impl EnvGuard {
    fn new(keys: &[&'static str]) -> Self {
        Self(
            keys.iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        )
    }
}
impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[test]
#[serial_test::serial]
fn hermes_profile_home_external_tilde_environment_and_relative_paths_are_all_preserved() {
    let _environment = EnvGuard::new(&[
        "USERPROFILE",
        "HOME",
        "LOCALAPPDATA",
        "HERMES_HOME",
        "ATB_HERMES_FIXTURE",
    ]);
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let hermes = home.join("hermes");
    std::env::set_var("USERPROFILE", home);
    std::env::set_var("HOME", home);
    std::env::set_var("LOCALAPPDATA", home);
    std::env::set_var("HERMES_HOME", hermes.join("profiles/coder"));
    std::env::set_var("ATB_HERMES_FIXTURE", home.join("env-skills"));
    for path in [
        hermes.join("skills/global"),
        hermes.join("profiles/coder/skills/coder"),
        hermes.join("profiles/research/skills/research"),
        hermes.join("profiles/coder/created/new"),
        home.join("shared/external"),
        home.join("env-skills/environment"),
    ] {
        skill(&path);
    }
    fs::write(
        hermes.join("config.yaml"),
        "skills:\n  external_dirs:\n    - ~/shared\n    - ${ATB_HERMES_FIXTURE}\n",
    )
    .unwrap();
    fs::write(
        hermes.join("profiles/coder/config.yaml"),
        "skills:\n  create_dir: created\n",
    )
    .unwrap();
    let roots: Vec<_> = scan::default_scan_roots()
        .into_iter()
        .filter(|root| root.agent == "hermes")
        .collect();
    assert_eq!(roots.len(), 6, "{roots:?}");
    let report = scan::scan_roots(&roots);
    assert_eq!(report.deployments.len(), 6);
    assert!(report.warnings.is_empty());
    assert_eq!(
        report
            .deployments
            .iter()
            .filter(|d| d.profile.as_deref() == Some("coder"))
            .count(),
        2
    );
    assert_eq!(
        report
            .deployments
            .iter()
            .filter(|d| d.profile.as_deref() == Some("research"))
            .count(),
        1
    );
    assert!(report
        .deployments
        .iter()
        .any(|d| d.path.ends_with("external")));
    assert!(report
        .deployments
        .iter()
        .any(|d| d.path.ends_with("environment")));
}
