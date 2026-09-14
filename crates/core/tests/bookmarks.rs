use agenthub_core::dispatch;
use serde_json::json;
use std::fs;

fn local_repo(path: &std::path::Path, url: &str) {
    fs::create_dir_all(path).unwrap();
    for args in [vec!["init"], vec!["remote", "add", "origin", url]] {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn project_upstream_links_existing_bookmark_and_archives_both_directions() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let path = t.path().join("repo");
    local_repo(&path, "git@github.com:Example/Repo.git");
    let existing=dispatch(&data,"bookmarks.save",json!({"url":"https://github.com/example/repo","notes":"keep notes","tags":["keep tag"],"status":"interested"})).unwrap();
    let project = dispatch(&data, "projects.save", json!({"path":path,"name":"Local"})).unwrap();
    dispatch(
        &data,
        "projects.trust",
        json!({"projectId":project["id"],"path":path,"trusted":true,"confirmed":true}),
    )
    .unwrap();
    let linked = dispatch(&data, "bookmarks.sync", json!({})).unwrap()["items"][0].clone();
    assert_eq!(linked["id"], existing["id"]);
    assert_eq!(linked["projectId"], project["id"]);
    assert_eq!(linked["notes"], existing["notes"]);
    assert_eq!(linked["tags"], existing["tags"]);
    assert_eq!(linked["status"], "in_use");
    dispatch(
        &data,
        "projects.archive",
        json!({"projectId":project["id"],"archived":true}),
    )
    .unwrap();
    let mut archived = dispatch(&data, "bookmarks.list", json!({})).unwrap()["items"][0].clone();
    assert_eq!(archived["archived"], true);
    archived["archived"] = json!(false);
    dispatch(&data, "bookmarks.save", json!({"bookmark":archived})).unwrap();
    assert_eq!(
        dispatch(&data, "snapshot", json!({})).unwrap()["projects"][0]["archived"],
        false
    );
    dispatch(&data, "bookmarks.delete", json!({"id":existing["id"]})).unwrap();
    assert_eq!(
        dispatch(&data, "bookmarks.sync", json!({})).unwrap()["items"],
        json!([])
    );
    assert!(path.join(".git").exists());
}

#[test]
fn auto_bookmark_respects_explicit_link_and_does_not_match_only_by_name() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let path = t.path().join("repo");
    local_repo(&path, "https://github.com/upstream/repo");
    let project = dispatch(&data, "projects.save", json!({"path":path})).unwrap();
    dispatch(
        &data,
        "projects.trust",
        json!({"projectId":project["id"],"path":path,"trusted":true,"confirmed":true}),
    )
    .unwrap();
    let items = dispatch(&data, "bookmarks.list", json!({})).unwrap();
    assert_eq!(items["items"].as_array().unwrap().len(), 1);
    let mut item = items["items"][0].clone();
    item["url"] = json!("https://github.com/my-fork/repo");
    item["notes"] = json!("explicit fork");
    dispatch(&data, "bookmarks.save", json!({"bookmark":item})).unwrap();
    let items = dispatch(&data, "bookmarks.sync", json!({})).unwrap();
    assert_eq!(items["items"].as_array().unwrap().len(), 1);
    assert_eq!(items["items"][0]["url"], "https://github.com/my-fork/repo");
    assert_eq!(items["items"][0]["projectId"], project["id"]);
    let another = t.path().join("another");
    local_repo(&another, "https://github.com/different/repo");
    let another = dispatch(
        &data,
        "projects.save",
        json!({"path":another,"name":"repo"}),
    )
    .unwrap();
    dispatch(
        &data,
        "projects.trust",
        json!({"projectId":another["id"],"path":another["path"],"trusted":true,"confirmed":true}),
    )
    .unwrap();
    let items = dispatch(&data, "bookmarks.list", json!({})).unwrap();
    assert_eq!(items["items"].as_array().unwrap().len(), 2);
    assert!(items["items"].as_array().unwrap().iter().any(
        |i| i["projectId"] == another["id"] && i["url"] == "https://github.com/different/repo"
    ));
}

#[test]
fn bookmark_tags_are_normalized_persisted_and_filterable() {
    let t = tempfile::tempdir().unwrap();
    let item = dispatch(t.path(), "bookmarks.save", json!({"url":"https://github.com/example/skills","tags":[" Obsidian ","写作","Obsidian",""]})).unwrap();
    assert_eq!(item["tags"], json!(["Obsidian", "写作"]));
    assert_eq!(
        dispatch(
            t.path(),
            "bookmarks.list",
            json!({"query":"obsidian","tag":"写作"})
        )
        .unwrap()["items"][0]["id"],
        item["id"]
    );
    assert_eq!(
        dispatch(t.path(), "bookmarks.list", json!({"tag":"开发"})).unwrap()["items"],
        json!([])
    );
    let mut edited = item.clone();
    edited["archived"] = json!(true);
    edited.as_object_mut().unwrap().remove("tags");
    assert_eq!(
        dispatch(t.path(), "bookmarks.save", json!({"bookmark":edited})).unwrap()["tags"],
        item["tags"]
    );
    for url in [
        "file:///C:/Windows",
        "javascript:alert(1)",
        "https://user:secret@example.com",
    ] {
        assert!(dispatch(t.path(), "links.open", json!({"url":url})).is_err());
    }
}

#[test]
fn bookmarks_work_offline_without_installing_and_preserve_manual_state() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let repos = [
        "viarotel-org/escrcpy",
        "unclecode/crawl4ai",
        "yanliudesign/mono-color-skill",
        "kangarooking/cangjie-skill",
        "NanmiCoder/MediaCrawler",
        "OpenCut-app/OpenCut",
        "KKKKhazix/khazix-skills",
    ];
    let mut saved = Vec::new();
    for repo in repos {
        let item = dispatch(
            &data,
            "bookmarks.save",
            json!({"url":format!("https://github.com/{repo}")}),
        )
        .unwrap();
        assert_eq!(item["name"], repo.split('/').last().unwrap());
        assert_eq!(item["status"], "interested");
        saved.push(item);
    }
    assert_eq!(
        dispatch(&data, "bookmarks.list", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    assert!(dispatch(&data, "snapshot", json!({})).unwrap()["projects"]
        .as_array()
        .unwrap()
        .is_empty());
    let project = t.path().join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("keep.txt"), "original").unwrap();
    let kit = dispatch(
        &data,
        "projects.save",
        json!({"path":project,"kind":"workspace"}),
    )
    .unwrap();
    let id = kit["toolkit"]["id"]
        .as_str()
        .or_else(|| kit["id"].as_str())
        .unwrap();
    let mut edited = saved[0].clone();
    edited["name"] = json!("手机镜像");
    edited["notes"] = json!("稍后了解");
    edited["status"] = json!("uninstalled");
    edited["projectId"] = json!(id);
    dispatch(&data, "bookmarks.save", json!({"bookmark":edited})).unwrap();
    assert_eq!(
        dispatch(
            &data,
            "bookmarks.list",
            json!({"query":"稍后","status":"in_use"})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    dispatch(
        &data,
        "projects.archive",
        json!({"projectId":id,"archived":true}),
    )
    .unwrap();
    assert_eq!(
        dispatch(&data, "bookmarks.list", json!({"status":"in_use"})).unwrap()["items"][0]
            ["status"],
        "in_use"
    );
    dispatch(&data, "bookmarks.delete", json!({"id":edited["id"]})).unwrap();
    assert_eq!(
        fs::read_to_string(project.join("keep.txt")).unwrap(),
        "original"
    );
    assert_eq!(
        dispatch(&data, "bookmarks.list", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(dispatch(
        &data,
        "bookmarks.save",
        json!({"url":"javascript:alert(1)"})
    )
    .is_err());
}

#[test]
fn normalized_repository_addresses_cannot_duplicate_or_overwrite_another_bookmark() {
    let t = tempfile::tempdir().unwrap();
    let first = dispatch(
        t.path(),
        "bookmarks.save",
        json!({"url":"https://github.com/Owner/Repo", "notes":"keep", "status":"in_use"}),
    )
    .unwrap();
    for url in [
        "http://GITHUB.com/owner/repo.git/",
        "https://github.com/owner/repo/?tab=readme#intro",
    ] {
        assert!(dispatch(t.path(), "bookmarks.save", json!({"url":url}))
            .unwrap_err()
            .to_string()
            .contains("已在收藏"));
    }
    let second = dispatch(
        t.path(),
        "bookmarks.save",
        json!({"url":"https://github.com/owner/different"}),
    )
    .unwrap();
    assert!(dispatch(
        t.path(),
        "bookmarks.save",
        json!({"id":second["id"], "url":"https://github.com/owner/repo"})
    )
    .is_err());
    let items = dispatch(t.path(), "bookmarks.list", json!({})).unwrap();
    assert_eq!(items["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        items["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == first["id"])
            .unwrap()["notes"],
        "keep"
    );
    let mut edited = first.clone();
    edited["url"] = json!("https://github.com/owner/repo.git");
    dispatch(t.path(), "bookmarks.save", edited).unwrap();
}

#[test]
fn deleting_project_removes_only_its_linked_bookmarks_and_preserves_files() {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    let path = t.path().join("repo");
    local_repo(&path, "https://github.com/example/repo");
    fs::write(path.join("keep.txt"), "personal").unwrap();
    let project = dispatch(&data, "projects.save", json!({"path":path})).unwrap();
    dispatch(
        &data,
        "projects.trust",
        json!({"projectId":project["id"],"path":path,"trusted":true,"confirmed":true}),
    )
    .unwrap();
    dispatch(&data, "bookmarks.save", json!({"name":"same name","url":"https://github.com/example/second","projectId":project["id"]})).unwrap();
    let independent = dispatch(
        &data,
        "bookmarks.save",
        json!({"name":"same name","url":"https://github.com/example/independent"}),
    )
    .unwrap();
    assert_eq!(
        dispatch(&data, "bookmarks.list", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    dispatch(&data, "projects.delete", json!({"projectId":project["id"]})).unwrap();
    let after = dispatch(&data, "bookmarks.sync", json!({})).unwrap();
    assert_eq!(after["items"].as_array().unwrap().len(), 1);
    assert_eq!(after["items"][0]["id"], independent["id"]);
    assert_eq!(
        fs::read_to_string(path.join("keep.txt")).unwrap(),
        "personal"
    );
}
