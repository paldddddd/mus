#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod hotkeys;
mod ipc;
mod mouse_hook;
mod single_instance;
mod tray;
mod updater;

use anyhow::{bail, Context, Result};
use audio_engine::AudioEngine;
use common::{logs_dir, AppConfig, BindingItem, BindingScope, MusicSource, WallpaperItem};
use widget::{WallpaperVisualizerWindow, WidgetController};
use desktop_host::{
    apply_eco_profile, collect_diag, enumerate_monitors, DesktopHostWindow, RenderMode,
};
use governor::{Governor, GovernorState};
use hotkeys::{HotkeyAction, HotkeyManager};
use ipc::{send_request, IpcHandler, IpcServer};
use library::{resolve, Database, LibraryWatcher, TrackContext};
use media::import_wallpaper;
use mouse_hook::MouseHookManager;
use render::{DualLayerHost, MpvRenderer, Renderer, StaticImageRenderer};
use single_instance::SingleInstanceGuard;
use std::env;
use std::io::Write;
use std::panic;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use tray::SystemTray;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY, WM_QUIT,
};

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn file_to_data_uri(path: &str) -> String {
    if path.is_empty() || path.starts_with("data:") || path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    static URI_CACHE: std::sync::OnceLock<Mutex<std::collections::HashMap<String, String>>> =
        std::sync::OnceLock::new();
    let cache = URI_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(cached) = guard.get(path) {
            return cached.clone();
        }
    }

    let p = Path::new(path);
    if !p.exists() {
        return String::new();
    }
    if let Ok(bytes) = std::fs::read(p) {
        if bytes.len() <= 8 * 1024 * 1024 {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("jpg").to_lowercase();
            let mime = match ext.as_str() {
                "png" => "image/png",
                "bmp" => "image/bmp",
                "webp" => "image/webp",
                "gif" => "image/gif",
                _ => "image/jpeg",
            };
            let uri = format!("data:{};base64,{}", mime, encode_base64(&bytes));
            if let Ok(mut guard) = cache.lock() {
                if guard.len() > 256 {
                    guard.clear();
                }
                guard.insert(path.to_string(), uri.clone());
            }
            return uri;
        }
    }
    path.to_string()
}

fn enrich_wallpaper_for_ui(mut wp: WallpaperItem) -> WallpaperItem {
    if !wp.thumb_path.is_empty() {
        wp.thumb_path = file_to_data_uri(&wp.thumb_path);
    }
    wp
}

fn next_random_wp_index(current: Option<usize>, len: usize, rng_seed: &mut u64) -> usize {
    if len <= 1 {
        return 0;
    }
    *rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let rand_val = (*rng_seed >> 32) as usize;
    let next = rand_val % len;

    if Some(next) == current {
        (next + 1) % len
    } else {
        next
    }
}

fn pick_file_win32(title: &str, filter_with_nulls: &str) -> Option<String> {
    #[repr(C)]
    struct OPENFILENAMEW {
        l_struct_size: u32,
        hwnd_owner: isize,
        h_instance: isize,
        lpstr_filter: *const u16,
        lpstr_custom_filter: *mut u16,
        n_max_cust_filter: u32,
        n_filter_index: u32,
        lpstr_file: *mut u16,
        n_max_file: u32,
        lpstr_file_title: *mut u16,
        n_max_file_title: u32,
        lpstr_initial_dir: *const u16,
        lpstr_title: *const u16,
        flags: u32,
        n_file_offset: u16,
        n_file_extension: u16,
        lpstr_def_ext: *const u16,
        l_cust_data: isize,
        lpfn_hook: isize,
        lp_template_name: *const u16,
        pv_reserved: *mut std::ffi::c_void,
        dw_reserved: u32,
        flags_ex: u32,
    }

    #[link(name = "comdlg32")]
    extern "system" {
        fn GetOpenFileNameW(lpofn: *mut OPENFILENAMEW) -> i32;
    }

    desktop_host::ensure_default_desktop();

    let filter_w: Vec<u16> = filter_with_nulls.encode_utf16().collect();
    let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let mut file_buf = vec![0u16; 4096];

    // OFN_EXPLORER (0x00080000) | OFN_FILEMUSTEXIST (0x00001000) | OFN_PATHMUSTEXIST (0x00000800) | OFN_NOCHANGEDIR (0x00000008)
    let flags = 0x00080000 | 0x00001000 | 0x00000800 | 0x00000008;

    let mut ofn = OPENFILENAMEW {
        l_struct_size: std::mem::size_of::<OPENFILENAMEW>() as u32,
        // IMPORTANT: Do NOT set cross-process UI HWND as modal owner (causes Windows "Not Responding" lock)
        hwnd_owner: 0,
        h_instance: 0,
        lpstr_filter: filter_w.as_ptr(),
        lpstr_custom_filter: std::ptr::null_mut(),
        n_max_cust_filter: 0,
        n_filter_index: 1,
        lpstr_file: file_buf.as_mut_ptr(),
        n_max_file: file_buf.len() as u32,
        lpstr_file_title: std::ptr::null_mut(),
        n_max_file_title: 0,
        lpstr_initial_dir: std::ptr::null(),
        lpstr_title: title_w.as_ptr(),
        flags,
        n_file_offset: 0,
        n_file_extension: 0,
        lpstr_def_ext: std::ptr::null(),
        l_cust_data: 0,
        lpfn_hook: 0,
        lp_template_name: std::ptr::null(),
        pv_reserved: std::ptr::null_mut(),
        dw_reserved: 0,
        flags_ex: 0,
    };

    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok != 0 {
        let len = file_buf.iter().position(|&c| c == 0).unwrap_or(0);
        if len > 0 {
            return Some(String::from_utf16_lossy(&file_buf[..len]));
        }
    }
    None
}

fn pick_folder_win32(title: &str) -> Option<String> {
    #[repr(C)]
    struct BROWSEINFOW {
        hwnd_owner: isize,
        pidl_root: *const std::ffi::c_void,
        psz_display_name: *mut u16,
        lpsz_title: *const u16,
        ul_flags: u32,
        lpfn: isize,
        l_param: isize,
        i_image: i32,
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHBrowseForFolderW(lpbi: *mut BROWSEINFOW) -> *mut std::ffi::c_void;
        fn SHGetPathFromIDListW(pidl: *const std::ffi::c_void, psz_path: *mut u16) -> i32;
    }
    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pv_reserved: *mut std::ffi::c_void, dw_co_init: u32) -> i32;
        fn CoTaskMemFree(pv: *mut std::ffi::c_void);
    }

    desktop_host::ensure_default_desktop();

    unsafe {
        let _ = CoInitializeEx(std::ptr::null_mut(), 0x2); // COINIT_APARTMENTTHREADED
        let mut display_name = [0u16; 260];
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        // BIF_RETURNONLYFSDIRS (0x00000001) | BIF_NEWDIALOGSTYLE (0x00000040)
        let mut bi = BROWSEINFOW {
            hwnd_owner: 0,
            pidl_root: std::ptr::null(),
            psz_display_name: display_name.as_mut_ptr(),
            lpsz_title: title_w.as_ptr(),
            ul_flags: 0x00000001 | 0x00000040,
            lpfn: 0,
            l_param: 0,
            i_image: 0,
        };
        let pidl = SHBrowseForFolderW(&mut bi);
        if !pidl.is_null() {
            let mut path_buf = [0u16; 1024];
            let ok = SHGetPathFromIDListW(pidl, path_buf.as_mut_ptr());
            CoTaskMemFree(pidl);
            if ok != 0 {
                let len = path_buf.iter().position(|&c| c == 0).unwrap_or(0);
                if len > 0 {
                    return Some(String::from_utf16_lossy(&path_buf[..len]));
                }
            }
        }
    }
    None
}

/// Context data for pending track change debounce.
#[derive(Debug, Clone)]
pub struct TrackContextData {
    pub track_id: Option<String>,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub playlist_id: Option<i64>,
    pub is_external: bool,
}

impl TrackContextData {
    pub fn as_context(&self) -> TrackContext<'_> {
        TrackContext {
            track_id: self.track_id.as_deref(),
            artist: self.artist.as_deref(),
            title: self.title.as_deref(),
            album: self.album.as_deref(),
            playlist_id: self.playlist_id,
            is_external: self.is_external,
        }
    }
}

/// Daemon runtime controller managing wallpaper playback and IPC.
struct DaemonContext {
    db: Arc<Database>,
    config: Arc<Mutex<AppConfig>>,
    governor: Arc<Mutex<Governor>>,
    active_wallpaper: Arc<Mutex<Option<WallpaperItem>>>,
    should_quit: Arc<AtomicBool>,
    audio: Arc<AudioEngine>,
    watcher: Arc<Mutex<Option<LibraryWatcher>>>,
    pending_track_change: Arc<Mutex<Option<(TrackContextData, Instant)>>>,
    hotkey_manager: Arc<Mutex<HotkeyManager>>,
    pending_hotkey_reload: Arc<AtomicBool>,
    pending_widget_sync: Arc<AtomicBool>,
}

impl IpcHandler for DaemonContext {
    fn handle_method(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        match method {
            "app.getState" => {
                let cfg = self.config.lock().unwrap().clone();
                let active = self.active_wallpaper.lock().unwrap().clone().map(enrich_wallpaper_for_ui);
                let gov = self.governor.lock().unwrap();
                let is_paused = gov.is_manual_paused();

                Ok(serde_json::json!({
                    "running": true,
                    "is_paused": is_paused,
                    "active_wallpaper": active,
                    "config": cfg,
                }))
            }
            "app.quit" => {
                info!("IPC requested app.quit");
                self.should_quit.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({ "status": "quitting" }))
            }
            "app.openUi" => {
                info!("IPC requested app.openUi (shortcut or tray trigger)");
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.enabled = true;
                    let _ = cfg.save();
                }
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                launch_or_focus_ui();
                Ok(serde_json::json!({ "status": "ui_requested" }))
            }
            "app.openLogs" => {
                let dir = logs_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                Ok(serde_json::json!({ "status": "opened", "path": dir }))
            }
            "app.openUrl" => {
                if let Some(url) = params.get("url").and_then(|v| v.as_str()) {
                    let _ = std::process::Command::new("explorer").arg(url).spawn();
                    Ok(serde_json::json!({ "status": "opened", "url": url }))
                } else {
                    anyhow::bail!("Missing url parameter");
                }
            }
            "app.pickWallpaperFile" => {
                let filter = "Обои и изображения (*.mp4;*.webm;*.mkv;*.gif;*.jpg;*.jpeg;*.png;*.bmp;*.webp;*.html;*.glsl;*.frag)\0*.mp4;*.webm;*.mkv;*.gif;*.jpg;*.jpeg;*.png;*.bmp;*.webp;*.html;*.glsl;*.frag\0Изображения (*.jpg;*.jpeg;*.png;*.bmp;*.webp;*.gif)\0*.jpg;*.jpeg;*.png;*.bmp;*.webp;*.gif\0Видео (*.mp4;*.webm;*.mkv)\0*.mp4;*.webm;*.mkv\0Все файлы (*.*)\0*.*\0\0";
                let path = pick_file_win32("Выберите изображение или видео для обоев", filter);
                Ok(serde_json::json!({ "path": path }))
            }
            "app.pickAudioFile" => {
                let filter = "Аудиофайлы (*.mp3;*.flac;*.wav;*.ogg;*.m4a;*.aac)\0*.mp3;*.flac;*.wav;*.ogg;*.m4a;*.aac\0Все файлы (*.*)\0*.*\0\0";
                let path = pick_file_win32("Выберите музыкальный трек", filter);
                Ok(serde_json::json!({ "path": path }))
            }
            "app.pickMusicFolder" => {
                let path = pick_folder_win32("Выберите папку с музыкой");
                Ok(serde_json::json!({ "path": path }))
            }
            "wallpapers.list" => {
                let list: Vec<WallpaperItem> = self
                    .db
                    .list_wallpapers()?
                    .into_iter()
                    .map(enrich_wallpaper_for_ui)
                    .collect();
                Ok(serde_json::to_value(list)?)
            }
            "wallpapers.import" => {
                let path_str = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("Missing 'path' parameter")?;
                let copy = params
                    .get("copy")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let item = import_wallpaper(Path::new(path_str), copy)?;
                self.db.save_wallpaper(&item)?;
                Ok(serde_json::to_value(enrich_wallpaper_for_ui(item))?)
            }
            "wallpapers.delete" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .context("Missing 'id' parameter")?;
                self.db.delete_wallpaper(id)?;
                {
                    let mut active = self.active_wallpaper.lock().unwrap();
                    if active.as_ref().map(|w| w.id == id).unwrap_or(false) {
                        let remaining = self.db.list_wallpapers().unwrap_or_default();
                        *active = remaining.into_iter().next();
                    }
                }
                Ok(serde_json::json!({ "deleted": id }))
            }
            "wallpapers.setActive" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .context("Missing 'id' parameter")?;
                let list = self.db.list_wallpapers()?;
                let found = list.into_iter().find(|w| w.id == id);
                if let Some(wp) = found {
                    let mut active = self.active_wallpaper.lock().unwrap();
                    *active = Some(wp.clone());
                    {
                        let mut cfg = self.config.lock().unwrap();
                        cfg.active_wallpaper_id = Some(id.to_string());
                        let _ = cfg.save();
                    }
                    let _ = self.db.set_kv("active_wallpaper_id", id);
                    Ok(serde_json::to_value(enrich_wallpaper_for_ui(wp))?)
                } else {
                    bail!("Wallpaper with id '{}' not found", id);
                }
            }
            "governor.getState" => {
                let gov = self.governor.lock().unwrap();
                Ok(serde_json::json!({
                    "manual_paused": gov.is_manual_paused(),
                }))
            }
            "governor.setConfig" => {
                let gov_val = params.get("governor").unwrap_or(&params);
                let new_gov: common::GovernorConfig = serde_json::from_value(gov_val.clone())?;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.governor = new_gov.clone();
                    let _ = cfg.save();
                }
                self.governor.lock().unwrap().update_config(new_gov.clone());
                Ok(serde_json::to_value(new_gov)?)
            }
            "settings.setFpsLimit" => {
                let fps = params
                    .get("fps_limit")
                    .and_then(|v| v.as_u64())
                    .context("Missing 'fps_limit' parameter")? as u32;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.fps_limit = fps;
                    let _ = cfg.save();
                }
                Ok(serde_json::json!({ "fps_limit": fps }))
            }
            "settings.setGeneral" => {
                let mut cfg = self.config.lock().unwrap();
                if let Some(lang) = params.get("language").and_then(|v| v.as_str()) {
                    cfg.language = lang.to_string();
                }
                if let Some(theme) = params.get("theme").and_then(|v| v.as_str()) {
                    cfg.theme = theme.to_string();
                }
                if let Some(cf) = params.get("crossfade_duration_ms").and_then(|v| v.as_u64()) {
                    cfg.crossfade_duration_ms = cf;
                }
                if let Some(mode) = params.get("wallpaper_change_mode").and_then(|v| v.as_str()) {
                    cfg.wallpaper_change_mode = mode.to_string();
                }
                if let Some(resp) = params.get("respect_bindings_in_auto_change").and_then(|v| v.as_bool()) {
                    cfg.respect_bindings_in_auto_change = resp;
                }
                let _ = cfg.save();
                Ok(serde_json::json!({
                    "language": cfg.language,
                    "theme": cfg.theme,
                    "crossfade_duration_ms": cfg.crossfade_duration_ms,
                    "wallpaper_change_mode": cfg.wallpaper_change_mode,
                    "respect_bindings_in_auto_change": cfg.respect_bindings_in_auto_change,
                }))
            }
            "wallpapers.setChangeMode" => {
                let mut cfg = self.config.lock().unwrap();
                if let Some(mode) = params.get("mode").and_then(|v| v.as_str()) {
                    cfg.wallpaper_change_mode = mode.to_string();
                }
                if let Some(resp) = params.get("respect_bindings").and_then(|v| v.as_bool()) {
                    cfg.respect_bindings_in_auto_change = resp;
                }
                let _ = cfg.save();
                info!("Wallpaper change mode set to '{}' (respect_bindings: {})",
                    cfg.wallpaper_change_mode, cfg.respect_bindings_in_auto_change);
                Ok(serde_json::json!({
                    "wallpaper_change_mode": cfg.wallpaper_change_mode,
                    "respect_bindings_in_auto_change": cfg.respect_bindings_in_auto_change,
                }))
            }
            "wallpapers.pause" => {
                let gov = self.governor.lock().unwrap();
                gov.set_manual_pause(true);
                Ok(serde_json::json!({ "manual_paused": true }))
            }
            "wallpapers.resume" => {
                let gov = self.governor.lock().unwrap();
                gov.set_manual_pause(false);
                Ok(serde_json::json!({ "manual_paused": false }))
            }
            "diag.dump" => {
                let mut diag = collect_diag()?;
                diag.libmpv_version = render::get_mpv_version();
                Ok(serde_json::to_value(diag)?)
            }
            "settings.getAutostart" => {
                let enabled = crate::autostart::is_autostart_enabled();
                Ok(serde_json::json!({ "enabled": enabled }))
            }
            "settings.setAutostart" => {
                let enabled = params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .context("Missing 'enabled' boolean parameter")?;
                crate::autostart::set_autostart(enabled)?;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.autostart = enabled;
                    let _ = cfg.save();
                }
                Ok(serde_json::json!({ "enabled": enabled }))
            }
            "audio.getTrack" => {
                let local_playing = self.audio.player.is_playing();
                let has_local_track = self.audio.player.current_track().is_some();

                let active_wp_id = self
                    .active_wallpaper
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|w| w.id.clone());

                // Helper closures
                let get_local_json = || -> Option<serde_json::Value> {
                    self.audio.player.current_track().map(|local_t| {
                        let pos_ms = self.audio.player.get_position().as_millis() as u64;
                        let cover_uri = local_t.cover_path.as_deref().map(file_to_data_uri);
                        serde_json::json!({
                            "source": "local",
                            "key": local_t.id,
                            "title": local_t.title,
                            "artist": local_t.artist,
                            "album_title": local_t.album.unwrap_or_default(),
                            "album_artist": local_t.artist,
                            "playback_status": if self.audio.player.is_playing() { "Playing" } else { "Paused" },
                            "source_app": "wallmus",
                            "position_ms": pos_ms,
                            "duration_ms": local_t.duration_ms,
                            "cover_path": cover_uri,
                            "active_wallpaper_id": active_wp_id,
                        })
                    })
                };

                let get_external_json = || -> Option<(serde_json::Value, bool)> {
                    self.audio.smtc.get_current_track().ok().flatten().map(|t| {
                        let is_playing = t.playback_status == audio_engine::PlaybackStatus::Playing;
                        let key = common::normalize::external_track_key(&t.artist, &t.title);
                        let cover_uri = t.cover_path.as_deref().map(file_to_data_uri);
                        let val = serde_json::json!({
                            "source": "external",
                            "key": key,
                            "title": t.title,
                            "artist": t.artist,
                            "album_title": t.album_title,
                            "album_artist": t.album_artist,
                            "playback_status": format!("{:?}", t.playback_status),
                            "source_app": t.source_app,
                            "position_ms": t.position_ms,
                            "duration_ms": t.duration_ms,
                            "cover_path": cover_uri,
                            "active_wallpaper_id": active_wp_id,
                        });
                        (val, is_playing)
                    })
                };

                // 1. If local player is actively playing, always return local track
                if local_playing && has_local_track {
                    if let Some(j) = get_local_json() {
                        return Ok(j);
                    }
                }

                // 2. Otherwise check external SMTC; if external is actively playing, prefer it and mark local inactive
                let ext = get_external_json();
                if let Some((ref ext_val, true)) = ext {
                    self.audio.player.set_active_local(false);
                    return Ok(ext_val.clone());
                }

                // 3. If local player was explicitly started in this session (and not superseded by external), return local track
                if self.audio.player.is_active_local() && has_local_track {
                    if let Some(j) = get_local_json() {
                        return Ok(j);
                    }
                }

                // 4. Fallback to paused external session or Null
                if let Some((ext_val, _)) = ext {
                    return Ok(ext_val);
                }
                Ok(serde_json::Value::Null)
            }
            "audio.control" => {
                let action = params
                    .get("action")
                    .and_then(|v| v.as_str())
                    .context("Missing 'action' parameter (play/pause/toggle/next/previous)")?;
                let has_local = self.audio.player.current_track().is_some();
                let smtc_playing = self
                    .audio
                    .smtc
                    .get_current_track()
                    .ok()
                    .flatten()
                    .map(|t| t.playback_status == audio_engine::PlaybackStatus::Playing)
                    .unwrap_or(false);
                let use_local = self.audio.player.is_playing()
                    || (self.audio.player.is_active_local() && has_local && !smtc_playing);

                let res = match action {
                    "play" => {
                        if use_local {
                            self.audio.player.play();
                            true
                        } else {
                            self.audio.smtc.play().unwrap_or(false)
                        }
                    }
                    "pause" => {
                        if use_local {
                            self.audio.player.pause();
                            true
                        } else {
                            self.audio.smtc.pause().unwrap_or(false)
                        }
                    }
                    "toggle" => {
                        if use_local {
                            self.audio.player.toggle_play_pause();
                            true
                        } else {
                            self.audio.smtc.toggle_play_pause().unwrap_or(false)
                        }
                    }
                    "next" => {
                        if use_local {
                            let _ = self.audio.player.next();
                            true
                        } else {
                            self.audio.smtc.skip_next().unwrap_or(false)
                        }
                    }
                    "previous" => {
                        if use_local {
                            let _ = self.audio.player.previous();
                            true
                        } else {
                            self.audio.smtc.skip_previous().unwrap_or(false)
                        }
                    }
                    "stop" => {
                        if use_local {
                            self.audio.player.stop();
                            if let Ok(json) = serde_json::to_string(&self.audio.player.get_state()) {
                                let _ = self.db.set_kv("player_state", &json);
                            }
                            true
                        } else {
                            self.audio.smtc.pause().unwrap_or(false)
                        }
                    }
                    _ => bail!("Unknown audio action: {}", action),
                };

                Ok(serde_json::json!({ "success": res }))
            }
            "audio.getSpectrum" => {
                let bars = params
                    .get("bars")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(32) as usize;
                let spectrum = self.audio.visualizer.get_spectrum(bars);
                Ok(serde_json::json!({ "spectrum": spectrum }))
            }
            "audio.setWallpaperVolume" => {
                if let Some(vol) = params.get("volume").and_then(|v| v.as_u64()) {
                    self.audio.wallpaper_audio.set_volume(vol as u32);
                }
                if let Some(muted) = params.get("muted").and_then(|v| v.as_bool()) {
                    self.audio.wallpaper_audio.set_muted(muted);
                }
                Ok(serde_json::json!({
                    "effective_volume": self.audio.wallpaper_audio.get_effective_volume(),
                    "muted": self.audio.wallpaper_audio.is_muted(),
                }))
            }
            "audio.getWallpaperVolume" => {
                Ok(serde_json::json!({
                    "effective_volume": self.audio.wallpaper_audio.get_effective_volume(),
                    "muted": self.audio.wallpaper_audio.is_muted(),
                }))
            }
            "library.importFile" => {
                let path_str = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("Missing 'path' parameter")?;
                let path = Path::new(path_str);
                if !path.exists() {
                    bail!("Audio file does not exist: {}", path_str);
                }
                let mut track = library::scanner::read_track_metadata(path)?;
                self.db.save_track(&track)?;
                if let Some(parent) = path.parent().and_then(|p| p.to_str()) {
                    let parent_str = parent.to_string();
                    let mut cfg = self.config.lock().unwrap();
                    if !cfg.music.folders.iter().any(|f| f.eq_ignore_ascii_case(&parent_str)) {
                        cfg.music.folders.push(parent_str);
                        let _ = cfg.save();
                    }
                }
                if let Some(ref cp) = track.cover_path {
                    track.cover_path = Some(file_to_data_uri(cp));
                }
                Ok(serde_json::to_value(track)?)
            }
            "library.scan" => {
                let folders = if let Some(arr) = params.get("folders").and_then(|v| v.as_array()) {
                    arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                } else {
                    self.config.lock().unwrap().music.folders.clone()
                };
                let summary = library::scanner::scan_folders(&self.db, &folders, None::<fn(library::ScanProgress)>)?;
                Ok(serde_json::to_value(summary)?)
            }
            "library.listTracks" | "tracks.list" => {
                let mut tracks = self.db.list_tracks()?;
                for t in &mut tracks {
                    if let Some(ref cp) = t.cover_path {
                        t.cover_path = Some(file_to_data_uri(cp));
                    }
                }
                Ok(serde_json::to_value(tracks)?)
            }
            "library.getTrack" => {
                let id = params.get("id").and_then(|v| v.as_str()).context("Missing 'id'")?;
                let mut track = self.db.get_track(id)?;
                if let Some(ref mut t) = track {
                    if let Some(ref cp) = t.cover_path {
                        t.cover_path = Some(file_to_data_uri(cp));
                    }
                }
                Ok(serde_json::to_value(track)?)
            }
            "tracks.delete" | "library.deleteTrack" => {
                let id = params.get("id").and_then(|v| v.as_str()).context("Missing 'id'")?;
                if self.audio.player.current_track().as_ref().map(|t| t.id == id).unwrap_or(false) {
                    self.audio.player.stop();
                }
                let _ = self.db.delete_binding(BindingScope::Track, id);
                self.db.delete_track(id)?;
                Ok(serde_json::json!({ "deleted": id }))
            }
            "library.listPlaylists" => {
                let pls = self.db.list_playlists()?;
                Ok(serde_json::to_value(pls)?)
            }
            "library.createPlaylist" => {
                let name = params.get("name").and_then(|v| v.as_str()).context("Missing 'name'")?;
                let id = self.db.create_playlist(name)?;
                Ok(serde_json::json!({ "id": id }))
            }
            "library.updatePlaylist" => {
                let id = params.get("id").and_then(|v| v.as_i64()).context("Missing 'id'")?;
                let name = params.get("name").and_then(|v| v.as_str()).context("Missing 'name'")?;
                let track_ids: Vec<String> = params
                    .get("track_ids")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                self.db.update_playlist(id, name, &track_ids)?;
                Ok(serde_json::json!({ "status": "ok" }))
            }
            "library.deletePlaylist" => {
                let id = params.get("id").and_then(|v| v.as_i64()).context("Missing 'id'")?;
                self.db.delete_playlist(id)?;
                Ok(serde_json::json!({ "status": "ok" }))
            }
            "library.getFolders" => {
                let folders = self.config.lock().unwrap().music.folders.clone();
                Ok(serde_json::json!({ "folders": folders }))
            }
            "library.setFolders" => {
                let folders: Vec<String> = params
                    .get("folders")
                    .and_then(|v| v.as_array())
                    .context("Missing 'folders' array")?
                    .iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect();
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.music.folders = folders.clone();
                    let _ = cfg.save();
                }
                if let Ok(mut guard) = self.watcher.lock() {
                    if let Some(ref mut w) = *guard {
                        let _ = w.update_folders(&folders);
                    }
                }
                Ok(serde_json::json!({ "folders": folders }))
            }
            "player.play" => {
                self.audio.player.play();
                Ok(serde_json::json!({ "is_playing": self.audio.player.is_playing() }))
            }
            "player.pause" => {
                self.audio.player.pause();
                Ok(serde_json::json!({ "is_playing": false }))
            }
            "player.stop" => {
                self.audio.player.stop();
                if let Ok(json) = serde_json::to_string(&self.audio.player.get_state()) {
                    let _ = self.db.set_kv("player_state", &json);
                }
                Ok(serde_json::json!({ "is_playing": false }))
            }
            "player.toggle" => {
                self.audio.player.toggle_play_pause();
                Ok(serde_json::json!({ "is_playing": self.audio.player.is_playing() }))
            }
            "player.next" => {
                self.audio.player.next()?;
                Ok(serde_json::json!({ "current_track": self.audio.player.current_track() }))
            }
            "player.prev" => {
                self.audio.player.previous()?;
                Ok(serde_json::json!({ "current_track": self.audio.player.current_track() }))
            }
            "player.seek" => {
                let pos_ms = params.get("position_ms").and_then(|v| v.as_u64()).context("Missing 'position_ms'")?;
                let use_local = self.audio.player.is_playing()
                    || (self.audio.player.is_active_local() && self.audio.player.current_track().is_some());
                if use_local {
                    self.audio.player.seek(Duration::from_millis(pos_ms))?;
                } else {
                    let _ = self.audio.smtc.seek(pos_ms);
                }
                Ok(serde_json::json!({ "position_ms": pos_ms }))
            }
            "player.setVolume" => {
                let vol = params.get("volume").and_then(|v| v.as_f64()).context("Missing 'volume'")? as f32;
                self.audio.player.set_volume(vol);
                Ok(serde_json::json!({ "volume": self.audio.player.get_volume() }))
            }
            "player.setRepeat" => {
                let mode_str = params.get("mode").and_then(|v| v.as_str()).context("Missing 'mode'")?;
                let mode = match mode_str {
                    "all" => audio_engine::RepeatMode::All,
                    "one" => audio_engine::RepeatMode::One,
                    _ => audio_engine::RepeatMode::Off,
                };
                self.audio.player.set_repeat_mode(mode);
                Ok(serde_json::json!({ "repeat_mode": mode_str }))
            }
            "player.setShuffle" => {
                let shuffle = params.get("shuffle").and_then(|v| v.as_bool()).context("Missing 'shuffle'")?;
                self.audio.player.set_shuffle(shuffle);
                Ok(serde_json::json!({ "shuffle": shuffle }))
            }
            "player.setQueue" => {
                let tracks: Vec<audio_engine::LocalTrack> = serde_json::from_value(
                    params.get("tracks").cloned().context("Missing 'tracks' array")?,
                )?;
                let start_idx = params.get("start_index").and_then(|v| v.as_u64()).map(|u| u as usize);
                self.audio.player.set_queue(tracks, start_idx)?;
                Ok(serde_json::json!({ "status": "ok" }))
            }
            "player.enqueue" => {
                let track: audio_engine::LocalTrack = serde_json::from_value(
                    params.get("track").cloned().context("Missing 'track'")?,
                )?;
                self.audio.player.enqueue(track);
                Ok(serde_json::json!({ "status": "ok" }))
            }
            "player.playFile" => {
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("Missing 'path' parameter")?;
                let param_id = params.get("id").and_then(|v| v.as_str()).map(String::from);
                let param_title = params.get("title").and_then(|v| v.as_str()).map(String::from);
                let param_artist = params.get("artist").and_then(|v| v.as_str()).map(String::from);

                // Pause external SMTC player only if it is actively playing
                if let Ok(Some(ext_t)) = self.audio.smtc.get_current_track() {
                    if ext_t.playback_status == audio_engine::PlaybackStatus::Playing {
                        let _ = self.audio.smtc.pause();
                    }
                }

                // Look up full track metadata (including true Blake3 ID, duration_ms, cover_path) from DB or disk
                let db_track = param_id
                    .as_deref()
                    .and_then(|id| self.db.get_track(id).ok().flatten())
                    .or_else(|| self.db.get_track_by_path(path).ok().flatten())
                    .or_else(|| {
                        if let Ok(scanned) = library::scanner::read_track_metadata(Path::new(path)) {
                            let _ = self.db.save_track(&scanned);
                            Some(scanned)
                        } else {
                            None
                        }
                    });

                let local_track = if let Some(t) = db_track {
                    audio_engine::LocalTrack {
                        id: t.id,
                        path: t.path,
                        title: t.title.or(param_title).unwrap_or_else(|| "Unknown".to_string()),
                        artist: t.artist.or(param_artist).unwrap_or_else(|| "Unknown".to_string()),
                        album: t.album,
                        duration_ms: t.duration_ms.unwrap_or(0),
                        cover_path: t.cover_path,
                    }
                } else {
                    let fallback_id = param_id.unwrap_or_else(|| {
                        library::compute_track_id(Path::new(path)).unwrap_or_else(|_| {
                            format!(
                                "local_{}",
                                Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("track")
                            )
                        })
                    });
                    audio_engine::LocalTrack {
                        id: fallback_id,
                        path: path.to_string(),
                        title: param_title.unwrap_or_else(|| "Unknown".to_string()),
                        artist: param_artist.unwrap_or_else(|| "Unknown".to_string()),
                        album: None,
                        duration_ms: 0,
                        cover_path: None,
                    }
                };

                // Ensure full local library is in the player queue so Next/Previous/Shuffle work seamlessly
                if let Ok(all_db_tracks) = self.db.list_tracks() {
                    if all_db_tracks.len() > 1 && self.audio.player.get_queue().len() <= 1 {
                        let queue_tracks: Vec<audio_engine::LocalTrack> = all_db_tracks
                            .into_iter()
                            .map(|t| audio_engine::LocalTrack {
                                id: t.id,
                                path: t.path,
                                title: t.title.unwrap_or_else(|| "Unknown".to_string()),
                                artist: t.artist.unwrap_or_else(|| "Unknown".to_string()),
                                album: t.album,
                                duration_ms: t.duration_ms.unwrap_or(0),
                                cover_path: t.cover_path,
                            })
                            .collect();
                        let _ = self.audio.player.set_queue(queue_tracks, None);
                    }
                }

                self.audio.player.play_track(local_track.clone())?;

                // Immediately trigger bindings resolution without 300ms delay
                let data = TrackContextData {
                    track_id: Some(local_track.id.clone()),
                    artist: Some(local_track.artist.clone()),
                    title: Some(local_track.title.clone()),
                    album: local_track.album.clone(),
                    playlist_id: None,
                    is_external: false,
                };
                *self.pending_track_change.lock().unwrap() =
                    Some((data, Instant::now() - Duration::from_secs(1)));

                Ok(serde_json::json!({
                    "status": "playing",
                    "track_id": local_track.id,
                }))
            }
            "player.getState" => {
                let state = self.audio.player.get_state();
                Ok(serde_json::to_value(state)?)
            }
            "settings.getMusic" => {
                let cfg = self.config.lock().unwrap();
                Ok(serde_json::to_value(&cfg.music)?)
            }
            "settings.setMusic" => {
                let mut cfg = self.config.lock().unwrap();
                if let Some(src_str) = params.get("source").and_then(|v| v.as_str()) {
                    cfg.music.source = match src_str {
                        "external" => MusicSource::External,
                        _ => MusicSource::Local,
                    };
                }
                if let Some(folders) = params.get("folders").and_then(|v| v.as_array()) {
                    cfg.music.folders = folders.iter().filter_map(|x| x.as_str().map(String::from)).collect();
                }
                let _ = cfg.save();
                Ok(serde_json::to_value(&cfg.music)?)
            }
            "widget.getState" => {
                let cfg = self.config.lock().unwrap();
                Ok(serde_json::to_value(&cfg.widget)?)
            }
            "widget.setTheme" => {
                let theme = params
                    .get("theme")
                    .and_then(|v| v.as_str())
                    .context("Missing 'theme' parameter (spotify/pill/vinyl/glass)")?;
                let (w, h) = {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.theme = theme.to_string();
                    let w = cfg.widget.width.clamp(220, 1200);
                    let h = widget::preferred_height(theme, w);
                    cfg.widget.width = w;
                    cfg.widget.height = h;
                    let _ = cfg.save();
                    (w, h)
                };
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({ "theme": theme, "width": w, "height": h }))
            }
            "widget.setPosition" => {
                let x = params
                    .get("x")
                    .and_then(|v| v.as_i64())
                    .context("Missing 'x' parameter")? as i32;
                let y = params
                    .get("y")
                    .and_then(|v| v.as_i64())
                    .context("Missing 'y' parameter")? as i32;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.x = x;
                    cfg.widget.y = y;
                    let _ = cfg.save();
                }
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({ "x": x, "y": y }))
            }
            "widget.setEnabled" => {
                let enabled = params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .context("Missing 'enabled' parameter")?;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.enabled = enabled;
                    let _ = cfg.save();
                }
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({ "enabled": enabled }))
            }
            "widget.setSize" => {
                let width = params
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .context("Missing 'width' parameter")? as u32;
                let height = params
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .context("Missing 'height' parameter")? as u32;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.width = width.clamp(220, 1200);
                    cfg.widget.height = height.clamp(70, 900);
                    let _ = cfg.save();
                }
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({ "width": width, "height": height }))
            }
            "widget.setOpacity" => {
                let opacity = params
                    .get("opacity")
                    .and_then(|v| v.as_f64())
                    .context("Missing 'opacity' parameter")? as f32;
                let clamped = opacity.clamp(0.2, 1.0);
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.opacity = clamped;
                    let _ = cfg.save();
                }
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({ "opacity": clamped }))
            }
            "widget.setPinned" => {
                let pinned = params
                    .get("pinned")
                    .and_then(|v| v.as_bool())
                    .context("Missing 'pinned' parameter")?;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.pinned = pinned;
                    let _ = cfg.save();
                }
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({ "pinned": pinned }))
            }
            "widget.updateConfig" => {
                let updated = {
                    let mut cfg = self.config.lock().unwrap();
                    if let Some(en) = params.get("enabled").and_then(|v| v.as_bool()) {
                        cfg.widget.enabled = en;
                    }
                    if let Some(theme) = params.get("theme").and_then(|v| v.as_str()) {
                        cfg.widget.theme = theme.to_string();
                    }
                    if let Some(vs) = params.get("visualizer_style").and_then(|v| v.as_str()) {
                        cfg.widget.visualizer_style = vs.to_string();
                    }
                    if let Some(da) = params.get("dynamic_accent").and_then(|v| v.as_bool()) {
                        cfg.widget.dynamic_accent = da;
                    }
                    if let Some(ca) = params.get("custom_accent").and_then(|v| v.as_str()) {
                        cfg.widget.custom_accent = ca.to_string();
                    }
                    if let Some(cr) = params.get("corner_radius").and_then(|v| v.as_u64()) {
                        cfg.widget.corner_radius = (cr as u32).clamp(0, 48);
                    }
                    if let Some(op) = params.get("opacity").and_then(|v| v.as_f64()) {
                        cfg.widget.opacity = (op as f32).clamp(0.3, 1.0);
                    }
                    if let Some(sl) = params.get("show_lyrics").and_then(|v| v.as_bool()) {
                        cfg.widget.show_lyrics = sl;
                    }
                    if let Some(sv) = params.get("show_visualizer").and_then(|v| v.as_bool()) {
                        cfg.widget.show_visualizer = sv;
                    }
                    if let Some(ms) = params.get("magnetic_snap").and_then(|v| v.as_bool()) {
                        cfg.widget.magnetic_snap = ms;
                    }
                    if let Some(pin) = params.get("pinned").and_then(|v| v.as_bool()) {
                        cfg.widget.pinned = pin;
                    }
                    if let Some(w) = params.get("width").and_then(|v| v.as_u64()) {
                        cfg.widget.width = (w as u32).clamp(220, 1200);
                    }
                    if let Some(h) = params.get("height").and_then(|v| v.as_u64()) {
                        cfg.widget.height = (h as u32).clamp(70, 900);
                    }
                    let _ = cfg.save();
                    cfg.widget.clone()
                };
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                Ok(serde_json::to_value(updated)?)
            }
            "widget.savePreset" => {
                let preset_val = params.get("preset").unwrap_or(&params);
                let preset: common::CustomWidgetPreset = serde_json::from_value(preset_val.clone())?;
                let presets = {
                    let mut cfg = self.config.lock().unwrap();
                    if let Some(existing) = cfg.widget.custom_presets.iter_mut().find(|p| p.id == preset.id) {
                        *existing = preset;
                    } else {
                        cfg.widget.custom_presets.push(preset);
                    }
                    let _ = cfg.save();
                    cfg.widget.custom_presets.clone()
                };
                Ok(serde_json::to_value(presets)?)
            }
            "widget.deletePreset" => {
                let id = params.get("id").and_then(|v| v.as_str()).context("Missing 'id'")?;
                let presets = {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.custom_presets.retain(|p| p.id != id);
                    let _ = cfg.save();
                    cfg.widget.custom_presets.clone()
                };
                Ok(serde_json::to_value(presets)?)
            }
            "widget.applyPreset" => {
                let preset_val = params.get("preset").unwrap_or(&params);
                let preset: common::CustomWidgetPreset = serde_json::from_value(preset_val.clone())?;
                let updated = {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.widget.theme = preset.theme;
                    cfg.widget.visualizer_style = preset.visualizer_style;
                    cfg.widget.dynamic_accent = preset.dynamic_accent;
                    cfg.widget.custom_accent = preset.custom_accent;
                    cfg.widget.corner_radius = preset.corner_radius.clamp(0, 48);
                    cfg.widget.opacity = preset.bg_opacity.clamp(0.3, 1.0);
                    cfg.widget.show_lyrics = preset.show_lyrics;
                    cfg.widget.show_visualizer = preset.show_visualizer;
                    cfg.widget.width = preset.width.clamp(220, 1200);
                    cfg.widget.height = preset.height.clamp(70, 900);
                    let _ = cfg.save();
                    cfg.widget.clone()
                };
                self.pending_widget_sync.store(true, Ordering::SeqCst);
                Ok(serde_json::to_value(updated)?)
            }
            "bass.getConfig" => {
                let cfg = self.config.lock().unwrap();
                Ok(serde_json::json!({
                    "intensity": cfg.bass_pulse_intensity,
                }))
            }
            "bass.setConfig" => {
                let intensity = params
                    .get("intensity")
                    .and_then(|v| v.as_f64())
                    .context("Missing 'intensity' parameter")? as f32;
                let clamped = intensity.clamp(0.0, 1.0);
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.bass_pulse_intensity = clamped;
                    let _ = cfg.save();
                }
                Ok(serde_json::json!({ "intensity": clamped }))
            }
            "bindings.list" => {
                let list = self.db.list_bindings()?;
                Ok(serde_json::to_value(list)?)
            }
            "bindings.set" => {
                let scope_str = params
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .context("Missing 'scope' parameter (track/album/artist/playlist/external)")?;
                let scope: BindingScope = scope_str.parse()?;
                let key = params
                    .get("key")
                    .and_then(|v| v.as_str())
                    .context("Missing 'key' parameter")?;
                let wallpaper_id = params
                    .get("wallpaper_id")
                    .and_then(|v| v.as_str())
                    .context("Missing 'wallpaper_id' parameter")?;

                if self.db.get_wallpaper(wallpaper_id)?.is_none() {
                    bail!("Wallpaper with id '{}' not found", wallpaper_id);
                }

                let item = BindingItem {
                    id: None,
                    scope,
                    key: key.to_string(),
                    wallpaper_id: wallpaper_id.to_string(),
                };
                self.db.save_binding(&item)?;

                // Re-evaluate bindings for the currently active track right away
                if let Some(local_t) = self.audio.player.current_track() {
                    let data = TrackContextData {
                        track_id: Some(local_t.id),
                        artist: Some(local_t.artist),
                        title: Some(local_t.title),
                        album: local_t.album,
                        playlist_id: None,
                        is_external: false,
                    };
                    *self.pending_track_change.lock().unwrap() =
                        Some((data, Instant::now() - Duration::from_secs(1)));
                }

                Ok(serde_json::json!({ "status": "saved", "binding": item }))
            }
            "bindings.remove" => {
                if let Some(id) = params.get("id").and_then(|v| v.as_i64()) {
                    let deleted = self.db.delete_binding_by_id(id)?;
                    return Ok(serde_json::json!({ "deleted": deleted }));
                }
                let scope_str = params
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .context("Missing 'scope' or 'id'")?;
                let scope: BindingScope = scope_str.parse()?;
                let key = params
                    .get("key")
                    .and_then(|v| v.as_str())
                    .context("Missing 'key'")?;
                let deleted = self.db.delete_binding(scope, key)?;
                Ok(serde_json::json!({ "deleted": deleted }))
            }
            "bindings.resolve" => {
                let track_id = params.get("track_id").and_then(|v| v.as_str());
                let artist = params.get("artist").and_then(|v| v.as_str());
                let title = params.get("title").and_then(|v| v.as_str());
                let album = params.get("album").and_then(|v| v.as_str());
                let playlist_id = params.get("playlist_id").and_then(|v| v.as_i64());
                let is_external = params.get("is_external").and_then(|v| v.as_bool()).unwrap_or(false);

                let ctx = TrackContext {
                    track_id,
                    artist,
                    title,
                    album,
                    playlist_id,
                    is_external,
                };
                let def_wp = self.config.lock().unwrap().active_wallpaper_id.clone();
                let res = resolve(&ctx, self.db.as_ref(), def_wp.as_deref());
                if let Some(r) = res {
                    Ok(serde_json::json!({
                        "wallpaper_id": r.wallpaper_id,
                        "matched_scope": r.matched_scope.map(|s| s.as_str()),
                    }))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            "bindings.getMode" => {
                let follow_music = self.config.lock().unwrap().follow_music;
                let mode = if follow_music { "follow_music" } else { "fixed" };
                Ok(serde_json::json!({
                    "follow_music": follow_music,
                    "mode": mode,
                }))
            }
            "bindings.setMode" => {
                let mut cfg = self.config.lock().unwrap();
                if let Some(b) = params.get("follow_music").and_then(|v| v.as_bool()) {
                    cfg.follow_music = b;
                } else if let Some(mode) = params.get("mode").and_then(|v| v.as_str()) {
                    cfg.follow_music = mode == "follow_music";
                } else {
                    bail!("Missing 'mode' (follow_music/fixed) or 'follow_music' boolean");
                }
                let _ = cfg.save();
                let mode = if cfg.follow_music { "follow_music" } else { "fixed" };
                Ok(serde_json::json!({
                    "follow_music": cfg.follow_music,
                    "mode": mode,
                }))
            }
            "bindings.setDefault" => {
                let wp_id = params
                    .get("wallpaper_id")
                    .and_then(|v| v.as_str())
                    .context("Missing 'wallpaper_id'")?;
                if self.db.get_wallpaper(wp_id)?.is_none() {
                    bail!("Wallpaper with id '{}' not found", wp_id);
                }
                let mut cfg = self.config.lock().unwrap();
                cfg.active_wallpaper_id = Some(wp_id.to_string());
                let _ = cfg.save();
                let _ = self.db.set_kv("default_wallpaper_id", wp_id);
                Ok(serde_json::json!({ "default_wallpaper_id": wp_id }))
            }
            "bindings.getDefault" => {
                let def = self.config.lock().unwrap().active_wallpaper_id.clone();
                Ok(serde_json::json!({ "default_wallpaper_id": def }))
            }
            "bindings.triggerChange" => {
                let track_id = params.get("track_id").and_then(|v| v.as_str()).map(String::from);
                let artist = params.get("artist").and_then(|v| v.as_str()).map(String::from);
                let title = params.get("title").and_then(|v| v.as_str()).map(String::from);
                let album = params.get("album").and_then(|v| v.as_str()).map(String::from);
                let playlist_id = params.get("playlist_id").and_then(|v| v.as_i64());
                let is_external = params.get("is_external").and_then(|v| v.as_bool()).unwrap_or(false);

                let data = TrackContextData {
                    track_id,
                    artist,
                    title,
                    album,
                    playlist_id,
                    is_external,
                };
                *self.pending_track_change.lock().unwrap() = Some((data, Instant::now()));
                Ok(serde_json::json!({ "status": "change_triggered" }))
            }
            "hotkeys.get" => {
                let cfg = self.config.lock().unwrap().hotkeys.clone();
                let status = self.hotkey_manager.lock().unwrap().get_status();
                Ok(serde_json::json!({
                    "hotkeys": cfg,
                    "status": status,
                }))
            }
            "hotkeys.set" => {
                let hk_val = params.get("hotkeys").context("Missing 'hotkeys' object")?;
                let new_hk: common::HotkeysConfig = serde_json::from_value(hk_val.clone())?;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.hotkeys = new_hk.clone();
                    let _ = cfg.save();
                }
                self.pending_hotkey_reload.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({
                    "status": "pending_reload",
                    "hotkeys": new_hk,
                }))
            }
            "hotkeys.reset" => {
                let def_hk = common::HotkeysConfig::default();
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.hotkeys = def_hk.clone();
                    let _ = cfg.save();
                }
                self.pending_hotkey_reload.store(true, Ordering::SeqCst);
                Ok(serde_json::json!({
                    "status": "reset",
                    "hotkeys": def_hk,
                }))
            }
            "visualizer.getConfig" => {
                let cfg = self.config.lock().unwrap().visualizer.clone();
                Ok(serde_json::to_value(cfg)?)
            }
            "visualizer.setConfig" => {
                let vis_val = params.get("visualizer").unwrap_or(&params);
                let new_vis: common::WallpaperVisualizerConfig = serde_json::from_value(vis_val.clone())?;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.visualizer = new_vis.clone();
                    let _ = cfg.save();
                }
                Ok(serde_json::to_value(new_vis)?)
            }
            "mouse.getConfig" => {
                let cfg = self.config.lock().unwrap();
                Ok(serde_json::json!({
                    "enabled": cfg.mouse_interaction,
                }))
            }
            "mouse.setConfig" => {
                let enabled = params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .context("Missing 'enabled' parameter")?;
                {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.mouse_interaction = enabled;
                    let _ = cfg.save();
                }
                Ok(serde_json::json!({ "enabled": enabled }))
            }
            "app.isPortable" => {
                Ok(serde_json::json!({
                    "portable": common::is_portable_mode(),
                }))
            }
            "update.check" => {
                let updater = crate::updater::Updater::new();
                let info = updater.check()?;
                Ok(serde_json::to_value(info)?)
            }
            "update.apply" => {
                let installer_path_str = params
                    .get("installer_path")
                    .and_then(|v| v.as_str())
                    .context("Missing 'installer_path' parameter")?;
                let path = std::path::PathBuf::from(installer_path_str);
                crate::updater::Updater::apply(&path)?;
                Ok(serde_json::json!({ "status": "installer_launched" }))
            }
            _ => bail!("Unknown IPC method: {}", method),
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    // If invoked with CLI arguments from a terminal, attach to parent console so output is visible
    if args.len() > 1 {
        unsafe {
            use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }

    // 0. Check if process started on WinSta0\Default and bind main thread to Default desktop
    let started_on_default = desktop_host::is_on_default_desktop();
    desktop_host::ensure_default_desktop();

    // 1. Safety Panic Hook
    panic::set_hook(Box::new(|panic_info| {
        eprintln!("CRITICAL PANIC in wallmus daemon: {:?}", panic_info);
    }));

    // 2. Setup Logging to file and console
    let _ = common::ensure_app_dirs();
    let file_appender = tracing_appender::rolling::daily(logs_dir(), "wallmus.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(non_blocking)
        .init();

    // CLI Commands (support both `--cmd` and `cmd`)
    if args.iter().any(|arg| arg == "--diag" || arg == "diag") {
        return run_diag();
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--selftest" || arg == "selftest") {
        let secs = args.get(pos + 1).and_then(|s| s.parse().ok()).unwrap_or(10);
        return run_selftest(secs);
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--image" || arg == "test-image") {
        if let Some(image_path) = args.get(pos + 1) {
            let secs = args.get(pos + 2).and_then(|s| s.parse().ok()).unwrap_or(10);
            return run_image_test(image_path, secs);
        }
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--video" || arg == "test-video") {
        if let Some(video_path) = args.get(pos + 1) {
            let secs = args.get(pos + 2).and_then(|s| s.parse().ok()).unwrap_or(10);
            return run_video_test(video_path, secs);
        }
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--import" || arg == "import") {
        if let Some(file_path) = args.get(pos + 1) {
            return run_cli_import(file_path);
        }
    }
    if args.iter().any(|arg| arg == "--metrics" || arg == "metrics") {
        return run_cli_metrics();
    }
    if args.iter().any(|arg| arg == "--pause" || arg == "pause") {
        return run_cli_pause(true);
    }
    if args.iter().any(|arg| arg == "--resume" || arg == "resume") {
        return run_cli_pause(false);
    }
    if args.iter().any(|arg| arg == "--track" || arg == "track") {
        return run_cli_track();
    }
    if args.iter().any(|arg| arg == "--spectrum" || arg == "spectrum") {
        return run_cli_spectrum();
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--control" || arg == "control") {
        if let Some(action) = args.get(pos + 1) {
            return run_cli_control(action);
        }
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--widget-theme" || arg == "widget-theme") {
        if let Some(theme) = args.get(pos + 1) {
            return run_cli_widget_theme(theme);
        }
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--visualizer-style" || arg == "visualizer-style") {
        if let Some(style) = args.get(pos + 1) {
            return run_cli_visualizer_style(style);
        }
    }
    if args.iter().any(|arg| arg == "--bindings-list" || arg == "bindings-list") {
        return run_cli_bindings_list();
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--bindings-set" || arg == "bindings-set") {
        if let (Some(scope), Some(key), Some(wp_id)) = (args.get(pos + 1), args.get(pos + 2), args.get(pos + 3)) {
            return run_cli_bindings_set(scope, key, wp_id);
        } else {
            eprintln!("Usage: wallmus bindings-set <scope> <key> <wallpaper_id>");
            return Ok(());
        }
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--bindings-mode" || arg == "bindings-mode") {
        if let Some(mode) = args.get(pos + 1) {
            return run_cli_bindings_mode(mode);
        } else {
            eprintln!("Usage: wallmus bindings-mode <follow_music|fixed>");
            return Ok(());
        }
    }
    if args.iter().any(|arg| arg == "--quit" || arg == "quit" || arg == "--exit" || arg == "exit" || arg == "--stop" || arg == "stop") {
        return run_cli_quit();
    }

    // If the daemon was started from a non-Default desktop (e.g. sandboxed runner),
    // re-launch the entire daemon process on WinSta0\Default so all worker threads and child
    // processes natively inherit the interactive user desktop.
    if !started_on_default && env::var("WALLMUS_ON_DEFAULT").is_err() && !args.iter().any(|a| a == "--on-default") {
        env::set_var("WALLMUS_ON_DEFAULT", "1");
        if let Ok(exe) = env::current_exe() {
            let mut pass_args: Vec<String> = args.iter().skip(1).cloned().collect();
            pass_args.push("--on-default".to_string());
            let arg_refs: Vec<&str> = pass_args.iter().map(|s| s.as_str()).collect();
            info!("Re-spawning wallmus daemon directly on WinSta0\\Default desktop...");
            if desktop_host::spawn_on_default_desktop(&exe, &arg_refs).is_ok() {
                return Ok(());
            }
        }
    }

    // 3. Single Instance Guard
    let single_instance = SingleInstanceGuard::acquire("Local\\wallmus_SingleInstanceMutex")
        .context("Failed to evaluate single instance mutex")?;

    if !single_instance.is_primary() {
        info!("wallmus is already running in the background. Focusing running application...");
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX);
        }
        let _ = send_request("app.openUi", serde_json::Value::Null);
        return Ok(());
    }

    let open_ui_on_start = !args.iter().any(|a| {
        a == "--no-open-ui" || a == "--minimized" || a == "--background" || a == "--daemon"
    });

    // 4. Primary daemon execution
    run_daemon(open_ui_on_start)
}

fn run_daemon(open_ui_on_start: bool) -> Result<()> {
    info!("Starting wallmus daemon (Stage 1)...");

    // Apply process profile: Below Normal priority + EcoQoS
    if let Err(e) = apply_eco_profile() {
        warn!("Could not apply eco process profile: {e}");
    }

    let config = Arc::new(Mutex::new(AppConfig::load()));
    // Normalize widget height and autostart/fps_limit state on startup
    {
        let mut cfg = config.lock().unwrap();
        let mut changed = false;
        let expected_h = widget::preferred_height(&cfg.widget.theme, cfg.widget.width);
        if (cfg.widget.theme == "pill" && cfg.widget.height > 150)
            || (cfg.widget.theme != "pill" && cfg.widget.height < 110)
        {
            cfg.widget.height = expected_h;
            changed = true;
        }
        if cfg.fps_limit == 0 {
            cfg.fps_limit = 30;
            changed = true;
        }
        let reg_autostart = crate::autostart::is_autostart_enabled();
        if cfg.autostart || reg_autostart {
            if !cfg.autostart {
                cfg.autostart = true;
                changed = true;
            }
            let _ = crate::autostart::set_autostart(true);
        }
        if changed {
            let _ = cfg.save();
        }
    }

    let db = Arc::new(Database::open_default().context("Failed to open SQLite database")?);

    // Get active wallpaper or auto-seed first available
    let mut wallpapers = db.list_wallpapers().unwrap_or_default();

    // Helper to resolve relative paths against CWD and executable parent directories
    let resolve_existing_path = |rel: &str| -> Option<std::path::PathBuf> {
        let p = Path::new(rel);
        if p.exists() {
            return std::fs::canonicalize(p).ok().or_else(|| Some(p.to_path_buf()));
        }
        if let Ok(exe) = std::env::current_exe() {
            let mut ancestor = exe.parent();
            for _ in 0..4 {
                if let Some(dir) = ancestor {
                    let cand = dir.join(rel);
                    if cand.exists() {
                        return std::fs::canonicalize(&cand).ok().or(Some(cand));
                    }
                    ancestor = dir.parent();
                }
            }
        }
        None
    };

    // Repair any relative processed_path entries in DB so launching from dist/ always finds them
    for wp in &mut wallpapers {
        if !Path::new(&wp.processed_path).exists() {
            if let Some(abs) = resolve_existing_path(&wp.processed_path)
                .or_else(|| resolve_existing_path(&wp.source_path))
            {
                let abs_str = abs.to_string_lossy().to_string();
                let clean_str = abs_str.strip_prefix(r"\\?\").unwrap_or(&abs_str).to_string();
                wp.processed_path = clean_str;
                let _ = db.save_wallpaper(wp);
            }
        }
    }

    if wallpapers.is_empty() || !wallpapers.iter().any(|w| Path::new(&w.processed_path).exists()) {
        info!("No valid wallpapers in library. Auto-discovering and importing default sample wallpaper...");
        let candidates = [
            "test_wallpaper.bmp",
            "assets/wallpaper.jpg",
            "assets/wallpaper.bmp",
        ];
        for c in &candidates {
            if let Some(p) = resolve_existing_path(c) {
                if let Ok(item) = import_wallpaper(&p, true) {
                    let _ = db.save_wallpaper(&item);
                    info!("Successfully auto-imported default wallpaper '{}' ({})", item.name, item.id);
                    wallpapers.push(item);
                    break;
                }
            }
        }
    }

    // Prefer saved active_wallpaper_id from config or DB KV (only if its file exists on disk!)
    let saved_wp_id = config
        .lock()
        .unwrap()
        .active_wallpaper_id
        .clone()
        .or_else(|| db.get_kv("active_wallpaper_id").ok().flatten());

    let initial_wp = saved_wp_id
        .as_deref()
        .and_then(|id| {
            wallpapers
                .iter()
                .find(|w| w.id == id && Path::new(&w.processed_path).exists())
                .cloned()
        })
        .or_else(|| {
            wallpapers
                .iter()
                .find(|w| w.kind == "video" && Path::new(&w.processed_path).exists())
                .cloned()
        })
        .or_else(|| {
            wallpapers
                .iter()
                .find(|w| Path::new(&w.processed_path).exists())
                .cloned()
        });

    if let Some(ref wp) = initial_wp {
        let mut cfg = config.lock().unwrap();
        if cfg.active_wallpaper_id.as_deref() != Some(&wp.id) {
            cfg.active_wallpaper_id = Some(wp.id.clone());
            let _ = cfg.save();
            let _ = db.set_kv("active_wallpaper_id", &wp.id);
        }
    }

    let active_wallpaper = Arc::new(Mutex::new(initial_wp.clone()));

    let should_quit = Arc::new(AtomicBool::new(false));

    // Create DualLayerHost for active monitors supporting smooth crossfade
    let monitors = enumerate_monitors().context("Failed to enumerate monitors")?;
    let mut hosts = Vec::new();

    let our_pid = std::process::id();

    for monitor in &monitors {
        let mut host_res = DualLayerHost::new(monitor.clone());
        for attempt in 1..=15 {
            if host_res.is_ok() {
                break;
            }
            warn!(
                "Desktop shell not ready yet on {} (attempt {}/15), waiting 400ms...",
                monitor.name, attempt
            );
            std::thread::sleep(Duration::from_millis(400));
            host_res = DualLayerHost::new(monitor.clone());
        }
        let mut host = host_res.context("Failed to create DualLayerHost")?;

        let active_wp_guard = active_wallpaper.lock().unwrap();
        if let Some(ref wp) = *active_wp_guard {
            if let Err(e) = host.load_initial(wp) {
                warn!("Failed to load initial wallpaper '{}' on {}: {e}", wp.name, monitor.name);
            }
        }
        hosts.push(host);
    }

    // Create Wallpaper Visualizer overlays for active monitors only when enabled
    let mut visualizers: Vec<Box<WallpaperVisualizerWindow>> = Vec::new();
    let vis_cfg_initial = config.lock().unwrap().visualizer.clone();
    if vis_cfg_initial.enabled {
        for monitor in &monitors {
            match WallpaperVisualizerWindow::create(monitor.clone(), &vis_cfg_initial) {
                Ok(vis) => visualizers.push(vis),
                Err(e) => warn!("Failed to create wallpaper visualizer for monitor {}: {e}", monitor.name),
            }
        }
    }
    let mut current_vis_height = vis_cfg_initial.height_px;

    // Initialize Governor
    let gov_config = config.lock().unwrap().governor.clone();
    let mut governor = Governor::new(gov_config);
    if let Err(e) = governor.start_hooks() {
        warn!("Governor hooks could not be started: {e}");
    }
    let governor_arc = Arc::new(Mutex::new(governor));
    let audio = Arc::new(AudioEngine::new());

    // Restore player state from DB KV store if present
    if let Ok(Some(saved_state_json)) = db.get_kv("player_state") {
        if let Ok(saved_state) = serde_json::from_str(&saved_state_json) {
            audio.player.load_state(saved_state);
            info!("Restored audio player state from previous session");
        }
    }

    // Initialize pending track change debounce channel for bindings engine
    let pending_track_change: Arc<Mutex<Option<(TrackContextData, Instant)>>> =
        Arc::new(Mutex::new(None));

    // Auto-save player state on track changed and notify bindings engine
    let db_for_player = db.clone();
    let player_for_cb = audio.player.clone();
    let pending_for_player = pending_track_change.clone();
    audio.player.set_on_track_changed(move |track| {
        info!("Track changed: {} - {}", track.artist, track.title);
        let state = player_for_cb.get_state();
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = db_for_player.set_kv("player_state", &json);
        }
        let data = TrackContextData {
            track_id: Some(track.id.clone()),
            artist: Some(track.artist.clone()),
            title: Some(track.title.clone()),
            album: track.album.clone(),
            playlist_id: None,
            is_external: false,
        };
        *pending_for_player.lock().unwrap() = Some((data, Instant::now()));
    });

    // Initialize LibraryWatcher for monitored music folders
    let mut watcher = LibraryWatcher::new(db.clone()).ok();
    if let Some(ref mut w) = watcher {
        let folders = config.lock().unwrap().music.folders.clone();
        let _ = w.update_folders(&folders);
    }
    let watcher_arc = Arc::new(Mutex::new(watcher));
    let hotkey_manager = Arc::new(Mutex::new(HotkeyManager::new()));
    let pending_hotkey_reload = Arc::new(AtomicBool::new(false));
    let pending_widget_sync = Arc::new(AtomicBool::new(false));

    // Initial hotkey registration on main thread
    {
        let initial_hk_cfg = config.lock().unwrap().hotkeys.clone();
        let status = hotkey_manager.lock().unwrap().register_all(&initial_hk_cfg);
        info!("Registered global hotkeys: {:?}", status);
    }

    let ctx = Arc::new(DaemonContext {
        db: db.clone(),
        config: config.clone(),
        governor: governor_arc.clone(),
        active_wallpaper: active_wallpaper.clone(),
        should_quit: should_quit.clone(),
        audio: audio.clone(),
        watcher: watcher_arc.clone(),
        pending_track_change: pending_track_change.clone(),
        hotkey_manager: hotkey_manager.clone(),
        pending_hotkey_reload: pending_hotkey_reload.clone(),
        pending_widget_sync: pending_widget_sync.clone(),
    });

    // Start IPC Server
    let ipc_server = IpcServer::start(ctx.clone()).context("Failed to start IPC server")?;
    info!("IPC server listening on {}", ipc::PIPE_NAME);

    // Create System Tray (with retry for early Windows boot before taskbar is ready)
    let mut tray_res = SystemTray::create(false);
    for _ in 1..=10 {
        if tray_res.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(400));
        tray_res = SystemTray::create(false);
    }
    let tray = tray_res.context("Failed to create system tray")?;
    info!("System tray initialized");

    // Initialize Desktop Music Widget
    let widget_cfg = Arc::new(Mutex::new(config.lock().unwrap().widget.clone()));
    let audio_cb = audio.clone();
    let config_action = config.clone();
    let should_quit_action = should_quit.clone();
    let gov_action = governor_arc.clone();
    let db_action = db.clone();
    let active_wp_action = active_wallpaper.clone();
    let pending_theme_cycle = Arc::new(AtomicBool::new(false));
    let pending_theme_action = pending_theme_cycle.clone();
    let pending_widget_sync_action = pending_widget_sync.clone();

    let on_action: widget::ActionCallback = Arc::new(move |action: &str| {
        info!("Widget action clicked: {action}");
        let audio = audio_cb.clone();
        let cfg_arc = config_action.clone();

        if let Some(target_str) = action.strip_prefix("seek:") {
            let a = audio.clone();
            if let Ok(ms) = target_str.parse::<u64>() {
                std::thread::spawn(move || {
                    let has_local = a.player.current_track().is_some();
                    let smtc_playing = a
                        .smtc
                        .get_current_track()
                        .ok()
                        .flatten()
                        .map(|t| t.playback_status == audio_engine::PlaybackStatus::Playing)
                        .unwrap_or(false);
                    let use_local = a.player.is_playing()
                        || (a.player.is_active_local() && has_local && !smtc_playing);

                    if use_local {
                        let _ = a.player.seek(std::time::Duration::from_millis(ms));
                    } else {
                        let _ = a.smtc.seek(ms);
                    }
                });
            }
            return;
        }
        if let Some(vol_str) = action.strip_prefix("volume:") {
            let a = audio.clone();
            if let Ok(vol) = vol_str.parse::<f32>() {
                std::thread::spawn(move || {
                    a.player.set_volume(vol.clamp(0.0, 1.0));
                });
            }
            return;
        }
        if let Some(mute_str) = action.strip_prefix("volume_mute:") {
            let a = audio.clone();
            if let Ok(muted) = mute_str.parse::<bool>() {
                std::thread::spawn(move || {
                    if muted {
                        a.player.set_volume(0.0);
                    } else {
                        a.player.set_volume(0.8);
                    }
                });
            }
            return;
        }

        match action {
            "toggle" => {
                std::thread::spawn(move || {
                    let has_local = audio.player.current_track().is_some();
                    let smtc_playing = audio
                        .smtc
                        .get_current_track()
                        .ok()
                        .flatten()
                        .map(|t| t.playback_status == audio_engine::PlaybackStatus::Playing)
                        .unwrap_or(false);
                    if audio.player.is_playing()
                        || (audio.player.is_active_local() && has_local && !smtc_playing)
                    {
                        audio.player.toggle_play_pause();
                    } else {
                        let _ = audio.smtc.toggle_play_pause();
                    }
                });
            }
            "previous" => {
                std::thread::spawn(move || {
                    let has_local = audio.player.current_track().is_some();
                    let smtc_playing = audio
                        .smtc
                        .get_current_track()
                        .ok()
                        .flatten()
                        .map(|t| t.playback_status == audio_engine::PlaybackStatus::Playing)
                        .unwrap_or(false);
                    if audio.player.is_playing()
                        || (audio.player.is_active_local() && has_local && !smtc_playing)
                    {
                        let _ = audio.player.previous();
                    } else {
                        let _ = audio.smtc.skip_previous();
                    }
                });
            }
            "next" => {
                std::thread::spawn(move || {
                    let has_local = audio.player.current_track().is_some();
                    let smtc_playing = audio
                        .smtc
                        .get_current_track()
                        .ok()
                        .flatten()
                        .map(|t| t.playback_status == audio_engine::PlaybackStatus::Playing)
                        .unwrap_or(false);
                    if audio.player.is_playing()
                        || (audio.player.is_active_local() && has_local && !smtc_playing)
                    {
                        let _ = audio.player.next();
                    } else {
                        let _ = audio.smtc.skip_next();
                    }
                });
            }
            "volume_up" => {
                let a = audio.clone();
                std::thread::spawn(move || {
                    let cur = a.player.get_volume();
                    a.player.set_volume((cur + 0.05).clamp(0.0, 1.0));
                    unsafe {
                        use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_VOLUME_UP};
                        keybd_event(VK_VOLUME_UP.0 as u8, 0, Default::default(), 0);
                        keybd_event(VK_VOLUME_UP.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                    }
                });
            }
            "volume_down" => {
                let a = audio.clone();
                std::thread::spawn(move || {
                    let cur = a.player.get_volume();
                    a.player.set_volume((cur - 0.05).clamp(0.0, 1.0));
                    unsafe {
                        use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_VOLUME_DOWN};
                        keybd_event(VK_VOLUME_DOWN.0 as u8, 0, Default::default(), 0);
                        keybd_event(VK_VOLUME_DOWN.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                    }
                });
            }
            "settings" => {
                launch_or_focus_ui();
            }
            "close" => {
                let sync_flag = pending_widget_sync_action.clone();
                std::thread::spawn(move || {
                    let mut cfg = cfg_arc.lock().unwrap();
                    cfg.widget.enabled = false;
                    let _ = cfg.save();
                    sync_flag.store(true, Ordering::SeqCst);
                    info!("Widget hidden by user action");
                });
            }
            "quit" => {
                info!("Quit requested via widget context menu");
                should_quit_action.store(true, Ordering::SeqCst);
            }
            "toggle_wallpaper_pause" => {
                let gov_arc = gov_action.clone();
                std::thread::spawn(move || {
                    let gov = gov_arc.lock().unwrap();
                    let current = gov.is_manual_paused();
                    gov.set_manual_pause(!current);
                    info!("Toggled wallpaper pause via widget -> {}", !current);
                });
            }
            "next_wallpaper" => {
                let db_c = db_action.clone();
                let active_c = active_wp_action.clone();
                let cfg_c = cfg_arc.clone();
                std::thread::spawn(move || {
                    let all_wp = db_c.list_wallpapers().unwrap_or_default();
                    let valid: Vec<WallpaperItem> = all_wp
                        .into_iter()
                        .filter(|w| Path::new(&w.processed_path).exists())
                        .collect();
                    if valid.len() > 1 {
                        let mut active = active_c.lock().unwrap();
                        let cur_id = active.as_ref().map(|w| w.id.clone());
                        let cur_idx = cur_id.as_ref().and_then(|cid| valid.iter().position(|w| &w.id == cid));
                        let mode = cfg_c.lock().unwrap().wallpaper_change_mode.clone();
                        let next_idx = if mode == "random" {
                            let mut seed = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos() as u64;
                            next_random_wp_index(cur_idx, valid.len(), &mut seed)
                        } else {
                            match cur_idx {
                                Some(i) => (i + 1) % valid.len(),
                                None => 0,
                            }
                        };
                        info!("Switching wallpaper to: {} ({}/{})", valid[next_idx].name, next_idx + 1, valid.len());
                        *active = Some(valid[next_idx].clone());
                        let _ = db_c.set_kv("active_wallpaper_id", &valid[next_idx].id);
                    }
                });
            }
            "toggle_theme" => {
                pending_theme_action.store(true, Ordering::SeqCst);
            }
            "toggle_pinned" => {
                let sync_flag = pending_widget_sync_action.clone();
                std::thread::spawn(move || {
                    let mut cfg = cfg_arc.lock().unwrap();
                    cfg.widget.pinned = !cfg.widget.pinned;
                    let _ = cfg.save();
                    sync_flag.store(true, Ordering::SeqCst);
                    info!("Widget pinned state toggled to: {}", cfg.widget.pinned);
                });
            }
            "toggle_visualizer_style" => {
                let sync_flag = pending_widget_sync_action.clone();
                std::thread::spawn(move || {
                    let mut cfg = cfg_arc.lock().unwrap();
                    let next_style = match cfg.widget.visualizer_style.as_str() {
                        "bars" => "wave",
                        "wave" => "mirror",
                        "mirror" => "dots",
                        _ => "bars",
                    };
                    cfg.widget.visualizer_style = next_style.to_string();
                    let _ = cfg.save();
                    sync_flag.store(true, Ordering::SeqCst);
                    info!("Widget visualizer style cycled to: {}", cfg.widget.visualizer_style);
                });
            }
            "toggle_lyrics" => {
                let sync_flag = pending_widget_sync_action.clone();
                std::thread::spawn(move || {
                    let mut cfg = cfg_arc.lock().unwrap();
                    cfg.widget.show_lyrics = !cfg.widget.show_lyrics;
                    let _ = cfg.save();
                    sync_flag.store(true, Ordering::SeqCst);
                    info!("Widget show_lyrics toggled to: {}", cfg.widget.show_lyrics);
                });
            }
            _ => {}
        }
    });

    let config_cb = config.clone();
    let on_moved: widget::PositionCallback = Arc::new(move |x: i32, y: i32, w: u32, h: u32| {
        let cfg_arc = config_cb.clone();
        std::thread::spawn(move || {
            let mut cfg = cfg_arc.lock().unwrap();
            cfg.widget.x = x;
            cfg.widget.y = y;
            cfg.widget.width = w;
            cfg.widget.height = h;
            let _ = cfg.save();
            info!("Widget geometry updated to pos=({x}, {y}), size={w}x{h} - config saved");
        });
    });

    let on_action_cb = on_action.clone();
    let on_moved_cb = on_moved.clone();
    let audio_widget = audio.clone();
    let widget_cfg_holder = widget_cfg.clone();

    let mut widget_controller = if widget_cfg.lock().unwrap().enabled {
        match WidgetController::new(widget_cfg.clone(), audio.clone(), Some(on_action.clone()), Some(on_moved.clone())) {
            Ok(w) => {
                info!("Desktop music widget window initialized");
                Some(w)
            }
            Err(e) => {
                warn!("Failed to initialize desktop music widget: {e}");
                None
            }
        }
    } else {
        None
    };

    if open_ui_on_start {
        // Automatically launch settings UI on startup so user immediately sees management window
        launch_or_focus_ui();
    }

    info!("wallmus daemon running. Entering main event loop...");

    let daemon_start = Instant::now();
    let mut last_shell_revalidate = Instant::now();
    let mut last_gov_check = Instant::now();
    let mut last_state = GovernorState::Running;
    let mut current_wallpaper_id = initial_wp.as_ref().map(|w| w.id.clone());
    let mut last_external_poll = Instant::now();
    let mut last_external_track_key: Option<String> = None;
    let mut rng_seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    let mut mouse_hook_mgr = MouseHookManager::new();
    let mut target_mouse: Vec<(f32, f32)> = vec![(0.5, 0.5); monitors.len()];
    let mut smooth_mouse: Vec<(f32, f32)> = vec![(0.5, 0.5); monitors.len()];
    let mut current_smooth_bass: f32 = 0.0;

    unsafe {
        let mut msg = MSG::default();

        while !should_quit.load(Ordering::SeqCst) {
            // Check dynamic hotkeys reload from IPC
            if pending_hotkey_reload.swap(false, Ordering::SeqCst) {
                let hk_cfg = config.lock().unwrap().hotkeys.clone();
                let status = hotkey_manager.lock().unwrap().register_all(&hk_cfg);
                info!("Global hotkeys reloaded on main thread: {:?}", status);
            }

            // Process Win32 messages
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    should_quit.store(true, Ordering::SeqCst);
                    break;
                } else if msg.message == WM_HOTKEY {
                    let id = msg.wParam.0 as i32;
                    if let Some(action) = HotkeyAction::from_id(id) {
                        let use_local = audio.player.is_playing() || audio.player.is_active_local();
                        match action {
                            HotkeyAction::TogglePlayback | HotkeyAction::MediaPlayPause => {
                                if use_local {
                                    audio.player.toggle_play_pause();
                                } else {
                                    let _ = audio.smtc.toggle_play_pause();
                                }
                            }
                            HotkeyAction::NextTrack | HotkeyAction::MediaNext => {
                                if use_local {
                                    let _ = audio.player.next();
                                } else {
                                    let _ = audio.smtc.skip_next();
                                }
                            }
                            HotkeyAction::PrevTrack | HotkeyAction::MediaPrev => {
                                if use_local {
                                    let _ = audio.player.previous();
                                } else {
                                    let _ = audio.smtc.skip_previous();
                                }
                            }
                            HotkeyAction::MediaStop => {
                                if use_local {
                                    audio.player.stop();
                                } else {
                                    let _ = audio.smtc.pause();
                                }
                            }
                            HotkeyAction::ToggleWallpaperPause => {
                                let gov = governor_arc.lock().unwrap();
                                let current = gov.is_manual_paused();
                                gov.set_manual_pause(!current);
                                tray.update_state(!current);
                                info!("Hotkey: Toggle wallpaper pause -> {}", !current);
                            }
                            HotkeyAction::NextWallpaper => {
                                info!("Hotkey: Next wallpaper requested");
                                let all_wp = db.list_wallpapers().unwrap_or_default();
                                let valid: Vec<WallpaperItem> = all_wp
                                    .into_iter()
                                    .filter(|w| Path::new(&w.processed_path).exists())
                                    .collect();
                                if valid.len() > 1 {
                                    let mut active = active_wallpaper.lock().unwrap();
                                    let cur_id = active.as_ref().map(|w| w.id.clone());
                                    let cur_idx = cur_id.as_ref().and_then(|cid| valid.iter().position(|w| &w.id == cid));
                                    let mode = config.lock().unwrap().wallpaper_change_mode.clone();
                                    let next_idx = if mode == "random" {
                                        next_random_wp_index(cur_idx, valid.len(), &mut rng_seed)
                                    } else {
                                        match cur_idx {
                                            Some(i) => (i + 1) % valid.len(),
                                            None => 0,
                                        }
                                    };
                                    *active = Some(valid[next_idx].clone());
                                    let _ = db.set_kv("active_wallpaper_id", &valid[next_idx].id);
                                    info!("Hotkey: Switched to wallpaper '{}' ({}/{})", valid[next_idx].name, next_idx + 1, valid.len());
                                }
                            }
                            HotkeyAction::ToggleWidget => {
                                let mut w_cfg = config.lock().unwrap();
                                w_cfg.widget.enabled = !w_cfg.widget.enabled;
                                let is_en = w_cfg.widget.enabled;
                                let _ = w_cfg.save();
                                pending_widget_sync.store(true, Ordering::SeqCst);
                                info!("Hotkey: Toggle desktop widget -> {is_en}");
                            }
                        }
                    }
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Process Tray Events
            if let Ok(event) = muda::MenuEvent::receiver().try_recv() {
                if event.id == tray.quit_id {
                    info!("Tray: Quit clicked");
                    should_quit.store(true, Ordering::SeqCst);
                    break;
                } else if event.id == tray.open_ui_id {
                    info!("Tray: Open UI clicked");
                    launch_or_focus_ui();
                } else if event.id == tray.toggle_widget_id {
                    let mut w_cfg = config.lock().unwrap();
                    w_cfg.widget.enabled = !w_cfg.widget.enabled;
                    let is_en = w_cfg.widget.enabled;
                    let _ = w_cfg.save();
                    pending_widget_sync.store(true, Ordering::SeqCst);
                    info!("Tray: Toggle desktop widget -> {is_en}");
                } else if event.id == tray.pause_id {
                    let gov = governor_arc.lock().unwrap();
                    let current = gov.is_manual_paused();
                    gov.set_manual_pause(!current);
                    tray.update_state(!current);
                    info!("Tray: Toggle pause -> {}", !current);
                } else if event.id == tray.diag_id {
                    info!("Tray: Dump diagnostics requested");
                    let _ = run_diag();
                } else if event.id == tray.next_id {
                    info!("Tray: Next wallpaper clicked");
                    // Switch to next wallpaper in library
                    let list = db.list_wallpapers().unwrap_or_default();
                    if list.len() > 1 {
                        let mut active = active_wallpaper.lock().unwrap();
                        let next = list
                            .iter()
                            .find(|w| active.as_ref().map(|a| a.id != w.id).unwrap_or(true))
                            .cloned();
                        if let Some(next_wp) = next {
                            info!("Switching wallpaper to: {}", next_wp.name);
                            *active = Some(next_wp);
                        }
                    }
                }
            }

            // Periodic Governor evaluation (every 1 second or on WinEvent)
            let gov_triggered = {
                let gov = governor_arc.lock().unwrap();
                gov.take_event_trigger()
            };

            if (gov_triggered && last_gov_check.elapsed() >= Duration::from_millis(250))
                || last_gov_check.elapsed() >= Duration::from_millis(1000)
            {
                last_gov_check = Instant::now();

                if let Some(primary_mon) = monitors.first() {
                    let current_gov_state = {
                        let gov = governor_arc.lock().unwrap();
                        gov.evaluate_monitor(primary_mon, our_pid)
                    };

                    if current_gov_state != last_state {
                        info!("Governor state changed: {:?} -> {:?}", last_state, current_gov_state);
                        match &current_gov_state {
                            GovernorState::Paused(reason) => {
                                mouse_hook_mgr.set_enabled(false);
                                for h in &mut hosts {
                                    let _ = h.pause();
                                }
                                // Suspend FFT capture only during lock or display off, or if widget is disabled
                                let widget_en = config.lock().unwrap().widget.enabled;
                                let pause_fft = !widget_en
                                    || matches!(
                                        reason,
                                        governor::PauseReason::SessionLocked
                                            | governor::PauseReason::DisplayOff
                                    );
                                if pause_fft {
                                    audio.visualizer.set_paused(true);
                                }
                                audio.wallpaper_audio.fade_out();
                                tray.update_state(true);
                                info!("Paused wallpaper rendering due to: {:?}", reason);
                            }
                            GovernorState::Running => {
                                let mouse_enabled = config.lock().unwrap().mouse_interaction;
                                mouse_hook_mgr.set_enabled(mouse_enabled);
                                for h in &mut hosts {
                                    let _ = h.resume();
                                }
                                audio.visualizer.set_paused(false);
                                audio.wallpaper_audio.fade_in();
                                tray.update_state(false);
                                info!("Resumed rendering");
                            }
                            GovernorState::Unloaded => {
                                mouse_hook_mgr.set_enabled(false);
                                for h in &mut hosts {
                                    let _ = h.unload();
                                }
                                audio.visualizer.set_paused(true);
                                audio.wallpaper_audio.fade_out();
                                info!("Unloaded renderers");
                            }
                        }
                        last_state = current_gov_state;
                    }
                }
            }

            // Check external SMTC track changes whenever local player is not playing
            if last_external_poll.elapsed() >= Duration::from_millis(250) {
                last_external_poll = Instant::now();
                if !audio.player.is_playing() {
                    if let Ok(Some(ext_t)) = audio.smtc.get_current_track() {
                        if ext_t.playback_status == audio_engine::PlaybackStatus::Playing {
                            audio.player.set_active_local(false);
                        }
                        let key = format!("{}|{}", ext_t.artist, ext_t.title);
                        if last_external_track_key.as_deref() != Some(&key) {
                            info!("External SMTC track changed: {} - {}", ext_t.artist, ext_t.title);
                            last_external_track_key = Some(key);
                            let data = TrackContextData {
                                track_id: None,
                                artist: Some(ext_t.artist),
                                title: Some(ext_t.title),
                                album: if ext_t.album_title.is_empty() {
                                    None
                                } else {
                                    Some(ext_t.album_title)
                                },
                                playlist_id: None,
                                is_external: true,
                            };
                            *pending_track_change.lock().unwrap() = Some((data, Instant::now()));
                        }
                    }
                }
            }

            // Debounced bindings resolution (300 ms debounce when tracks switch rapidly)
            let ready_track_change = {
                let mut guard = pending_track_change.lock().unwrap();
                if let Some((_, ref req_time)) = *guard {
                    if req_time.elapsed() >= Duration::from_millis(300) {
                        guard.take().map(|(d, _)| d)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(track_data) = ready_track_change {
                let follow_music = config.lock().unwrap().follow_music;
                if !follow_music {
                    info!("Bindings engine: follow_music is disabled (fixed mode); keeping current wallpaper");
                } else {
                    let (default_wp_id, change_mode, respect_bindings) = {
                        let cfg = config.lock().unwrap();
                        (
                            cfg.active_wallpaper_id.clone(),
                            cfg.wallpaper_change_mode.clone(),
                            cfg.respect_bindings_in_auto_change,
                        )
                    };
                    let res = resolve(&track_data.as_context(), db.as_ref(), default_wp_id.as_deref());

                    let chosen_wallpaper: Option<WallpaperItem> = if let Some(ref res_item) = res {
                        let has_specific_binding = res_item.matched_scope.is_some();
                        if has_specific_binding && respect_bindings {
                            // Explicit manual binding has precedence
                            match db.get_wallpaper(&res_item.wallpaper_id) {
                                Ok(Some(wp)) if Path::new(&wp.processed_path).exists() => {
                                    info!(
                                        "Bindings engine: applying {:?} binding -> switching to wallpaper '{}' ({})",
                                        res_item.matched_scope, wp.name, wp.id
                                    );
                                    Some(wp)
                                }
                                _ => None,
                            }
                        } else {
                            // Auto-change mode: random or sequential or off
                            match change_mode.as_str() {
                                "random" => {
                                    let all_wp = db.list_wallpapers().unwrap_or_default();
                                    let valid: Vec<WallpaperItem> = all_wp
                                        .into_iter()
                                        .filter(|w| Path::new(&w.processed_path).exists())
                                        .collect();
                                    if !valid.is_empty() {
                                        let cur_id = active_wallpaper.lock().unwrap().as_ref().map(|w| w.id.clone());
                                        let cur_idx = cur_id.as_ref().and_then(|cid| valid.iter().position(|w| &w.id == cid));
                                        let next_idx = next_random_wp_index(cur_idx, valid.len(), &mut rng_seed);
                                        info!(
                                            "Auto-change wallpaper (random): selected '{}' ({}/{})",
                                            valid[next_idx].name, next_idx + 1, valid.len()
                                        );
                                        Some(valid[next_idx].clone())
                                    } else {
                                        None
                                    }
                                }
                                "sequential" => {
                                    let all_wp = db.list_wallpapers().unwrap_or_default();
                                    let valid: Vec<WallpaperItem> = all_wp
                                        .into_iter()
                                        .filter(|w| Path::new(&w.processed_path).exists())
                                        .collect();
                                    if !valid.is_empty() {
                                        let cur_id = active_wallpaper.lock().unwrap().as_ref().map(|w| w.id.clone());
                                        let cur_idx = cur_id.as_ref().and_then(|cid| valid.iter().position(|w| &w.id == cid));
                                        let next_idx = match cur_idx {
                                            Some(i) => (i + 1) % valid.len(),
                                            None => 0,
                                        };
                                        info!(
                                            "Auto-change wallpaper (sequential): selected '{}' ({}/{})",
                                            valid[next_idx].name, next_idx + 1, valid.len()
                                        );
                                        Some(valid[next_idx].clone())
                                    } else {
                                        None
                                    }
                                }
                                _ => {
                                    // "off": keep bound wallpaper or default
                                    match db.get_wallpaper(&res_item.wallpaper_id) {
                                        Ok(Some(wp)) if Path::new(&wp.processed_path).exists() => Some(wp),
                                        _ => {
                                            if let Some(ref def_id) = default_wp_id {
                                                db.get_wallpaper(def_id).ok().flatten()
                                            } else {
                                                None
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        None
                    };

                    if let Some(wp) = chosen_wallpaper {
                        let mut active = active_wallpaper.lock().unwrap();
                        let already_active = active.as_ref().map(|a| a.id == wp.id).unwrap_or(false);
                        if !already_active {
                            *active = Some(wp.clone());
                            let _ = db.set_kv("active_wallpaper_id", &wp.id);
                        }
                    }
                }
            }

            // Sync active wallpaper to hosts with smooth crossfade
            let current_target = active_wallpaper.lock().unwrap().clone();
            if current_target.as_ref().map(|w| &w.id) != current_wallpaper_id.as_ref() {
                if let Some(ref wp) = current_target {
                    let crossfade_ms = config.lock().unwrap().crossfade_duration_ms;
                    info!("Transitioning active wallpaper '{}' on {} hosts (crossfade: {} ms)", wp.name, hosts.len(), crossfade_ms);
                    for h in &mut hosts {
                        if let Err(e) = h.start_transition(wp, crossfade_ms) {
                            warn!("Failed to start transition on host {}: {e}", h.monitor.name);
                        }
                    }
                    current_wallpaper_id = Some(wp.id.clone());
                } else {
                    for h in &mut hosts {
                        let _ = h.unload();
                    }
                    current_wallpaper_id = None;
                }
            }

            // Sync dynamic mouse interaction & audio-reactive bass pulse for ALL wallpaper types
            let (current_mouse_interaction, bass_intensity) = {
                let cfg = config.lock().unwrap();
                (cfg.mouse_interaction, cfg.bass_pulse_intensity)
            };
            let has_active_wp = active_wallpaper.lock().unwrap().is_some();
            let should_track_mouse =
                current_mouse_interaction && has_active_wp && last_state == GovernorState::Running;
            if should_track_mouse != mouse_hook_mgr.is_active() {
                mouse_hook_mgr.set_enabled(should_track_mouse);
            }

            // Compute audio-reactive bass kick (20-120 Hz low-end energy) from live WASAPI audio capture
            let raw_bass = if bass_intensity > 0.01
                && has_active_wp
                && last_state == GovernorState::Running
            {
                let low_spec = audio.visualizer.get_spectrum(8);
                let bass_energy = (low_spec.get(0).copied().unwrap_or(0.0) * 1.3
                    + low_spec.get(1).copied().unwrap_or(0.0) * 1.0
                    + low_spec.get(2).copied().unwrap_or(0.0) * 0.7) / 3.0;
                (bass_energy - 0.08).max(0.0) * bass_intensity
            } else {
                0.0
            };

            // Fast attack and natural spring decay envelope for punchy bass bounce
            if raw_bass > current_smooth_bass {
                current_smooth_bass += (raw_bass - current_smooth_bass) * 0.75;
            } else {
                current_smooth_bass *= 0.84;
            }

            if current_smooth_bass > 0.003 || raw_bass > 0.003 {
                for host in &mut hosts {
                    host.update_bass(current_smooth_bass);
                }
            } else if current_smooth_bass <= 0.003 && current_smooth_bass > 0.0 {
                current_smooth_bass = 0.0;
                for host in &mut hosts {
                    host.update_bass(0.0);
                }
            }

            // Poll cursor position and smoothly interpolate (Lerp) parallax at 60 FPS
            let needs_relax = smooth_mouse
                .iter()
                .zip(target_mouse.iter())
                .any(|(&(sx, sy), &(tx, ty))| (tx - sx).abs() > 0.001 || (ty - sy).abs() > 0.001);
            if mouse_hook_mgr.is_active() || needs_relax {
                if mouse_hook_mgr.is_active() {
                    if let Some((mx, my)) = MouseHookManager::poll_position() {
                        for (i, mon) in monitors.iter().enumerate() {
                            if mx >= mon.x && mx < mon.x + mon.width && my >= mon.y && my < mon.y + mon.height {
                                let norm_x = ((mx - mon.x) as f32 / mon.width as f32).clamp(0.0, 1.0);
                                let norm_y = ((my - mon.y) as f32 / mon.height as f32).clamp(0.0, 1.0);
                                if let Some(target) = target_mouse.get_mut(i) {
                                    *target = (norm_x, norm_y);
                                }
                            }
                        }
                    }
                }
                for (i, host) in hosts.iter_mut().enumerate() {
                    if let (Some(&(tx, ty)), Some((sx, sy))) =
                        (target_mouse.get(i), smooth_mouse.get_mut(i))
                    {
                        let dx = tx - *sx;
                        let dy = ty - *sy;
                        if dx.abs() > 0.0004 || dy.abs() > 0.0004 {
                            *sx += dx * 0.25;
                            *sy += dy * 0.28;
                            host.update_mouse(*sx, *sy);
                        }
                    }
                }
            }

            // Update crossfade animation step for all hosts
            let mut any_fading = false;
            for h in &mut hosts {
                if h.update() {
                    any_fading = true;
                }
            }

            // Periodically revalidate shell attachment (frequently in first 30s after boot, then every 15s)
            let reval_interval = if daemon_start.elapsed() < Duration::from_secs(30) {
                Duration::from_secs(3)
            } else {
                Duration::from_secs(15)
            };
            if !any_fading
                && has_active_wp
                && last_state == GovernorState::Running
                && last_shell_revalidate.elapsed() >= reval_interval
            {
                last_shell_revalidate = Instant::now();
                for h in &mut hosts {
                    let _ = h.revalidate();
                }
            }

            // Dynamic widget enable / disable / sync / self-healing if HWND was destroyed
            let current_widget_cfg = config.lock().unwrap().widget.clone();
            if widget_controller.as_ref().map(|w| !w.is_valid()).unwrap_or(false) {
                warn!("Widget HWND became invalid; recreating desktop music widget...");
                widget_controller = None;
            }
            if current_widget_cfg.enabled && widget_controller.is_none() {
                *widget_cfg_holder.lock().unwrap() = current_widget_cfg.clone();
                match WidgetController::new(widget_cfg_holder.clone(), audio_widget.clone(), Some(on_action_cb.clone()), Some(on_moved_cb.clone())) {
                    Ok(w) => {
                        info!("Desktop music widget window dynamically created");
                        widget_controller = Some(w);
                    }
                    Err(e) => {
                        warn!("Failed to dynamically create desktop music widget: {e}");
                    }
                }
            } else if !current_widget_cfg.enabled && widget_controller.is_some() {
                info!("Desktop music widget window dynamically hidden");
                widget_controller = None;
            }

            // Update widget window frame (suspend during FullScreenApp / Game Mode to save 100% GPU)
            if let Some(ref mut w) = widget_controller {
                if pending_widget_sync.swap(false, Ordering::SeqCst) {
                    w.apply_config(&current_widget_cfg);
                }
                if pending_theme_cycle.swap(false, Ordering::SeqCst) {
                    let (new_theme, new_w, new_h) = w.cycle_theme();
                    let mut cfg = config.lock().unwrap();
                    cfg.widget.theme = new_theme;
                    cfg.widget.width = new_w;
                    cfg.widget.height = new_h;
                    let _ = cfg.save();
                }
                let is_display_locked = matches!(
                    last_state,
                    GovernorState::Paused(
                        governor::PauseReason::SessionLocked
                            | governor::PauseReason::DisplayOff
                    )
                );
                w.update(is_display_locked);
            }

            // Update wallpaper audio visualizer layer on all monitors
            let is_system_paused = last_state != GovernorState::Running;
            let vis_cfg = config.lock().unwrap().visualizer.clone();
            if !vis_cfg.enabled {
                if !visualizers.is_empty() {
                    visualizers.clear();
                }
            } else {
                if visualizers.is_empty() || vis_cfg.height_px != current_vis_height {
                    visualizers.clear();
                    for monitor in &monitors {
                        if let Ok(vis) = WallpaperVisualizerWindow::create(monitor.clone(), &vis_cfg) {
                            visualizers.push(vis);
                        }
                    }
                    current_vis_height = vis_cfg.height_px;
                }

                if !is_system_paused {
                    let spectrum = audio.visualizer.get_spectrum(vis_cfg.bars_count as usize);
                    for vis in &mut visualizers {
                        vis.update(&spectrum, &vis_cfg, false);
                    }
                } else {
                    for vis in &mut visualizers {
                        vis.update(&[], &vis_cfg, true);
                    }
                }
            }

            // Dynamic FPS limiter: 8ms (~125 FPS) when fps_limit == 0 (unconstrained), else target frame interval
            let fps_limit = config.lock().unwrap().fps_limit;
            let mut sleep_ms = if fps_limit == 0 {
                8
            } else {
                1000u64.checked_div(fps_limit as u64).unwrap_or(16).max(1)
            };
            if widget_controller.is_some() || mouse_hook_mgr.is_active() {
                sleep_ms = sleep_ms.clamp(4, 16);
            }
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }

    mouse_hook_mgr.set_enabled(false);
    hotkey_manager.lock().unwrap().unregister_all();

    // Save final player state to DB KV
    let state = audio.player.get_state();
    if let Ok(json) = serde_json::to_string(&state) {
        let _ = db.set_kv("player_state", &json);
    }

    info!("Shutting down wallmus daemon...");
    ipc_server.stop();
    drop(visualizers);
    drop(hosts);
    info!("Clean shutdown complete.");

    Ok(())
}

fn run_cli_import(file_path: &str) -> Result<()> {
    let path = Path::new(file_path);
    println!("Importing wallpaper: {}", path.display());
    let item = import_wallpaper(path, true)?;
    let db = Database::open_default()?;
    db.save_wallpaper(&item)?;
    println!("Successfully imported wallpaper!");
    println!("  ID   : {}", item.id);
    println!("  Name : {}", item.name);
    println!("  Kind : {}", item.kind);
    println!("  Thumb: {}", item.thumb_path);
    println!("  File : {}", item.processed_path);
    Ok(())
}

fn run_cli_metrics() -> Result<()> {
    match send_request("app.getState", serde_json::Value::Null) {
        Ok(resp) => {
            if resp.ok {
                println!("wallmus Daemon Metrics:");
                println!("{}", serde_json::to_string_pretty(&resp.result)?);
            } else {
                eprintln!("Error from daemon: {:?}", resp.error);
            }
        }
        Err(e) => {
            eprintln!("Daemon is not running ({e}). Collecting offline diagnostics...");
            run_diag()?;
        }
    }
    Ok(())
}

fn run_cli_pause(pause: bool) -> Result<()> {
    let method = if pause { "wallpapers.pause" } else { "wallpapers.resume" };
    println!("Sending {} command to daemon...", method);
    let resp = send_request(method, serde_json::Value::Null)?;
    println!("Daemon response: {:?}", resp);
    Ok(())
}

fn run_cli_track() -> Result<()> {
    let resp = send_request("audio.getTrack", serde_json::Value::Null)?;
    if resp.ok {
        match resp.result {
            Some(ref v) if !v.is_null() => {
                println!("Currently Playing Media Track:");
                println!("{}", serde_json::to_string_pretty(v)?);
            }
            _ => println!("No media currently playing."),
        }
    } else {
        eprintln!("Error querying track: {:?}", resp.error);
    }
    Ok(())
}

fn run_cli_spectrum() -> Result<()> {
    let resp = send_request("audio.getSpectrum", serde_json::json!({ "bars": 32 }))?;
    if resp.ok {
        if let Some(val) = resp.result {
            if let Some(arr) = val.get("spectrum").and_then(|s| s.as_array()) {
                println!("Real-time Audio Spectrum (32 bands):");
                for (i, v) in arr.iter().enumerate() {
                    let level = v.as_f64().unwrap_or(0.0);
                    let bar_len = (level * 40.0) as usize;
                    let bar_str = "#".repeat(bar_len);
                    println!("{:2}: {:<40} ({:.2})", i + 1, bar_str, level);
                }
            }
        }
    } else {
        eprintln!("Error querying spectrum: {:?}", resp.error);
    }
    Ok(())
}

fn run_cli_control(action: &str) -> Result<()> {
    println!("Sending audio.control action '{}'...", action);
    let resp = send_request("audio.control", serde_json::json!({ "action": action }))?;
    println!("Daemon response: {:?}", resp);
    Ok(())
}

fn run_cli_widget_theme(theme: &str) -> Result<()> {
    println!("Setting widget theme to '{}'...", theme);
    let resp = send_request("widget.setTheme", serde_json::json!({ "theme": theme }))?;
    println!("Daemon response: {:?}", resp);
    Ok(())
}

fn run_cli_bindings_list() -> Result<()> {
    println!("Querying wallpaper-to-music bindings from daemon...");
    let resp = send_request("bindings.list", serde_json::Value::Null)?;
    if resp.ok {
        println!("{}", serde_json::to_string_pretty(&resp.result)?);
    } else {
        eprintln!("Error from daemon: {:?}", resp.error);
    }
    Ok(())
}

fn run_cli_bindings_set(scope: &str, key: &str, wallpaper_id: &str) -> Result<()> {
    println!("Setting binding: scope='{scope}', key='{key}' -> wallpaper='{wallpaper_id}'...");
    let resp = send_request("bindings.set", serde_json::json!({
        "scope": scope,
        "key": key,
        "wallpaper_id": wallpaper_id,
    }))?;
    println!("Daemon response: {:?}", resp);
    Ok(())
}

fn run_cli_bindings_mode(mode: &str) -> Result<()> {
    println!("Setting bindings mode to '{}'...", mode);
    let resp = send_request("bindings.setMode", serde_json::json!({ "mode": mode }))?;
    println!("Daemon response: {:?}", resp);
    Ok(())
}

fn run_cli_quit() -> Result<()> {
    println!("Requesting wallmus daemon to quit...");
    let resp = send_request("app.quit", serde_json::Value::Null)?;
    println!("Daemon response: {:?}", resp);
    Ok(())
}

fn run_diag() -> Result<()> {
    info!("Collecting system diagnostics...");
    let mut diag = collect_diag().context("Failed to collect diagnostics")?;
    diag.libmpv_version = render::get_mpv_version();

    let mut out = std::io::stdout();
    let _ = writeln!(out, "============================================================");
    let _ = writeln!(out, "                  WALLMUS DIAGNOSTIC REPORT                 ");
    let _ = writeln!(out, "============================================================");
    let _ = writeln!(out, "OS Product Name : {}", diag.os_product_name);
    let _ = writeln!(out, "OS Build        : {}", diag.os_build);
    let _ = writeln!(out, "Shell Topology  : {:?}", diag.topology);
    let _ = writeln!(
        out,
        "GPU Adapter     : {}",
        diag.gpu_adapter.as_deref().unwrap_or("Unknown")
    );
    let _ = writeln!(
        out,
        "libmpv Version  : {}",
        diag.libmpv_version.as_deref().unwrap_or("Not loaded")
    );
    let _ = writeln!(out, "\nActive Monitors ({}):", diag.monitors.len());
    for (i, m) in diag.monitors.iter().enumerate() {
        let _ = writeln!(
            out,
            "  [{}] {} | Primary: {} | Rect: [{}, {} to {}, {}] ({}x{}) | DPI: {}x{} | Scale: {:.2}x",
            i, m.name, m.is_primary, m.x, m.y, m.x + m.width, m.y + m.height, m.width, m.height, m.dpi_x, m.dpi_y, m.scale_factor
        );
    }
    let _ = writeln!(out, "============================================================");

    let json_data = serde_json::to_string_pretty(&diag)?;
    let _ = std::fs::write("diag_report.json", json_data.as_bytes());
    info!("Diagnostics saved to diag_report.json");

    Ok(())
}

fn run_selftest(duration_secs: u64) -> Result<()> {
    info!("Starting wallmus Stage 0 desktop attachment self-test ({} seconds)...", duration_secs);

    let monitors = enumerate_monitors().context("Failed to enumerate monitors")?;
    if monitors.is_empty() {
        bail!("No active monitors found!");
    }

    let mut hosts = Vec::new();
    for monitor in monitors {
        let host = DesktopHostWindow::create(monitor, RenderMode::GdiPattern)?;
        hosts.push(host);
    }

    let start_time = Instant::now();
    let max_duration = Duration::from_secs(duration_secs);

    unsafe {
        let mut msg = MSG::default();
        while start_time.elapsed() < max_duration {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    drop(hosts);
    info!("Self-test complete. Desktop restored.");
    Ok(())
}

fn run_image_test(image_path: &str, duration_secs: u64) -> Result<()> {
    info!("Starting WIC static image test: {}...", image_path);
    let monitors = enumerate_monitors()?;
    let mut hosts = Vec::new();

    for monitor in monitors {
        let mut host = DesktopHostWindow::create(monitor.clone(), RenderMode::StaticImage)?;
        let mut renderer = StaticImageRenderer::new(host.hwnd, monitor.width, monitor.height);
        renderer.load(image_path)?;

        let renderer_arc = Arc::new(Mutex::new(renderer));
        host.set_paint_callback(move |hdc, _w, _h| {
            if let Ok(guard) = renderer_arc.lock() {
                guard.paint(hdc);
            }
        });
        hosts.push(host);
    }

    let start_time = Instant::now();
    let max_duration = Duration::from_secs(duration_secs);

    unsafe {
        let mut msg = MSG::default();
        while start_time.elapsed() < max_duration {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    drop(hosts);
    Ok(())
}

fn run_video_test(video_path: &str, duration_secs: u64) -> Result<()> {
    info!("Starting libmpv video test: {}...", video_path);
    let monitors = enumerate_monitors()?;
    let mut hosts = Vec::new();
    let mut renderers = Vec::new();

    for monitor in monitors {
        let host = DesktopHostWindow::create(monitor.clone(), RenderMode::Video)?;
        let mut renderer = MpvRenderer::new(host.hwnd.0 as usize, monitor.width, monitor.height)?;
        renderer.load(video_path)?;
        hosts.push(host);
        renderers.push(renderer);
    }

    let start_time = Instant::now();
    let max_duration = Duration::from_secs(duration_secs);

    unsafe {
        let mut msg = MSG::default();
        while start_time.elapsed() < max_duration {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    drop(renderers);
    drop(hosts);
    Ok(())
}

fn run_cli_visualizer_style(style: &str) -> Result<()> {
    println!("Setting visualizer style to '{}'...", style);
    let resp = send_request("visualizer.setConfig", serde_json::json!({ "style": style }))?;
    println!("Daemon response: {:?}", resp);
    Ok(())
}

pub fn launch_or_focus_ui() {
    std::thread::spawn(|| {
        // CRITICAL: Each newly spawned thread inherits the process startup desktop unless switched!
        desktop_host::ensure_default_desktop();

        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                BringWindowToTop, FindWindowW, IsIconic, SetForegroundWindow, SetWindowPos,
                ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
                SW_RESTORE, SW_SHOW,
            };
            use windows::core::w;
            // Tauri window title
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
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
                return;
            }
        }

        let mut candidates = Vec::new();
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                candidates.push(parent.join("wallmus-ui.exe"));
                candidates.push(parent.join("wallmus").join("wallmus-ui.exe"));
            }
        }
        candidates.push(std::path::PathBuf::from("dist/wallmus/wallmus-ui.exe"));
        candidates.push(std::path::PathBuf::from("dist/wallmus-ui.exe"));
        candidates.push(std::path::PathBuf::from("target/release/wallmus-ui.exe"));
        candidates.push(std::path::PathBuf::from("target/debug/wallmus-ui.exe"));

        for p in candidates {
            if p.exists() {
                info!("Launching UI process on Default desktop: {}", p.display());
                if let Err(e) = desktop_host::spawn_on_default_desktop(&p, &[]) {
                    warn!("spawn_on_default_desktop failed ({e}), falling back to Command::spawn");
                    let _ = std::process::Command::new(p).spawn();
                }
                return;
            }
        }
        warn!("wallmus-ui.exe not found in candidates");
    });
}
