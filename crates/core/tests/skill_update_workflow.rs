use agenthub_core::{
    dispatch,
    model::{Skill, SkillDeployment},
    safe_files, skills,
    store::Store,
};
use serde_json::{json, Value};
use std::{fs, path::Path};

fn write_skill(path: &Path, name: &str, text: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Fixture\n---\n{text}\n"),
    )
    .unwrap();
}
fn add(data: &Path, source: &Path, subpath: &str) -> Skill {
    let inspection = dispatch(
        data,
        "sources.inspect",
        json!({"source":{"kind":"local","locator":source}}),
    )
    .unwrap();
    serde_json::from_value(
        dispatch(
            data,
            "skills.add",
            json!({"inspectionId":inspection["inspectionId"],"subpath":subpath}),
        )
        .unwrap()["skill"]
            .clone(),
    )
    .unwrap()
}
fn update(data: &Path, id: &str, check: &Value, locations: Value) -> Value {
    let plan=dispatch(data,"skills.update.preview",json!({"skillId":id,"checkId":check["checkId"],"locationIds":locations,"retainBackup":true})).unwrap();
    dispatch(
        data,
        "skills.update",
        json!({"planId":plan["id"],"confirmed":true}),
    )
    .unwrap()
}
fn status(data: &Path, id: &str) -> Value {
    dispatch(data, "snapshot", json!({})).unwrap()["skillUpdates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["skillId"] == id)
        .unwrap()
        .clone()
}

#[test]
fn latest_status_survives_reopen_and_partial_updates_keep_remaining_locations_visible() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let source = t.path().join("source");
    write_skill(&source, "writer", "v1");
    let skill = add(&data, &source, "");
    assert!(Path::new(&skill.path).ends_with("writer"));
    let agent = t.path().join("agent/writer");
    safe_files::copy_tree(&source, &agent).unwrap();
    let deployment = SkillDeployment {
        id: uuid::Uuid::new_v4().to_string(),
        skill_id: Some(skill.id.clone()),
        name: skill.name.clone(),
        description: String::new(),
        agent: "codex".into(),
        scope: "global".into(),
        profile: None,
        project_id: None,
        path: agent.to_string_lossy().into(),
        source: skill.source.clone(),
        owner: "agenthub".into(),
        status: "present".into(),
        ignored: false,
        baseline_digest: skill.source_digest.clone(),
    };
    Store::open(&data)
        .unwrap()
        .save_deployment(&deployment)
        .unwrap();
    write_skill(&source, "writer", "v2");
    let checked = dispatch(&data, "skills.check", json!({"skillId":skill.id})).unwrap();
    assert_eq!(checked["status"], "available");
    assert_eq!(status(&data, &skill.id)["checkId"], checked["checkId"]);
    let first = update(&data, &skill.id, &checked, json!(["library"]));
    assert_eq!(first["status"], "succeeded");
    let partial = status(&data, &skill.id);
    assert_eq!(partial["status"], "available");
    assert_ne!(partial["checkId"], checked["checkId"]);
    let changed: Vec<_> = partial["locations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| !l["differences"].as_array().unwrap().is_empty())
        .collect();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0]["id"], deployment.id);
    assert!(fs::read_to_string(agent.join("SKILL.md"))
        .unwrap()
        .contains("v1"));
    update(&data, &skill.id, &partial, json!([deployment.id]));
    assert_eq!(status(&data, &skill.id)["status"], "current");
    assert!(fs::read_to_string(agent.join("SKILL.md"))
        .unwrap()
        .contains("v2"));
    dispatch(
        &data,
        "backups.restore",
        json!({"id":first["backupId"],"confirmed":true}),
    )
    .unwrap();
    assert_eq!(status(&data, &skill.id)["status"], "available");
    let store = Store::open(&data).unwrap();
    assert_eq!(store.list_skills().unwrap()[0].id, skill.id);
    let conn = rusqlite::Connection::open(store.database_path()).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM skill_update_status", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn cached_status_is_not_lost_on_expiry_and_opening_update_rechecks_local_files() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let source = t.path().join("source");
    write_skill(&source, "writer", "v1");
    let skill = add(&data, &source, "");
    write_skill(&source, "writer", "v2");
    let first = dispatch(&data, "skills.check", json!({"skillId":skill.id})).unwrap();
    write_skill(Path::new(&skill.path), "writer", "personal edit");
    let opened = dispatch(
        &data,
        "skills.check",
        json!({"skillId":skill.id,"useCached":true}),
    )
    .unwrap();
    assert_eq!(opened["status"], "diverged");
    assert_ne!(opened["checkId"], first["checkId"]);
    assert_eq!(opened["checkedAt"], first["checkedAt"]);
    assert!(skills::checked_update(
        &Store::open(&data).unwrap(),
        first["checkId"].as_str().unwrap()
    )
    .is_err());
    let conn = rusqlite::Connection::open(data.join("agenthub.sqlite3")).unwrap();
    conn.execute("UPDATE skill_checks SET payload_json=json_set(payload_json,'$.createdAt','2020-01-01T00:00:00Z')",[]).unwrap();
    conn.execute("UPDATE skill_update_status SET payload_json=json_set(payload_json,'$.result.checkedAt','2020-01-01T00:00:00Z')",[]).unwrap();
    let old = status(&data, &skill.id);
    assert_eq!(old["status"], "diverged");
    assert_eq!(old["needsRefresh"], true);
    write_skill(&source, "writer", "v3");
    let fresh = dispatch(
        &data,
        "skills.check",
        json!({"skillId":skill.id,"useCached":true}),
    )
    .unwrap();
    assert_ne!(fresh["checkedAt"], old["checkedAt"]);
    fs::rename(&source, t.path().join("removed-source")).unwrap();
    let failed = dispatch(&data, "skills.check", json!({"skillId":skill.id})).unwrap();
    assert_eq!(failed["status"], "failed");
    assert_eq!(status(&data, &skill.id)["status"], "failed");
    dispatch(&data, "skills.unbindSource", json!({"skillId":skill.id})).unwrap();
    assert!(
        dispatch(&data, "snapshot", json!({})).unwrap()["skillUpdates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn one_batch_reuses_a_repository_and_keeps_independent_update_snapshots() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let source = t.path().join("repo");
    write_skill(&source.join("a"), "one", "v1");
    write_skill(&source.join("b"), "two", "v1");
    let one = add(&data, &source, "a");
    let two = add(&data, &source, "b");
    let batch = dispatch(&data, "skills.checkBatch.start", json!({})).unwrap();
    let first = dispatch(
        &data,
        "skills.check",
        json!({"skillId":one.id,"batchId":batch["batchId"]}),
    )
    .unwrap();
    write_skill(&source.join("b"), "two", "v2");
    let second = dispatch(
        &data,
        "skills.check",
        json!({"skillId":two.id,"batchId":batch["batchId"]}),
    )
    .unwrap();
    assert_eq!(second["status"], "current");
    dispatch(
        &data,
        "skills.checkBatch.finish",
        json!({"batchId":batch["batchId"]}),
    )
    .unwrap();
    assert!(!data
        .join("skill-check-batches")
        .join(batch["batchId"].as_str().unwrap())
        .exists());
    let store = Store::open(&data).unwrap();
    let a = skills::checked_update(&store, first["checkId"].as_str().unwrap()).unwrap();
    let b = skills::checked_update(&store, second["checkId"].as_str().unwrap()).unwrap();
    assert_ne!(a.source_path, b.source_path);
    let latest = dispatch(&data, "skills.check", json!({"skillId":two.id})).unwrap();
    assert_eq!(latest["status"], "available");
}

#[test]
fn concurrent_dispatch_checks_share_a_batch_without_lock_failures() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let source = t.path().join("repo");
    let mut ids = Vec::new();
    for n in 0..6 {
        let name = format!("skill-{n}");
        write_skill(&source.join(&name), &name, "v1");
    }
    for n in 0..6 {
        ids.push(add(&data, &source, &format!("skill-{n}")).id);
    }
    for n in 0..6 {
        let name = format!("skill-{n}");
        write_skill(&source.join(&name), &name, "v2");
    }
    let batch = dispatch(&data, "skills.checkBatch.start", json!({})).unwrap();
    std::thread::scope(|scope| {
        let handles: Vec<_> = ids
            .iter()
            .map(|id| {
                let data = &data;
                let batch = &batch;
                scope.spawn(move || {
                    dispatch(
                        data,
                        "skills.check",
                        json!({"skillId":id,"batchId":batch["batchId"]}),
                    )
                    .unwrap()
                })
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap()["status"], "available");
        }
    });
    dispatch(
        &data,
        "skills.checkBatch.finish",
        json!({"batchId":batch["batchId"]}),
    )
    .unwrap();
    assert_eq!(
        dispatch(&data, "snapshot", json!({})).unwrap()["skillUpdates"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}
