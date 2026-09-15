# Acceptance automation

Run from the repository root. No conversation history is required. The entry point orchestrates existing tests and native Windows verification; it never builds a release candidate or publishes GitHub Releases.

## Requirements

- Host: Windows, PowerShell 7 (`pwsh`), Node/npm, Rust/MSVC toolchain from [contributing](../CONTRIBUTING.md), Git, and installed Microsoft Edge. Run `npm ci` first. Playwright is pinned in the lockfile and uses Edge; no global Playwright install or browser download is needed.
- Optional local bundled build tools: set `AGENTHUB_BUILD_ROOT` as described in `scripts/build-env.ps1`. An explicit `RUSTUP_TOOLCHAIN` override is process-local; record it when used.
- Release: OpenSSH client and key-based access to the disposable **TEST / Try** Windows VM. The Try desktop must be logged in and unlocked. VMware, Hyper-V, VirtualBox, KVM/QEMU hardware is checked. The fixed root is `C:\AgentHub-VM-Test`. Scheduled tasks run as Try in its interactive desktop. A disconnected/locked desktop can block native UI checks.
- Start from a clean VM with WebView2 installed, no registered AgentHub, remembered installer path, default AgentHub data directory, AgentHub shortcuts or running AgentHub. Preserve previous evidence; restore an appropriate VM snapshot rather than deleting unrelated data to satisfy a guard. Keep personal accounts/files out of this VM.
- Preserve installer, portable ZIP, manifest and checksum file from the same successful CI candidate. The internal `agenthub-dev.exe` fixture helper is **not** a release asset: build it from the matching candidate source (and separately from the public base source). The runner records its hash and checks version; maintainers must verify its source provenance. A matching version alone does not establish provenance.

## One command per profile

```powershell
pwsh -NoProfile -File scripts/acceptance.ps1 -Profile PullRequest
```

This runs `scripts/test.ps1` stages (Rust workspace, Vitest, TypeScript/production build), then the fixed Playwright UI smoke suite. Failures in one stage do not hide later results. This is the existing full automatic check set plus UI checks; `scripts/test.ps1` without arguments remains supported.

Copy `scripts/acceptance/vm.example.json` to an ignored local file, fill in absolute paths and the VM address, then run:

```powershell
pwsh -NoProfile -File scripts/acceptance.ps1 -Profile Release `
  -CandidatePath output/release-candidate -BaseVersion 0.1.0 `
  -VmConfig output/acceptance-vm.json
```

`BaseVersion` is the previous **public** version, not the candidate version. `baseCandidatePath` must contain the same four candidate files for that base. Omit the base paths if unavailable; upgrade becomes `environment-blocked`. Without `VmConfig`, no installer is executed and VM scenarios are blocked.

```powershell
pwsh -NoProfile -File scripts/acceptance.ps1 -Profile Release -ListScenarios
pwsh -NoProfile -File scripts/acceptance.ps1 -Profile Release -DryRun
pwsh -NoProfile -File scripts/acceptance/self-test.ps1
```

ListScenarios is read-only and exits 0. DryRun creates a plan/report with `not-run` statuses and exits **2**; it does not establish acceptance. `OutputRoot` optionally selects the evidence parent; existing runs are never overwritten.

## Coverage and boundaries

| Profile / scenario | What executes |
| --- | --- |
| PullRequest | Full Rust workspace, frontend tests, TypeScript and production build |
| PR UI fixture | Plain Prompt pills; loaded original Agent icons; source link in viewer heading and absent for local sources; independent keyboard scroll at 1280×860 and 860×640; cancellation followed by successful retry; bookmark notes do not navigate |
| Release lifecycle | Fresh default install, shortcuts and versions, seeded Prompt/Skill/deployment/backup, same-version reinstall, uninstall retaining database bytes, reinstall reading retained data, native four-page smoke |
| Release portable | Used directory A moved to B; native startup first; unchanged external files; install and restore a pre-move backup |
| Release upgrade | Seed with base CLI and actual public base installer, upgrade to candidate, verify retained records/files, then same-version lifecycle |
| Release languages | English and Chinese repair/upgrade/downgrade/running warnings; cancellation preserves bytes. Page-selection fixtures simulate registration versions; the upgrade scenario separately tests real migration |
| Release cleanup | Before/after proxy values, WebView2 executable hashes, registration, remembered path, shortcuts, default directory and AgentHub processes; owned task removed; candidate bytes checked again |
| Failure cleanup probe | Deliberately fail immediately after installation; require a nonzero child exit, the exact injected error and successful `finally` restoration |
| Manual release scenarios | Missing WebView2, offline download/recovery, real browser download/security prompts, and detailed packaged-WebView2 interaction: explicit `not-run` with the [manual runbook](acceptance-manual.md) |

The browser fixture loads the actual frontend with a fictional in-memory IPC implementation. It does **not** prove real Git cancellation, installed WebView2 behavior, backend persistence or aesthetic quality. Rust tests and native release scenarios provide separate evidence. The fixture is outside the production entry point and is not bundled into the app.

Windows CI retains its existing job names and packaging dependencies. The frontend job additionally runs harness self-tests and the same Edge UI suite, then uploads their evidence even when a test fails. `npm run test:ui` and `npm run test:acceptance` are convenient individual checks.

The standard Release run intentionally remains incomplete until the manual scenarios are recorded. Do not convert missing evidence into passed results. Existing WebView2 is exercised by native startup; the runner does not rename/delete shared runtime folders or change proxy settings to create missing-runtime conditions.

## Read results

Each run writes `output/acceptance/a-<UTC>-<random>/`:

```text
acceptance.json         # schemaVersion, harness commit, dirty flag, per-scenario status/times/evidence
acceptance.md           # readable summary
environment.json        # host environment; VM before/after are under vm/
candidate-hashes.json   # candidate source commit, all four files before/after, unchanged flag
cleanup.json            # aggregate cleanup, including remote restoration errors
logs/                  # independent command logs and working-tree state
screenshots/           # PR screenshots and failure traces
vm/                    # native reports, screenshots, before/after state and cleanup
staging/               # exact transmitted scripts, inputs and helper hashes
```

Harness commit and candidate commit are different concepts: testing an old release with new automation is valid. A dirty harness is recorded, not silently attributed to HEAD. Preserve the exact diff or commit it before using a report for final delivery. Candidate manifests record CI provenance; checking hashes does not query or prove CI success. Verify that run separately under [release testing](release-testing.md).

Exit codes: **0** all selected scenarios passed; **1** a failure (including cleanup); **2** incomplete (`not-run`/`environment-blocked`). Never use `exit 0` around this runner in CI. Open the failed scenario's log first; report file paths link native evidence. Re-run in a fresh evidence directory after a fix; do not edit an automated result to green.

## Failure recovery and ownership

The host stages only validated inputs, creates a unique task and takes an exclusive VM lock. The worker runs each existing native script in its own PowerShell process. Each system-changing case owns a fresh directory and installation marker. Cleanup runs in `finally`, including on scenario errors. Uninstalled fictional data remains under the case directory as evidence; it is not an active installation.

If SSH disconnects or the 30-minute host deadline expires, the host does **not** stop the worker mid-cleanup. Inspect `C:\AgentHub-VM-Test\runs\<runId>\evidence\progress.json`, the task `AgentHub-Acceptance-<runId>` and its logs. After the worker has finished, retrieve the evidence directory with `scp -r`. The root `acceptance-lock/owner.txt` identifies the owner. A cleanup failure retains this lock to prevent another test from compounding damage.

For recovery, verify the run ID, task action path and installation-owner.json; compare the before/after state and inspect the retained installation. Use the case's matching uninstaller or restore the VM snapshot after copying evidence. Never delete an unknown registration, terminate an unrelated process, remove a lock owned by an active task, recurse through links, clear Mark of the Web or disable security controls. A failed cleanup remains a failed run even after later recovery.

## Add or change a scenario

1. Add its ID and scope to `scripts/acceptance/scenarios.json`.
2. Reuse an existing native verifier or add the smallest cohesive adapter under `scripts/acceptance/`; no product behavior belongs here.
3. Return one of the four statuses with a reason and evidence. Only an executed assertion can establish `passed`. Prerequisite absence is blocked; deliberately manual work is not-run.
4. Snapshot any state before mutation, prove ownership, restore in `finally`, and expose restoration failure. Extend the self-test for a new guard or report rule.
5. For UI contracts, add deterministic fictional fixture behavior and a semantic Playwright assertion in `tests/ui`. Avoid internet-dependent repositories, arbitrary sleep-based assertions and subjective visual scores.
6. Run self-tests, the full PullRequest profile, and affected Release scenarios against immutable known bytes. Review the diff and preserve failures as evidence.

## Prohibited actions

No destructive release scenarios on a personal host; no guard bypass; no edits to real Agent/project data; no broad process killing, recursive cleanup of computed unchecked paths, shared runtime removal, security-prompt suppression, candidate rebuilding during acceptance, or automatic GitHub publication. The maintainer decides subjective UI acceptance and formal publication.
