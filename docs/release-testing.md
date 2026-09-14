# Release testing

Each candidate should connect its source commit, successful CI, distribution files, and Windows acceptance. Test records apply to those exact files, not to other versions or rebuilt binaries.

## Build and preserve

1. Complete public-content and license review, then run `node scripts/verify-release.mjs` to check versions and license materials.
2. Run Windows CI on the final commit and confirm that Rust, frontend, and packaging jobs all succeed.
3. Preserve the CI installer, portable ZIP, `build-manifest.json`, and `SHA256SUMS.txt`. Verify the source commit and every file hash.
4. Use the same files for Windows testing and Release attachments. Rebuilding or changing a distribution file requires updated checksums and renewed validation of affected scenarios.

`scripts/prepare-candidate.ps1` records build provenance and signing status; it does not claim Windows acceptance is complete. Save a separate acceptance record before release with the Windows version, WebView2 state, steps, results, evidence, and unfinished checks.

## Windows scenarios

| Scenario | Expected checks |
| --- | --- |
| Installer lifecycle | Clean installation, default directory, shortcuts, displayed version, same-version reinstall, data-preserving uninstall, and reading data after reinstall |
| Portable use | Complete package contents; move used data from A to B, then verify reads, installation, and backup restoration without rewriting external paths |
| WebView2 | Existing runtime, missing runtime, retry after installer download failure, and actual portable behavior when the dependency is absent |
| Browser download | Download through a real browser, record Windows download/startup prompts or blocks, and verify downloaded file hashes |
| Core flows and recovery | Flows in the [product scope](product-scope.md), plus file results and available functions during file locks, stale plans, and recovery conflicts |
| Future upgrades | Use fictional data fixtures from a published version, following the [data maintenance baseline](maintenance-baseline.md) |

`scripts/verify-windows-vm.ps1` and `scripts/verify-vm-portable.ps1` assist with installer lifecycle and portable testing. They target isolated Windows test desktops and enforce fixed test-host, user, and directory guards. Read their parameters and guards before use. They do not cover real browser downloads or every runtime scenario. Do not bypass these guards on a personal working machine.

Check custom repair and upgrade pages, downgrade rejection, and the running-app warning in both installer languages. Setup normally follows the system language; launch with `/LANG=1033` for English or `/LANG=2052` for Simplified Chinese when validating both on one test desktop. Other values leave the default selection unchanged. Verify that cancelling maintenance preserves the existing program and data.

## Acceptance result

Distinguish passed, failed, not run, and environment-blocked checks. Preserve reviewable logs and screenshots using example data. For unsigned releases, report publisher and reputation prompts honestly, including device-policy blocks. An unverified scenario cannot be marked as passed.

Release notes provide supported platforms, installation choices, signing policy, known limitations, and feedback routes. Separate release blockers from deferrable improvements at handoff. Once blockers are closed and evidence is complete, proceed to release confirmation.
