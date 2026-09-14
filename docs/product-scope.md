# Product scope

AgentHub is a local AI resource manager for Windows. It helps people organize Prompts, manage Skills and their installation locations, and keep track of local projects and repository links. Public data compatibility and maintenance start with **0.1.0**.

## Current modules

| Module | Capabilities |
| --- | --- |
| Prompt | Create, edit, categorize, tag, search, copy, and export text |
| Skill | Add from Git, ZIP, or local directories; import existing Skills; install, remove, check sources, preview updates, and restore backups |
| Projects | Register local directories; check upstream for trusted Git projects; bookmark repositories, save notes and tags, link local projects, and archive records |
| Settings | Configure Agent Skill locations, search locations, networking, local data, and interface language |

## Boundaries

AgentHub manages resources and files. It does not run Agents, execute custom project commands, or install arbitrary toolchains. Adding a Skill to the library and installing it for an Agent are separate actions. Registering or deleting a local project does not change project files, and checking Git upstream does not merge changes automatically.

The internal CLI is a development and test tool, not a product interface, and is not distributed with the app. The browser preview shows static examples; actual file operations run in the desktop app.

## Core user flows

1. Edit, search, copy, and export Prompts.
2. Select and add multiple Skills from Git or ZIP sources.
3. Import Skills already on the computer into the library.
4. Install and remove global Skills for one Agent.
5. Install and remove Skills for a specific project.
6. Show the effects of shared physical directories while keeping independent directories isolated.
7. Set sources, check changes, select update locations, and restore backups.
8. Check upstream for trusted Git projects without changing working files or HEAD.
9. Bookmark repositories, maintain their status, link local projects, and archive records.

Validation also covers file protection, failure recovery, and data preservation. See [release testing](release-testing.md) and the [glossary](glossary.md).
