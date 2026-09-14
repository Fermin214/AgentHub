use agenthub_core::dispatch;
use serde_json::json;
use std::{fs, path::Path};
fn seed(data: &Path, n: usize, _protected: bool) -> String {
    dispatch(data, "backups.list", json!({})).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let root = data.join("backups").join(&id);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("keep.txt"), "backup").unwrap();
    let conn = rusqlite::Connection::open(data.join("agenthub.sqlite3")).unwrap();
    conn.execute(
        "INSERT INTO backups(id,name,path,created_at,original_path) VALUES(?1,?2,?3,?4,'fixture')",
        rusqlite::params![
            id,
            format!("Skill {n}"),
            root.to_string_lossy(),
            format!("2026-09-11T00:00:{n:02}Z")
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO skill_backup_manifests(backup_id,payload_json) VALUES(?1,'fixture')",
        [&id],
    )
    .unwrap();
    id
}
#[test]
fn global_limit_prunes_oldest_backups_and_keeps_latest() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path();
    let ids: Vec<_> = (0..5).map(|n| seed(data, n, false)).collect();
    assert!(dispatch(data, "maintenance.get", json!({})).unwrap()["maxBackups"].is_null());
    let preview = dispatch(data, "maintenance.preview", json!({"maxBackups":2})).unwrap();
    assert_eq!(preview["pruneCount"], 3);
    assert_eq!(preview["protectedCount"], 0);
    let saved = dispatch(data, "maintenance.save", json!({"maxBackups":2})).unwrap();
    assert_eq!(saved["retention"]["removedCount"], 3);
    for id in &ids[..3] {
        assert!(!data.join("backups").join(id).exists());
    }
    for id in ids[3..].iter() {
        assert!(data.join("backups").join(id).exists());
    }
    let conn = rusqlite::Connection::open(data.join("agenthub.sqlite3")).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM skill_backup_manifests", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    for limit in [json!(0), json!(-1), json!(1.5), json!("2")] {
        assert!(dispatch(data, "maintenance.save", json!({"maxBackups":limit})).is_err());
    }
    assert_eq!(
        dispatch(data, "maintenance.get", json!({})).unwrap()["maxBackups"],
        2
    );
    dispatch(data, "maintenance.save", json!({"maxBackups":null})).unwrap();
    assert_eq!(
        dispatch(data, "backups.list", json!({})).unwrap()["backups"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn invalid_backup_paths_are_reported_and_an_active_operation_blocks_policy_changes() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let id = seed(&data, 0, false);
    seed(&data, 1, false);
    let outside = t.path().join("personal");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "personal").unwrap();
    let conn = rusqlite::Connection::open(data.join("agenthub.sqlite3")).unwrap();
    conn.execute(
        "UPDATE backups SET path=?1 WHERE id=?2",
        rusqlite::params![outside.to_string_lossy(), id],
    )
    .unwrap();
    let saved = dispatch(&data, "maintenance.save", json!({"maxBackups":1})).unwrap();
    assert_eq!(saved["retention"]["removedCount"], 0);
    assert_eq!(saved["retention"]["failures"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(outside.join("keep.txt")).unwrap(),
        "personal"
    );
    assert!(data.join("backups").join(id).exists());
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(data.join("skill-write.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(dispatch(&data, "maintenance.save", json!({"maxBackups":3})).is_err());
    assert_eq!(
        dispatch(&data, "maintenance.get", json!({})).unwrap()["maxBackups"],
        1
    );
}
