# AgentHub 0.1.2

## 中文

本次维护更新修复 Prompt 布局、Skill 详情与收藏交互，不增加新的产品功能。

- Prompt 卡片保持一致大小，长标题和正文截断显示；标题与操作按钮对齐，详情可阅读完整标题。
- Skill 详情统一读取 Skill 库内容，移除“阅读位置”选择器。
- 收藏保存仅影响当前条目，防止重复请求；修复连续切换、失败恢复、乱序回包及收藏与标签并发保存时的状态同步。
- 改善中英文和最小窗口下的操作按钮布局，收藏等待时不再显示禁止鼠标指针。
- 缺少 WebView2 Runtime 时，启动提示和确认按钮统一跟随 Windows 界面语言；关闭提示后退出，不再残留空白窗口和进程。
- 补强自动化回归与隔离的桌面验收，并修复 Windows PowerShell 5.1 验收脚本编码兼容性。

### 下载与升级

- Windows x64 安装版：`AgentHub_0.1.2_x64-setup.exe`
- Windows x64 便携版：`AgentHub-0.1.2-windows-x64.zip`
- 安装版会在缺少 Microsoft Edge WebView2 Runtime 时尝试下载安装；便携版需要已有运行时。
- 从 0.1.1 升级前，请退出 AgentHub 并备份完整 `data` 文件夹。普通卸载保留数据；搬移便携版时保留程序与 `data` 文件夹。
- 应用仍未签名，Windows 或浏览器可能显示未知发布者、信誉提示或设备策略阻止。请使用 `SHA256SUMS.txt` 核对文件；校验和不能替代签名。
- 应用内一键更新和 Prompt 来源备注不在本次更新范围。

问题请通过 [Issues](https://github.com/Fermin214/AgentHub/issues/new/choose) 反馈；安全问题按 [SECURITY.md](https://github.com/Fermin214/AgentHub/blob/main/SECURITY.md) 私下报告。

## English

This maintenance update fixes Prompt layout, Skill details and favorite interactions without adding new product features.

- Keep Prompt cards consistently sized, truncate long titles and body previews, align titles with actions, and show the full title in the detail view.
- Read Skill details from the library and remove the read-location selector.
- Keep favorite saving local to the selected entry and prevent duplicate requests. Fix repeated toggles, failure recovery, out-of-order responses, and concurrent favorite/tag updates.
- Improve action layouts in both languages and at the minimum window size, and avoid a prohibited cursor while saving a favorite.
- Match the missing-WebView2 startup message and acknowledgement to the Windows UI language, and exit after dismissal without leaving a blank window or process behind.
- Strengthen regression and isolated desktop acceptance coverage, including Windows PowerShell 5.1 script encoding compatibility.

### Downloads and upgrade

Use `AgentHub_0.1.2_x64-setup.exe` for installation or extract `AgentHub-0.1.2-windows-x64.zip` for portable use. The installer attempts to download WebView2 Runtime if missing; portable use requires an existing runtime.

Quit AgentHub and back up the complete `data` folder before upgrading from 0.1.1. Ordinary uninstall preserves data. Move the portable program and its `data` folder together.

The app remains unsigned. Browser/Windows publisher or reputation prompts and device-policy blocks may occur. Compare downloads with `SHA256SUMS.txt`; checksums do not replace a publisher signature. In-app updating and Prompt source notes remain outside this release.

Report ordinary issues through [Issues](https://github.com/Fermin214/AgentHub/issues/new/choose) and security concerns privately as described in [SECURITY.md](https://github.com/Fermin214/AgentHub/blob/main/SECURITY.md).
