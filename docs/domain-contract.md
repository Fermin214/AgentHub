# Domain contract

This page describes current domain objects and operation boundaries. Full requests and responses are defined in [src/contracts.ts](../src/contracts.ts), [src/types.ts](../src/types.ts), and Rust [model.rs](../crates/core/src/model.rs). The Rust core performs runtime validation; TypeScript types do not replace IPC input checks.

## Objects and responsibilities

| Object | Responsibility |
| --- | --- |
| `Prompt` | User-saved text and metadata |
| `Skill` / `Source` | Library content and its optional checkable source |
| `SkillDeployment` / `AgentTarget` | Discovered or installed locations and Agent target configuration |
| `LocalProject` | A registered directory and its Git trust state |
| `RepositoryBookmark` | Repository links and organizational information, optionally linked to a local project |
| `UpdateCheck` / `SkillChangePlan` | Check results and file-change plans awaiting confirmation |
| `ChangeResult` / `BackupRecord` | Change results and restorable backup records |

The shared entry point is `dispatch(data_dir, method, args)`, with camelCase JSON fields. The frontend uses typed calls; the desktop and internal CLI share the same core implementation. Combining objects in the interface does not combine their storage responsibilities or file ownership.

## Reading and writing Skills

`sources.inspect` returns an inspection identifier and candidates; `skills.add` adds the selected candidate to the library. `skills.import` copies existing installations, reports success or failure for each item, and does not change original files. Binding a source does not overwrite content; unbinding does not delete library copies or installations.

Interactive inspection reserves an in-memory `requestId` with `sources.begin`, passes it to `sources.inspect`, and uses `sources.status` / `sources.cancel` for progress and cancellation in the same host process and data directory. One inspection may run per data directory; its slot remains occupied through cleanup. Cancellation terminates acquisition work and removes its disposable snapshot before the inspection returns. The whole inspection has a 120-second deadline. Saving a selected Skill is a separate, non-cancellable operation. Calls without a request ID retain synchronous inspection behavior for the internal CLI; request IDs are not persisted or portable between CLI processes.

`skills.check` obtains comparison results, and `skills.diff` reads bounded text differences. Installation, removal, update, and deletion each create a plan through `*.preview`, followed by `{ planId, confirmed: true }` sent to the matching execution method. The core stores execution details; the client cannot supply arbitrary commands or file-operation steps.

Before execution, the core revalidates the action, plan validity, source, target, configuration, and records. Each physical directory is handled once while reporting every affected Agent. Deleting a library Skill keeps installation locations by default; including them requires an explicit choice. Backup restoration also requires confirmation, ownership checks, and content-integrity verification.

## Projects and failure boundaries

Git checks require explicit trust for the specific project path. Changing the path requires renewed confirmation, and revoking trust prevents subsequent checks. Archiving and deletion affect records, not project files. Upstream checks may update Git remote information but leave working files and HEAD unchanged.

Host-managed paths remain read-only. Out-of-bound paths and unsupported links are rejected. Failed recovery preserves diagnostics and available operations such as reading and exporting Prompts, while pausing Skill writes and operations that would alter recovery evidence. Verify actual files and failure recovery when changing these boundaries; see the [architecture](architecture.md).
