# Data maintenance baseline

Public data compatibility starts with **0.1.0**. Keep the original installer, portable ZIP, and checksums for each release so they can be used in future upgrade validation.

The baseline fixture includes every field of a multiline prompt, a local Skill source, library content, an external Agent installation, a local project, and a backup from before an update. Use fictional content and isolated directories, and record data fields and file hashes.

Before a later release, create the fixture with 0.1.0, fully exit, and open it with the new version. Verify readable records, correct paths, restorable backups, and unchanged external directories. For installed copies, also verify upgrades, data retention on uninstall, and reading retained data after reinstalling. For portable copies, verify relocation with existing data and recovery after relocation.

Data-format changes should include a migration path and a way to recover from failure. Record the old and new file hashes and source commits separately from ordinary feature-test results.
