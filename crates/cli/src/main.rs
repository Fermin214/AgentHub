use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "agenthub-dev",
    version,
    about = "AgentHub internal development and test runner"
)]
struct Cli {
    /// Override local application data directory (use this for isolated tests).
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    /// Emit JSON. All command output is JSON to keep the automation contract stable.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Read current desktop data.
    Snapshot,
    /// Inspect configured Skill roots without changing Agent files.
    Scan,
    /// Check Skill and Git upstream sources without applying changes.
    Check,
    /// Invoke a shared core method. Data writes require --apply.
    Call {
        method: String,
        #[arg(long, default_value = "{}", conflicts_with = "args_file")]
        args: String,
        #[arg(long)]
        args_file: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
    },
}

fn default_data_dir() -> Result<PathBuf> {
    agenthub_core::default_data_dir().context("Set --data-dir or AGENTHUB_DATA_DIR")
}

fn json_file(path: PathBuf) -> Result<Value> {
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    Ok(serde_json::from_str(
        content.trim_start_matches('\u{feff}'),
    )?)
}

fn run(cli: Cli) -> Result<Value> {
    let data_dir = cli.data_dir.map(Ok).unwrap_or_else(default_data_dir)?;
    let data_dir = if data_dir.is_absolute() {
        data_dir
    } else {
        std::path::absolute(data_dir).context("Cannot resolve application data directory")?
    };
    let (method, args) = match cli.command {
        Commands::Snapshot => ("snapshot".to_owned(), json!({})),
        Commands::Scan => ("skills.scan".to_owned(), json!({})),
        Commands::Check => ("updates.check".to_owned(), json!({})),
        Commands::Call {
            method,
            args,
            args_file,
            apply,
        } => {
            const READ_METHODS: &[&str] = &[
                "snapshot",
                "projects.inspect",
                "projects.commits",
                "projects.status",
                "projects.check",
                "skills.scan",
                "skills.check",
                "skills.checkBatch.start",
                "skills.checkBatch.finish",
                "skills.diff",
                "skills.files",
                "skills.read",
                "skills.install.preview",
                "skills.remove.preview",
                "skills.update.preview",
                "skills.delete.preview",
                "sources.inspect",
                "repositories.list",
                "bookmarks.list",
                "targets.list",
                "network.get",
                "maintenance.get",
                "maintenance.preview",
                "git.probe",
                "updates.check",
                "prompts.export",
                "backups.list",
            ];
            if !READ_METHODS.contains(&method.as_str()) && !apply {
                bail!("Method {method} changes data. Review its arguments and pass --apply explicitly.");
            }
            let args = if let Some(path) = args_file {
                json_file(path)?
            } else {
                serde_json::from_str(&args)?
            };
            (method, args)
        }
    };
    agenthub_core::dispatch(&data_dir, &method, args)
}

fn main() {
    match run(Cli::parse()) {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            if matches!(
                value.get("status").and_then(Value::as_str),
                Some("failed" | "partial")
            ) {
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!("{}", json!({"error": format!("{error:#}")}));
            std::process::exit(1);
        }
    }
}
