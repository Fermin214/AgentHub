use agenthub_core::{native, skill_changes, skills, sources, store::Store};
use serde_json::{json, Value};
use std::{fs, path::Path};

fn write_skill(path: &Path, name: &str, body: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Fixture description\n---\n{body}\n"),
    )
    .unwrap();
}
fn inspect(store: &Store, path: &Path) -> Value {
    sources::dispatch(
        store,
        "sources.inspect",
        &json!({"source":{"kind":"local","locator":path}}),
    )
    .unwrap()
}
fn add(store: &Store, inspection: &Value, subpath: &str) -> Value {
    skills::dispatch(
        store,
        "skills.add",
        &json!({"inspectionId":inspection["inspectionId"],"subpath":subpath}),
    )
    .unwrap()["skill"]
        .clone()
}
fn scan(store: &Store, roots: Value) -> Value {
    skills::dispatch(store, "skills.scan", &json!({"scanRoots":roots})).unwrap()
}

#[test]
fn same_named_candidates_keep_real_paths_and_compare_entire_contents() {
    let t = tempfile::tempdir().unwrap();
    let source = t.path().join("source");
    for sub in ["alpha", "arbitrary/deep/beta", "plugins/gamma"] {
        write_skill(&source.join(sub), "Same", "identical skill text");
        fs::create_dir(source.join(sub).join("assets")).unwrap();
        fs::write(
            source.join(sub).join("assets/example.txt"),
            if sub == "plugins/gamma" {
                "different"
            } else {
                "same"
            },
        )
        .unwrap();
    }
    let store = Store::open(&t.path().join("data")).unwrap();
    let inspected = inspect(&store, &source);
    let candidates = inspected["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0]["subpath"], "alpha");
    assert_eq!(
        candidates[0]["contentComparison"]["same"],
        json!(["arbitrary/deep/beta"])
    );
    assert_eq!(
        candidates[0]["contentComparison"]["different"],
        json!(["plugins/gamma"])
    );
    assert_eq!(
        candidates[2]["contentComparison"]["different"],
        json!(["alpha", "arbitrary/deep/beta"])
    );
    assert!(store.list_skills().unwrap().is_empty());
}

#[test]
fn removing_source_preserves_library_and_deployed_files_and_records_only_real_change() {
    let t = tempfile::tempdir().unwrap();
    let store = Store::open(&t.path().join("data")).unwrap();
    let root = t.path().join("agent-skills");
    write_skill(&root.join("writer"), "writer", "personal content");
    let scanned = scan(
        &store,
        json!([{"id":"fixture","agent":"codex","scope":"global","path":root}]),
    );
    let imported = skills::dispatch(
        &store,
        "skills.import",
        &json!({"deploymentIds":[scanned["deployments"][0]["id"]]}),
    )
    .unwrap();
    let item = &imported["results"][0]["skill"];
    let source = t.path().join("source");
    write_skill(&source, "writer", "remote content");
    skills::dispatch(&store,"skills.bindSource",&json!({"skillId":item["id"],"inspectionId":inspect(&store,&source)["inspectionId"],"subpath":""})).unwrap();
    let before = store.list_operations().unwrap().len();
    skills::dispatch(
        &store,
        "skills.unbindSource",
        &json!({"skillId":item["id"]}),
    )
    .unwrap();
    let saved = store
        .get_skill(item["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(saved.source.kind, "unknown");
    assert!(saved.source.locator.is_empty());
    assert_eq!(
        fs::read(root.join("writer/SKILL.md")).unwrap(),
        fs::read(Path::new(&saved.path).join("SKILL.md")).unwrap()
    );
    assert_eq!(store.list_deployments().unwrap().len(), 1);
    assert_eq!(store.list_operations().unwrap().len(), before + 1);
    skills::dispatch(
        &store,
        "skills.unbindSource",
        &json!({"skillId":item["id"]}),
    )
    .unwrap();
    assert_eq!(store.list_operations().unwrap().len(), before + 1);
}

#[test]
fn adding_matching_source_binds_imported_skill_without_replacing_library_or_agent_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let root = temp.path().join("agent-skills");
    write_skill(&root.join("obsidian-bases"), "obsidian-bases", "original");
    let scanned = scan(
        &store,
        json!([{"id":"fixture","agent":"codex","scope":"global","path":root}]),
    );
    let imported = skills::dispatch(
        &store,
        "skills.import",
        &json!({"deploymentIds":[scanned["deployments"][0]["id"]]}),
    )
    .unwrap();
    let item = &imported["results"][0]["skill"];
    assert_eq!(item["source"]["kind"], "unknown");
    let source = temp.path().join("repository/skills/obsidian-bases");
    write_skill(&source, "obsidian-bases", "original");
    let rebound = add(
        &store,
        &inspect(&store, &temp.path().join("repository")),
        "skills/obsidian-bases",
    );
    assert_eq!(rebound["id"], item["id"]);
    assert_eq!(rebound["source"]["kind"], "local");
    assert_eq!(rebound["source"]["subpath"], "skills/obsidian-bases");
    assert_eq!(store.list_skills().unwrap().len(), 1);
    assert_eq!(
        fs::read(root.join("obsidian-bases/SKILL.md")).unwrap(),
        fs::read(Path::new(item["path"].as_str().unwrap()).join("SKILL.md")).unwrap()
    );
    let mut bound = store.list_skills().unwrap().remove(0);
    bound.source.kind = "git".into();
    bound.source.locator = "https://github.com/example/obsidian-skills".into();
    store.save_skill(&bound).unwrap();
    let repositories = sources::dispatch(&store, "repositories.list", &json!({})).unwrap();
    assert_eq!(repositories["repositories"][0]["derived"], true);
    assert_eq!(
        repositories["repositories"][0]["source"]["locator"],
        "https://github.com/example/obsidian-skills"
    );
    sources::dispatch(
        &store,
        "repositories.remove",
        &json!({"id":repositories["repositories"][0]["id"]}),
    )
    .unwrap();
    assert_eq!(
        sources::dispatch(&store, "repositories.list", &json!({})).unwrap()["repositories"],
        json!([])
    );
    assert_eq!(
        store.get_skill(&bound.id).unwrap().unwrap().source,
        bound.source
    );
    assert!(skills::dispatch(
        &store,
        "skills.add",
        &json!({"inspectionId":inspect(&store, &source)["inspectionId"],"subpath":""})
    )
    .is_err());
}

#[test]
fn metadata_only_changes_do_not_report_an_update_without_visible_differences() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    write_skill(&source, "Metadata", "same content");
    fs::create_dir(source.join(".git")).unwrap();
    fs::write(source.join(".git/state"), "first").unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let item = add(&store, &inspect(&store, &source), "");
    fs::write(source.join(".git/state"), "second").unwrap();
    let checked = skills::check(&store, item["id"].as_str().unwrap()).unwrap();
    assert_eq!(checked["status"], "current", "{checked}");
    assert_eq!(checked["locations"][0]["differences"], json!([]));
    write_skill(&source, "Metadata", "new content");
    let checked = skills::check(&store, item["id"].as_str().unwrap()).unwrap();
    assert_eq!(checked["status"], "available", "{checked}");
    assert!(!checked["locations"][0]["differences"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn git_source_identity_normalizes_remote_suffix_but_preserves_revision_subpath_and_local_path() {
    let source = |url: &str, revision: &str, subpath: &str| {
        serde_json::from_value(
            json!({"kind":"git", "locator":url,"revision":revision,"subpath":subpath}),
        )
        .unwrap()
    };
    let first = sources::source_identity(&source(
        "https://github.com/Owner/Repo.git",
        "main",
        "skills/one",
    ))
    .unwrap();
    assert_eq!(
        first,
        sources::source_identity(&source(
            "https://github.com/owner/repo",
            "main",
            "skills/one"
        ))
        .unwrap()
    );
    assert_ne!(
        first,
        sources::source_identity(&source("https://github.com/owner/repo", "v1", "skills/one"))
            .unwrap()
    );
    assert_ne!(
        first,
        sources::source_identity(&source(
            "https://github.com/owner/repo",
            "main",
            "skills/two"
        ))
        .unwrap()
    );
    let fixture = tempfile::tempdir().unwrap();
    let local = fixture
        .path()
        .join("bare.git")
        .to_string_lossy()
        .to_string();
    assert_eq!(
        sources::source_identity(&source(&local, "main", ""))
            .unwrap()
            .locator,
        local
    );
}

#[test]
fn multi_skill_source_with_plugin_marker_is_browsable_and_released_snapshots_expire() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let repository = temp.path().join("published");
    write_skill(&repository.join("skills/one"), "One", "one");
    write_skill(&repository.join("skills/two"), "Two", "two");
    fs::create_dir(repository.join(".claude-plugin")).unwrap();
    fs::write(repository.join(".claude-plugin/plugin.json"), "{}").unwrap();
    let inspected = inspect(&store, &repository);
    assert_eq!(
        inspected["candidates"].as_array().unwrap().len(),
        2,
        "{inspected}"
    );
    let first = add(&store, &inspected, "skills/one");
    let second = add(&store, &inspected, "skills/two");
    assert_ne!(first["id"], second["id"]);
    assert_eq!(first["source"]["subpath"], "skills/one");
    assert_eq!(first["description"], "Fixture description");
    assert_eq!(store.list_deployments().unwrap().len(), 0);
    assert!(repository.join(".claude-plugin/plugin.json").is_file());
    sources::dispatch(
        &store,
        "sources.release",
        &json!({"inspectionId":inspected["inspectionId"]}),
    )
    .unwrap();
    assert!(skills::dispatch(
        &store,
        "skills.add",
        &json!({"inspectionId":inspected["inspectionId"],"subpath":"skills/one"})
    )
    .is_err());
    assert!(Path::new(first["path"].as_str().unwrap())
        .join("SKILL.md")
        .is_file());
}

#[test]
fn browsing_a_saved_repository_reports_library_membership_per_skill_without_installing() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let repository = temp.path().join("published");
    write_skill(&repository.join("skills/one"), "One", "one");
    write_skill(&repository.join("skills/two"), "Two", "two");
    let first = inspect(&store, &repository);
    for candidate in first["candidates"].as_array().unwrap() {
        assert!(candidate["skillId"].is_null(), "{candidate}");
    }
    let saved = add(&store, &first, "skills/one");
    let again = inspect(&store, &repository);
    let membership: Vec<_> = again["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["subpath"].clone(), c["skillId"].clone()))
        .collect();
    assert_eq!(
        membership,
        vec![
            (json!("skills/one"), saved["id"].clone()),
            (json!("skills/two"), Value::Null)
        ],
        "{again}"
    );
    // Browsing and library membership say nothing about Agent install locations.
    assert_eq!(store.list_deployments().unwrap().len(), 0);
    let plan = skill_changes::dispatch(
        &store,
        "skills.delete.preview",
        &json!({"skillId":saved["id"]}),
    )
    .unwrap();
    skill_changes::dispatch(
        &store,
        "skills.delete",
        &json!({"planId":plan["id"],"confirmed":true}),
    )
    .unwrap();
    let removed = inspect(&store, &repository);
    for candidate in removed["candidates"].as_array().unwrap() {
        assert!(candidate["skillId"].is_null(), "{candidate}");
    }
}

#[test]
fn import_links_identical_agent_copies_but_reports_conflicting_content_without_changes() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let first = temp.path().join("codex/skills");
    let second = temp.path().join("claude/skills");
    let conflict = temp.path().join("hermes/skills");
    write_skill(&first.join("same"), "Same", "common");
    write_skill(&second.join("same"), "Same", "common");
    write_skill(&conflict.join("same"), "Same", "personal changes");
    let roots = json!([{"agent":"codex","scope":"global","path":first},{"agent":"claude","scope":"global","path":second},{"agent":"hermes","scope":"global","path":conflict}]);
    let discovered = scan(&store, roots.clone());
    let ids: Vec<_> = discovered["deployments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].clone())
        .collect();
    let imported =
        skills::dispatch(&store, "skills.import", &json!({"deploymentIds":ids})).unwrap();
    assert_eq!(
        imported["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["status"] == "succeeded")
            .count(),
        2,
        "{imported}"
    );
    assert_eq!(
        imported["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["status"] == "failed")
            .count(),
        1,
        "{imported}"
    );
    assert_eq!(store.list_skills().unwrap().len(), 1);
    let linked = store.list_deployments().unwrap();
    assert_eq!(linked.iter().filter(|d| d.skill_id.is_some()).count(), 2);
    assert!(fs::read_to_string(conflict.join("same/SKILL.md"))
        .unwrap()
        .contains("personal changes"));
    let failed = linked.iter().find(|d| d.skill_id.is_none()).unwrap();
    skills::dispatch(
        &store,
        "skills.ignore",
        &json!({"deploymentIds":[failed.id],"ignored":true}),
    )
    .unwrap();
    scan(&store, roots.clone());
    assert!(store.get_deployment(&failed.id).unwrap().unwrap().ignored);
    fs::remove_file(first.join("same/SKILL.md")).unwrap();
    let rescanned = scan(&store, roots);
    let missing = rescanned["deployments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["agent"] == "codex")
        .unwrap();
    assert_eq!(missing["present"], false);
    assert_eq!(missing["status"], "missing");
    assert!(missing["skillId"].is_string());
    assert_eq!(store.list_skills().unwrap().len(), 1);
}

#[test]
fn source_check_diff_metadata_and_text_access_preserve_explicit_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let upstream = temp.path().join("upstream");
    write_skill(&upstream, "Example", "version one");
    let inspected = inspect(&store, &upstream);
    let item = add(&store, &inspected, "");
    let id = &item["id"];
    let current = skills::dispatch(&store, "skills.check", &json!({"skillId":id})).unwrap();
    assert_eq!(current["status"], "current", "{current}");
    write_skill(&upstream, "Example", "version two");
    let checked = skills::dispatch(&store, "skills.check", &json!({"skillId":id})).unwrap();
    assert_eq!(checked["status"], "available", "{checked}");
    let diff = skills::dispatch(
        &store,
        "skills.diff",
        &json!({"checkId":checked["checkId"],"locationId":"library","path":"SKILL.md"}),
    )
    .unwrap();
    assert!(diff["oldText"].as_str().unwrap().contains("version one"));
    assert!(diff["newText"].as_str().unwrap().contains("version two"));
    let content = skills::dispatch(&store, "skills.read", &json!({"skillId":id})).unwrap();
    assert!(content["content"].as_str().unwrap().contains("version one"));
    assert!(skills::dispatch(
        &store,
        "skills.read",
        &json!({"skillId":id,"path":"../agenthub.sqlite3"})
    )
    .is_err());
    let files = skills::dispatch(&store, "skills.files", &json!({"skillId":id})).unwrap();
    assert_eq!(files["files"][0]["path"], "SKILL.md");
    let metadata = skills::dispatch(
        &store,
        "skills.metadata.save",
        &json!({"ids":[id],"tags":[" Notes ","Notes",""],"favorite":true}),
    )
    .unwrap();
    assert_eq!(metadata["skills"][0]["tags"], json!(["Notes"]));
    assert_eq!(metadata["skills"][0]["favorite"], true);
    assert!(
        skills::checked_update(&store, checked["checkId"].as_str().unwrap()).is_err(),
        "record changes invalidate prior comparisons"
    );
    let rebound = inspect(&store, &upstream);
    skills::dispatch(
        &store,
        "skills.bindSource",
        &json!({"skillId":id,"inspectionId":rebound["inspectionId"],"subpath":""}),
    )
    .unwrap();
    assert!(
        fs::read_to_string(Path::new(item["path"].as_str().unwrap()).join("SKILL.md"))
            .unwrap()
            .contains("version one")
    );
    let checked = skills::dispatch(&store, "skills.check", &json!({"skillId":id})).unwrap();
    assert_eq!(checked["status"], "modified", "{checked}");
    fs::write(
        Path::new(item["path"].as_str().unwrap()).join("SKILL.md"),
        "local edit after check",
    )
    .unwrap();
    assert!(skills::checked_update(&store, checked["checkId"].as_str().unwrap()).is_err());
}

#[test]
fn shared_physical_location_is_compared_once_for_both_agents() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let source = temp.path().join("source");
    write_skill(&source, "Shared", "v1");
    let deployed = temp.path().join("shared/skills");
    write_skill(&deployed.join("shared"), "Shared", "v1");
    let scanned = scan(
        &store,
        json!([{"agent":"codex","scope":"global","path":deployed},{"agent":"claude","scope":"global","path":deployed}]),
    );
    let ids: Vec<_> = scanned["deployments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].clone())
        .collect();
    let imported =
        skills::dispatch(&store, "skills.import", &json!({"deploymentIds":ids})).unwrap();
    let id = imported["results"][0]["skill"]["id"].clone();
    let inspected = inspect(&store, &source);
    skills::dispatch(
        &store,
        "skills.bindSource",
        &json!({"skillId":id,"inspectionId":inspected["inspectionId"]}),
    )
    .unwrap();
    let checked = skills::dispatch(&store, "skills.check", &json!({"skillId":id})).unwrap();
    assert_eq!(checked["status"], "current", "{checked}");
    assert_eq!(checked["locations"].as_array().unwrap().len(), 2);
    let shared = checked["locations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] != "library")
        .unwrap();
    assert_eq!(shared["agents"], json!(["claude", "codex"]));
}

#[test]
fn changed_inspection_and_same_name_collision_never_silently_overwrite_library() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let source = temp.path().join("source");
    write_skill(&source, "Same", "original");
    let inspected = inspect(&store, &source);
    let item = add(&store, &inspected, "");
    write_skill(&source, "Same", "different");
    let different = inspect(&store, &source);
    assert!(skills::dispatch(
        &store,
        "skills.add",
        &json!({"inspectionId":different["inspectionId"]})
    )
    .is_err());
    assert!(
        fs::read_to_string(Path::new(item["path"].as_str().unwrap()).join("SKILL.md"))
            .unwrap()
            .contains("original")
    );
    let snapshot = store
        .data_dir()
        .join("source-inspections")
        .join(inspected["inspectionId"].as_str().unwrap())
        .join("source/SKILL.md");
    fs::write(snapshot, "tampered").unwrap();
    assert!(skills::dispatch(
        &store,
        "skills.add",
        &json!({"inspectionId":inspected["inspectionId"]})
    )
    .is_err());
}

#[test]
fn zip_wrapper_with_multiple_skills_has_the_same_inspection_contract() {
    use std::io::{Cursor, Write};
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (path, name) in [
        ("release/skills/one/SKILL.md", "One"),
        ("release/skills/two/SKILL.md", "Two"),
    ] {
        writer.start_file(path, options).unwrap();
        writer
            .write_all(format!("---\nname: {name}\n---\nSkill").as_bytes())
            .unwrap();
    }
    writer
        .start_file("release/.claude-plugin/plugin.json", options)
        .unwrap();
    writer.write_all(b"{}").unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let temp = tempfile::tempdir().unwrap();
    let extracted = temp.path().join("extracted");
    native::extract_zip(Cursor::new(bytes), &extracted).unwrap();
    assert_eq!(native::unwrap_archive_root(&extracted).unwrap(), "release");
    let store = Store::open(&temp.path().join("data")).unwrap();
    let inspected = inspect(&store, &extracted);
    assert_eq!(inspected["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(add(&store, &inspected, "skills/two")["name"], "Two");
}

#[test]
fn metadata_returns_only_requested_records_and_preserves_unrelated_fields() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("data")).unwrap();
    let source = temp.path().join("source");
    write_skill(&source.join("one"), "one", "fictional");
    write_skill(&source.join("two"), "two", "fictional");
    let inspection = inspect(&store, &source);
    let first = add(&store, &inspection, "one");
    let second = add(&store, &inspection, "two");
    let save = |args: Value| skills::dispatch(&store, "skills.metadata.save", &args).unwrap();
    let tags = save(json!({"ids":[first["id"]],"tags":[" kept ","kept",""]}));
    assert_eq!(tags["skills"].as_array().unwrap().len(), 1);
    assert_eq!(tags["skills"][0]["id"], first["id"]);
    assert_eq!(tags["skills"][0]["tags"], json!(["kept"]));
    assert_eq!(tags["skills"][0]["favorite"], false);
    let favorite = save(json!({"ids":[first["id"]],"favorite":true}));
    assert_eq!(favorite["skills"].as_array().unwrap().len(), 1);
    assert_eq!(favorite["skills"][0]["tags"], json!(["kept"]));
    let retag = save(json!({"ids":[first["id"]],"tags":["new"]}));
    assert_eq!(retag["skills"][0]["favorite"], true);
    let unrelated = save(json!({"ids":[second["id"]]}));
    assert_eq!(unrelated["skills"][0]["tags"], json!([]));
    assert_eq!(unrelated["skills"][0]["favorite"], false);
    assert!(skills::dispatch(&store, "skills.metadata.save", &json!({"ids":[first["id"]],"favorite":"true"})).is_err());
}
