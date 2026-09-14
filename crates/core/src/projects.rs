//! Read-only Git upstream checks for registered local projects.
use crate::{adapters, model::LocalProject, protection, safe_files, store::Store};
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

fn schema(store: &Store) -> Result<()> {
    store.conn.execute_batch("CREATE TABLE IF NOT EXISTS project_checks(project_id TEXT PRIMARY KEY, payload_json TEXT NOT NULL);")?;
    Ok(())
}

fn root(store: &Store, id: &str) -> Result<PathBuf> {
    let project = store.get_project(id)?.context("项目已不存在")?;
    if !project.git_trusted {
        return Err(crate::errors::coded(
            "GIT_TRUST_REQUIRED",
            json!({"path":project.path}),
            "请先确认信任此项目，才能运行 Git 检查",
        ));
    }
    let path = PathBuf::from(project.path);
    if !path.is_absolute() || !path.is_dir() {
        bail!("项目目录不存在");
    }
    protection::ensure_mutable(&path)?;
    safe_files::safe_directory(&path)?;
    Ok(path)
}

fn last_check(store: &Store, id: &str) -> Result<Value> {
    let raw: Option<String> = store
        .conn
        .query_row(
            "SELECT payload_json FROM project_checks WHERE project_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(raw
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or(Value::Null))
}

fn record_check(
    store: &Store,
    id: &str,
    status: &str,
    message: &str,
    state: Value,
) -> Result<Value> {
    let kit = store.get_project(id)?.context("项目已不存在")?;
    let checked_at = chrono::Utc::now().to_rfc3339();
    let previous = last_check(store, id)?;
    let fetched_at = if matches!(status, "current" | "available" | "diverged") {
        json!(checked_at)
    } else {
        previous["fetchedAt"].clone()
    };
    let update = json!({"projectId":id,"name":kit.name,"status":status,"message":message,"checkedAt":checked_at,"fetchedAt":fetched_at,"state":state});
    store.conn.execute("INSERT INTO project_checks(project_id,payload_json) VALUES(?1,?2) ON CONFLICT(project_id) DO UPDATE SET payload_json=excluded.payload_json", params![id,serde_json::to_string(&update)?])?;
    Ok(update)
}

fn check_git(store: &Store, id: &str, root: &Path) -> Result<Value> {
    let before = inspect(root)?;
    if before["canCheck"] != true {
        let update = record_check(
            store,
            id,
            "unsupported",
            "未配置更新检查：请选择 Git 仓库根目录并设置跟踪上游。",
            before.clone(),
        )?;
        return Ok(json!({"state":before,"checkedAt":update["checkedAt"],"update":update}));
    }
    let work = || -> Result<Value> {
        let remote = before["remote"]
            .as_str()
            .filter(|r| !r.is_empty())
            .context("当前分支未设置远程上游")?;
        if remote.starts_with('-') {
            bail!("远程名称无效");
        }
        let proxy = crate::network::proxy_url(store)?;
        let proxy_arg = format!("http.proxy={proxy}");
        let mut fetch = vec![];
        // An empty application preference means use Git's existing network
        // configuration. Passing http.proxy= would explicitly disable it.
        if !proxy.is_empty() {
            fetch.extend(["-c", proxy_arg.as_str()]);
        }
        fetch.extend([
            "fetch",
            "--no-tags",
            "--no-recurse-submodules",
            "--",
            remote,
        ]);
        git(root, &fetch)?;
        inspect(root)
    };
    match work() {
        Ok(state) => {
            let behind = state["behind"].as_u64().unwrap_or(0);
            let ahead = state["ahead"].as_u64().unwrap_or(0);
            let status = if behind > 0 && ahead > 0 {
                "diverged"
            } else if behind > 0 {
                "available"
            } else {
                "current"
            };
            let message = if behind > 0 {
                format!(
                    "上游有 {behind} 个新提交。{}",
                    state["message"].as_str().unwrap_or("")
                )
            } else {
                "已获取上游，暂无新提交。".into()
            };
            let update = record_check(store, id, status, &message, state.clone())?;
            Ok(json!({"state":state,"checkedAt":update["checkedAt"],"update":update}))
        }
        Err(error) => {
            let update = record_check(store, id, "failed", &format!("{error:#}"), before.clone())?;
            Ok(json!({"state":before,"checkedAt":update["checkedAt"],"update":update}))
        }
    }
}

/// Fetch each registered Git project and cache its upstream comparison.
pub fn check_all(store: &Store) -> Result<Vec<Value>> {
    schema(store)?;
    let mut updates = vec![];
    for kit in store
        .list_projects()?
        .into_iter()
        .filter(|p| !p.archived && p.git_trusted)
    {
        let checked = root(store, &kit.id).and_then(|path| {
            if !path.join(".git").exists() {
                return Ok(Value::Null);
            }
            Ok(check_git(store, &kit.id, &path)?["update"].clone())
        });
        match checked {
            Ok(value) if !value.is_null() => updates.push(value),
            Ok(_) => {}
            Err(error) => updates.push(record_check(
                store,
                &kit.id,
                "failed",
                &format!("{error:#}"),
                Value::Null,
            )?),
        }
    }
    Ok(updates)
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut safe = vec![
        "-c".into(),
        "core.fsmonitor=false".into(),
        "-c".into(),
        format!(
            "core.hooksPath={}",
            std::env::temp_dir()
                .join(format!("agenthub-no-project-hooks-{}", Uuid::new_v4()))
                .display()
        ),
        "-c".into(),
        "submodule.recurse=false".into(),
    ];
    safe.extend(args.iter().map(|s| s.to_string()));
    let result = adapters::run_command("git", &safe, Some(root), Duration::from_secs(120))?;
    if !result.succeeded() {
        bail!("Git：{}", result.message());
    }
    Ok(result.stdout.trim().into())
}
fn inspect(root: &Path) -> Result<Value> {
    // A nested folder must not inspect or fetch the enclosing repository.
    if !root.join(".git").exists() {
        return Ok(
            json!({"isGit":false,"canCheck":false,"path":root,"message":"未配置更新检查：此目录不是 Git 仓库根目录。"}),
        );
    }
    let top = git(root, &["rev-parse", "--show-toplevel"])?;
    if protection::path_key(Path::new(&top)) != protection::path_key(&root.canonicalize()?) {
        bail!("请选择 Git 仓库根目录");
    }
    let head = git(root, &["rev-parse", "HEAD"])?;
    let branch = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default();
    let status = git(
        root,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
    )?;
    let upstream =
        git(root, &["rev-parse", "--symbolic-full-name", "@{upstream}"]).unwrap_or_default();
    let upstream_head = if upstream.is_empty() {
        String::new()
    } else {
        git(root, &["rev-parse", "@{upstream}"])?
    };
    let (ahead, behind) = if upstream.is_empty() {
        (0, 0)
    } else {
        let count = git(
            root,
            &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
        )?;
        let numbers: Vec<usize> = count
            .split_whitespace()
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        (
            *numbers.first().context("无法读取提交数量")?,
            *numbers.get(1).context("无法读取提交数量")?,
        )
    };
    let remote = if branch.is_empty() {
        String::new()
    } else {
        git(
            root,
            &["config", "--get", &format!("branch.{branch}.remote")],
        )
        .unwrap_or_default()
    };
    let remote_url = if remote.is_empty() {
        String::new()
    } else {
        git(root, &["remote", "get-url", &remote]).unwrap_or_default()
    };
    let gitdir = PathBuf::from(git(root, &["rev-parse", "--absolute-git-dir"])?);
    let in_progress = [
        "MERGE_HEAD",
        "rebase-merge",
        "rebase-apply",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_LOG",
    ]
    .iter()
    .any(|p| gitdir.join(p).exists());
    let reason = if in_progress {
        "Git 操作尚未完成。"
    } else if branch.is_empty() {
        "当前未处于分支，未配置跟踪上游。"
    } else if upstream.is_empty() {
        "未配置更新检查：当前分支未设置跟踪上游。"
    } else if !status.is_empty() {
        "项目有本地修改。"
    } else if ahead > 0 && behind > 0 {
        "本地和上游已分叉。"
    } else if behind == 0 {
        "当前本地 Git 记录中没有上游新提交。"
    } else {
        "当前本地 Git 记录中存在上游新提交。"
    };
    let common_ancestor = if upstream_head.is_empty() {
        String::new()
    } else {
        git(root, &["merge-base", "HEAD", &upstream_head]).unwrap_or_default()
    };
    let ancestor_summary = if common_ancestor.is_empty() {
        String::new()
    } else {
        git(
            root,
            &["show", "-s", "--format=%h %s", &common_ancestor, "--"],
        )
        .unwrap_or_default()
    };
    let commits = if common_ancestor.is_empty() {
        String::new()
    } else {
        commit_page(root, &head, &upstream_head, 0)?
    };
    let total = behind;
    // This comparison uses already fetched objects and is available in details
    // without another network request, including after a failed fetch.
    let changes = if common_ancestor.is_empty() {
        String::new()
    } else {
        git(
            root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--stat",
                &common_ancestor,
                &upstream_head,
                "--",
            ],
        )?
    };
    Ok(
        json!({"isGit":true,"path":root,"head":head,"branch":branch,"upstream":upstream,"upstreamHead":upstream_head,"remote":remote,"remoteUrl":remote_url,"ahead":ahead,"behind":behind,"dirty":!status.is_empty(),"changes":status,"inProgress":in_progress,"canCheck":!branch.is_empty()&&!upstream.is_empty(),"message":reason,"commonAncestor":common_ancestor,"commonAncestorSummary":ancestor_summary,"upstreamCommits":commits,"commitRangeTotal":total,"upstreamChanges":changes}),
    )
}

fn commit_page(root: &Path, base: &str, upstream: &str, offset: u64) -> Result<String> {
    let range = format!("{base}..{upstream}");
    let total = git(root, &["rev-list", "--count", &range, "--"])?.parse::<u64>()?;
    if offset >= total {
        return Ok(String::new());
    }
    let limit = (total - offset).min(50);
    git(
        root,
        &[
            "log",
            "--format=%h %s",
            "--topo-order",
            &format!("--max-count={limit}"),
            &format!("--skip={offset}"),
            &range,
            "--",
        ],
    )
}

/// Read the configured upstream/origin address without fetching or changing Git.
pub(crate) fn bookmark_url(project: &LocalProject) -> Option<String> {
    if !project.git_trusted {
        return None;
    }
    let path = Path::new(&project.path);
    if !path.join(".git").exists() || safe_files::safe_directory(path).is_err() {
        return None;
    }
    let branch = git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok();
    let remote = branch
        .and_then(|b| git(path, &["config", "--get", &format!("branch.{b}.remote")]).ok())
        .filter(|r| !r.is_empty() && r != "." && !r.starts_with('-'))
        .unwrap_or_else(|| "origin".into());
    let raw = git(path, &["remote", "get-url", &remote]).ok()?;
    let web = if let Some(rest) = raw.strip_prefix("git@") {
        let (host, repo) = rest.split_once(':')?;
        format!("https://{host}/{repo}")
    } else if let Some(rest) = raw.strip_prefix("ssh://git@") {
        format!("https://{rest}")
    } else {
        raw
    };
    let url = reqwest::Url::parse(&web).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(
        web.trim_end_matches('/')
            .trim_end_matches(".git")
            .to_string(),
    )
}

pub fn dispatch(store: &Store, method: &str, args: &Value) -> Result<Value> {
    schema(store)?;
    match method {
        "projects.save" => {
            let input = args.get("project").unwrap_or(args);
            let old = input["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|id| store.get_project(id)?.context("项目已不存在"))
                .transpose()?;
            let path = PathBuf::from(input["path"].as_str().context("请填写项目目录")?);
            safe_files::safe_directory(&path)?;
            protection::ensure_mutable(&path)?;
            if !path.is_dir() {
                bail!("项目目录不存在");
            }
            let now = chrono::Utc::now().to_rfc3339();
            let name = input["name"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| path.file_name().and_then(|n| n.to_str()).unwrap_or("项目"));
            let project = LocalProject {
                id: old
                    .as_ref()
                    .map(|p| p.id.clone())
                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                name: name.trim().into(),
                path: path.to_string_lossy().into(),
                git_trusted: old
                    .as_ref()
                    .is_some_and(|p| p.git_trusted && p.path == path.to_string_lossy()),
                archived: input["archived"]
                    .as_bool()
                    .unwrap_or_else(|| old.as_ref().is_some_and(|p| p.archived)),
                created_at: old
                    .as_ref()
                    .map(|p| p.created_at.clone())
                    .unwrap_or(now.clone()),
                updated_at: now,
            };
            let transaction = store.conn.unchecked_transaction()?;
            store.save_project(&project)?;
            if old.as_ref().is_some_and(|p| p.path != project.path) {
                store.conn.execute(
                    "DELETE FROM project_checks WHERE project_id=?1",
                    [&project.id],
                )?;
            }
            crate::bookmarks::sync_project(store, &project)?;
            if old.as_ref().is_none_or(|p| p.path != project.path) {
                store.record_activity(
                    "project",
                    &format!(
                        "{}本机项目 · {}",
                        if old.is_none() {
                            "添加"
                        } else {
                            "更改项目目录："
                        },
                        project.name
                    ),
                    &[project.path.clone()],
                )?;
            }
            transaction.commit()?;
            return Ok(json!(project));
        }
        "projects.trust" => {
            let id = args["projectId"].as_str().context("缺少项目 ID")?;
            let mut project = store.get_project(id)?.context("项目已不存在")?;
            let trusted = args["trusted"].as_bool().context("缺少信任状态")?;
            if args["path"].as_str() != Some(&project.path) {
                return Err(crate::errors::coded(
                    "PROJECT_PATH_CHANGED",
                    json!({"path":project.path}),
                    "项目目录已变化，请重新确认信任",
                ));
            }
            if trusted && args["confirmed"] != true {
                return Err(crate::errors::coded(
                    "GIT_TRUST_CONFIRMATION_REQUIRED",
                    json!({"path":project.path}),
                    "需要确认信任项目及其 Git 配置",
                ));
            }
            let transaction = store.conn.unchecked_transaction()?;
            project.git_trusted = trusted;
            project.updated_at = chrono::Utc::now().to_rfc3339();
            store.save_project(&project)?;
            store
                .conn
                .execute("DELETE FROM project_checks WHERE project_id=?1", [id])?;
            if trusted {
                crate::bookmarks::sync_project(store, &project)?;
            }
            transaction.commit()?;
            return Ok(json!(project));
        }
        "projects.archive" => {
            let id = args["projectId"].as_str().context("缺少项目 ID")?;
            let mut project = store.get_project(id)?.context("项目已不存在")?;
            let was_archived = project.archived;
            project.archived = args["archived"].as_bool().context("缺少归档状态")?;
            project.updated_at = chrono::Utc::now().to_rfc3339();
            let transaction = store.conn.unchecked_transaction()?;
            store.save_project(&project)?;
            crate::bookmarks::sync_archive(store, &project.id, project.archived)?;
            if was_archived != project.archived {
                store.record_activity(
                    "project",
                    &format!(
                        "{}项目及关联收藏 · {}",
                        if project.archived {
                            "归档"
                        } else {
                            "取消归档"
                        },
                        project.name
                    ),
                    &[project.path.clone()],
                )?;
            }
            transaction.commit()?;
            return Ok(json!(project));
        }
        "projects.delete" => {
            let id = args["projectId"].as_str().context("缺少项目 ID")?;
            let old = store.get_project(id)?;
            let tx = store.conn.unchecked_transaction()?;
            crate::bookmarks::delete_project_bookmarks(store, id)?;
            for mut location in store
                .list_deployments()?
                .into_iter()
                .filter(|d| d.project_id.as_deref() == Some(id))
            {
                location.project_id = None;
                store.save_deployment(&location)?;
            }
            store.delete_project(id)?;
            if let Some(old) = old {
                store.record_activity(
                    "project",
                    &format!("删除本机项目及关联收藏 · {}", old.name),
                    &[old.path],
                )?;
            }
            store
                .conn
                .execute("DELETE FROM project_checks WHERE project_id=?1", [id])?;
            tx.commit()?;
            return Ok(json!({"ok":true}));
        }
        _ => {}
    }
    let id = args["projectId"].as_str().context("缺少项目 ID")?;
    if method == "projects.status" {
        return Ok(json!({"update":last_check(store,id)?}));
    }
    let root = root(store, id)?;
    match method {
        "projects.inspect" => {
            Ok(json!({"state":inspect(&root)?,"lastCheck":last_check(store,id)?}))
        }
        "projects.check" => check_git(store, id, &root),
        "projects.commits" => {
            let state = inspect(&root)?;
            if args["head"] != state["head"] || args["upstreamHead"] != state["upstreamHead"] {
                bail!("本地或上游提交已变化，请重新打开检查详情");
            }
            let offset = args["offset"].as_u64().context("缺少提交分页位置")?;
            let head = state["head"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("没有本地提交")?;
            Ok(
                json!({"commits":commit_page(&root,head,state["upstreamHead"].as_str().context("没有上游提交")?,offset)?,"total":state["commitRangeTotal"]}),
            )
        }
        _ => bail!("未知项目操作"),
    }
}
