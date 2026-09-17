# Coding agent guide

See the [technical overview](docs/architecture.md) and [contributing guide](CONTRIBUTING.md).

## Working approach

- Check the branch and working tree before editing, preserving the user's uncommitted files.
- Before changing product behavior, read the [product scope](docs/product-scope.md) and [domain contract](docs/domain-contract.md). Use the [glossary](docs/glossary.md) for interface wording; code contracts and serialization types define actual fields.
- Keep shared behavior in the Rust core, with Tauri and the internal CLI as thin transports. Update core types, frontend contracts, and actual consumers together when changing an interface.
- Keep host-managed content read-only. Use temporary directories and fictional data in tests; do not alter real Agent or project files.
- Public data compatibility starts with 0.1.0. Validate released-version data when changing storage or recovery behavior.

## Style

TypeScript uses two-space indentation, single quotes, and semicolons. Use PascalCase for React components and camelCase for functions and variables. Rust uses four spaces and snake_case. Follow adjacent code.

The interface and README support Chinese and English. Other project documentation is maintained in English. Keep facts, links, and feature scope aligned between README translations.

Keep versions synchronized in `package.json`, `Cargo.toml`, `src-tauri/tauri.conf.json`, and lockfiles. Verify with `node scripts/verify-release.mjs`.

## Validation

Use focused tests while developing. Before submitting executable code changes, run:

```powershell
pwsh -NoProfile -File scripts/acceptance.ps1 -Profile PullRequest
```

This is the default unified validation entry point for the Rust workspace, frontend tests, TypeScript/production build, and deterministic UI smoke checks. Small, reversible documentation-only changes may use narrower validation when they do not affect executable contracts.

When changing the acceptance harness, report schema, safety guards, cleanup behavior, or exit-code rules, also run:

```powershell
pwsh -NoProfile -File scripts/acceptance/self-test.ps1
```

UI changes require tests in the real App shell and a native WebView2 smoke check. Browser fixtures, native development checks, and packaged-release acceptance are distinct evidence layers; none substitutes for another.

For the isolated native development check, run `pwsh -NoProfile -File scripts/acceptance.ps1 -Profile DevelopmentDesktop`; see [acceptance automation](docs/acceptance-automation.md) for prerequisites and evidence boundaries.

Plain `cargo test` covers core and CLI by default; `cargo test --workspace` also includes the desktop crate.

Acceptance exit codes are contractual: `0` means all selected scenarios passed, `1` means failure (including cleanup failure), and `2` means incomplete (`not-run` or `environment-blocked`). Never mask failures or edit results to claim a pass.

## Bug-fix delivery rules

- For a new fix, regression tests must fail on the frozen BASE for the intended defect and pass on final HEAD. When adding coverage for an existing fix, use an identified defective revision instead; record both revisions. Compilation or environment failures do not establish reproduction. Report any comparison that cannot be reproduced reliably.
- Test fixtures must match real Rust/IPC requests and responses. Verify against the Rust implementation; partial responses must not be treated as full snapshots.
- For affected async state, cover duplicate requests, out-of-order completion, save failure, refresh failure, and concurrent unrelated-field updates.
- A passing canonical acceptance run does not override a reproducible failing review probe.

## Release validation

Desktop-host, installer, packaging, storage, upgrade, backup/recovery, portable-data, or startup changes require the relevant protected Release scenarios in [acceptance automation](docs/acceptance-automation.md) and [release testing](docs/release-testing.md). Desktop-host and installer changes also require `scripts/build.ps1 -Installer`. Never run destructive Release scenarios on a personal development host.

Release files must be traceable to a source commit, successful CI, and acceptance of those exact files. Building successfully does not establish installer-lifecycle acceptance. Rebuilding or replacing a candidate requires updated checksums and renewed acceptance of the affected scenarios before publication.

The standard Release profile leaves missing-WebView2 behavior, runtime download failure/recovery, real browser download/security prompts, and detailed packaged native UI review for the [manual Windows runbook](docs/acceptance-manual.md). Full release acceptance combines the automated report with those operator records. Do not publish GitHub Releases automatically.
