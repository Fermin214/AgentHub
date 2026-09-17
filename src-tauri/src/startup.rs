//! Dependency errors must be readable before a WebView or settings database exists.
use windows::{
    core::{w, HRESULT, HSTRING, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Globalization::GetUserDefaultUILanguage,
        UI::{
            Controls::{
                TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOG_BUTTON,
                TASKDIALOG_NOTIFICATIONS, TDF_ALLOW_DIALOG_CANCELLATION, TDF_ENABLE_HYPERLINKS,
                TDN_HYPERLINK_CLICKED, TD_ERROR_ICON,
            },
            Shell::ShellExecuteW,
            WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, SW_SHOWNORMAL},
        },
    },
};

const DOWNLOAD: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

pub fn runtime_available() -> bool {
    if tauri::webview_version().is_ok() {
        return true;
    }
    // Startup dependency errors follow Windows UI language, before any database
    // is opened. Explicit button text avoids the framework's English/system mix.
    let chinese = unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x04;
    let (title, body, ok) = if chinese {
        (
            "AgentHub - 无法启动",
            "未找到 WebView2 Runtime。\n\n请运行 AgentHub 安装包安装所需运行时，或从微软官网下载：\n\n其他 Windows 用户安装的运行时可能对当前账户不可用。",
            "确定",
        )
    } else {
        (
            "AgentHub - Unable to start",
            "The WebView2 Runtime was not found.\n\nRun the AgentHub installer to install it, or download it from Microsoft:\n\nA runtime installed for another Windows user may not be available to this account.",
            "OK",
        )
    };
    let title = HSTRING::from(title);
    let plain_body = HSTRING::from(format!("{body}\n\n{DOWNLOAD}"));
    let body = HSTRING::from(format!(
        "{body}\n\n<A href=\"{DOWNLOAD}\">WebView2 Runtime</A>"
    ));
    let ok = HSTRING::from(ok);
    let button = TASKDIALOG_BUTTON {
        nButtonID: 100,
        pszButtonText: PCWSTR(ok.as_ptr()),
    };
    let config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_ENABLE_HYPERLINKS,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        pszContent: PCWSTR(body.as_ptr()),
        Anonymous1: TASKDIALOGCONFIG_0 {
            pszMainIcon: TD_ERROR_ICON,
        },
        cButtons: 1,
        pButtons: &button,
        nDefaultButton: 100,
        pfCallback: Some(open_download),
        ..Default::default()
    };
    // All UTF-16 buffers and the button live through the synchronous dialog.
    unsafe {
        if TaskDialogIndirect(&config, None, None, None).is_err() {
            MessageBoxW(None, &plain_body, &title, MB_ICONERROR);
        }
    }
    false
}

extern "system" fn open_download(
    _: HWND,
    notification: TASKDIALOG_NOTIFICATIONS,
    _: WPARAM,
    _: LPARAM,
    _: isize,
) -> HRESULT {
    if notification == TDN_HYPERLINK_CLICKED {
        // Only the fixed Microsoft link can be opened; no installer is run here.
        unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &HSTRING::from(DOWNLOAD),
                None,
                None,
                SW_SHOWNORMAL,
            );
        }
    }
    HRESULT(0)
}
