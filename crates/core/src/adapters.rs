//! Agent program detection and bounded subprocess execution.
//!
//! Git is the only program this application ever runs, and it is run without a
//! shell: the command is a program plus an argv vector executed with
//! `std::process::Command`.  This matters on Windows, where paths often contain
//! spaces and where invoking a shell would make a source locator executable
//! input.  Agent programs are only ever *detected*, never started.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
#[cfg(windows)]
use std::time::UNIX_EPOCH;
use std::time::{Duration, Instant};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Configured Agent program paths. An empty value means that detection should
/// look for the installed program itself. Only `.exe` candidates are accepted
/// on Windows so an npm-generated `.cmd` shim earlier in PATH cannot stand in
/// for the real application.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExecConfig {
    pub codex: String,
    pub claude: String,
    pub dsh: String,
    pub hermes: String,
    pub zcode: String,
}

impl ExecConfig {
    pub fn program(&self, id: &str) -> String {
        let configured = match id {
            "codex" => self.codex.as_str(),
            "claude" => self.claude.as_str(),
            "dsh" => self.dsh.as_str(),
            "hermes" => self.hermes.as_str(),
            "zcode" => self.zcode.as_str(),
            _ => "",
        };
        if configured.trim().is_empty() {
            if crate::agents::known(id) {
                if let Some(path) = discover_program(id) {
                    return path.to_string_lossy().into_owned();
                }
            }
            if id == "dsh" {
                if let Some(home) = std::env::var_os("USERPROFILE") {
                    let entry = PathBuf::from(home)
                        .join(".dsh/profiles/node_modules/@deepseek-ai/dsh/lib/bin.js");
                    if entry.is_file() {
                        return entry.to_string_lossy().into_owned();
                    }
                }
            }
            id.to_string()
        } else {
            configured.to_string()
        }
    }

    pub fn from_settings(settings: &Value) -> Self {
        serde_json::from_value(settings["executables"].clone()).unwrap_or_default()
    }
}

/// Find the installed program on PATH, then in the standard Windows install
/// locations used by Codex and npm-installed Claude Code.
fn discover_program(id: &str) -> Option<PathBuf> {
    let path_entries = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    find_native_on_paths(id, &path_entries).or_else(|| {
        #[cfg(windows)]
        {
            discover_windows_program(id, &path_entries)
        }
        #[cfg(not(windows))]
        {
            None
        }
    })
}

/// Search PATH for the installed program. On Windows, only `.exe` is accepted;
/// an npm-generated `.cmd`/PowerShell wrapper does not prove that the
/// application itself is present, so a shim in one directory can never hide an
/// `.exe` in a later directory.
fn find_native_on_paths(id: &str, entries: &[PathBuf]) -> Option<PathBuf> {
    let names = if cfg!(windows) {
        vec![format!("{id}.exe")]
    } else {
        vec![id.to_string()]
    };
    for name in names {
        for directory in entries {
            let candidate = directory.join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(windows)]
fn discover_windows_program(id: &str, path_entries: &[PathBuf]) -> Option<PathBuf> {
    match id {
        "codex" => windows_codex_roots()
            .into_iter()
            .find_map(|root| find_codex_in_bin_root(&root)),
        "claude" => {
            // A custom npm prefix may not be covered by APPDATA/ProgramFiles,
            // but its PATH entry still exposes the generated shim. Inspect
            // that location only to derive the adjacent installed program;
            // never execute or parse the shim.
            path_entries
                .iter()
                .find_map(|directory| {
                    let shim = directory.join("claude.cmd");
                    if shim.is_file() {
                        find_claude_in_npm_root(directory)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    windows_claude_roots()
                        .into_iter()
                        .find_map(|root| find_claude_in_npm_root(&root))
                })
        }
        "zcode" => std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("Programs/ZCode/ZCode.exe"))
            .filter(|p| p.is_file()),
        "hermes" => std::env::var_os("HERMES_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("hermes")))
            .map(|p| p.join("bin/hermes.exe"))
            .filter(|p| p.is_file()),
        _ => None,
    }
}

#[cfg(windows)]
fn windows_codex_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(value) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(value).join("OpenAI/Codex/bin"));
    }
    if let Some(value) = std::env::var_os("USERPROFILE") {
        roots.push(PathBuf::from(value).join("AppData/Local/OpenAI/Codex/bin"));
    }
    for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(value) = std::env::var_os(variable) {
            roots.push(PathBuf::from(value).join("OpenAI/Codex/bin"));
        }
    }
    roots
}

#[cfg(windows)]
fn windows_claude_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(value) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(value).join("npm"));
    }
    if let Some(value) = std::env::var_os("USERPROFILE") {
        let profile = PathBuf::from(value);
        roots.push(profile.join("AppData/Roaming/npm"));
        roots.push(profile.join(".local/bin"));
    }
    if let Some(value) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(value).join("npm"));
    }
    if let Some(value) = std::env::var_os("NPM_CONFIG_PREFIX") {
        roots.push(PathBuf::from(value));
    }
    for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(value) = std::env::var_os(variable) {
            roots.push(PathBuf::from(value).join("nodejs"));
        }
    }
    roots
}

#[cfg(windows)]
fn find_codex_in_bin_root(root: &Path) -> Option<PathBuf> {
    // Keep this direct check for installations that do not use a release
    // directory, then scan all release directories. The release directory
    // name is intentionally never assumed to be a particular version/hash.
    if let Some(path) = native_executable(&root.join("codex.exe")) {
        return Some(path);
    }
    let mut candidates = std::fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .map(|path| path.join("codex.exe"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        modified_time(right)
            .cmp(&modified_time(left))
            .then_with(|| left.cmp(right))
    });
    candidates
        .into_iter()
        .find_map(|path| native_executable(&path))
}

#[cfg(windows)]
fn find_claude_in_npm_root(root: &Path) -> Option<PathBuf> {
    if let Some(path) = native_executable(&root.join("claude.exe")) {
        return Some(path);
    }
    let package_entry = root.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe");
    native_executable(&package_entry)
}

#[cfg(windows)]
fn native_executable(path: &Path) -> Option<PathBuf> {
    path.is_file().then(|| path.to_path_buf())
}

#[cfg(windows)]
fn modified_time(path: &Path) -> Duration {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct RunOutput {
    pub status: Option<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl RunOutput {
    pub fn succeeded(&self) -> bool {
        self.status.map(|status| status.success()).unwrap_or(false) && !self.timed_out
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.status.and_then(|status| status.code())
    }

    pub fn message(&self) -> String {
        let mut parts = Vec::new();
        if !self.stdout.trim().is_empty() {
            parts.push(self.stdout.trim().to_string());
        }
        if !self.stderr.trim().is_empty() {
            parts.push(self.stderr.trim().to_string());
        }
        if self.timed_out {
            parts.push("command timed out".to_string());
        }
        if parts.is_empty() {
            self.exit_code()
                .map(|code| format!("command exited with code {code}"))
                .unwrap_or_else(|| "command did not return an exit status".to_string())
        } else {
            parts.join("\n")
        }
    }
}

/// Execute a command without a shell and with bounded stdout/stderr.  The
/// reader threads keep the child's pipes drained while the parent polls for a
/// timeout, avoiding the common Windows deadlock where a child fills stderr.
pub fn run_command(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<RunOutput> {
    if program.trim().is_empty() {
        bail!("adapter command has an empty executable path");
    }
    if args.iter().any(|arg| arg.contains('\0')) {
        bail!("adapter command contains a NUL byte in its arguments");
    }

    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW for backend helpers.
    }
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start adapter executable `{program}`"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("adapter stdout pipe was not created"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("adapter stderr pipe was not created"))?;
    let stdout_reader = spawn_bounded_reader(stdout);
    let stderr_reader = spawn_bounded_reader(stderr);

    let started = Instant::now();
    let mut timed_out = false;
    let mut interrupted = None;
    let status = loop {
        if let Some(status) = child.try_wait().context("failed to poll adapter process")? {
            break Some(status);
        }
        interrupted = crate::source_control::checkpoint().err();
        if started.elapsed() >= timeout || interrupted.is_some() {
            timed_out = true;
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                let _ = Command::new("taskkill.exe")
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .creation_flags(0x08000000)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            // A best-effort kill is still followed by wait so no child is
            // leaked.  The process may have exited between try_wait and kill.
            let _ = child.kill();
            let status = child.wait().ok();
            break status;
        }
        thread::sleep(Duration::from_millis(20));
    };

    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    if let Some(error) = interrupted {
        return Err(error);
    }
    Ok(RunOutput {
        status,
        stdout,
        stderr,
        timed_out,
    })
}

fn spawn_bounded_reader<R>(mut reader: R) -> JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buf = [0_u8; 4096];
        let mut truncated = false;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(count) => {
                    if bytes.len() < MAX_OUTPUT_BYTES {
                        let keep = (MAX_OUTPUT_BYTES - bytes.len()).min(count);
                        bytes.extend_from_slice(&buf[..keep]);
                        if keep != count {
                            truncated = true;
                        }
                    } else {
                        truncated = true;
                    }
                }
                Err(_) => break,
            }
        }
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        if truncated {
            text.push_str("\n...[output truncated]");
        }
        text
    })
}

fn join_reader(reader: JoinHandle<String>) -> String {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !reader.is_finished() {
        if Instant::now() >= deadline {
            return "adapter output stream did not close within deadline".into();
        }
        thread::sleep(Duration::from_millis(10));
    }
    reader
        .join()
        .unwrap_or_else(|_| "adapter output reader failed".to_string())
}

/// Report whether Git can be run, for the one place the UI shows it. Git is
/// resolved by the operating system from PATH; no configured program applies.
pub fn git_status() -> Value {
    match run_command("git", &["--version".to_string()], None, PROBE_TIMEOUT) {
        Ok(output) if output.succeeded() => {
            json!({"available":true,"version":parse_version(&output.stdout)})
        }
        _ => json!({"available":false,"version":null}),
    }
}

pub fn parse_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|token| {
            let trimmed = token.trim_matches(|ch: char| {
                !ch.is_ascii_alphanumeric() && ch != '.' && ch != '-' && ch != '_'
            });
            let has_digit = trimmed.chars().any(|ch| ch.is_ascii_digit());
            has_digit && trimmed.chars().any(|ch| ch == '.')
        })
        .map(|token| {
            token
                .trim_matches(|ch: char| {
                    !ch.is_ascii_alphanumeric() && ch != '.' && ch != '-' && ch != '_'
                })
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use std::fs;

    #[test]
    fn explicit_executable_path_has_priority_over_discovery() {
        let config = ExecConfig {
            codex: r"C:\custom\codex.exe".into(),
            claude: r"C:\custom\claude.exe".into(),
            ..ExecConfig::default()
        };

        assert_eq!(config.program("codex"), r"C:\custom\codex.exe");
        assert_eq!(config.program("claude"), r"C:\custom\claude.exe");
    }

    #[cfg(windows)]
    #[test]
    fn path_discovery_prefers_a_native_exe_and_ignores_cmd_shims() {
        let fixture = tempfile::tempdir().unwrap();
        let shim_dir = fixture.path().join("shim");
        let native_dir = fixture.path().join("native");
        fs::create_dir_all(&shim_dir).unwrap();
        fs::create_dir_all(&native_dir).unwrap();
        fs::write(shim_dir.join("claude.cmd"), "@echo off").unwrap();
        fs::write(native_dir.join("claude.exe"), b"fixture").unwrap();

        let found = find_native_on_paths("claude", &[shim_dir, native_dir.clone()]);
        assert_eq!(found, Some(native_dir.join("claude.exe")));
    }

    #[cfg(windows)]
    #[test]
    fn codex_fallback_scans_hashed_release_directories() {
        let fixture = tempfile::tempdir().unwrap();
        let release = fixture.path().join("release-hash-without-a-version");
        fs::create_dir_all(&release).unwrap();
        fs::write(release.join("codex.exe"), b"fixture").unwrap();
        fs::write(fixture.path().join("codex.cmd"), "@echo off").unwrap();

        assert_eq!(
            find_codex_in_bin_root(fixture.path()),
            Some(release.join("codex.exe"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn claude_fallback_uses_the_native_npm_package_entry() {
        let fixture = tempfile::tempdir().unwrap();
        let entry = fixture
            .path()
            .join("node_modules/@anthropic-ai/claude-code/bin/claude.exe");
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        fs::write(&entry, b"fixture").unwrap();

        assert_eq!(find_claude_in_npm_root(fixture.path()), Some(entry));
    }
}
