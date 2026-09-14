//! Fixed Agent identity and program detection, shared by UI and operations.
use std::path::{Path, PathBuf};

pub struct Definition {
    pub id: &'static str,
    pub name: &'static str,
    pub global: &'static str,
    pub project: &'static str,
}
pub const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "codex",
        name: "Codex",
        global: ".codex/skills",
        project: ".agents/skills",
    },
    Definition {
        id: "claude",
        name: "Claude Code",
        global: ".claude/skills",
        project: ".claude/skills",
    },
    Definition {
        id: "dsh",
        name: "DeepSeek Harness",
        global: ".dsh/skills",
        project: ".dsh/skills",
    },
    Definition {
        id: "zcode",
        name: "ZCode",
        global: ".zcode/skills",
        project: ".zcode/skills",
    },
    Definition {
        id: "hermes",
        name: "Hermes",
        global: ".hermes/skills",
        project: ".hermes/skills",
    },
];
pub fn known(id: &str) -> bool {
    DEFINITIONS.iter().any(|d| d.id == id)
}
pub fn global_path(d: &Definition, home: &Path) -> PathBuf {
    let override_path = match d.id {
        "codex" => std::env::var_os("CODEX_HOME").map(|p| PathBuf::from(p).join("skills")),
        "hermes" => std::env::var_os("HERMES_HOME")
            .map(|p| PathBuf::from(p).join("skills"))
            .or_else(|| {
                if cfg!(windows) {
                    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("hermes/skills"))
                } else {
                    None
                }
            }),
        _ => None,
    };
    override_path.unwrap_or_else(|| home.join(d.global))
}
pub fn available(config: &crate::adapters::ExecConfig, id: &str) -> bool {
    use std::io::Read;
    let program = config.program(id);
    let path = Path::new(&program);
    if !path.is_file() {
        return false;
    }
    if cfg!(windows)
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        let mut header = [0; 2];
        return std::fs::File::open(path)
            .and_then(|mut f| f.read_exact(&mut header))
            .is_ok()
            && header == *b"MZ";
    }
    true
}
