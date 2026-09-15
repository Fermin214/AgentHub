# Contributing

Bug reports, documentation improvements, and fixes are welcome. For a larger change, open an issue first to describe the problem so we can discuss the scope.

## Local development

You need Windows, PowerShell 7, Node.js 22+, Rust MSVC (see `rust-toolchain.toml` for the version), and the Windows SDK / Visual Studio C++ Build Tools.

```powershell
npm ci
npm run tauri:dev
```

`npm run dev` provides a static browser preview. Real file operations require the desktop app.

```powershell
npm run check                       # TypeScript
npm test                            # Frontend tests
.\scripts\test.ps1                  # Rust and frontend tests, frontend build
.\scripts\build.ps1 -Installer      # Windows installer
.\scripts\package-portable.ps1 -SkipBuild
node scripts/verify-release.mjs
```

Use `cargo test -p agenthub-core <test_name>` or `npx vitest run src/App.test.tsx` for focused checks. Use temporary directories and fictional test data, rather than real Agent or project files.

For a repeatable report including Edge UI smoke tests, run `pwsh -NoProfile -File scripts/acceptance.ps1 -Profile PullRequest`. The [acceptance guide](docs/acceptance-automation.md) covers prerequisites, protected VM release testing, structured results and recovery. Release testing never runs destructive scenarios on the development host.

## Submitting a change

Explain the problem, the resulting behavior, and how you validated it. For interface changes, a screenshot with example data is helpful. Run relevant tests before submitting; code changes should also pass the unified test script. Use `fix:`, `feat:`, `docs:`, or `chore:` in commit titles.

Do not commit databases, private prompts, tokens, proxy credentials, or unredacted logs. Data loss, unintended overwrites, installation failures, and recovery problems take priority. Report security issues privately as described in [SECURITY.md](SECURITY.md).

The [product scope](docs/product-scope.md) and [glossary](docs/glossary.md) describe features and terminology. The [architecture](docs/architecture.md) and [domain contract](docs/domain-contract.md) explain core modules and operation boundaries. See [release testing](docs/release-testing.md) and the [data maintenance baseline](docs/maintenance-baseline.md) for acceptance and upgrade validation. Instructions for coding agents are in [AGENTS.md](AGENTS.md).
