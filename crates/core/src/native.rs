use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 20000;

/// Normalize public source syntax without making a network request.
pub fn normalize_source(value: &Value) -> Result<Value> {
    let mut source: crate::model::Source = serde_json::from_value(value.clone())?;
    source.locator = source.locator.trim().to_string();
    if source.locator.is_empty() {
        bail!("source locator required");
    }
    if source.kind == "https-zip" {
        source.kind = "zip".into();
    }
    if source.kind == "git" {
        let pieces: Vec<_> = source.locator.split('/').collect();
        if pieces.len() == 2
            && pieces.iter().all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            })
        {
            source.locator = format!("https://github.com/{}", source.locator);
        }
        let url = https_url(&source.locator)?;
        if url.host_str() == Some("github.com") {
            let parts = url
                .path_segments()
                .map(|parts| {
                    parts
                        .filter(|p| !p.is_empty())
                        .map(decode_segment)
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?
                .unwrap_or_default();
            if parts.len() >= 4 && parts[2] == "tree" {
                let revision = source.revision.clone().unwrap_or_else(|| parts[3].clone());
                let tail = parts[3..].join("/");
                let subpath = tail
                    .strip_prefix(&(revision.clone() + "/"))
                    .map(str::to_string)
                    .or_else(|| (tail == revision).then(String::new))
                    .context("GitHub tree URL does not match the explicit revision")?;
                if source.subpath.is_none() && !subpath.is_empty() {
                    source.subpath = Some(subpath);
                }
                source.revision = Some(revision);
                source.locator = format!(
                    "https://github.com/{}/{}.git",
                    parts[0],
                    parts[1].trim_end_matches(".git")
                );
            }
        }
    } else if source.kind == "zip" {
        https_url(&source.locator)?;
    } else if source.kind != "local" {
        bail!("不支持此来源类型，请使用 Git、ZIP 或本机目录");
    }
    crate::sources::validate_source(&source)?;
    if let Some(subpath) = source.subpath.as_deref().filter(|p| !p.is_empty()) {
        validate_subpath(subpath)?;
    }
    if let Some(revision) = source.revision.as_deref() {
        if revision.is_empty() || revision.starts_with('-') || revision.contains(['\0', '\r', '\n'])
        {
            bail!("invalid Git revision");
        }
    }
    Ok(serde_json::to_value(source)?)
}
fn decode_segment(value: &str) -> Result<String> {
    let mut decoded = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                bail!("invalid URL encoding");
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])?;
            decoded.push(u8::from_str_radix(hex, 16)?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    Ok(String::from_utf8(decoded)?)
}
fn validate_subpath(value: &str) -> Result<()> {
    if value.contains(['\\', ':', '\0'])
        || Path::new(value).is_absolute()
        || value
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        bail!("source subpath must remain inside source");
    }
    Ok(())
}
fn https_url(value: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(value)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("Source and redirects require HTTPS without credentials or fragments");
    }
    Ok(url)
}
/// Shared redirect validation is public so offline regressions cover the actual policy.
pub fn validate_redirect(current: &str, location: &str, redirects: usize) -> Result<reqwest::Url> {
    if redirects >= 5 {
        bail!("Source exceeded five HTTPS redirects");
    }
    let next = https_url(current)?.join(location)?;
    https_url(next.as_str())
}
fn download_zip(locator: &str, payload: &Path, proxy: Option<&str>) -> Result<()> {
    let client = crate::network::client_builder(proxy)?
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut url = https_url(locator)?;
    let mut redirects = 0;
    let mut response = loop {
        let response = client.get(url.clone()).send()?.error_for_status()?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("ZIP redirect has no Location")?
                .to_str()?;
            url = validate_redirect(url.as_str(), location, redirects)?;
            redirects += 1;
            continue;
        }
        break response;
    };
    if response.content_length().is_some_and(|n| n > MAX_BYTES) {
        bail!("ZIP exceeds download limit");
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        bail!("ZIP exceeds download limit");
    }
    extract_zip(std::io::Cursor::new(bytes), payload)?;
    Ok(())
}
#[cfg(test)]
fn git(root: &Path, args: Vec<String>) -> Result<()> {
    git_with_proxy(root, args, None)
}
fn git_with_proxy(root: &Path, args: Vec<String>, proxy: Option<&str>) -> Result<()> {
    let clone_destination = if args.first().is_some_and(|s| s == "clone") {
        args.last()
            .map(PathBuf::from)
            .filter(|p| p.parent() == Some(root) && !p.exists())
    } else {
        None
    };
    let can_retry = !args.first().is_some_and(|s| s == "clone") || clone_destination.is_some();
    let mut safe = vec![
        "-c".into(),
        format!(
            "core.hooksPath={}",
            std::env::temp_dir()
                .join(format!("agenthub-disabled-hooks-{}", uuid::Uuid::new_v4()))
                .display()
        ),
        "-c".into(),
        "core.fsmonitor=false".into(),
        "-c".into(),
        "core.autocrlf=false".into(),
        "-c".into(),
        "http.followRedirects=false".into(),
    ];
    let proxy = crate::network::validate_proxy(proxy.unwrap_or(""))?;
    if !proxy.is_empty() {
        safe.extend(["-c".into(), format!("http.proxy={proxy}")]);
    }
    safe.extend(args);
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    for attempt in 0..3 {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            bail!("读取 Git 来源超时，请稍后重试");
        }
        let output = crate::adapters::run_command("git", &safe, Some(root), remaining)?;
        if output.succeeded() {
            return Ok(());
        }
        let message = output.message();
        if attempt == 2 || output.timed_out || !can_retry || !transient_git_error(&message) {
            bail!("读取 Git 来源失败：{message}");
        }
        if let Some(path) = &clone_destination {
            if path.exists() {
                crate::safe_files::safe_directory(path)?;
                inspect_tree(path)?;
                fs::remove_dir_all(path)?;
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    unreachable!()
}
fn transient_git_error(message: &str) -> bool {
    let text = message.to_lowercase();
    if [
        "certificate",
        "authentication",
        "permission denied",
        "repository not found",
        "could not read username",
        "403",
        "401",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        return false;
    }
    [
        "unexpected eof",
        "early eof",
        "connection reset",
        "recv failure",
        "connection timed out",
        "could not resolve host",
        "failed to connect",
        "transfer closed",
        "ssl_read",
    ]
    .iter()
    .any(|s| text.contains(s))
}
#[cfg(test)]
fn acquire_git(locator: &str, revision: Option<&str>, root: &Path, payload: &Path) -> Result<()> {
    acquire_git_with_proxy(locator, revision, root, payload, None)
}
fn acquire_git_with_proxy(
    locator: &str,
    revision: Option<&str>,
    root: &Path,
    payload: &Path,
    proxy: Option<&str>,
) -> Result<()> {
    let commit = revision
        .is_some_and(|r| matches!(r.len(), 40 | 64) && r.chars().all(|c| c.is_ascii_hexdigit()));
    let mut args = vec![
        "clone".into(),
        "--no-checkout".into(),
        "--depth".into(),
        "1".into(),
    ];
    if !commit {
        if let Some(revision) = revision {
            args.extend(["--branch".into(), revision.into()]);
        }
    }
    args.extend([
        "--".into(),
        locator.into(),
        payload.to_string_lossy().into_owned(),
    ]);
    git_with_proxy(root, args, proxy)?;
    if commit {
        let revision = revision.unwrap();
        git_with_proxy(
            &payload,
            vec![
                "fetch".into(),
                "--depth".into(),
                "1".into(),
                "origin".into(),
                revision.into(),
            ],
            proxy,
        )?;
        git_with_proxy(
            &payload,
            vec![
                "checkout".into(),
                "--detach".into(),
                revision.into(),
                "--".into(),
            ],
            proxy,
        )?;
    } else {
        git_with_proxy(
            &payload,
            vec![
                "checkout".into(),
                "--detach".into(),
                "HEAD".into(),
                "--".into(),
            ],
            proxy,
        )?;
    }
    inspect_tree(&payload)?;
    // The checkout is a disposable acquisition. Git's transport metadata is
    // not Skill content and changes independently of the published files.
    let metadata = payload.join(".git");
    if metadata.is_dir() {
        crate::safe_files::safe_directory(&metadata)?;
        fs::remove_dir_all(metadata)?;
    }
    Ok(())
}
fn acquire(source: &Value, root: &Path, proxy: Option<&str>) -> Result<Value> {
    let mut source = normalize_source(source)?;
    crate::safe_files::safe_directory(root)?;
    crate::protection::ensure_mutable(root)?;
    let payload = root.join("source");
    if payload.exists() {
        bail!("staging payload already exists");
    }
    fs::create_dir_all(root)?;
    let locator = source["locator"]
        .as_str()
        .context("source locator required")?;
    match source["kind"].as_str().unwrap_or("") {
        "local" => {
            let path = Path::new(locator);
            if !path.is_absolute() || !path.is_dir() {
                bail!("local source must be an existing absolute directory");
            }
            if root.canonicalize()?.starts_with(path.canonicalize()?) {
                bail!("staging directory must not be inside source");
            }
            crate::safe_files::safe_directory(path)?;
            copy_bounded(path, &payload)?;
        }
        "git" => {
            acquire_git_with_proxy(locator, source["revision"].as_str(), root, &payload, proxy)?
        }
        "zip" => {
            download_zip(locator, &payload, proxy)?;
            let removed = unwrap_archive_root(&payload)?;
            if let Some(sub) = source["subpath"].as_str().filter(|s| !s.is_empty()) {
                if !payload.join(sub).exists() {
                    if let Some(trimmed) = sub.strip_prefix(&(removed.clone() + "/")) {
                        source["subpath"] = Value::String(trimmed.into());
                    } else if sub == removed {
                        source["subpath"] = Value::Null;
                    }
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(source)
}
/// Remove a sole packaging folder, preserving every file beneath it.
/// The directory must be an isolated extraction owned by the caller.
pub fn unwrap_archive_root(payload: &Path) -> Result<String> {
    inspect_tree(payload)?;
    let entries = fs::read_dir(payload)?.collect::<std::result::Result<Vec<_>, _>>()?;
    if entries.len() != 1 || !entries[0].file_type()?.is_dir() || payload.join("SKILL.md").is_file()
    {
        return Ok(String::new());
    }
    let wrapper = entries[0].path();
    let name = entries[0].file_name().to_string_lossy().into_owned();
    let parked = payload.join(format!(".agenthub-unwrap-{}", uuid::Uuid::new_v4()));
    fs::rename(&wrapper, &parked)?;
    for child in fs::read_dir(&parked)? {
        let child = child?;
        fs::rename(child.path(), payload.join(child.file_name()))?;
    }
    fs::remove_dir(&parked)?;
    Ok(name)
}
/// Metadata follows the current Skill file while user labels and identity remain stable.
pub(crate) fn skill_description(folder: &Path) -> Option<String> {
    let text = crate::safe_files::read_small_text(&folder.join("SKILL.md"), 1024 * 1024).ok()?;
    let text = text.trim_start_matches('\u{feff}');
    let front = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let end = front.find("\n---")?;
    let meta = serde_yaml::from_str::<serde_yaml::Value>(&front[..end]).ok()?;
    meta.as_mapping()?;
    Some(meta["description"].as_str().unwrap_or_default().to_string())
}

pub(crate) fn candidates(payload: &Path) -> Result<Vec<Value>> {
    inspect_tree(payload)?;
    let mut result = Vec::new();
    for entry in walkdir::WalkDir::new(payload)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !matches!(entry.file_name().to_str(), Some(".git" | "node_modules")))
    {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
            continue;
        }
        let folder = entry.path().parent().context("Skill parent missing")?;
        let subpath = folder
            .strip_prefix(payload)?
            .to_string_lossy()
            .replace('\\', "/");
        let mut name = folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut description = String::new();
        if fs::metadata(entry.path())?.len() <= 1024 * 1024 {
            let text = fs::read_to_string(entry.path())?;
            let text = text.trim_start_matches('\u{feff}');
            if let Some(front) = text
                .strip_prefix("---\n")
                .or_else(|| text.strip_prefix("---\r\n"))
            {
                if let Some(end) = front.find("\n---") {
                    if let Ok(meta) = serde_yaml::from_str::<serde_yaml::Value>(&front[..end]) {
                        if let Some(value) = meta["name"].as_str() {
                            name = value.into();
                        }
                        if let Some(value) = meta["description"].as_str() {
                            description = value.into();
                        }
                    }
                }
            }
        }
        result.push(serde_json::json!({"name":name,"description":description,"subpath":subpath}));
    }
    result.sort_by(|a, b| a["subpath"].as_str().cmp(&b["subpath"].as_str()));
    compare_candidates(payload, &mut result);
    Ok(result)
}

// Compare full directory trees, including relative paths and file bytes. Never
// infer distribution roles from folder names or equate only SKILL.md contents.
fn compare_candidates(payload: &Path, candidates: &mut [Value]) {
    let names: Vec<_> = candidates
        .iter()
        .map(|c| c["name"].as_str().unwrap_or("").trim().to_lowercase())
        .collect();
    let digests: Vec<_> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if names
                .iter()
                .enumerate()
                .any(|(j, name)| i != j && name == &names[i])
            {
                let path = payload.join(c["subpath"].as_str().unwrap_or(""));
                if path.join("SKILL.md").is_file() {
                    crate::safe_files::digest(&path)
                        .ok()
                        .filter(|value| value != "absent")
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();
    let paths: Vec<_> = candidates
        .iter()
        .map(|c| c["subpath"].as_str().unwrap_or("").to_owned())
        .collect();
    for (i, candidate) in candidates.iter_mut().enumerate() {
        let mut same = vec![];
        let mut different = vec![];
        let mut unknown = vec![];
        for (j, name) in names.iter().enumerate() {
            if i == j || name != &names[i] {
                continue;
            }
            match (&digests[i], &digests[j]) {
                (Some(a), Some(b)) if a == b => same.push(&paths[j]),
                (Some(_), Some(_)) => different.push(&paths[j]),
                _ => unknown.push(&paths[j]),
            }
        }
        if !same.is_empty() || !different.is_empty() || !unknown.is_empty() {
            candidate["contentComparison"] =
                serde_json::json!({"same":same,"different":different,"unknown":unknown});
        }
    }
}
/// Acquire a source and list independently selectable Skills. No deployment is performed.
pub fn inspect_source(source: &Value, root: &Path) -> Result<Value> {
    inspect_source_with_proxy(source, root, None)
}
pub fn inspect_source_with_proxy(
    source: &Value,
    root: &Path,
    proxy: Option<&str>,
) -> Result<Value> {
    let source = acquire(source, root, proxy)?;
    let payload = root.join("source");
    let mut choices = candidates(&payload)?;
    if let Some(sub) = source["subpath"].as_str().filter(|s| !s.is_empty()) {
        choices.retain(|c| {
            c["subpath"] == sub
                || c["subpath"]
                    .as_str()
                    .is_some_and(|p| p.starts_with(&(sub.to_owned() + "/")))
        });
    }
    Ok(serde_json::json!({"source":source,"candidates":choices}))
}
/// Keep all source resources, deploying only the chosen or single discovered Skill.
pub fn stage(request: &Value, root: &Path) -> Result<PathBuf> {
    let inspection =
        inspect_source_with_proxy(&request["source"], root, request["networkProxy"].as_str())?;
    let payload = root.join("source");
    if let Some(sub) = inspection["source"]["subpath"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        let selected = payload.join(sub);
        if selected.join("SKILL.md").is_file() {
            return Ok(selected);
        }
    } else if payload.join("SKILL.md").is_file() {
        return Ok(payload);
    }
    let choices = inspection["candidates"]
        .as_array()
        .context("candidate list missing")?;
    match choices.as_slice() {
        [only] => Ok(payload.join(only["subpath"].as_str().context("candidate path missing")?)),
        [] => {
            bail!("No SKILL.md found in selected source; a workspace is not an installable Skill")
        }
        _ => bail!("Multiple Skills found; choose one candidate and set source.subpath explicitly"),
    }
}
fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}
pub fn inspect_tree(root: &Path) -> Result<()> {
    let mut bytes = 0u64;
    let mut entries = 0usize;
    for item in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(65)
    {
        let item = item?;
        entries += 1;
        if entries > MAX_ENTRIES || item.depth() > 64 {
            bail!("source exceeds entry/depth limit");
        }
        let meta = fs::symlink_metadata(item.path())?;
        if is_link(&meta) {
            bail!("links and reparse points are not copied");
        }
        if !meta.is_file() && !meta.is_dir() {
            bail!("unsupported filesystem object");
        }
        bytes = bytes
            .checked_add(if meta.is_file() { meta.len() } else { 0 })
            .context("source size overflow")?;
        if bytes > MAX_BYTES {
            bail!("source exceeds size limit");
        }
    }
    Ok(())
}
pub fn copy_bounded(source: &Path, dest: &Path) -> Result<()> {
    inspect_tree(source)?;
    for item in walkdir::WalkDir::new(source).follow_links(false) {
        let item = item?;
        let path = dest.join(item.path().strip_prefix(source)?);
        let meta = fs::symlink_metadata(item.path())?;
        if is_link(&meta) {
            bail!("source changed to link during copy");
        }
        if meta.is_dir() {
            fs::create_dir_all(&path)?;
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(item.path(), path)?;
        }
    }
    Ok(())
}
pub fn extract_zip<R: Read + std::io::Seek>(reader: R, dest: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(reader)?;
    if archive.len() > MAX_ENTRIES {
        bail!("ZIP exceeds entry limit");
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let raw = entry.name();
        if raw.contains('\\') || raw.contains(':') {
            bail!("ambiguous ZIP path");
        }
        let relative = entry
            .enclosed_name()
            .context("ZIP path escapes destination")?;
        if relative.components().count() > 64
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("unsafe ZIP path");
        }
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            bail!("ZIP links are forbidden");
        }
        total = total
            .checked_add(entry.size())
            .context("ZIP size overflow")?;
        if total > MAX_BYTES {
            bail!("ZIP exceeds expanded size limit");
        }
        let path = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(path)?;
        } else {
            if let Some(p) = path.parent() {
                fs::create_dir_all(p)?;
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            let size = std::io::copy(&mut entry.by_ref().take(MAX_BYTES + 1), &mut file)?;
            if size > MAX_BYTES {
                bail!("ZIP entry exceeds size limit");
            }
            file.flush()?;
        }
    }
    Ok(())
}

pub fn differences(source: &Path, target: Option<&Path>) -> Result<Value> {
    use std::collections::BTreeMap;
    fn files(root: &Path) -> Result<BTreeMap<String, String>> {
        use sha2::{Digest, Sha256};
        let mut files = BTreeMap::new();
        if !root.exists() {
            return Ok(files);
        }
        inspect_tree(root)?;
        for entry in walkdir::WalkDir::new(root) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let name = entry
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(
                    name,
                    format!("{:x}", Sha256::digest(fs::read(entry.path())?)),
                );
            }
        }
        Ok(files)
    }
    let incoming = files(source)?;
    let old = target.map(files).transpose()?.unwrap_or_default();
    let mut diff = Vec::new();
    for (path, digest) in &incoming {
        if old.get(path) != Some(digest) {
            diff.push(serde_json::json!({"path":path,"change":if old.contains_key(path){"modified"}else{"added"}}));
        }
    }
    for path in old.keys() {
        if !incoming.contains_key(path) {
            diff.push(serde_json::json!({"path":path,"change":"removed"}));
        }
    }
    Ok(serde_json::json!(diff))
}

#[cfg(test)]
mod acquisition_git_tests {
    use super::*;
    #[test]
    fn unavailable_same_named_content_is_not_assumed_identical() {
        let t = tempfile::tempdir().unwrap();
        let mut choices = vec![
            serde_json::json!({"name":"Same","subpath":"a"}),
            serde_json::json!({"name":"Same","subpath":"b"}),
        ];
        compare_candidates(t.path(), &mut choices);
        assert_eq!(
            choices[0]["contentComparison"]["same"],
            serde_json::json!([])
        );
        assert_eq!(
            choices[0]["contentComparison"]["unknown"],
            serde_json::json!(["b"])
        );
        assert_eq!(choices.len(), 2);
    }
    #[test]
    fn real_git_selects_branch_tag_and_old_commit_without_network() {
        let fixture = tempfile::tempdir().unwrap();
        let repo = fixture.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, vec!["init".into(), "--initial-branch=main".into()]).unwrap();
        for text in ["first", "second"] {
            fs::write(repo.join("SKILL.md"), text).unwrap();
            git(&repo, vec!["add".into(), "SKILL.md".into()]).unwrap();
            git(
                &repo,
                vec![
                    "-c".into(),
                    "user.name=Fixture".into(),
                    "-c".into(),
                    "user.email=fixture@example.test".into(),
                    "-c".into(),
                    "commit.gpgsign=false".into(),
                    "commit".into(),
                    "-m".into(),
                    text.into(),
                ],
            )
            .unwrap();
            if text == "first" {
                git(&repo, vec!["tag".into(), "v1".into()]).unwrap();
            }
        }
        let first = fs::read_to_string(repo.join(".git/refs/tags/v1")).unwrap();
        for (index, revision, expected) in [
            (0, "main", "second"),
            (1, "v1", "first"),
            (2, first.trim(), "first"),
        ] {
            let stage = fixture.path().join(format!("stage-{index}"));
            fs::create_dir(&stage).unwrap();
            acquire_git(
                repo.to_str().unwrap(),
                Some(revision),
                &stage,
                &stage.join("source"),
            )
            .unwrap();
            assert_eq!(
                fs::read_to_string(stage.join("source/SKILL.md")).unwrap(),
                expected
            );
        }
    }
}
