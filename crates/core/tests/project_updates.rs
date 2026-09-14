use agenthub_core::dispatch;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "core.autocrlf=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
struct Repo {
    _temp: tempfile::TempDir,
    data: PathBuf,
    local: PathBuf,
    origin: PathBuf,
    kit: Value,
}
impl Repo {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let origin = temp.path().join("upstream");
        fs::create_dir(&origin).unwrap();
        git(&origin, &["init", "--initial-branch=main"]);
        fs::write(origin.join("README.md"), "v1").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "-m", "initial"]);
        let local = temp.path().join("checkout");
        git(
            temp.path(),
            &["clone", origin.to_str().unwrap(), local.to_str().unwrap()],
        );
        let data = temp.path().join("data");
        let kit = dispatch(
            &data,
            "projects.save",
            json!({"path":local,"kind":"repository"}),
        )
        .unwrap();
        dispatch(
            &data,
            "projects.trust",
            json!({"projectId":kit["id"],"path":local,"trusted":true,"confirmed":true}),
        )
        .unwrap();
        Self {
            _temp: temp,
            data,
            local,
            origin,
            kit,
        }
    }
    fn call(&self, method: &str, mut args: Value) -> Value {
        args["projectId"] = self.kit["id"].clone();
        dispatch(&self.data, method, args).unwrap()
    }
    fn advance(&self) {
        fs::write(self.origin.join("README.md"), "v2").unwrap();
        git(&self.origin, &["add", "."]);
        git(&self.origin, &["commit", "-m", "new content"]);
    }
}

#[test]
fn git_checks_report_upstream_changes_without_touching_worktree() {
    let r = Repo::new();
    assert_eq!(
        r.call("projects.check", json!({}))["update"]["status"],
        "current"
    );
    let head = r.call("projects.inspect", json!({}))["state"]["head"].clone();
    fs::write(r.local.join(".git/info/exclude"), "personal.txt\n").unwrap();
    fs::write(r.local.join("personal.txt"), "private").unwrap();
    fs::write(r.local.join("local.md"), "untracked").unwrap();
    fs::write(r.local.join("README.md"), "my edit").unwrap();
    r.advance();
    let checked = r.call("projects.check", json!({}));
    assert_eq!(checked["update"]["status"], "available");
    assert_eq!(checked["state"]["behind"], 1);
    assert_eq!(checked["state"]["commonAncestor"], head);
    assert_eq!(checked["state"]["dirty"], true);
    assert_eq!(checked["state"]["head"], head);
    assert!(checked["state"]["upstreamCommits"]
        .as_str()
        .unwrap()
        .contains("new content"));
    assert!(checked["state"]["upstreamChanges"]
        .as_str()
        .unwrap()
        .contains("README.md"));
    for (file, text) in [
        ("README.md", "my edit"),
        ("personal.txt", "private"),
        ("local.md", "untracked"),
    ] {
        assert_eq!(fs::read_to_string(r.local.join(file)).unwrap(), text);
    }
    assert_eq!(
        r.call("projects.status", json!({}))["update"],
        checked["update"]
    );
    git(
        &r.local,
        &[
            "remote",
            "set-url",
            "origin",
            r.local.join("missing-origin").to_str().unwrap(),
        ],
    );
    let failed = r.call("projects.check", json!({}));
    assert_eq!(failed["update"]["status"], "failed");
    assert_eq!(
        failed["update"]["fetchedAt"],
        checked["update"]["fetchedAt"]
    );
    let local = r.call("projects.inspect", json!({}));
    assert_eq!(local["state"]["behind"], 1);
    assert!(local["state"]["upstreamChanges"]
        .as_str()
        .unwrap()
        .contains("README.md"));
    assert_eq!(
        local["lastCheck"]["checkedAt"],
        failed["update"]["checkedAt"]
    );
    assert_eq!(
        r.call("projects.status", json!({}))["update"]["status"],
        "failed"
    );
}

#[test]
fn checks_handle_divergence_missing_upstream_and_nested_folders() {
    let r = Repo::new();
    r.advance();
    fs::write(r.local.join("local.md"), "mine").unwrap();
    git(&r.local, &["add", "."]);
    git(&r.local, &["commit", "-m", "local change"]);
    let checked = r.call("projects.check", json!({}));
    assert_eq!(checked["update"]["status"], "diverged");
    assert_eq!(checked["state"]["ahead"], 1);
    assert_eq!(checked["state"]["behind"], 1);
    assert!(checked["state"]["commonAncestorSummary"]
        .as_str()
        .unwrap()
        .ends_with("initial"));
    assert!(!checked["state"]["upstreamCommits"]
        .as_str()
        .unwrap()
        .contains("local change"));
    assert!(!checked["state"]["upstreamCommits"]
        .as_str()
        .unwrap()
        .contains("initial"));
    let nested = r.local.join("nested");
    fs::create_dir(&nested).unwrap();
    let kit = dispatch(
        &r.data,
        "projects.save",
        json!({"path":nested,"kind":"workspace"}),
    )
    .unwrap();
    dispatch(
        &r.data,
        "projects.trust",
        json!({"projectId":kit["id"],"path":nested,"trusted":true,"confirmed":true}),
    )
    .unwrap();
    let result = dispatch(&r.data, "projects.check", json!({"projectId":kit["id"]})).unwrap();
    assert_eq!(result["update"]["status"], "unsupported");
    assert_eq!(result["state"]["isGit"], false);
    git(&r.local, &["branch", "--unset-upstream"]);
    assert_eq!(
        r.call("projects.check", json!({}))["update"]["status"],
        "unsupported"
    );
    assert_eq!(fs::read_to_string(r.local.join("README.md")).unwrap(), "v1");
}

#[test]
fn upstream_only_range_pages_newest_first_and_rejects_stale_heads() {
    let r = Repo::new();
    for n in 1..=57 {
        git(
            &r.origin,
            &["commit", "--allow-empty", "-m", &format!("upstream-{n:02}")],
        );
    }
    let checked = r.call("projects.check", json!({}));
    let state = &checked["state"];
    assert_eq!(state["commitRangeTotal"], 57);
    assert!(state["commonAncestorSummary"]
        .as_str()
        .unwrap()
        .ends_with("initial"));
    let mut lines: Vec<_> = state["upstreamCommits"]
        .as_str()
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 50);
    let next = r.call(
        "projects.commits",
        json!({"offset":50,"head":state["head"],"upstreamHead":state["upstreamHead"]}),
    );
    lines.extend(next["commits"].as_str().unwrap().lines().map(str::to_owned));
    assert_eq!(lines.len(), 57);
    for (i, line) in lines.iter().enumerate() {
        assert!(line.ends_with(&format!("upstream-{:02}", 57 - i)), "{line}");
    }
    git(
        &r.local,
        &["commit", "--allow-empty", "-m", "local changes"],
    );
    assert!(dispatch(&r.data,"projects.commits",json!({"projectId":r.kit["id"],"offset":50,"head":state["head"],"upstreamHead":state["upstreamHead"]})).is_err());
}

#[test]
fn git_trust_gates_all_entries_and_revocation_and_path_edits_invalidate_it() {
    let r = Repo::new();
    // A harmless SSH helper writes one marker and exits; it never connects.
    let marker = r._temp.path().join("ssh-helper-marker");
    let command = format!(
        "echo helper-invoked >> '{}'; exit 1",
        marker.to_string_lossy().replace('\\', "/")
    );
    git(&r.local, &["config", "core.sshCommand", &command]);
    git(
        &r.local,
        &[
            "remote",
            "set-url",
            "origin",
            "ssh://git@example.invalid/repo",
        ],
    );
    r.call("projects.trust", json!({"path":r.local,"trusted":false}));
    assert!(!marker.exists());
    let saved = dispatch(
        &r.data,
        "projects.save",
        json!({"id":r.kit["id"],"path":r.local,"gitTrusted":true}),
    )
    .unwrap();
    assert_eq!(
        saved["gitTrusted"], false,
        "ordinary save cannot grant trust"
    );
    dispatch(&r.data, "bookmarks.sync", json!({})).unwrap();
    dispatch(
        &r.data,
        "bookmarks.save",
        json!({"url":"https://example.invalid/unrelated"}),
    )
    .unwrap();
    dispatch(&r.data, "updates.check", json!({})).unwrap();
    for method in ["projects.inspect", "projects.check", "projects.commits"] {
        let error =
            dispatch(&r.data, method, json!({"projectId":r.kit["id"],"offset":0})).unwrap_err();
        assert!(error.to_string().contains("信任"), "{error:#}");
    }
    assert!(
        !marker.exists(),
        "untrusted manual and automatic entries must not start the helper"
    );
    assert!(dispatch(
        &r.data,
        "projects.trust",
        json!({"projectId":r.kit["id"],"path":r.local,"trusted":true})
    )
    .is_err());
    assert!(dispatch(
        &r.data,
        "projects.trust",
        json!({"projectId":r.kit["id"],"path":r.origin,"trusted":true,"confirmed":true})
    )
    .is_err());
    r.call(
        "projects.trust",
        json!({"path":r.local,"trusted":true,"confirmed":true}),
    );
    assert_eq!(
        r.call("projects.check", json!({}))["update"]["status"],
        "failed"
    );
    assert!(
        marker.exists(),
        "trusted repositories preserve configured SSH helpers"
    );
    fs::remove_file(&marker).unwrap();
    r.call("projects.trust", json!({"path":r.local,"trusted":false}));
    assert!(r.call("projects.status", json!({}))["update"].is_null());
    dispatch(&r.data, "updates.check", json!({})).unwrap();
    assert!(!marker.exists());
    r.call(
        "projects.trust",
        json!({"path":r.local,"trusted":true,"confirmed":true}),
    );
    let changed = dispatch(
        &r.data,
        "projects.save",
        json!({"id":r.kit["id"],"path":r.origin}),
    )
    .unwrap();
    assert_eq!(changed["gitTrusted"], false);
    assert!(dispatch(&r.data, "projects.check", json!({"projectId":r.kit["id"]})).is_err());
    let back = dispatch(
        &r.data,
        "projects.save",
        json!({"id":r.kit["id"],"path":r.local}),
    )
    .unwrap();
    assert_eq!(
        back["gitTrusted"], false,
        "returning to an old path does not restore trust"
    );
}
