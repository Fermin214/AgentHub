use agenthub_core::native::{
    extract_zip, inspect_source, normalize_source, stage, unwrap_archive_root, validate_redirect,
};
use serde_json::json;
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
};
fn skill(path: &Path, name: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: readable description\n---\n# body\n"),
    )
    .unwrap();
}
#[test]
fn multi_skill_inspection_does_not_install_all_and_selection_preserves_resources() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("repo");
    skill(&source.join("skills/one"), "one");
    skill(&source.join("skills/two"), "two");
    fs::write(source.join("README.md"), "complete resource").unwrap();
    let spec = json!({"kind":"local","locator":source});
    let inspected = inspect_source(&spec, &fixture.path().join("inspection")).unwrap();
    assert_eq!(inspected["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(inspected["candidates"][0]["subpath"], "skills/one");
    assert_eq!(
        inspected["candidates"][0]["description"],
        "readable description"
    );
    assert!(
        stage(&json!({"source":spec}), &fixture.path().join("ambiguous"))
            .unwrap_err()
            .to_string()
            .contains("Multiple Skills")
    );
    let selected = stage(
        &json!({"source":{"kind":"local","locator":source,"subpath":"skills/two"}}),
        &fixture.path().join("selected"),
    )
    .unwrap();
    assert!(selected.ends_with("source/skills/two"));
    assert!(fixture.path().join("selected/source/README.md").is_file());
}
#[test]
fn unique_nested_skill_is_auto_selected_but_workspace_without_skill_is_not() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("workspace");
    skill(&source.join(".agents/skills/one"), "one");
    let selected = stage(
        &json!({"source":{"kind":"local","locator":source}}),
        &fixture.path().join("stage"),
    )
    .unwrap();
    assert!(selected.ends_with("source/.agents/skills/one"));
    let empty = fixture.path().join("empty");
    fs::create_dir(&empty).unwrap();
    fs::write(empty.join("package.json"), "{}").unwrap();
    assert!(stage(
        &json!({"source":{"kind":"local","locator":empty}}),
        &fixture.path().join("nostage")
    )
    .is_err());
}
#[test]
fn archive_wrapper_is_removed_without_changing_candidate_subpaths() {
    let fixture = tempfile::tempdir().unwrap();
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in [
        ("release/skills/one/SKILL.md", "# one"),
        ("release/skills/two/SKILL.md", "# two"),
        ("release/README.md", "preserve"),
    ] {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(text.as_bytes()).unwrap();
    }
    let payload = fixture.path().join("payload");
    extract_zip(archive.finish().unwrap(), &payload).unwrap();
    assert_eq!(unwrap_archive_root(&payload).unwrap(), "release");
    assert_eq!(
        fs::read_to_string(payload.join("README.md")).unwrap(),
        "preserve"
    );
    let result = inspect_source(
        &json!({"kind":"local","locator":payload}),
        &fixture.path().join("inspect"),
    )
    .unwrap();
    assert_eq!(result["candidates"][0]["subpath"], "skills/one");
    assert_eq!(result["candidates"][1]["subpath"], "skills/two");
}
#[test]
fn redirect_policy_accepts_https_relative_and_signed_download_but_rejects_downgrade() {
    assert_eq!(
        validate_redirect("https://example.test/a", "../b", 0)
            .unwrap()
            .as_str(),
        "https://example.test/b"
    );
    assert!(validate_redirect(
        "https://example.test/a",
        "https://cdn.test/archive?signature=transient",
        4
    )
    .is_ok());
    for location in [
        "http://example.test/archive",
        "https://user:password@example.test/archive",
        "file:///tmp/archive",
        "https://example.test/archive#secret",
    ] {
        assert!(validate_redirect("https://example.test/a", location, 0).is_err());
    }
    assert!(validate_redirect("https://example.test/a", "/b", 5).is_err());
}
#[test]
fn github_syntax_normalization_preserves_explicit_selection_and_revision() {
    let simple = normalize_source(&json!({"kind":"git","locator":"org/repo"})).unwrap();
    assert_eq!(simple["locator"], "https://github.com/org/repo");
    let tree = normalize_source(
        &json!({"kind":"git","locator":"https://github.com/org/repo/tree/main/skills/one"}),
    )
    .unwrap();
    assert_eq!(tree["locator"], "https://github.com/org/repo.git");
    assert_eq!(tree["revision"], "main");
    assert_eq!(tree["subpath"], "skills/one");
    let slash=normalize_source(&json!({"kind":"git","locator":"https://github.com/org/repo/tree/feature/test/skills/one","revision":"feature/test"})).unwrap();
    assert_eq!(slash["subpath"], "skills/one");
    for sub in ["../outside", "/root", "x\\y", "a/../b", "C:/Windows"] {
        assert!(
            normalize_source(&json!({"kind":"local","locator":"source","subpath":sub})).is_err()
        );
    }
}
#[test]
fn sha_revision_and_branch_tag_are_not_mistaken_for_command_options() {
    for revision in [
        "main",
        "v1.0.0",
        "abcdef0123456789abcdef0123456789abcdef0123",
    ] {
        assert!(
            normalize_source(&json!({"kind":"git","locator":"org/repo","revision":revision}))
                .is_ok()
        );
    }
    assert!(normalize_source(
        &json!({"kind":"git","locator":"org/repo","revision":"--upload-pack=evil"})
    )
    .is_err());
}
#[test]
fn traversal_and_archive_links_are_rejected() {
    let fixture = tempfile::tempdir().unwrap();
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file("../outside", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"bad").unwrap();
    assert!(extract_zip(archive.finish().unwrap(), &fixture.path().join("payload")).is_err());
    assert!(!fixture.path().join("outside").exists());
    let mut linked = zip::ZipWriter::new(Cursor::new(Vec::new()));
    linked
        .add_symlink("link", "outside", zip::write::SimpleFileOptions::default())
        .unwrap();
    assert!(extract_zip(linked.finish().unwrap(), &fixture.path().join("linked")).is_err());
}
#[cfg(windows)]
#[test]
fn discovery_rejects_junction_source_instead_of_following_it() {
    let fixture = tempfile::tempdir().unwrap();
    let real = fixture.path().join("real");
    let link = fixture.path().join("junction");
    skill(&real, "real");
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&real)
        .output()
        .unwrap();
    assert!(output.status.success(), "junction fixture creation failed");
    assert!(inspect_source(
        &json!({"kind":"local","locator":link}),
        &fixture.path().join("inspection")
    )
    .is_err());
    fs::remove_dir(&link).unwrap();
}

#[test]
fn archive_wrapper_can_contain_directory_with_its_own_name() {
    let fixture = tempfile::tempdir().unwrap();
    let payload = fixture.path().join("payload");
    skill(&payload.join("release/release"), "nested");
    assert_eq!(unwrap_archive_root(&payload).unwrap(), "release");
    assert!(payload.join("release/SKILL.md").is_file());
}
