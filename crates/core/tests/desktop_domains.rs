use agenthub_core::{dispatch, installed_root, store::Store};
use serde_json::json;
use std::fs;

#[test]
fn project_archive_and_record_deletion_preserve_files_and_remove_linked_bookmark() {
    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("data");
    let directory = fixture.path().join("personal-project");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("keep.txt"), "my content").unwrap();
    let project = dispatch(&data, "projects.save", json!({"path":directory})).unwrap();
    assert!(dispatch(&data, "projects.save", json!({"path":directory})).is_err());
    dispatch(
        &data,
        "bookmarks.save",
        json!({"url":"https://example.com/project", "projectId":project["id"], "status":"in_use"}),
    )
    .unwrap();
    dispatch(
        &data,
        "projects.archive",
        json!({"projectId":project["id"], "archived":true}),
    )
    .unwrap();
    assert_eq!(
        dispatch(&data, "snapshot", json!({})).unwrap()["projects"][0]["archived"],
        true
    );
    dispatch(&data, "projects.delete", json!({"projectId":project["id"]})).unwrap();
    assert!(dispatch(&data, "bookmarks.list", json!({})).unwrap()["items"].as_array().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(directory.join("keep.txt")).unwrap(),
        "my content"
    );
}

#[test]
fn fresh_schema_has_no_legacy_tables_and_cancelled_routes_are_not_aliases() {
    let fixture = tempfile::tempdir().unwrap();
    let store = Store::open(fixture.path()).unwrap();
    let database = rusqlite::Connection::open(store.database_path()).unwrap();
    let legacy_count: i64 = database.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('tools','toolkits','installations','operation_plans','external_installations')",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(legacy_count, 0);
    for method in [
        "companion.status",
        "tools.save",
        "toolkits.list",
        "external.prepare",
        "operations.preview",
        "library.preview",
        "projects.execute",
    ] {
        assert!(
            dispatch(fixture.path(), method, json!({}))
                .unwrap_err()
                .to_string()
                .contains("unknown core method"),
            "{method}"
        );
    }
    let databases: Vec<_> = fs::read_dir(fixture.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sqlite3"))
        .collect();
    assert_eq!(databases, vec![fixture.path().join("agenthub.sqlite3")]);
}

#[test]
fn distributed_desktop_resolves_its_own_data_layout() {
    let fixture = tempfile::tempdir().unwrap();
    fs::create_dir_all(fixture.path().join("data/runtime")).unwrap();
    fs::write(
        fixture.path().join("data/runtime/agenthub-layout.json"),
        "{}",
    )
    .unwrap();
    assert_eq!(
        installed_root(&fixture.path().join("AgentHub.exe")),
        Some(fixture.path().to_path_buf())
    );
    assert_eq!(
        installed_root(&fixture.path().join("other/test-runner.exe")),
        None
    );
}
