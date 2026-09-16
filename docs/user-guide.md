# User guide

[简体中文](user-guide.zh-CN.md) · [Back to README](../README.en.md)

## Prompts and Skills

Save a title and text in **Prompts**, then search, edit, copy, or export it. Categories and tags help you find it again; tags do not run any commands.

Built-in Skill targets are **Codex, Claude Code, DeepSeek Harness, ZCode, and Hermes**. Review detected locations in Settings; availability depends on the Agent and directories on your computer. AgentHub manages Skill copies and does not install the Agents themselves. It keeps host-managed content read-only.

To add a Skill, open **Skills → Add Skill**, enter a Git repository, HTTPS ZIP URL, or local directory, and find its Skills. A source must contain `SKILL.md`; repositories with multiple Skills let you choose which to add. Adding to the library does not install it for an Agent. Select an Agent or project separately and review the destination before confirming.

Source inspection shows its current stage and elapsed time. Use **Cancel**, the close button, or Escape in the Add Skill dialog to stop an inspection. Wait for cancellation and cleanup before retrying; a second inspection cannot run in the same data directory at the same time. Errors provide a short explanation and expandable diagnostics. Saving a selected Skill is a separate operation; its dialog stays protected until saving finishes.

Click a Skill name to view the files of the Skill library copy. The file list and text scroll independently; focus either pane to use Page Up / Page Down. For a Git web source, **Open source repository ↗** opens the repository home. Local, ZIP, or unknown sources keep the internal viewer without a repository link. Project repository bookmarks also use an explicit external link.

Check updates before applying changes and review the affected locations. Agent icons retain their brand colors; a check mark and green border indicate installation, while a disabled control is dimmed. Prompts and Skills remain local records even when their source is online.

## Verify a download

Run `Get-FileHash .\filename -Algorithm SHA256` in PowerShell and compare it with `SHA256SUMS.txt` from the same release. `build-manifest.json` records the version, source commit, CI run, file sizes, and signing status. Compare these records with the official release and its CI run to check file integrity and trace the build. Self-reported metadata is not independent proof of publisher identity and does not replace a Windows publisher signature. Signing would not guarantee immediate SmartScreen reputation.

## Runtime requirements

The target is Windows x64. The release's acceptance attachments record the Windows versions, scenarios, and exact file hashes actually tested.

If WebView2 is missing, the installer downloads it from Microsoft. After an offline, failed, or cancelled download, restore connectivity and retry, or install the Evergreen Runtime from [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/#download-section). This first release does not provide a complete offline runtime installer. The portable app needs WebView2 too.

Git project checks are disabled until you explicitly trust the project. Git can invoke SSH, credential helpers, and other programs from local configuration. Revoking trust prevents future checks; changing the project path requires confirmation again. AgentHub fetches and compares upstream changes without automatically merging them. Source and file-operation safeguards do not guarantee that a Skill is safe when later executed by an Agent.

## Data and recovery

Reinstalling remembers the previously selected installation directory. The database is `data/agenthub.sqlite3`. A normal uninstall keeps data; explicitly selecting data deletion removes it. Fully exit before copying the entire data folder for a backup. Prompts can also be exported separately.

When relocating the portable directory, AgentHub updates references only to its own Skill library and backups. External projects, Agent directories, and local sources keep their paths; run update checks again. Complete unfinished file operations at the original location before moving. If already moved, follow the app's instruction to move it back for recovery.

If Skill recovery fails, you can still use and export prompts and view backups and settings. Diagnostics, affected paths, and a retry action remain available. All Skill writes and operations that would alter recovery evidence are conservatively paused until recovery succeeds. Do not delete recovery records to force further writes.

When multiple Agents share a directory, the interface shows the related impact and handles the directory once. Network operations follow local proxy and tool configuration. Local storage does not mean the app never connects to the internet.
