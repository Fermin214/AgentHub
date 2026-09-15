# Coding agent guide

This is the project's coding-agent guidance. See the [technical overview](docs/architecture.md) and [contributing guide](CONTRIBUTING.md).

## Working approach

- Check the branch and working tree before editing, preserving the user's uncommitted files.
- Keep shared behavior in the Rust core, with Tauri and the internal CLI as thin transports. Update core types, frontend contracts, and actual consumers together when changing an interface.
- Keep host-managed content read-only. Use temporary directories and fictional data in tests; do not alter real Agent or project files.
- Public data compatibility starts with 0.1.0. Validate released-version data when changing storage or recovery behavior.

## Style and validation

TypeScript uses two-space indentation, single quotes, and semicolons. Use PascalCase for React components and camelCase for functions and variables. Rust uses four spaces and snake_case. Follow adjacent code.

The interface and README support Chinese and English. Other project documentation is maintained in English. Keep facts, links, and feature scope aligned between README translations.

Use focused tests while developing. Before submitting executable code changes, run:

```powershell
pwsh -NoProfile -File scripts/acceptance.ps1 -Profile PullRequest
```

This is the default unified validation entry point for the Rust workspace, frontend tests, TypeScript/production build, and deterministic UI smoke checks. Small, reversible documentation-only changes may use narrower validation when they do not affect executable contracts.

When changing the acceptance harness, report schema, safety guards, cleanup behavior, or exit-code rules, also run:

```powershell
pwsh -NoProfile -File scripts/acceptance/self-test.ps1
```

Desktop-host and installer changes also need `scripts/build.ps1 -Installer`. Plain `cargo test` uses core and CLI as its default members; `cargo test --workspace` includes the desktop crate.

Desktop-host, installer, packaging, storage, upgrade, backup/recovery, portable-data, or startup changes require the relevant protected Release validation described in [acceptance automation](docs/acceptance-automation.md) and [release testing](docs/release-testing.md). Never run destructive Release scenarios on a personal development host.

Acceptance exit codes are contractual: `0` means all selected scenarios passed, `1` means failure including cleanup failure, and `2` means incomplete because a scenario is `not-run` or `environment-blocked`. Never mask, rewrite, or describe exit `2` as complete acceptance.

Keep versions synchronized in package.json, Cargo.toml, src-tauri/tauri.conf.json, and lockfiles. Check them with `scripts/verify-release.mjs`.

## Release validation

Release files must be traceable to a source commit, successful CI, and acceptance of those exact files. Building successfully does not establish installer-lifecycle acceptance. Rebuilding or replacing a candidate requires updated checksums and renewed acceptance of the affected scenarios before publication.

The standard Release profile deliberately leaves missing-WebView2 behavior, runtime download failure/recovery, real browser download/security prompts, and detailed packaged native UI review for the [manual Windows runbook](docs/acceptance-manual.md). Full release acceptance combines the automated report with those operator records. Do not edit automated results to green and do not publish GitHub Releases automatically.

Read the [product scope](docs/product-scope.md) and [domain contract](docs/domain-contract.md) before changing product behavior. Use the [glossary](docs/glossary.md) for interface wording. Documentation describes current behavior; code contracts and serialization types define the actual fields.
