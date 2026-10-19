//! Tauri 2 backend bridge communicating with wallmus background daemon over Named Pipe IPC.

use common::ipc::send_request;
use tauri::{Manager, WindowEvent};

/// Forward an IPC method call to the wallmus daemon over Named Pipe.
#[tauri::command]
async fn daemon_request(method: String, params: serde_json::Value) -> Result<serde_json::Value, String> {
    // Run blocking Named Pipe I/O in worker pool
    tauri::async_runtime::spawn_blocking(move || {
        let resp = send_request(&method, params).map_err(|e| e.to_string())?;
        if resp.ok {
            Ok(resp.result.unwrap_or(serde_json::Value::Null))
        } else {
            let msg = resp
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown daemon error".to_string());
            Err(msg)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Control the visibility of the frameless desktop widget window.
#[tauri::command]
async fn set_widget_visible(app_handle: tauri::AppHandle, visible: bool) -> Result<(), String> {
    if let Some(widget_win) = app_handle.get_webview_window("widget") {
        if visible {
            widget_win.show().map_err(|e| e.to_string())?;
        } else {
            widget_win.hide().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Set whether the widget window is always on top.
#[tauri::command]
async fn set_widget_always_on_top(app_handle: tauri::AppHandle, always_on_top: bool) -> Result<(), String> {
    if let Some(widget_win) = app_handle.get_webview_window("widget") {
        widget_win.set_always_on_top(always_on_top).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            daemon_request,
            set_widget_visible,
            set_widget_always_on_top,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let label = window.label();

                if label == "main" {
                    // Check if widget window is currently shown
                    let widget_visible = app
                        .get_webview_window("widget")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);

                    if !widget_visible {
                        // Section 4: If no windows remain open, terminate UI process within 2 seconds
                        app.exit(0);
                    }
                } else if label == "widget" {
                    // Hide rather than destroy so it can be re-opened instantly
                    let _ = window.hide();
                    api.prevent_close();

                    let main_visible = app
                        .get_webview_window("main")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);

                    if !main_visible {
                        app.exit(0);
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running wallmus-ui");
}
