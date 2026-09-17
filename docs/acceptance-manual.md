# Manual Windows release evidence

Scenarios without an enabled, executed adapter remain explicit `not-run` in the automated Release report. Run remaining checks on a prepared disposable VM and keep an operator record alongside the automated report; do not edit automated statuses. The authorized runtime adapter and packaged maintenance subset are described in [acceptance automation](acceptance-automation.md). No prior chat scripts are required. Full release acceptance combines the automated report with the remaining operator record.

For **each** scenario record ID, candidate source commit, all tested SHA256 values, VM identity/build, WebView2 version/state, start/end UTC, operator, exact actions, result (`passed`, `failed`, `not-run`, `environment-blocked`), reason, screenshots/logs, and restoration proof. Hash files before and after. An unexplained missing screenshot/check is not a pass. Link the record from the release acceptance summary.

## webview-missing

1. Prefer a clean snapshot without WebView2. Alternatively, after explicit operator authorization, use the reversible TEST/Try runtime-isolation adapter described in [acceptance automation](acceptance-automation.md). Never improvise shared-runtime deletion or bypass its guards. If neither route is available, record `environment-blocked`.
2. Preserve the snapshot identity and evidence that the runtime is absent. Extract the candidate portable ZIP into this run's new directory.
3. Launch the portable executable and record its actual message/exit behavior. Do not claim it starts if dependency absence prevents startup.
4. Launch the candidate installer and record its dependency-handling UI. The online successful path is completed in the next scenario.
5. Close only this run's processes, export evidence, restore the snapshot and verify runtime/network/installation baseline.

## webview-download-failure-recovery

1. Start from the runtime-free snapshot. At the hypervisor/test-network layer, disconnect only this VM's network; record original adapter state. Alternatively the explicitly authorized adapter injects an unreachable proxy for the TEST VM user and records that narrower fault model. Do not change the personal host proxy or network.
2. Run the candidate installer and record the runtime download failure and available recovery/cancel action. Check no partial AgentHub installation is presented as successful.
3. Restore VM networking and retry through the offered UI (or relaunch if that is the supported recovery). Confirm installed WebView2, successful AgentHub startup and readable main navigation.
4. Uninstall the test app, verify retained fictional data behavior where applicable, export evidence and restore the snapshot in a `finally`-equivalent operator cleanup step. If cleanup cannot finish, record `failed` and keep the VM isolated.

## browser-download-startup

1. Open Edge in the VM using a fresh profile and dedicated download directory. Use the actual CI artifact or public release download URL; a CLI download is insufficient. Record the URL without preserving expiring signed query credentials.
2. Download the candidate installer and portable ZIP; compare their SHA256 to the accepted candidate. If an outer CI ZIP is downloaded, extract with Explorer and compare the inner distribution bytes.
3. Record `Zone.Identifier` / Mark of the Web and all browser, publisher, SmartScreen or device-policy prompts. The operator may approve a prompt only after confirming the expected filename/hash. Never use `Unblock-File`, turn off reputation checks or hide a policy block.
4. Launch the downloaded installer through Explorer, verify installation/startup, then launch the Explorer-extracted portable app. Record successful rendered navigation or the exact block; absence of a prompt is also evidence, not something to simulate.
5. Close owned browser/app processes; uninstall the owned test installation; preserve screenshots, hashes and observations; restore the VM snapshot/baseline.

## native-ui-details

Use the candidate desktop with fictional data, not the browser fixture or personal data. Retained automated portable fixtures provide a seeded Skill and Prompt, but add a separate long-document fixture if needed.

- Prompt tags: plain text pills, no generated `#` prefix; readable long tags.
- Skill row: original icons for supported Agents load; source link is muted and beside the title; no external source link for a local Skill.
- Viewer: many files and a long document; mouse scroll and PageDown/PageUp affect the focused pane independently; close and source controls stay reachable at the supported minimum 860×640 client size.
- Source lookup: cancel an in-progress controlled test repository lookup, observe cancellation completion, then retry successfully. Do not rely on a particular public repository's current contents; prepare a disposable local/test Git repository with one fixed Skill.
- Repository bookmark: selecting/clicking notes or blank space does not open a browser; clicking the explicit title link does.
- Check both supported languages where wording matters. Record subjective visual approval separately from functional assertions.

Restore any fixture target paths to run-owned directories, close owned processes, retain evidence and verify no pending lookup/process remains. The native four-page smoke already validates basic startup; this record addresses the detailed packaged WebView2 behavior that mock transport tests cannot establish.
