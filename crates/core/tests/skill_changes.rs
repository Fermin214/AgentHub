use agenthub_core::{
    model::{LocalProject, Skill, SkillDeployment},
    safe_files, skill_changes, skills,
    store::Store,
    targets,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    root: TempDir,
    store: Store,
    skill: Skill,
    upstream: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(&root.path().join("data")).unwrap();
        for mut target in targets::list(&store).unwrap() {
            target.enabled = false;
            targets::dispatch(&store, "targets.save", &json!({"target":target})).unwrap();
        }
        let upstream = root.path().join("upstream");
        write_skill(&upstream, "original");
        let inspected = agenthub_core::dispatch(
            store.data_dir(),
            "sources.inspect",
            json!({"source":{"kind":"local","locator":upstream}}),
        )
        .unwrap();
        let added = agenthub_core::dispatch(
            store.data_dir(),
            "skills.add",
            json!({"inspectionId":inspected["inspectionId"],"subpath":""}),
        )
        .unwrap();
        let skill: Skill = serde_json::from_value(added["skill"].clone()).unwrap();
        Self {
            root,
            store,
            skill,
            upstream,
        }
    }
    fn target(&self, id: &str, relative: &str, project: &str) {
        targets::dispatch(&self.store,"targets.save",&json!({"target":{"id":id,"name":id,"globalPath":self.root.path().join(relative),"projectPath":project,"enabled":true}})).unwrap();
    }
    fn preview(&self, action: &str, args: Value) -> Value {
        skill_changes::dispatch(&self.store, &format!("skills.{action}.preview"), &args).unwrap()
    }
    fn execute(&self, action: &str, plan: &Value) -> Value {
        skill_changes::dispatch(
            &self.store,
            &format!("skills.{action}"),
            &json!({"planId":plan["id"],"confirmed":true}),
        )
        .unwrap()
    }
    fn install(&self, target: &str) -> Value {
        let plan = self.preview(
            "install",
            json!({"skillId":self.skill.id,"targetId":target}),
        );
        self.execute("install", &plan)
    }
    fn deployment(&self, agent: &str) -> SkillDeployment {
        self.store
            .list_deployments()
            .unwrap()
            .into_iter()
            .find(|item| item.agent == agent)
            .unwrap()
    }
    fn backups(&self) -> Vec<Value> {
        skill_changes::dispatch(&self.store, "backups.list", &json!({})).unwrap()["backups"]
            .as_array()
            .unwrap()
            .clone()
    }
    fn check(&self) -> Value {
        skills::dispatch(
            &self.store,
            "skills.check",
            &json!({"skillId":self.skill.id}),
        )
        .unwrap()
    }
}
fn write_skill(path: &Path, content: &str) {
    fs::create_dir_all(path.join("references")).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: sample-skill\n---\n{content}\n"),
    )
    .unwrap();
    fs::write(path.join("references/guide.md"), content).unwrap();
}
fn text(path: &str) -> String {
    fs::read_to_string(Path::new(path).join("references/guide.md")).unwrap()
}

#[test]
fn a_writer_rechecks_recovery_after_an_earlier_ready_snapshot() {
    let f = Fixture::new();
    let ready = agenthub_core::dispatch(f.store.data_dir(), "snapshot", json!({})).unwrap();
    assert_eq!(ready["recovery"]["status"], "ready");
    let before = f.store.get_skill(&f.skill.id).unwrap().unwrap();
    // Another request leaves an intent after this request's entry check but
    // before its domain operation acquires the shared file-write lock.
    let conn = rusqlite::Connection::open(f.store.database_path()).unwrap();
    conn.execute(
        "INSERT INTO skill_change_plans(id,payload_json,state) VALUES(?1,'{}','executing')",
        [Uuid::new_v4().to_string()],
    )
    .unwrap();
    let result = skills::dispatch(
        &f.store,
        "skills.metadata.save",
        &json!({"ids":[f.skill.id],"favorite":true}),
    );
    assert!(
        result.is_err(),
        "a stale entry check must not admit a Skill writer"
    );
    for method in [
        "skills.add",
        "skills.import",
        "skills.scan",
        "skills.ignore",
        "skills.bindSource",
        "skills.unbindSource",
        "skills.check",
    ] {
        let error = skills::dispatch(&f.store, method, &json!({"skillId":f.skill.id})).unwrap_err();
        assert!(
            error.to_string().contains("RECOVERY_REQUIRED"),
            "{method}: {error:#}"
        );
    }
    let before_settings =
        agenthub_core::maintenance::dispatch(&f.store, "maintenance.get", &json!({})).unwrap();
    let error = agenthub_core::maintenance::dispatch(
        &f.store,
        "maintenance.save",
        &json!({"maxBackups":1}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("RECOVERY_REQUIRED"));
    assert_eq!(
        agenthub_core::maintenance::dispatch(&f.store, "maintenance.get", &json!({})).unwrap(),
        before_settings
    );
    assert_eq!(f.store.get_skill(&f.skill.id).unwrap().unwrap(), before);
}

#[test]
fn library_writes_require_the_registered_directory() {
    let f = Fixture::new();
    assert_eq!(
        f.preview("delete", json!({"skillId":f.skill.id}))["canExecute"],
        true
    );
    let unrelated = f.store.data_dir().join("library/skills/personal");
    write_skill(&unrelated, "personal content");
    let mut forged = f.skill.clone();
    forged.path = unrelated.to_string_lossy().into();
    f.store.save_skill(&forged).unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.delete.preview",
        &json!({"skillId":f.skill.id}),
    )
    .is_err());
    assert_eq!(text(&forged.path), "personal content");
    f.store.save_skill(&f.skill).unwrap();
    let conn = rusqlite::Connection::open(f.store.database_path()).unwrap();
    conn.execute(
        "DELETE FROM skill_library_paths WHERE skill_id=?1",
        [&f.skill.id],
    )
    .unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.delete.preview",
        &json!({"skillId":f.skill.id}),
    )
    .is_err());
    assert_eq!(text(&f.skill.path), "original");
}

#[test]
fn writers_recheck_changed_data_roots_under_lock() {
    let f = Fixture::new();
    agenthub_core::dispatch(f.store.data_dir(), "snapshot", json!({})).unwrap();
    let before = f.store.get_skill(&f.skill.id).unwrap().unwrap();
    let conn = rusqlite::Connection::open(f.store.database_path()).unwrap();
    conn.execute(
        "UPDATE app_settings SET value_json=?1 WHERE key='ownedDataRoot'",
        [json!(f.root.path().join("other-data-root")).to_string()],
    )
    .unwrap();
    let error = skills::dispatch(
        &f.store,
        "skills.metadata.save",
        &json!({"ids":[f.skill.id],"favorite":true}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("RECOVERY_REQUIRED"));
    assert_eq!(f.store.get_skill(&f.skill.id).unwrap().unwrap(), before);
}

#[test]
fn shared_install_and_remove_touch_one_directory_and_preserve_independent_copy() {
    let f = Fixture::new();
    f.target("codex", "shared/skills", ".agents/skills");
    f.target("claude", "shared/skills", ".agents/skills");
    f.target("dsh", "separate/skills", ".dsh/skills");
    let preview = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    assert_eq!(preview["locations"].as_array().unwrap().len(), 1);
    assert_eq!(
        preview["locations"][0]["agents"].as_array().unwrap().len(),
        2
    );
    assert!(!f.root.path().join("shared/skills/sample-skill").exists());
    f.execute("install", &preview);
    f.install("dsh");
    let codex = f.deployment("codex");
    let claude = f.deployment("claude");
    let independent = f.deployment("dsh");
    assert_eq!(codex.path, claude.path);
    assert_eq!(f.store.list_deployments().unwrap().len(), 3);
    fs::write(
        Path::new(&codex.path).join("personal.txt"),
        "personal content",
    )
    .unwrap();
    let plan = f.preview(
        "remove",
        json!({"deploymentId":codex.id,"targetPath":independent.path}),
    );
    assert_eq!(plan["locations"].as_array().unwrap().len(), 1);
    assert_eq!(plan["locations"][0]["agents"].as_array().unwrap().len(), 2);
    let removed = f.execute("remove", &plan);
    assert!(!Path::new(&codex.path).exists());
    assert!(Path::new(&independent.path).is_dir());
    assert_eq!(text(&f.skill.path), "original");
    assert_eq!(f.store.list_deployments().unwrap().len(), 1);
    skill_changes::dispatch(
        &f.store,
        "backups.restore",
        &json!({"id":removed["backupId"],"confirmed":true}),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(Path::new(&codex.path).join("personal.txt")).unwrap(),
        "personal content"
    );
    assert_eq!(f.store.list_deployments().unwrap().len(), 3);
}

#[test]
fn project_install_uses_registered_project_and_ignores_arbitrary_path() {
    let f = Fixture::new();
    f.target("codex", "global/skills", ".agents/skills");
    f.target("claude", "other/skills", ".agents/skills");
    let project = f.root.path().join("project");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("keep.txt"), "project").unwrap();
    f.store
        .save_project(&LocalProject {
            id: "project".into(),
            name: "project".into(),
            path: project.to_string_lossy().into(),
            ..Default::default()
        })
        .unwrap();
    let wrong = f.root.path().join("arbitrary");
    let plan = f.preview(
        "install",
        json!({"skillId":f.skill.id,"targetId":"codex","projectId":"project","targetPath":wrong}),
    );
    f.execute("install", &plan);
    assert!(!wrong.exists());
    assert!(project
        .join(".agents/skills/sample-skill/SKILL.md")
        .is_file());
    let rows = f.store.list_deployments().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .all(|item| item.scope == "project" && item.project_id.as_deref() == Some("project")));
    assert_eq!(
        fs::read_to_string(project.join("keep.txt")).unwrap(),
        "project"
    );
}

#[test]
fn stale_files_configuration_and_records_reject_without_overwrite() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    let destination = f.root.path().join("target/skills/sample-skill");
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    write_skill(&destination, "appeared after preview");
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    assert_eq!(
        text(destination.to_str().unwrap()),
        "appeared after preview"
    );
    let plan = f.preview(
        "install",
        json!({"skillId":f.skill.id,"targetId":"codex","replaceModified":true}),
    );
    fs::write(
        Path::new(&f.skill.path).join("references/guide.md"),
        "source changed",
    )
    .unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    let plan = f.preview(
        "install",
        json!({"skillId":f.skill.id,"targetId":"codex","replaceModified":true}),
    );
    f.target("codex", "different/skills", ".agents/skills");
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    let mut changed = f.skill.clone();
    changed.favorite = true;
    f.store.save_skill(&changed).unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    assert!(!f.root.path().join("different/skills/sample-skill").exists());
}

#[test]
fn plan_stage_tampering_expiry_cancellation_and_action_mismatch_are_rejected() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.remove",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    assert!(
        skill_changes::dispatch(&f.store, "skills.install", &json!({"planId":plan["id"]})).is_err()
    );
    let stage = f
        .store
        .data_dir()
        .join("skill-change-plans")
        .join(plan["id"].as_str().unwrap())
        .join("content/SKILL.md");
    fs::write(stage, "tampered").unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    let connection = rusqlite::Connection::open(f.store.database_path()).unwrap();
    let raw: String = connection
        .query_row(
            "SELECT payload_json FROM skill_change_plans WHERE id=?1",
            [plan["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    value["createdAt"] = json!("2000-01-01T00:00:00Z");
    connection
        .execute(
            "UPDATE skill_change_plans SET payload_json=?2 WHERE id=?1",
            rusqlite::params![plan["id"].as_str().unwrap(), value.to_string()],
        )
        .unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    skill_changes::dispatch(&f.store, "skills.cancel", &json!({"planId":plan["id"]})).unwrap();
    assert!(!f
        .store
        .data_dir()
        .join("skill-change-plans")
        .join(plan["id"].as_str().unwrap())
        .exists());
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    assert!(!f.root.path().join("target/skills/sample-skill").exists());
}

#[test]
fn delete_library_keeps_deployments_and_restore_relinks_them() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    f.install("codex");
    let deployed = f.deployment("codex");
    let plan = f.preview("delete", json!({"skillId":f.skill.id}));
    assert_eq!(plan["locations"].as_array().unwrap().len(), 1);
    let result = f.execute("delete", &plan);
    assert!(f.store.get_skill(&f.skill.id).unwrap().is_none());
    assert!(!Path::new(&f.skill.path).exists());
    assert!(Path::new(&deployed.path).exists());
    assert!(f.deployment("codex").skill_id.is_none());
    skill_changes::dispatch(
        &f.store,
        "backups.restore",
        &json!({"id":result["backupId"],"confirmed":true}),
    )
    .unwrap();
    assert_eq!(text(&f.skill.path), "original");
    assert_eq!(
        f.deployment("codex").skill_id.as_deref(),
        Some(f.skill.id.as_str())
    );
    let plan = f.preview(
        "delete",
        json!({"skillId":f.skill.id,"removeDeployments":true}),
    );
    let result = f.execute("delete", &plan);
    assert!(!Path::new(&deployed.path).exists());
    skill_changes::dispatch(
        &f.store,
        "backups.restore",
        &json!({"id":result["backupId"],"confirmed":true}),
    )
    .unwrap();
    assert!(Path::new(&deployed.path).exists());
    assert_eq!(f.store.list_deployments().unwrap().len(), 1);
}

#[test]
fn backup_content_tampering_is_rejected_before_recreating_files() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    f.install("codex");
    let deployed = f.deployment("codex");
    let plan = f.preview("remove", json!({"deploymentId":deployed.id}));
    let result = f.execute("remove", &plan);
    let backup = f
        .backups()
        .into_iter()
        .find(|backup| backup["id"] == result["backupId"])
        .unwrap();
    fs::write(
        Path::new(backup["path"].as_str().unwrap()).join("0/SKILL.md"),
        "tampered backup",
    )
    .unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "backups.restore",
        &json!({"id":result["backupId"],"confirmed":true})
    )
    .is_err());
    assert!(!Path::new(&deployed.path).exists());
    assert!(f.store.list_deployments().unwrap().is_empty());
}

#[test]
fn selected_update_changes_one_location_and_database_failure_rolls_back_all_locations() {
    let f = Fixture::new();
    f.target("codex", "first/skills", ".agents/skills");
    f.target("claude", "second/skills", ".claude/skills");
    f.install("codex");
    f.install("claude");
    let codex = f.deployment("codex");
    let claude = f.deployment("claude");
    write_skill(&f.upstream, "upstream two");
    let check = f.check();
    let chosen = check["locations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|location| {
            agenthub_core::protection::path_key(Path::new(location["path"].as_str().unwrap()))
                == agenthub_core::protection::path_key(Path::new(&codex.path))
        })
        .unwrap()["id"]
        .clone();
    let plan = f.preview(
        "update",
        json!({"skillId":f.skill.id,"checkId":check["checkId"],"locationIds":[chosen]}),
    );
    assert_eq!(plan["locations"].as_array().unwrap().len(), 1);
    f.execute("update", &plan);
    assert_eq!(text(&codex.path), "upstream two");
    assert_eq!(text(&claude.path), "original");
    assert_eq!(text(&f.skill.path), "original");
    write_skill(&f.upstream, "upstream three");
    let check = f.check();
    let before_records = f.store.list_deployments().unwrap();
    let before_skill = f.store.get_skill(&f.skill.id).unwrap();
    let before_backups = f.backups();
    let plan = f.preview(
        "update",
        json!({"skillId":f.skill.id,"checkId":check["checkId"]}),
    );
    let connection = rusqlite::Connection::open(f.store.database_path()).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_deployment_update BEFORE UPDATE ON skill_deployments BEGIN SELECT RAISE(ABORT,'injected fixture database failure'); END;").unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.update",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    assert_eq!(text(&codex.path), "upstream two");
    assert_eq!(text(&claude.path), "original");
    assert_eq!(text(&f.skill.path), "original");
    assert_eq!(f.store.list_deployments().unwrap(), before_records);
    assert_eq!(f.store.get_skill(&f.skill.id).unwrap(), before_skill);
    assert_eq!(f.backups(), before_backups);
    for base in [
        f.root.path().join("first/skills"),
        f.root.path().join("second/skills"),
        f.store.data_dir().join("library/skills"),
    ] {
        assert!(fs::read_dir(base).unwrap().all(|item| !item
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".atb-")));
    }
}

#[test]
fn changed_unselected_location_invalidates_update_preview() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    f.install("codex");
    write_skill(&f.upstream, "next");
    let check = f.check();
    let deployed = f.deployment("codex");
    fs::write(
        Path::new(&deployed.path).join("local.txt"),
        "changed since check",
    )
    .unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.update.preview",
        &json!({"skillId":f.skill.id,"checkId":check["checkId"]})
    )
    .is_err());
    assert_eq!(text(&f.skill.path), "original");
}

#[test]
fn host_and_plugin_locations_cannot_be_removed_and_unsafe_names_cannot_install() {
    let f = Fixture::new();
    for relative in [
        ".codex/skills/.system/hosted",
        ".claude/plugins/cache/plugin/skills/hosted",
    ] {
        let path = f.root.path().join(relative);
        write_skill(&path, "host");
        let deployed = SkillDeployment {
            id: Uuid::new_v4().to_string(),
            name: "hosted".into(),
            agent: "codex".into(),
            scope: "global".into(),
            path: path.to_string_lossy().into(),
            ..Default::default()
        };
        f.store.save_deployment(&deployed).unwrap();
        assert!(skill_changes::dispatch(
            &f.store,
            "skills.remove.preview",
            &json!({"deploymentId":deployed.id})
        )
        .is_err());
        assert_eq!(text(path.to_str().unwrap()), "host");
    }
    f.target("codex", "target/skills", ".agents/skills");
    let mut skill = f.skill.clone();
    skill.name = "../escape".into();
    f.store.save_skill(&skill).unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install.preview",
        &json!({"skillId":skill.id,"targetId":"codex"})
    )
    .is_err());
}

#[test]
fn independent_writer_lock_is_exclusive() {
    let f = Fixture::new();
    let guard = safe_files::lock(&f.store).unwrap();
    assert!(safe_files::lock(&f.store).is_err());
    drop(guard);
    assert!(safe_files::lock(&f.store).is_ok());
}

fn set_plan_state(f: &Fixture, plan: &Value, state: &str) -> Value {
    let connection = rusqlite::Connection::open(f.store.database_path()).unwrap();
    let raw: String = connection
        .query_row(
            "SELECT payload_json FROM skill_change_plans WHERE id=?1",
            [plan["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    let digests: serde_json::Map<String, Value> = value["locations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|location| {
            (
                agenthub_core::protection::path_key(Path::new(
                    location["path"].as_str().unwrap(),
                )),
                value["sourceDigest"].as_str().unwrap_or("absent").into(),
            )
        })
        .collect();
    value["recoveryDigests"] = Value::Object(digests);
    connection
        .execute(
            "UPDATE skill_change_plans SET payload_json=?2,state=?3 WHERE id=?1",
            rusqlite::params![plan["id"].as_str().unwrap(), value.to_string(), state],
        )
        .unwrap();
    value
}
fn temporary(path: &Path, id: &str, index: usize, suffix: &str) -> PathBuf {
    path.parent()
        .unwrap()
        .join(format!(".atb-{id}-{index}-{suffix}"))
}

#[test]
fn recovery_preflights_all_incoming_trees_before_removing_any_retained_files() {
    for state in ["executing", "committed"] {
        let f = Fixture::new();
        f.target("codex", "first/skills", ".agents/skills");
        f.install("codex");
        write_skill(&f.upstream, "new upstream");
        let check = f.check();
        let plan = f.preview(
            "update",
            json!({"skillId":f.skill.id,"checkId":check["checkId"]}),
        );
        let saved = set_plan_state(&f, &plan, state);
        let id = plan["id"].as_str().unwrap();
        let locations = saved["locations"].as_array().unwrap();
        assert!(locations.len() > 1);
        let mut retained = Vec::new();
        for (index, location) in locations.iter().enumerate() {
            let path = Path::new(location["path"].as_str().unwrap());
            let old = temporary(path, id, index, "old");
            let incoming = temporary(path, id, index, "new");
            safe_files::copy_tree(path, &old).unwrap();
            safe_files::copy_tree(&f.upstream, &incoming).unwrap();
            retained.push((old, incoming));
        }
        let changed = &retained.last().unwrap().1;
        fs::write(changed.join("references/guide.md"), "user edit in incoming").unwrap();
        assert!(
            skill_changes::recover(&f.store).is_err(),
            "{state} must preserve modified incoming content"
        );
        for (old, incoming) in &retained {
            assert!(
                old.exists(),
                "{state} removed an earlier original before checking every tree"
            );
            assert!(incoming.exists());
        }
        assert_eq!(text(&changed.to_string_lossy()), "user edit in incoming");
        fs::write(changed.join("references/guide.md"), "new upstream").unwrap();
        skill_changes::recover(&f.store).unwrap();
        for (old, incoming) in retained {
            assert!(!old.exists());
            assert!(!incoming.exists());
        }
    }
}

#[test]
fn interrupted_intent_before_any_file_write_is_cancelled_without_installing() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    set_plan_state(&f, &plan, "executing");
    skill_changes::recover(&f.store).unwrap();
    assert!(!f.root.path().join("target/skills/sample-skill").exists());
    assert!(f.store.list_deployments().unwrap().is_empty());
    assert!(!f
        .store
        .data_dir()
        .join("skill-change-plans")
        .join(plan["id"].as_str().unwrap())
        .exists());
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
}

#[test]
fn interrupted_exchange_restores_all_positions_and_leaves_database_unchanged() {
    let f = Fixture::new();
    f.target("codex", "first/skills", ".agents/skills");
    f.target("claude", "second/skills", ".claude/skills");
    f.install("codex");
    f.install("claude");
    let before = f.store.list_deployments().unwrap();
    let before_skill = f.store.get_skill(&f.skill.id).unwrap();
    write_skill(&f.upstream, "new upstream");
    let check = f.check();
    let plan = f.preview(
        "update",
        json!({"skillId":f.skill.id,"checkId":check["checkId"]}),
    );
    let saved = set_plan_state(&f, &plan, "executing");
    let id = plan["id"].as_str().unwrap();
    let source = f
        .store
        .data_dir()
        .join("skill-change-plans")
        .join(id)
        .join("content");
    for (index, location) in saved["locations"]
        .as_array()
        .unwrap()
        .iter()
        .take(2)
        .enumerate()
    {
        let path = Path::new(location["path"].as_str().unwrap());
        fs::rename(path, temporary(path, id, index, "old")).unwrap();
        let incoming = temporary(path, id, index, "new");
        safe_files::copy_tree(&source, &incoming).unwrap();
        if index == 0 {
            fs::rename(incoming, path).unwrap();
        }
    }
    skill_changes::recover(&f.store).unwrap();
    assert_eq!(text(&f.skill.path), "original");
    for item in &before {
        assert_eq!(text(&item.path), "original");
    }
    assert_eq!(f.store.list_deployments().unwrap(), before);
    assert_eq!(f.store.get_skill(&f.skill.id).unwrap(), before_skill);
    for (index, location) in saved["locations"].as_array().unwrap().iter().enumerate() {
        let path = Path::new(location["path"].as_str().unwrap());
        assert!(!temporary(path, id, index, "old").exists());
        assert!(!temporary(path, id, index, "new").exists());
    }
    assert!(!source.exists());
    skill_changes::recover(&f.store).unwrap();
}

#[test]
fn committed_leftovers_are_cleaned_without_reverting_later_user_edits() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    f.install("codex");
    write_skill(Path::new(&f.skill.path), "new library");
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    f.execute("install", &plan);
    let deployed = f.deployment("codex");
    let path = Path::new(&deployed.path);
    let id = plan["id"].as_str().unwrap();
    let old = temporary(path, id, 0, "old");
    safe_files::copy_tree(&f.upstream, &old).unwrap();
    set_plan_state(&f, &plan, "committed");
    fs::write(path.join("references/guide.md"), "later user edit").unwrap();
    fs::write(old.join("references/guide.md"), "changed retained original").unwrap();
    assert!(skill_changes::recover(&f.store).is_err());
    assert_restricted_access(&f);
    assert_eq!(text(&deployed.path), "later user edit");
    assert_eq!(f.deployment("codex"), deployed);
    fs::write(old.join("references/guide.md"), "original").unwrap();
    skill_changes::recover(&f.store).unwrap();
    assert!(!old.exists());
    assert_eq!(text(&deployed.path), "later user edit");
    assert_eq!(f.deployment("codex"), deployed);
}

fn assert_restricted_access(f: &Fixture) {
    use agenthub_core::dispatch;
    for _ in 0..2 {
        let snapshot = dispatch(f.store.data_dir(), "snapshot", json!({})).unwrap();
        assert_eq!(snapshot["recovery"]["status"], "restricted");
        assert_eq!(snapshot["recovery"]["code"], "RECOVERY_REQUIRED");
        assert!(!snapshot["recovery"]["issues"]
            .as_array()
            .unwrap()
            .is_empty());
        dispatch(
            f.store.data_dir(),
            "prompts.export",
            json!({"format":"json"}),
        )
        .unwrap();
        dispatch(f.store.data_dir(), "backups.list", json!({})).unwrap();
        dispatch(f.store.data_dir(), "maintenance.get", json!({})).unwrap();
        let error = dispatch(
            f.store.data_dir(),
            "skills.install.preview",
            json!({"skillId":f.skill.id,"targetId":"codex"}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("RECOVERY_REQUIRED"));
    }
}

#[test]
fn damaged_recovery_record_keeps_safe_features_available_and_preserves_journal() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    let connection = rusqlite::Connection::open(f.store.database_path()).unwrap();
    connection
        .execute(
            "UPDATE skill_change_plans SET state='executing',payload_json='broken' WHERE id=?1",
            [plan["id"].as_str().unwrap()],
        )
        .unwrap();
    assert_restricted_access(&f);
    let raw: String = connection
        .query_row(
            "SELECT payload_json FROM skill_change_plans WHERE id=?1",
            [plan["id"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(raw, "broken");
}

#[test]
fn interrupted_partial_exchange_preserves_user_changes_and_all_originals_in_restricted_mode() {
    let f = Fixture::new();
    f.target("codex", "first/skills", ".agents/skills");
    f.target("claude", "second/skills", ".claude/skills");
    f.install("codex");
    f.install("claude");
    write_skill(&f.upstream, "new upstream");
    let check = f.check();
    let plan = f.preview(
        "update",
        json!({"skillId":f.skill.id,"checkId":check["checkId"]}),
    );
    let saved = set_plan_state(&f, &plan, "executing");
    let location = &saved["locations"][0];
    let path = Path::new(location["path"].as_str().unwrap());
    let old = temporary(path, plan["id"].as_str().unwrap(), 0, "old");
    fs::rename(path, &old).unwrap();
    write_skill(path, "user change after interruption");
    assert_restricted_access(&f);
    assert_eq!(
        text(&path.to_string_lossy()),
        "user change after interruption"
    );
    assert_eq!(text(&old.to_string_lossy()), "original");
    for location in saved["locations"].as_array().unwrap().iter().skip(1) {
        assert_eq!(text(location["path"].as_str().unwrap()), "original");
    }
}

#[cfg(windows)]
#[test]
fn locked_committed_temporary_file_does_not_block_the_app_and_retry_finishes_cleanup() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    f.install("codex");
    write_skill(Path::new(&f.skill.path), "new library");
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    f.execute("install", &plan);
    let deployed = f.deployment("codex");
    let old = temporary(
        Path::new(&deployed.path),
        plan["id"].as_str().unwrap(),
        0,
        "old",
    );
    safe_files::copy_tree(&f.upstream, &old).unwrap();
    set_plan_state(&f, &plan, "committed");
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(old.join("references/guide.md"))
        .unwrap();
    assert_restricted_access(&f);
    assert!(old.exists());
    drop(held);
    let snapshot = agenthub_core::dispatch(f.store.data_dir(), "snapshot", json!({})).unwrap();
    assert_eq!(snapshot["recovery"]["status"], "ready");
    assert!(!old.exists());
    assert_eq!(text(&deployed.path), "new library");
}

#[test]
fn failed_backup_restore_rolls_back_created_files_and_records() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    f.install("codex");
    let deployed = f.deployment("codex");
    let plan = f.preview("remove", json!({"deploymentId":deployed.id}));
    let result = f.execute("remove", &plan);
    let before_backups = f.backups();
    let connection = rusqlite::Connection::open(f.store.database_path()).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_restored_deployment BEFORE INSERT ON skill_deployments BEGIN SELECT RAISE(ABORT,'injected restore failure'); END;").unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "backups.restore",
        &json!({"id":result["backupId"],"confirmed":true})
    )
    .is_err());
    assert!(!Path::new(&deployed.path).exists());
    assert!(f.store.list_deployments().unwrap().is_empty());
    assert_eq!(f.backups(), before_backups);
}

#[cfg(windows)]
fn directory_link(link: &Path, target: &Path) {
    let result = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[cfg(not(windows))]
fn directory_link(link: &Path, target: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[test]
fn replacement_ancestor_junction_and_source_child_links_are_rejected() {
    let f = Fixture::new();
    f.target("codex", "agent/skills", ".agents/skills");
    fs::create_dir_all(f.root.path().join("agent/skills")).unwrap();
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    let real = f.root.path().join("real-agent");
    fs::rename(f.root.path().join("agent"), &real).unwrap();
    directory_link(&f.root.path().join("agent"), &real);
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    assert!(!real.join("skills/sample-skill").exists());
    let linked = f.root.path().join("source-with-link");
    write_skill(&linked, "fixture");
    directory_link(&linked.join("linked-content"), &f.upstream);
    assert!(safe_files::digest(&linked).is_err());
    assert!(safe_files::copy_tree(&linked, &f.root.path().join("rejected-copy")).is_err());
    assert!(!f.root.path().join("rejected-copy").exists());
}

#[test]
fn semantic_baseline_ignores_git_metadata_but_pending_plan_still_checks_all_files() {
    let f = Fixture::new();
    f.target("codex", "target/skills", ".agents/skills");
    f.install("codex");
    let deployed = f.deployment("codex");
    let path = Path::new(&deployed.path);
    assert_eq!(
        deployed.baseline_digest,
        Some(skills::content_digest(path).unwrap())
    );
    fs::create_dir(path.join(".git")).unwrap();
    fs::write(path.join(".git/state"), "metadata one").unwrap();
    let plan = f.preview("install", json!({"skillId":f.skill.id,"targetId":"codex"}));
    assert_eq!(plan["canExecute"], true);
    fs::write(path.join(".git/state"), "metadata two").unwrap();
    assert!(skill_changes::dispatch(
        &f.store,
        "skills.install",
        &json!({"planId":plan["id"],"confirmed":true})
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(path.join(".git/state")).unwrap(),
        "metadata two"
    );
}

#[test]
fn update_refreshes_descriptions_and_snapshot_repairs_stale_display_metadata() {
    let f = Fixture::new();
    f.target("codex", "first/skills", ".agents/skills");
    f.install("codex");
    fs::write(
        f.upstream.join("SKILL.md"),
        "---\nname: new-upstream-name\ndescription: Updated description\n---\nnew body",
    )
    .unwrap();
    let check = f.check();
    let plan = f.preview(
        "update",
        json!({"skillId":f.skill.id,"checkId":check["checkId"]}),
    );
    f.execute("update", &plan);
    let saved = f.store.get_skill(&f.skill.id).unwrap().unwrap();
    assert_eq!(saved.description, "Updated description");
    assert_eq!(saved.name, f.skill.name);
    assert_eq!(f.deployment("codex").description, "Updated description");
    let mut stale = saved.clone();
    stale.description = "outdated metadata".into();
    stale.tags = vec!["personal tag".into()];
    f.store.save_skill(&stale).unwrap();
    let snapshot = f.store.snapshot().unwrap();
    assert_eq!(snapshot["skills"][0]["description"], "Updated description");
    assert_eq!(snapshot["skills"][0]["tags"], json!(["personal tag"]));
    assert_eq!(
        f.store.get_skill(&f.skill.id).unwrap().unwrap().description,
        "outdated metadata"
    );
}
