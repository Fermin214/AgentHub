# AgentHub

**Keep your prompts, Skills, and local projects in one place.**

[Download](https://github.com/Fermin214/AgentHub/releases) · [Report an issue](https://github.com/Fermin214/AgentHub/issues) · [简体中文](README.md)

I built AgentHub as a small Windows desktop app for the prompts and Skills you collect while using AI Agents. It helps you find something when you need it, see where a Skill came from and where it is installed, and keep track of updates.

## What you can do

- **Keep your useful prompts.** Organize them with categories and tags, search, edit, copy, or export them. They remain your own text, ready to use in whichever tool you prefer.
- **Manage your Skills.** Add them from a Git repository, ZIP, or local directory, then choose which Agent or project to install them for. Review changes before an update and restore a backup when needed.
- **Keep track of projects and repositories.** Save local folders, repository links, and notes together. You can also check upstream changes for Git projects you explicitly trust.

The interface supports English and Chinese. Switch languages under **Settings → About → Interface language**.

Built-in Skill targets include **Codex, Claude Code, DeepSeek Harness, ZCode, and Hermes**. Available locations depend on local discovery and your settings.

## Download and install

AgentHub supports **Windows x64**. Visit [Releases](https://github.com/Fermin214/AgentHub/releases) to download the installer or portable version.

| How you want to use it | Choose this file |
| --- | --- |
| Install normally, with desktop and Start menu shortcuts | `AgentHub_0.1.0_x64-setup.exe` |
| Extract into your own folder and move the app together with its data | `AgentHub-0.1.0-windows-x64.zip` |

For the portable version, extract the ZIP and run `AgentHub.exe`. GitHub's Source code downloads are for building the project. You do not need them, Node.js, or Rust to use the app.

The interface needs the [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/#download-section). The installer downloads it if it is missing; the portable app expects it to be installed already. If the first installation runs into a network problem, you can install WebView2 separately and try again. Git sources and project update checks also need Git on your computer.

Version 0.1.0 **does not have a Windows digital signature**. Your browser or Windows may show an unknown-publisher or reputation warning, and some device policies may block it. Follow your device's security policy when deciding whether to run it. Each release includes checksums so you can verify your download; the [user guide](docs/user-guide.md) explains how.

## Try it out

Start with a prompt: open **Prompts**, click **Add**, enter a title and some text, and save. The next time you need it, search for it and copy the text.

To try a Skill:

1. Prepare a test folder containing a `SKILL.md` file.
2. Open **Skills → Add Skill** and add that local folder to your library.
3. Choose an Agent or project, review the installation location, and confirm.

Adding a Skill to the library and installing it for an Agent are separate steps. You can keep something in your library before deciding where to use it. A test folder is a good place to get familiar with the workflow.

## Where your data lives

Your records stay on your computer. The installer defaults to `%LOCALAPPDATA%\AgentHub`, with a `data` folder inside. In the portable version, `data` sits next to the executable.

A normal uninstall keeps your data. It is removed only if you explicitly choose to delete application data. To make a backup, fully exit the app and copy the entire `data` folder. You can also export prompts separately.

You can move the portable app together with `data` after closing it, but external Agent installations, local projects, and source folders stay at their existing paths. If the app shows a recovery message, follow its instructions before removing any files. The [user guide](docs/user-guide.md) explains backups, relocation, and recovery in more detail.

Local storage does not mean the app never connects to the internet. Downloading Skills and checking remote sources or Git upstreams contact the relevant services. Please review a Skill's content before using it with an Agent.

## Feedback is welcome

This is the first public version. If something gets in your way, please [open an issue](https://github.com/Fermin214/AgentHub/issues/new/choose). Include the app version, Windows version, what you did, what you expected, and what happened instead. Remove private information from logs and screenshots before sharing them.

For security issues, please email **admin@hifermin.com** privately instead of posting vulnerability details in a public issue. See [SECURITY.md](SECURITY.md).

Public data compatibility starts with **0.1.0**. Installation, file protection, and recovery issues come first in future maintenance.

## Development and license

If you would like to build the app or contribute, start with the [contributing guide](CONTRIBUTING.md). The [technical overview](docs/architecture.md) describes the current implementation.

AgentHub is [MIT licensed](LICENSE). Thanks to Tauri, React, Lobe Icons, and the other open-source projects it builds on. Their notices and license texts are preserved in the [third-party notices](THIRD-PARTY-NOTICES.md) and application packages.
