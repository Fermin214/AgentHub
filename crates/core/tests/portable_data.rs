use agenthub_core::{dispatch, store::Store};
use serde_json::{json, Value};
use std::{fs, path::Path};

fn call(data: &Path, method: &str, args: Value) -> Value {
    dispatch(data, method, args).unwrap()
}
fn write_skill(path: &Path, content: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: portable-writer\ndescription: Portable fixture\n---\n{content}\n"),
    )
    .unwrap();
}

#[test]
fn portable_data_move_keeps_owned_content_usable_and_external_paths_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("A/data");
    let b = temp.path().join("B/data");
    let source = temp.path().join("source");
    let project = temp.path().join("external-project");
    let agent = temp.path().join("external-agent");
    fs::create_dir_all(&project).unwrap();
    write_skill(&source, "version one");
    call(&a, "settings.save", json!({"settings":{"scanRoots":[]}}));
    call(
        &a,
        "prompts.save",
        json!({"prompt":{"title":"portable prompt","body":"preserved content"}}),
    );
    call(
        &a,
        "projects.save",
        json!({"project":{"name":"external project","path":project}}),
    );
    let targets = call(&a, "targets.list", json!({}));
    for target in targets["targets"].as_array().unwrap() {
        let mut target = target.clone();
        target["enabled"] = json!(false);
        call(&a, "targets.save", json!({"target":target}));
    }
    call(
        &a,
        "targets.save",
        json!({"target":{"id":"codex","name":"Codex","globalPath":agent,"projectPath":".agents/skills","enabled":true}}),
    );
    let inspection = call(
        &a,
        "sources.inspect",
        json!({"source":{"kind":"local","locator":source}}),
    );
    let skill = call(
        &a,
        "skills.add",
        json!({"inspectionId":inspection["inspectionId"],"subpath":""}),
    )["skill"]
        .clone();
    let install = call(
        &a,
        "skills.install.preview",
        json!({"skillId":skill["id"],"targetId":"codex"}),
    );
    call(
        &a,
        "skills.install",
        json!({"planId":install["id"],"confirmed":true}),
    );
    write_skill(&source, "version two");
    let checked = call(&a, "skills.check", json!({"skillId":skill["id"]}));
    let plan = call(
        &a,
        "skills.update.preview",
        json!({"skillId":skill["id"],"checkId":checked["checkId"],"locationIds":["library"],"retainBackup":true}),
    );
    let updated = call(
        &a,
        "skills.update",
        json!({"planId":plan["id"],"confirmed":true}),
    );
    let stale = call(
        &a,
        "skills.install.preview",
        json!({"skillId":skill["id"],"targetId":"codex"}),
    );
    let before = call(&a, "snapshot", json!({}));
    fs::rename(a.parent().unwrap(), b.parent().unwrap()).unwrap();

    let after = call(&b, "snapshot", json!({}));
    let moved = &after["skills"][0];
    assert_eq!(
        Path::new(moved["path"].as_str().unwrap()),
        b.join("library/skills/portable-writer")
    );
    assert_eq!(after["prompts"], before["prompts"]);
    assert_eq!(after["projects"], before["projects"]);
    assert_eq!(after["deployments"], before["deployments"]);
    assert_eq!(moved["source"], skill["source"]);
    assert!(after["skillUpdates"].as_array().unwrap().is_empty());
    assert!(dispatch(
        &b,
        "skills.install",
        json!({"planId":stale["id"],"confirmed":true})
    )
    .is_err());
    let existing = call(
        &b,
        "skills.install.preview",
        json!({"skillId":skill["id"],"targetId":"codex"}),
    );
    call(
        &b,
        "skills.install",
        json!({"planId":existing["id"],"confirmed":true}),
    );
    let new_check = call(&b, "skills.check", json!({"skillId":skill["id"]}));
    assert_ne!(new_check["checkId"], checked["checkId"]);
    // Restore the library-only update; external Agent content is not part of it.
    let external_before_restore = fs::read(agent.join("portable-writer/SKILL.md")).unwrap();
    call(
        &b,
        "backups.restore",
        json!({"id":updated["backupId"],"confirmed":true}),
    );
    assert!(
        fs::read_to_string(b.join("library/skills/portable-writer/SKILL.md"))
            .unwrap()
            .contains("version one")
    );
    assert_eq!(
        fs::read(agent.join("portable-writer/SKILL.md")).unwrap(),
        external_before_restore
    );
    assert_eq!(
        Store::open(&b).unwrap().list_projects().unwrap()[0].path,
        project.to_string_lossy()
    );
    assert!(!a.exists());
}

#[test]
fn portable_move_restores_a_deleted_library_skill() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("A");
    let b = temp.path().join("B");
    let source = temp.path().join("source");
    write_skill(&source, "saved before deletion");
    let inspected = call(
        &a,
        "sources.inspect",
        json!({"source":{"kind":"local","locator":source}}),
    );
    let skill = call(
        &a,
        "skills.add",
        json!({"inspectionId":inspected["inspectionId"],"subpath":""}),
    )["skill"]
        .clone();
    let preview = call(&a, "skills.delete.preview", json!({"skillId":skill["id"]}));
    let deleted = call(
        &a,
        "skills.delete",
        json!({"planId":preview["id"],"confirmed":true}),
    );
    fs::rename(&a, &b).unwrap();
    assert_eq!(
        call(&b, "snapshot", json!({}))["recovery"]["status"],
        "ready"
    );
    call(
        &b,
        "backups.restore",
        json!({"id":deleted["backupId"],"confirmed":true}),
    );
    let restored = call(&b, "snapshot", json!({}))["skills"][0].clone();
    assert_eq!(restored["id"], skill["id"]);
    assert_eq!(restored["source"], skill["source"]);
    assert!(
        fs::read_to_string(Path::new(restored["path"].as_str().unwrap()).join("SKILL.md"))
            .unwrap()
            .contains("saved before deletion")
    );
    assert!(!a.exists());
}

#[test]
fn portable_move_with_unfinished_exchange_preserves_journal_and_external_files() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("A");
    let b = temp.path().join("B");
    let external = temp.path().join("external-agent/skill");
    write_skill(&external, "personal change after interruption");
    call(
        &a,
        "prompts.save",
        json!({"prompt":{"title":"safe prompt","body":"export remains available"}}),
    );
    let id = uuid::Uuid::new_v4().to_string();
    let raw = json!({"id":id,"locations":[{"path":external,"role":"deployment"}]}).to_string();
    {
        let conn = rusqlite::Connection::open(a.join("agenthub.sqlite3")).unwrap();
        conn.execute(
            "INSERT INTO skill_change_plans(id,payload_json,state) VALUES(?1,?2,'executing')",
            rusqlite::params![id, raw],
        )
        .unwrap();
    }
    fs::rename(&a, &b).unwrap();
    for _ in 0..2 {
        let snapshot = call(&b, "snapshot", json!({}));
        assert_eq!(snapshot["recovery"]["status"], "restricted");
        assert!(snapshot["recovery"]["detail"]
            .as_str()
            .unwrap()
            .contains("移回"));
        assert_eq!(
            snapshot["recovery"]["issues"][0]["paths"][0],
            json!(external)
        );
        assert_eq!(snapshot["prompts"][0]["body"], "export remains available");
        call(&b, "prompts.export", json!({"format":"json"}));
        assert!(dispatch(
            &b,
            "skills.install.preview",
            json!({"skillId":id,"targetId":"codex"})
        )
        .is_err());
    }
    let conn = rusqlite::Connection::open(b.join("agenthub.sqlite3")).unwrap();
    let saved: (String, String) = conn
        .query_row(
            "SELECT payload_json,state FROM skill_change_plans WHERE id=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(saved, (raw, "executing".into()));
    assert!(fs::read_to_string(external.join("SKILL.md"))
        .unwrap()
        .contains("personal change after interruption"));
    assert!(!a.exists());
}
