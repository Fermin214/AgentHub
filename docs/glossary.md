# Product glossary

| Term | Meaning |
| --- | --- |
| Prompt | A text template that can be saved, edited, searched, copied, and exported |
| Skill | A capability package containing `SKILL.md` for an Agent to use |
| Skill library | Skill copies managed by AgentHub; adding one does not mean it is installed |
| Skill source | A Git, ZIP, or local source and any required subdirectory |
| Skill installation location | The actual directory an Agent reads for all projects or a specific project |
| Agent | An application that uses Skills, such as Codex or Claude Code |
| Local project | An existing directory on the computer, including non-Git folders |
| Repository bookmark | A repository link, notes, tags, status, and optional local-project association |
| Archived | Hidden from everyday lists while retaining information and project files |
| Backup | A copy of file contents and associated records used for recovery |

| Action | Actual effect |
| --- | --- |
| Add to the Skill library | Save a library copy without automatically installing it for an Agent |
| Install for / remove from an Agent | Change the specified installation location; removal keeps the library copy |
| Delete from the Skill library | Delete the library copy; removing installation locations requires a separate explicit choice |
| Set source | Save a source for future checks without overwriting current Skill content |
| Check for updates / view changes | Retrieve and compare source content without writing to installation locations |
| Apply update | Write to selected locations after confirmation and retain backups as selected |
| Add local project | Register a directory without cloning or updating project files |
| Check upstream | After trust confirmation, retrieve and compare Git remote information without changing working files or HEAD |
| Archive / delete record | Hide or delete AgentHub records while keeping local files; deleting a local-project record also deletes its linked bookmark |

Internal code uses `SkillDeployment` for an actual installation location. User-facing text uses direct terms such as “installation location.” Describe the affected scope with Agent names and concrete paths.
