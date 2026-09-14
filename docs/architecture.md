# Technical overview

AgentHub uses React and TypeScript for its interface, Tauri as the Windows desktop host, and a Rust core for SQLite, source retrieval, and file operations.

## Layout

| Path | Contents |
| --- | --- |
| `src/` | Interface, frontend contracts, and tests |
| `crates/core/` | Domain logic, storage, scanning, file changes, and integration tests |
| `crates/cli/` | Internal development and test CLI; not distributed with the app |
| `src-tauri/` | Desktop host, capabilities, icons, and installer |
| `scripts/` | Build, packaging, and Windows validation |
| `licenses/` | Original third-party license texts and dependency inventory |

## Calls and data

The desktop and internal CLI share `agenthub_core::dispatch(data_dir, method, args)`, defined in `crates/core/src/lib.rs`. JSON fields use camelCase. Frontend contracts are in `src/contracts.ts` and `src/api.ts`; serialization types are in `crates/core/src/model.rs`.

Shared behavior belongs in the core. Prompts, library Skills, actual installation locations, local projects, and repository bookmarks are stored as separate concepts. The SQLite file is `agenthub.sqlite3` under the data directory. Built-in Agent definitions live in `crates/core/src/agents.rs`.

## File operations

Skill writes go through preview and confirmation, with plan, source, and target state checked again before execution. The core owns file transactions, backups, recovery, and shared-directory deduplication. Host-managed content remains read-only.

If recovery fails, diagnostics and available read/export operations remain accessible while writes that would alter recovery evidence are paused. Git project checks require explicit trust and do not automatically merge. Changes to these behaviors should verify actual file results and recovery paths, rather than interface state alone.

See the [contributing guide](../CONTRIBUTING.md) for build commands. Installer and portable acceptance should use the same candidate files in a disposable Windows environment, recording SHA-256 hashes, environment information, and actual observations. `scripts/prepare-candidate.ps1` records the version, source commit, CI provenance, and signing status of release files.
