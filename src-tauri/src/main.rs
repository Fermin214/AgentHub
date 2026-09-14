#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::Value;
use std::path::PathBuf;
use tauri::Manager;

struct CoreState {
    data_dir: PathBuf,
}

#[tauri::command]
async fn dispatch(
    state: tauri::State<'_, CoreState>,
    method: String,
    args: Value,
) -> Result<Value, agenthub_core::errors::CoreError> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        agenthub_core::dispatch(&data_dir, &method, args)
            .map_err(agenthub_core::errors::CoreError::from_error)
    })
    .await
    .map_err(|e| agenthub_core::errors::CoreError {
        code: "HOST_FAILURE".into(),
        params: serde_json::json!({}),
        detail: e.to_string(),
    })?
}

fn main() {
    let data_dir =
        agenthub_core::default_data_dir().expect("Application data directory unavailable");
    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none() {
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", data_dir.join("webview"));
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let data_dir = agenthub_core::default_data_dir()
                .expect("Application data directory unavailable");
            app.manage(CoreState { data_dir });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![dispatch])
        .run(tauri::generate_context!())
        .expect("AgentHub could not start");
}
