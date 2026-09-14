use agenthub_core::dispatch;
use serde_json::{json, Value};

fn count(data: &std::path::Path) -> usize {
    dispatch(data, "snapshot", json!({})).unwrap()["operations"]
        .as_array()
        .unwrap()
        .len()
}

#[test]
fn key_changes_are_logged_but_reads_noops_and_metadata_are_quiet() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let path = t.path().join("project");
    std::fs::create_dir(&path).unwrap();
    let project = dispatch(
        &data,
        "projects.save",
        json!({"path":path,"name":"Project"}),
    )
    .unwrap();
    assert_eq!(count(&data), 1);
    dispatch(&data, "projects.save", json!({"project":project})).unwrap();
    dispatch(&data, "projects.trust", json!({"projectId":project["id"],"path":path,"trusted":true,"confirmed":true})).unwrap();
    dispatch(
        &data,
        "projects.inspect",
        json!({"projectId":project["id"]}),
    )
    .unwrap();
    dispatch(
        &data,
        "projects.archive",
        json!({"projectId":project["id"],"archived":false}),
    )
    .unwrap();
    dispatch(&data, "bookmarks.sync", json!({})).unwrap();
    assert_eq!(count(&data), 1);
    let mut prompt = dispatch(
        &data,
        "prompts.save",
        json!({"title":"Prompt","body":"original"}),
    )
    .unwrap();
    assert_eq!(count(&data), 2);
    prompt["favorite"] = json!(true);
    prompt["tags"] = json!(["tag"]);
    dispatch(&data, "prompts.save", json!({"prompt":prompt})).unwrap();
    dispatch(&data, "prompts.export", json!({"format":"json"})).unwrap();
    assert_eq!(count(&data), 2);
    prompt["body"] = json!("changed");
    dispatch(&data, "prompts.save", json!({"prompt":prompt})).unwrap();
    assert_eq!(count(&data), 3);
    dispatch(&data, "prompts.delete", json!({"id":prompt["id"]})).unwrap();
    dispatch(&data, "prompts.delete", json!({"id":prompt["id"]})).unwrap();
    assert_eq!(count(&data), 4);
    let mut bookmark = dispatch(
        &data,
        "bookmarks.save",
        json!({"url":"https://github.com/example/repo"}),
    )
    .unwrap();
    assert_eq!(count(&data), 5);
    bookmark["notes"] = json!("note");
    bookmark["tags"] = json!(["tag"]);
    dispatch(&data, "bookmarks.save", json!({"bookmark":bookmark})).unwrap();
    assert_eq!(count(&data), 5);
    bookmark["projectId"] = project["id"].clone();
    bookmark = dispatch(&data, "bookmarks.save", json!({"bookmark":bookmark})).unwrap();
    assert_eq!(bookmark["status"], "in_use");
    assert_eq!(count(&data), 6);
    dispatch(
        &data,
        "projects.archive",
        json!({"projectId":project["id"],"archived":true}),
    )
    .unwrap();
    assert_eq!(count(&data), 7);
    bookmark["projectId"] = json!("missing");
    assert!(dispatch(&data, "bookmarks.save", json!({"bookmark":bookmark})).is_err());
    assert_eq!(count(&data), 7);
    dispatch(&data, "projects.delete", json!({"projectId":project["id"]})).unwrap();
    assert_eq!(count(&data), 8);
    assert!(path.is_dir());
    let events: Value = dispatch(&data, "snapshot", json!({})).unwrap()["operations"].clone();
    assert!(events
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["status"] == "succeeded"));
}
