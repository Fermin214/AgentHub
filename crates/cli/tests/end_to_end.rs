use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn invoke(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agenthub-dev"))
        .arg("--data-dir")
        .arg(root.join("app"))
        .args(args)
        .env("USERPROFILE", root.join("home"))
        .env("HOME", root.join("home"))
        .env("APPDATA", root.join("appdata"))
        .env("LOCALAPPDATA", root.join("localappdata"))
        .env_remove("HERMES_HOME")
        .output()
        .expect("CLI starts")
}

fn value(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI emits valid JSON")
}

fn call(root: &Path, method: &str, args: Value) -> Value {
    let file = root.join("请求 参数.json");
    fs::write(&file, serde_json::to_vec(&args).unwrap()).unwrap();
    value(invoke(
        root,
        &[
            "call",
            method,
            "--args-file",
            file.to_str().unwrap(),
            "--apply",
        ],
    ))
}

#[test]
fn cli_refuses_implicit_writes_and_removed_commands() {
    let root = TempDir::new().unwrap();
    let out = invoke(
        root.path(),
        &["call", "prompts.delete", "--args", r#"{"id":"any"}"#],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--apply"));
    let out = invoke(root.path(), &["execute", "not-a-plan"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unrecognized subcommand"));
    assert!(!root.path().join("app/agenthub.sqlite3").exists());
}

#[test]
fn prompt_save_and_export_work_across_cli_processes() {
    let root = TempDir::new().unwrap();
    let saved = call(
        root.path(),
        "prompts.save",
        json!({"prompt":{
            "id":"", "title":"中文写作", "body":"第一行\n第二行 **Markdown**", "category":"写作",
            "tags":["常用","中文"], "favorite":true, "createdAt":"", "updatedAt":""
        }}),
    );
    assert!(!saved["id"].as_str().unwrap().is_empty());
    let snapshot = value(invoke(root.path(), &["snapshot", "--json"]));
    assert_eq!(snapshot["prompts"].as_array().unwrap().len(), 1);
    assert_eq!(
        snapshot["prompts"][0]["body"],
        "第一行\n第二行 **Markdown**"
    );
    let exported = call(root.path(), "prompts.export", json!({"format":"json"}));
    assert!(exported["content"].as_str().unwrap().contains("中文写作"));
}

#[test]
fn source_scan_is_read_only_and_same_names_in_different_roots_keep_identity() {
    let root = TempDir::new().unwrap();
    let a = root.path().join("甲 Agent/skills/same-name");
    let b = root.path().join("乙 Agent/skills/same-name");
    for path in [&a, &b] {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            "---\nname: same-name\ndescription: 测试\n---\n保持原文。\n",
        )
        .unwrap();
    }
    let before = fs::read(a.join("SKILL.md")).unwrap();
    let snapshot = value(invoke(root.path(), &["snapshot"]));
    let mut settings = snapshot["settings"].clone();
    settings["scanRoots"] = json!([
        {"id":"a","agent":"codex","scope":"global","path":a.parent().unwrap()},
        {"id":"b","agent":"claude","scope":"project","path":b.parent().unwrap()}
    ]);
    call(root.path(), "settings.save", json!({"settings":settings}));
    let scanned = value(invoke(root.path(), &["scan"]));
    let rows = scanned["deployments"].as_array().unwrap();
    let matching: Vec<_> = rows.iter().filter(|r| r["name"] == "same-name").collect();
    assert_eq!(matching.len(), 2, "{scanned}");
    assert_ne!(matching[0]["id"], matching[1]["id"]);
    assert_eq!(fs::read(a.join("SKILL.md")).unwrap(), before);
    assert_eq!(fs::read(b.join("SKILL.md")).unwrap(), before);
    assert_eq!(fs::read_dir(&a).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&b).unwrap().count(), 1);
}

#[test]
fn relative_data_directory_is_absolute_in_snapshot_and_scan() {
    let root = TempDir::new().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_agenthub-dev"))
        .current_dir(root.path())
        .args(["--data-dir", "relative-data", "snapshot"])
        .env("USERPROFILE", root.path().join("home"))
        .env("HOME", root.path().join("home"))
        .output()
        .unwrap();
    let result = value(output);
    let connection_path = Path::new(result["dataDir"].as_str().unwrap());
    assert!(connection_path.is_absolute());
    assert_eq!(connection_path, root.path().join("relative-data"));
    let output = Command::new(env!("CARGO_BIN_EXE_agenthub-dev"))
        .current_dir(root.path())
        .args([
            "--data-dir",
            "relative-data",
            "call",
            "settings.save",
            "--args",
            r#"{"scanRoots":[]}"#,
            "--apply",
        ])
        .env("USERPROFILE", root.path().join("home"))
        .output()
        .unwrap();
    value(output);
    let output = Command::new(env!("CARGO_BIN_EXE_agenthub-dev"))
        .current_dir(root.path())
        .args(["--data-dir", "relative-data", "scan"])
        .env("USERPROFILE", root.path().join("home"))
        .output()
        .unwrap();
    assert!(value(output)["deployments"].as_array().unwrap().is_empty());
}
