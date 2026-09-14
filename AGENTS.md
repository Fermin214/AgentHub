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

Run checks appropriate to the change. Run `scripts/test.ps1` before submitting code changes; desktop-host and installer changes also need `scripts/build.ps1 -Installer`. Plain `cargo test` uses core and CLI as its default members; `cargo test --workspace` includes the desktop crate. Small, reversible documentation or style changes do not need tests that simply restate the implementation.

Keep versions synchronized in package.json, Cargo.toml, src-tauri/tauri.conf.json, and lockfiles. Check them with `scripts/verify-release.mjs`.

## Release validation

Release files must be traceable to a source commit, successful CI, and acceptance of those exact files. Building successfully does not establish installer-lifecycle acceptance. See [release testing](docs/release-testing.md).

Read the [product scope](docs/product-scope.md) and [domain contract](docs/domain-contract.md) before changing product behavior. Use the [glossary](docs/glossary.md) for interface wording. Documentation describes current behavior; code contracts and serialization types define the actual fields.
