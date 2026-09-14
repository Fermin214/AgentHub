# AgentHub

**把常用 Prompt、Skill 和本机项目放在一起管理。**

[下载](https://github.com/Fermin214/AgentHub/releases) · [反馈问题](https://github.com/Fermin214/AgentHub/issues) · [English](README.en.md)

AgentHub 是我做的一个 Windows 桌面小工具。如果你也在使用不同的 AI Agent，积累了一些常用提示词和 Skill，可以用它把这些内容整理起来：需要时找得到，也能看清一个 Skill 从哪里来、装到了哪里、有没有更新。

## 可以用它做什么

- **保存常用 Prompt。** 按分类和标签整理，搜索、编辑、复制，也可以导出成文件。保存的是你自己的文本，方便在不同工具里使用。
- **管理 Skill。** 从 Git 仓库、ZIP 或本机目录加入 Skill 库，再选择装给哪个 Agent、哪个项目。更新前先看变化，需要时可以恢复备份。
- **记住本机项目和仓库。** 把本机目录、仓库链接和备注放在一起。对你确认信任的 Git 项目，还可以检查上游变化。

界面支持中文和英文，可以在「设置 → 关于 → 界面语言」中切换。

## 下载与安装

目前支持 **Windows x64**。下载安装包或便携版，请前往 [Releases](https://github.com/Fermin214/AgentHub/releases)。

| 你想怎样使用 | 选择这个文件 |
| --- | --- |
| 正常安装，使用桌面和开始菜单快捷方式 | `AgentHub_0.1.0_x64-setup.exe` |
| 解压到自己的目录，带着程序和数据一起移动 | `AgentHub-0.1.0-windows-x64.zip` |

便携版解压后运行 `AgentHub.exe`。GitHub 自动提供的 Source code 是源码，使用软件不需要下载它，也不需要安装 Node.js 或 Rust。

桌面界面需要 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/#download-section)。安装版会在缺少它时联网下载安装；便携版需要电脑上已经安装。第一次安装遇到网络问题时，可以装好 WebView2 后再试。使用 Git 来源或检查项目更新，还需要电脑上有 Git。

0.1.0 **没有 Windows 数字签名**，下载或打开时可能遇到未知发布者、信誉提示，部分设备策略也可能阻止运行。请按自己设备的安全策略决定是否运行。每次发布都会附上校验和，方便你核对下载文件；具体方式见[使用说明](docs/user-guide.md)。

## 先试一次

可以先从一条 Prompt 开始：打开 **Prompts**，点击 **添加**，写下名称和正文并保存。下次需要时，搜索它、复制正文就可以了。

想试试 Skill 的话：

1. 准备一个包含 `SKILL.md` 的测试目录。
2. 在 **Skill → 添加 Skill** 中选择本机目录，把它加入库。
3. 选择要使用它的 Agent 或项目，核对显示的安装位置后确认。

「加入库」和「安装给 Agent」是两件事，你可以先收藏，再决定用在哪里。刚开始建议用测试目录熟悉一下流程。

## 数据放在哪里

你的记录保存在本机。安装版默认放在 `%LOCALAPPDATA%\AgentHub`，数据在其中的 `data` 文件夹；便携版的 `data` 就在程序旁边。

普通卸载会保留数据，只有你主动选择删除应用数据时才会移除它。备份时，先完全退出程序，再复制整个 `data` 文件夹；Prompt 也可以单独导出。

便携版可以在退出后连同 `data` 一起搬家，不过外部 Agent 的安装目录、本机项目和来源目录不会跟着移动。遇到恢复提示时，先按界面说明处理，不要急着删除文件。搬迁、备份和恢复的具体边界放在[使用说明](docs/user-guide.md)里。

本地保存不等于不联网：下载 Skill、检查远程来源或 Git 上游时，会连接相应的服务。使用别人提供的 Skill 前，也请先看看其中的内容。

## 遇到问题，欢迎告诉我

这是第一个公开版本，欢迎通过 [Issues](https://github.com/Fermin214/AgentHub/issues/new/choose) 告诉我哪里不好用。带上软件版本、Windows 版本，以及「做了什么、预期怎样、实际发生了什么」，会更方便我定位问题。日志和截图里记得去掉私人内容。

如果涉及安全问题，请私密发到 **admin@hifermin.com**，不要直接公开漏洞细节。相关说明见 [SECURITY.md](SECURITY.md)。

后续更新会从 **0.1.0** 开始维护公开版本的数据兼容，优先处理安装、文件保护和恢复问题。

## 开发与许可

想自己构建或参与改进，可以从[贡献指南](CONTRIBUTING.md)开始。当前技术结构见[开发说明](docs/architecture.md)。

AgentHub 使用 [MIT 许可](LICENSE)。感谢 Tauri、React、Lobe Icons 等开源项目；第三方组件的归属和许可原文保留在[第三方声明](THIRD-PARTY-NOTICES.md)及分发包中。
