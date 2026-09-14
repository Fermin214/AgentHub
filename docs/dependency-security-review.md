# Dependency security review

Checked on 2026-09-14 against the 0.1.0 source based on `d75c5dac072540254dc7d8b9c858a076f0f68be0`. The machine used Node 24.15.0 and npm 11.12.1; CI uses Node 22. Exact UTC timestamps, exit codes, and the resulting lockfile SHA-256 are in [the audit metadata](audits/2026-09-14/audit-after-metadata.json).

The initial restricted-network attempt failed to contact the npm audit endpoint and was not treated as a vulnerability result. The completed official-registry checks returned five affected package entries (three moderate, one high, one critical); the production-only audit returned zero. After the targeted upgrade, both complete and production-only audits returned zero, with exit code 0. This is a point-in-time npm check, not a guarantee about all dependencies or future advisories.

## Findings and disposition

The five package entries represent six distinct advisory IDs plus transitive effects. All affected paths were development dependencies. Production bundles contain the compiled frontend, not the Vitest/Vite development servers. The project runs `vitest run` with jsdom, without UI, Browser Mode, or an exposed Vitest API. These facts limit applicability; they were not used to suppress findings.

| Advisory | Trigger and affected dependency path | Disposition |
| --- | --- | --- |
| [GHSA-5xrq-8626-4rwp](https://github.com/vitest-dev/vitest/security/advisories/GHSA-5xrq-8626-4rwp) | Listening Vitest UI/API server; direct `vitest@2.1.9` | Upgraded to 4.1.11; above the 3.2.6 fix boundary. |
| [GHSA-82fw-gwwq-j7x9](https://github.com/vitest-dev/vitest/security/advisories/GHSA-82fw-gwwq-j7x9) | Redirect mocks through a reachable development-server mock interface; `vitest` and its `@vitest/mocker` | Both are 4.1.11, the patched 4.x version. |
| [GHSA-4w7w-66w2-5vf9](https://github.com/vitejs/vite/security/advisories/GHSA-4w7w-66w2-5vf9) | Exposed dev server and predictable source-map paths; nested Vite 5.4.21 under `vitest` and `vite-node` | Removed nested Vite 5; resolved Vite is 6.4.3. |
| [GHSA-v6wh-96g9-6wx3](https://github.com/advisories/GHSA-v6wh-96g9-6wx3) | Windows editor-launch handling of an attacker-controlled UNC path; the same nested Vite paths | Removed affected paths; Vite 6.4.3 remains. |
| [GHSA-fx2h-pf6j-xcff](https://github.com/vitejs/vite/security/advisories/GHSA-fx2h-pf6j-xcff) | Reachable Windows dev server with alternate file paths bypassing deny rules; the same nested Vite paths | Removed affected paths; Vite 6.4.3 remains. |
| [GHSA-67mh-4wv8-2f99](https://github.com/evanw/esbuild/security/advisories/GHSA-67mh-4wv8-2f99) | A malicious webpage accessing esbuild's development server; nested esbuild under `vitest` and `vite-node` | Old nested copies removed; remaining esbuild is 0.25.12. |

`vite-node` was an affected transitive entry through Vite, not another independent advisory. It is no longer in the lockfile. No force-fix, overrides, deleted tests, or audit exclusions were used. Vitest is pinned to 4.1.11; top-level Vite remains 6.4.3. The obsolete Vitest 2 duplicate-type cast was removed from the test configuration.

## Evidence and license scope

- Before: [complete audit](audits/2026-09-14/npm-audit-before.json), [production audit](audits/2026-09-14/npm-audit-production-before.json), [metadata](audits/2026-09-14/audit-before-metadata.json).
- After: [complete audit](audits/2026-09-14/npm-audit-after.json), [production audit](audits/2026-09-14/npm-audit-production-after.json), [metadata](audits/2026-09-14/audit-after-metadata.json).
- [License-scope comparison](audits/2026-09-14/license-scope.json): npm runtime lock entries and Cargo.lock are unchanged. The existing 355-entry distributed dependency inventory and license texts remain applicable; pure test-tool changes do not require regenerating those notices.

Local validation passed all 107 Rust tests, 72 frontend tests across 23 files, TypeScript checking, and the frontend build. Release-version, branding, and license checks passed. Distribution-file acceptance must still refer to the exact build being released, as described in [release testing](release-testing.md).
