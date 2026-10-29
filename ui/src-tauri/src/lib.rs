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

fn ensure_ui_default_desktop() {
    use windows::core::w;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::StationsAndDesktops::{
        GetThreadDesktop, GetUserObjectInformationW, OpenDesktopW, SetThreadDesktop,
        DESKTOP_CONTROL_FLAGS, UOI_NAME,
    };
    use windows::Win32::System::Threading::GetCurrentThreadId;

    unsafe {
        if let Ok(desk) = GetThreadDesktop(GetCurrentThreadId()) {
            let mut desk_buf = [0u16; 256];
            let mut desk_needed = 0;
            let _ = GetUserObjectInformationW(
                HANDLE(desk.0 as _),
                UOI_NAME,
                Some(desk_buf.as_mut_ptr() as *mut _),
                (desk_buf.len() * 2) as u32,
                Some(&mut desk_needed),
            );
            let desk_len = (desk_needed as usize / 2).saturating_sub(1);
            let desk_name = String::from_utf16_lossy(&desk_buf[..desk_len]);
            let clean_name = desk_name.trim_matches('\0').trim();
            if !clean_name.eq_ignore_ascii_case("default") {
                if let Ok(default_desk) = OpenDesktopW(
                    w!("Default"),
                    DESKTOP_CONTROL_FLAGS(0),
                    false,
                    0x10000000,
                ) {
                    let _ = SetThreadDesktop(default_desk);
                }
            }
        }
    }
}

fn focus_existing_ui_window() -> bool {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, SetWindowPos, ShowWindow, HWND_NOTOPMOST,
        HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE, SW_SHOW,
    };

    unsafe {
        let hwnd1 = FindWindowW(None, w!("wallmus — Live Wallpapers & Music")).unwrap_or_default();
        let hwnd2 = FindWindowW(None, w!("wallmus - Live Wallpaper & Music Engine")).unwrap_or_default();
        let hwnd = if hwnd1.0 as usize != 0 { hwnd1 } else { hwnd2 };
        if hwnd.0 as usize != 0 {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let _ = SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let _ = SetForegroundWindow(hwnd);
            return true;
        }
    }
    false
}

fn ensure_daemon_running() {
    if send_request("app.getState", serde_json::Value::Null).is_ok() {
        return;
    }

    let mut candidates = Vec::new();
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            candidates.push(parent.join("wallmus.exe"));
            candidates.push(parent.join("wallmus\\wallmus.exe"));
        }
    }
    candidates.push(std::path::PathBuf::from("dist/wallmus/wallmus.exe"));
    candidates.push(std::path::PathBuf::from("dist/wallmus.exe"));
    candidates.push(std::path::PathBuf::from("target/release/wallmus.exe"));

    for p in candidates {
        if p.exists() {
            let _ = std::process::Command::new(&p)
                .arg("--no-open-ui")
                .current_dir(p.parent().unwrap_or_else(|| std::path::Path::new(".")))
                .spawn();
            // Wait briefly for the daemon IPC server to come online
            for _ in 0..15 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if send_request("app.getState", serde_json::Value::Null).is_ok() {
                    break;
                }
            }
            return;
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ensure_ui_default_desktop();

    // If UI window is already open, focus it and exit this second instance
    if focus_existing_ui_window() {
        return;
    }

    // Ensure background daemon is running if user launched wallmus-ui.exe directly
    ensure_daemon_running();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            daemon_request,
            set_widget_visible,
            set_widget_always_on_top,
        ])
        .on_window_event(|window, event| match event {
            WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                let win = window.clone();
                let paths = paths.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let audio_exts = ["mp3", "flac", "wav", "ogg", "m4a", "aac"];
                    let wp_exts = [
                        "mp4", "webm", "mkv", "gif", "jpg", "jpeg", "png", "bmp", "webp", "html",
                        "glsl", "frag",
                    ];
                    let mut changed = false;
                    for path in paths {
                        let path_str = path.to_string_lossy().to_string();
                        if path.is_dir() {
                            if let Ok(resp) = send_request("library.getFolders", serde_json::Value::Null) {
                                let mut folders: Vec<String> = resp
                                    .result
                                    .and_then(|r| r.get("folders").cloned())
                                    .and_then(|f| serde_json::from_value(f).ok())
                                    .unwrap_or_default();
                                if !folders.iter().any(|f| f.eq_ignore_ascii_case(&path_str)) {
                                    folders.push(path_str.clone());
                                    let _ = send_request(
                                        "settings.setMusic",
                                        serde_json::json!({ "folders": folders }),
                                    );
                                }
                                let _ = send_request(
                                    "library.scan",
                                    serde_json::json!({ "folders": [path_str] }),
                                );
                                changed = true;
                            }
                        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            let ext_l = ext.to_lowercase();
                            if audio_exts.contains(&ext_l.as_str()) {
                                let _ = send_request(
                                    "library.importFile",
                                    serde_json::json!({ "path": path_str }),
                                );
                                changed = true;
                            } else if wp_exts.contains(&ext_l.as_str()) {
                                let _ = send_request(
                                    "wallpapers.import",
                                    serde_json::json!({ "path": path_str, "copy": true }),
                                );
                                changed = true;
                            }
                        }
                    }
                    if changed {
                        if let Some(webview) = win.app_handle().get_webview_window(win.label()) {
                            let _ = webview.eval(
                                "window.dispatchEvent(new CustomEvent('wallmus-files-dropped'));",
                            );
                        }
                    }
                });
            }
            WindowEvent::CloseRequested { api, .. } => {
                let app = window.app_handle();
                let label = window.label();

                if label == "main" {
                    let widget_visible = app
                        .get_webview_window("widget")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);

                    if !widget_visible {
                        app.exit(0);
                    }
                } else if label == "widget" {
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
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running wallmus-ui");
}
