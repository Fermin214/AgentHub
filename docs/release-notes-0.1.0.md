# AgentHub 0.1.0

[中文](#中文说明) · [English](#english)

## 中文说明

AgentHub 是一款 Windows 本地应用，帮你把常用 Prompt、Skill 和项目整理在一起，方便查找、使用和维护。

你可以用它：

- 保存、搜索、复制和导出常用 Prompt。
- 从 Git、ZIP 或本地文件夹添加 Skill，选择安装位置、查看更新并恢复备份。
- 整理本地项目和仓库收藏，为信任的 Git 项目检查上游更新。

界面支持中文和英文。

### 下载和使用

在页面下方的 **Assets** 中选择适合你的文件：

| 文件 | 用途 |
| --- | --- |
| `AgentHub_0.1.0_x64-setup.exe` | Windows x64 安装版，下载后打开安装器 |
| `AgentHub-0.1.0-windows-x64.zip` | 便携版，完整解压后运行 `AgentHub.exe` |
| `SHA256SUMS.txt` | 下载文件的 SHA-256 校验和 |
| `build-manifest.json` | 源码提交、CI 构建来源、文件大小和签名状态 |

需要核对文件时，可以在 PowerShell 中运行下面的命令，再与 `SHA256SUMS.txt` 中对应的条目比较：

```powershell
Get-FileHash .\AgentHub_0.1.0_x64-setup.exe -Algorithm SHA256
```

### 运行要求和使用须知

AgentHub 需要 **WebView2 Evergreen Runtime**。安装版会在缺少运行时时下载安装；使用便携版时，需要单独安装运行时。Git 来源和上游更新检查需要 Git；检查项目更新前，需要确认信任该项目及其本地 Git 配置。

AgentHub 0.1.0 **未签名**。浏览器或 Windows 可能显示“不常见下载”“未知发布者”等提示，部分设备策略可能阻止运行。校验和用于核对文件完整性，不能代替发布者签名。

普通卸载会保留应用数据。备份或搬移便携版前，请先完全退出应用，并将程序与 `data` 文件夹一起保留。已经安装到外部 Agent 目录的 Skill 和项目文件夹仍使用原来的路径。更多操作说明见[使用指南（英文）](https://github.com/Fermin214/AgentHub/blob/main/docs/user-guide.md)。

验收范围和结果以本次发布文件对应的 Windows 测试记录为准。

### 问题反馈

遇到问题时，欢迎[提交反馈](https://github.com/Fermin214/AgentHub/issues/new/choose)，附上应用版本、Windows 版本和复现步骤。安全问题请私下发送至 **admin@hifermin.com**。

---

## English

AgentHub is a Windows app for keeping prompts, Skills, and local projects in one place.

- Save, search, copy, and export your prompts.
- Add Skills from Git, ZIP, or local folders, choose installation locations, review updates, and restore backups.
- Keep track of local projects and repository bookmarks, with upstream checks for Git projects you trust.

The interface is available in Chinese and English.

### Downloads

Choose a file from **Assets** below:

| File | Use |
| --- | --- |
| `AgentHub_0.1.0_x64-setup.exe` | Windows x64 installer |
| `AgentHub-0.1.0-windows-x64.zip` | Portable app; extract and run `AgentHub.exe` |
| `SHA256SUMS.txt` | SHA-256 checksums |
| `build-manifest.json` | Source commit, CI provenance, file sizes, and signing status |

Compare your download with the corresponding entry in `SHA256SUMS.txt` using PowerShell:

```powershell
Get-FileHash .\AgentHub_0.1.0_x64-setup.exe -Algorithm SHA256
```

### Requirements and limitations

WebView2 Evergreen Runtime is required. The installer downloads it when missing; the portable app requires it to be installed separately. Git sources and upstream checks need Git. Project checks require explicit trust in the project and its local Git configuration.

AgentHub 0.1.0 is **unsigned**. Windows or your browser may display publisher or reputation warnings, and device policies may block execution. Checksums verify file integrity; they do not replace a publisher signature.

A normal uninstall keeps application data. Fully exit before backing up or moving the portable app with its `data` directory. External Agent installations and project folders retain their original paths. See the [user guide](https://github.com/Fermin214/AgentHub/blob/main/docs/user-guide.md) for recovery and relocation details.

See the Windows acceptance record associated with these release files for the tested environment and results.

### Feedback

Please [report issues](https://github.com/Fermin214/AgentHub/issues/new/choose) with the app version, Windows version, and reproduction steps. Report security issues privately to **admin@hifermin.com**.