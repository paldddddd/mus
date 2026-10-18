mod autostart;
mod ipc;
mod single_instance;
mod tray;

use anyhow::{bail, Context, Result};
use audio_engine::AudioEngine;
use common::{logs_dir, AppConfig, WallpaperItem};
use widget::WidgetController;
use desktop_host::{
    apply_eco_profile, collect_diag, enumerate_monitors, DesktopHostWindow, RenderMode,
};
use governor::{Governor, GovernorState};
use ipc::{send_request, IpcHandler, IpcServer};
use library::Database;
use media::import_wallpaper;
use render::{MpvRenderer, Renderer, StaticImageRenderer};
use single_instance::SingleInstanceGuard;
use std::env;
use std::fs::File;
use std::io::Write;
use std::panic;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use tray::SystemTray;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
};

/// Daemon runtime controller managing wallpaper playback and IPC.
struct DaemonContext {
    db: Arc<Database>,
    config: Arc<Mutex<AppConfig>>,
    governor: Arc<Mutex<Governor>>,
    active_wallpaper: Arc<Mutex<Option<WallpaperItem>>>,
    should_quit: Arc<AtomicBool>,
    audio: Arc<AudioEngine>,
}

impl IpcHandler for DaemonContext {
    fn handle_method(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        match method {
            "app.getState" => {
                let cfg = self.config.lock().unwrap().clone();
                let active = self.active_wallpaper.lock().unwrap().clone();
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
                // In Stage 4, this launches or focuses Tauri UI
                Ok(serde_json::json!({ "status": "ui_requested" }))
            }
            "wallpapers.list" => {
                let list = self.db.list_wallpapers()?;
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
                Ok(serde_json::to_value(item)?)
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
                    let _ = self.db.set_kv("active_wallpaper_id", id);
                    Ok(serde_json::to_value(wp)?)
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
                let diag = collect_diag()?;
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
                Ok(serde_json::json!({ "enabled": enabled }))
            }
            "audio.getTrack" => {
                // Try SMTC first
                let track = self.audio.smtc.get_current_track()?;
                if let Some(t) = track {
                    return Ok(serde_json::to_value(t)?);
                }

                // Check local player
                if let Some(local_t) = self.audio.player.current_track() {
                    return Ok(serde_json::json!({
                        "title": local_t.title,
                        "artist": local_t.artist,
                        "album_title": "",
                        "album_artist": "",
                        "playback_status": if self.audio.player.is_playing() { "Playing" } else { "Paused" },
                        "source_app": "wallmus",
                        "position_ms": null,
                        "duration_ms": local_t.duration_ms,
                        "cover_path": null,
                    }));
                }

                Ok(serde_json::Value::Null)
            }
            "audio.control" => {
                let action = params
                    .get("action")
                    .and_then(|v| v.as_str())
                    .context("Missing 'action' parameter (play/pause/toggle/next/previous)")?;

                let res = match action {
                    "play" => {
                        if self.audio.player.is_playing() {
                            self.audio.player.play();
                            true
                        } else {
                            self.audio.smtc.play().unwrap_or(false)
                        }
                    }
                    "pause" => {
                        if self.audio.player.is_playing() {
                            self.audio.player.pause();
                            true
                        } else {
                            self.audio.smtc.pause().unwrap_or(false)
                        }
                    }
                    "toggle" => {
                        if self.audio.player.is_playing() {
                            self.audio.player.toggle_play_pause();
                            true
                        } else {
                            self.audio.smtc.toggle_play_pause().unwrap_or(false)
                        }
                    }
                    "next" => {
                        if self.audio.player.is_playing() {
                            let _ = self.audio.player.next();
                            true
                        } else {
                            self.audio.smtc.skip_next().unwrap_or(false)
                        }
                    }
                    "previous" => {
                        if self.audio.player.is_playing() {
                            let _ = self.audio.player.previous();
                            true
                        } else {
                            self.audio.smtc.skip_previous().unwrap_or(false)
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
            "player.playFile" => {
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .context("Missing 'path' parameter")?;
                let title = params
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let artist = params
                    .get("artist")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();

                self.audio.player.play_file(Path::new(path), title, artist)?;
                Ok(serde_json::json!({ "status": "playing" }))
            }
            "player.getState" => {
                Ok(serde_json::json!({
                    "is_playing": self.audio.player.is_playing(),
                    "volume": self.audio.player.get_volume(),
                    "repeat_mode": format!("{:?}", self.audio.player.get_repeat_mode()),
                    "shuffle": self.audio.player.is_shuffle(),
                    "current_track": self.audio.player.current_track(),
                }))
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
                let mut cfg = self.config.lock().unwrap();
                cfg.widget.theme = theme.to_string();
                let _ = cfg.save();
                Ok(serde_json::json!({ "theme": theme }))
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
                let mut cfg = self.config.lock().unwrap();
                cfg.widget.x = x;
                cfg.widget.y = y;
                let _ = cfg.save();
                Ok(serde_json::json!({ "x": x, "y": y }))
            }
            "widget.setEnabled" => {
                let enabled = params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .context("Missing 'enabled' parameter")?;
                let mut cfg = self.config.lock().unwrap();
                cfg.widget.enabled = enabled;
                let _ = cfg.save();
                Ok(serde_json::json!({ "enabled": enabled }))
            }
            _ => bail!("Unknown IPC method: {}", method),
        }
    }
}

fn main() -> Result<()> {
    // 0. Ensure main thread is bound to WinSta0\Default desktop before creating any windows or hooks
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

    let args: Vec<String> = env::args().collect();

    // CLI Commands
    if args.iter().any(|arg| arg == "--diag") {
        return run_diag();
    }
    if args.iter().any(|arg| arg == "--selftest") {
        return run_selftest(10);
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--image") {
        if let Some(image_path) = args.get(pos + 1) {
            return run_image_test(image_path, 10);
        }
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--video") {
        if let Some(video_path) = args.get(pos + 1) {
            return run_video_test(video_path, 10);
        }
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--import") {
        if let Some(file_path) = args.get(pos + 1) {
            return run_cli_import(file_path);
        }
    }
    if args.iter().any(|arg| arg == "--metrics") {
        return run_cli_metrics();
    }
    if args.iter().any(|arg| arg == "--pause") {
        return run_cli_pause(true);
    }
    if args.iter().any(|arg| arg == "--resume") {
        return run_cli_pause(false);
    }
    if args.iter().any(|arg| arg == "--track") {
        return run_cli_track();
    }
    if args.iter().any(|arg| arg == "--spectrum") {
        return run_cli_spectrum();
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--control") {
        if let Some(action) = args.get(pos + 1) {
            return run_cli_control(action);
        }
    }
    if let Some(pos) = args.iter().position(|arg| arg == "--widget-theme") {
        if let Some(theme) = args.get(pos + 1) {
            return run_cli_widget_theme(theme);
        }
    }
    if args.iter().any(|arg| arg == "--quit" || arg == "--exit" || arg == "--stop") {
        return run_cli_quit();
    }

    // 3. Single Instance Guard
    let single_instance = SingleInstanceGuard::acquire("Local\\wallmus_SingleInstanceMutex")
        .context("Failed to evaluate single instance mutex")?;

    if !single_instance.is_primary() {
        println!("wallmus is already running in the background.");
        println!("Focusing running application...");
        let _ = send_request("app.openUi", serde_json::Value::Null);
        return Ok(());
    }

    // 4. Primary daemon execution
    run_daemon()
}

fn run_daemon() -> Result<()> {
    info!("Starting wallmus daemon (Stage 1)...");

    // Apply process profile: Below Normal priority + EcoQoS
    if let Err(e) = apply_eco_profile() {
        warn!("Could not apply eco process profile: {e}");
    }

    let config = Arc::new(Mutex::new(AppConfig::load()));
    let db = Arc::new(Database::open_default().context("Failed to open SQLite database")?);

    // Get active wallpaper or use first in database
    let wallpapers = db.list_wallpapers().unwrap_or_default();
    let initial_wp = wallpapers.first().cloned();
    let active_wallpaper = Arc::new(Mutex::new(initial_wp));

    let should_quit = Arc::new(AtomicBool::new(false));

    // Create Desktop Host Windows for active monitors FIRST (before installing hooks or tray)
    let monitors = enumerate_monitors().context("Failed to enumerate monitors")?;
    let mut hosts = Vec::new();
    let mut renderers: Vec<Box<dyn Renderer>> = Vec::new();

    let our_pid = std::process::id();

    for monitor in &monitors {
        let mut host = DesktopHostWindow::create(monitor.clone(), RenderMode::Video)
            .context("Failed to create DesktopHostWindow")?;

        let active_wp_guard = active_wallpaper.lock().unwrap();
        if let Some(ref wp) = *active_wp_guard {
            match wp.kind.as_str() {
                "video" => {
                    if let Ok(mut mpv) =
                        MpvRenderer::new(host.hwnd.0 as usize, monitor.width, monitor.height)
                    {
                        let _ = mpv.load(&wp.processed_path);
                        renderers.push(Box::new(mpv));
                    }
                }
                _ => {
                    let mut img_rend =
                        StaticImageRenderer::new(host.hwnd, monitor.width, monitor.height);
                    let _ = img_rend.load(&wp.processed_path);
                    let img_arc = Arc::new(Mutex::new(img_rend));
                    host.set_paint_callback(move |hdc, _w, _h| {
                        if let Ok(g) = img_arc.lock() {
                            g.paint(hdc);
                        }
                    });
                }
            }
        }
        hosts.push(host);
    }

    // Initialize Governor
    let gov_config = config.lock().unwrap().governor.clone();
    let mut governor = Governor::new(gov_config);
    if let Err(e) = governor.start_hooks() {
        warn!("Governor hooks could not be started: {e}");
    }
    let governor_arc = Arc::new(Mutex::new(governor));
    let audio = Arc::new(AudioEngine::new());

    let ctx = Arc::new(DaemonContext {
        db: db.clone(),
        config: config.clone(),
        governor: governor_arc.clone(),
        active_wallpaper: active_wallpaper.clone(),
        should_quit: should_quit.clone(),
        audio: audio.clone(),
    });

    // Start IPC Server
    let ipc_server = IpcServer::start(ctx.clone()).context("Failed to start IPC server")?;
    info!("IPC server listening on {}", ipc::PIPE_NAME);

    // Create System Tray
    let tray = SystemTray::create(false).context("Failed to create system tray")?;
    info!("System tray initialized");

    // Initialize Desktop Music Widget
    let widget_cfg = Arc::new(Mutex::new(config.lock().unwrap().widget.clone()));
    let audio_cb = audio.clone();
    let on_action: widget::ActionCallback = Arc::new(move |action: &str| {
        info!("Widget action clicked: {action}");
        match action {
            "toggle" => {
                if audio_cb.player.is_playing() {
                    audio_cb.player.toggle_play_pause();
                } else {
                    let _ = audio_cb.smtc.toggle_play_pause();
                }
            }
            "previous" => {
                if audio_cb.player.is_playing() {
                    let _ = audio_cb.player.previous();
                } else {
                    let _ = audio_cb.smtc.skip_previous();
                }
            }
            "next" => {
                if audio_cb.player.is_playing() {
                    let _ = audio_cb.player.next();
                } else {
                    let _ = audio_cb.smtc.skip_next();
                }
            }
            _ => {}
        }
    });

    let config_cb = config.clone();
    let on_moved: widget::PositionCallback = Arc::new(move |x: i32, y: i32| {
        let mut cfg = config_cb.lock().unwrap();
        cfg.widget.x = x;
        cfg.widget.y = y;
        let _ = cfg.save();
        info!("Widget moved to ({x}, {y}) - config saved");
    });

    let mut widget_controller = if widget_cfg.lock().unwrap().enabled {
        match WidgetController::new(widget_cfg.clone(), audio.clone(), Some(on_action), Some(on_moved)) {
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

    info!("wallmus daemon running. Entering main event loop...");

    let mut last_gov_check = Instant::now();
    let mut last_state = GovernorState::Running;

    unsafe {
        let mut msg = MSG::default();

        while !should_quit.load(Ordering::SeqCst) {
            // Process Win32 messages
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    should_quit.store(true, Ordering::SeqCst);
                    break;
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
                            for r in &mut renderers {
                                let _ = r.load(&next_wp.processed_path);
                            }
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

            if gov_triggered || last_gov_check.elapsed() >= Duration::from_millis(1000) {
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
                                for r in &mut renderers {
                                    let _ = r.pause();
                                }
                                audio.visualizer.set_paused(true);
                                audio.wallpaper_audio.fade_out();
                                tray.update_state(true);
                                info!("Paused rendering due to: {:?}", reason);
                            }
                            GovernorState::Running => {
                                for r in &mut renderers {
                                    let _ = r.resume();
                                }
                                audio.visualizer.set_paused(false);
                                audio.wallpaper_audio.fade_in();
                                tray.update_state(false);
                                info!("Resumed rendering");
                            }
                            GovernorState::Unloaded => {
                                for r in &mut renderers {
                                    let _ = r.unload();
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

            // Update widget window frame
            if let Some(ref mut w) = widget_controller {
                let is_paused = last_state != GovernorState::Running;
                w.update(is_paused);
            }

            std::thread::sleep(Duration::from_millis(20));
        }
    }

    info!("Shutting down wallmus daemon...");
    ipc_server.stop();
    drop(renderers);
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

    println!("============================================================");
    println!("                  WALLMUS DIAGNOSTIC REPORT                 ");
    println!("============================================================");
    println!("OS Product Name : {}", diag.os_product_name);
    println!("OS Build        : {}", diag.os_build);
    println!("Shell Topology  : {:?}", diag.topology);
    println!(
        "GPU Adapter     : {}",
        diag.gpu_adapter.as_deref().unwrap_or("Unknown")
    );
    println!(
        "libmpv Version  : {}",
        diag.libmpv_version.as_deref().unwrap_or("Not loaded")
    );
    println!("\nActive Monitors ({}):", diag.monitors.len());
    for (i, m) in diag.monitors.iter().enumerate() {
        println!(
            "  [{}] {} | Primary: {} | Rect: [{}, {} to {}, {}] ({}x{}) | DPI: {}x{} | Scale: {:.2}x",
            i, m.name, m.is_primary, m.x, m.y, m.x + m.width, m.y + m.height, m.width, m.height, m.dpi_x, m.dpi_y, m.scale_factor
        );
    }
    println!("============================================================");

    let json_data = serde_json::to_string_pretty(&diag)?;
    let mut file = File::create("diag_report.json")?;
    file.write_all(json_data.as_bytes())?;
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
