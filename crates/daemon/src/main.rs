mod autostart;
mod ipc;
mod single_instance;
mod tray;

use anyhow::{bail, Context, Result};
use audio_engine::AudioEngine;
use common::{logs_dir, AppConfig, BindingItem, BindingScope, MusicSource, WallpaperItem};
use widget::WidgetController;
use desktop_host::{
    apply_eco_profile, collect_diag, enumerate_monitors, DesktopHostWindow, RenderMode,
};
use governor::{Governor, GovernorState};
use ipc::{send_request, IpcHandler, IpcServer};
use library::{resolve, Database, LibraryWatcher, TrackContext};
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
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY, WM_QUIT,
};

const HOTKEY_MEDIA_NEXT: i32 = 1001;
const HOTKEY_MEDIA_PREV: i32 = 1002;
const HOTKEY_MEDIA_STOP: i32 = 1003;
const HOTKEY_MEDIA_PLAY_PAUSE: i32 = 1004;

const VK_MEDIA_NEXT_TRACK: u32 = 0xB0;
const VK_MEDIA_PREV_TRACK: u32 = 0xB1;
const VK_MEDIA_STOP: u32 = 0xB2;
const VK_MEDIA_PLAY_PAUSE: u32 = 0xB3;

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
                let source = self.config.lock().unwrap().music.source;
                match source {
                    MusicSource::External => {
                        let track = self.audio.smtc.get_current_track()?;
                        if let Some(t) = track {
                            let key = common::normalize::external_track_key(&t.artist, &t.title);
                            return Ok(serde_json::json!({
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
                                "cover_path": t.cover_path,
                            }));
                        }
                        Ok(serde_json::Value::Null)
                    }
                    MusicSource::Local => {
                        if let Some(local_t) = self.audio.player.current_track() {
                            let pos_ms = self.audio.player.get_position().as_millis() as u64;
                            return Ok(serde_json::json!({
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
                                "cover_path": local_t.cover_path,
                            }));
                        }
                        Ok(serde_json::Value::Null)
                    }
                }
            }
            "audio.control" => {
                let action = params
                    .get("action")
                    .and_then(|v| v.as_str())
                    .context("Missing 'action' parameter (play/pause/toggle/next/previous)")?;
                let source = self.config.lock().unwrap().music.source;

                let res = match action {
                    "play" => {
                        if source == MusicSource::Local {
                            self.audio.player.play();
                            true
                        } else {
                            self.audio.smtc.play().unwrap_or(false)
                        }
                    }
                    "pause" => {
                        if source == MusicSource::Local {
                            self.audio.player.pause();
                            true
                        } else {
                            self.audio.smtc.pause().unwrap_or(false)
                        }
                    }
                    "toggle" => {
                        if source == MusicSource::Local {
                            self.audio.player.toggle_play_pause();
                            true
                        } else {
                            self.audio.smtc.toggle_play_pause().unwrap_or(false)
                        }
                    }
                    "next" => {
                        if source == MusicSource::Local {
                            let _ = self.audio.player.next();
                            true
                        } else {
                            self.audio.smtc.skip_next().unwrap_or(false)
                        }
                    }
                    "previous" => {
                        if source == MusicSource::Local {
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
            "library.scan" => {
                let folders = if let Some(arr) = params.get("folders").and_then(|v| v.as_array()) {
                    arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                } else {
                    self.config.lock().unwrap().music.folders.clone()
                };
                let summary = library::scanner::scan_folders(&self.db, &folders, None::<fn(library::ScanProgress)>)?;
                Ok(serde_json::to_value(summary)?)
            }
            "library.listTracks" => {
                let tracks = self.db.list_tracks()?;
                Ok(serde_json::to_value(tracks)?)
            }
            "library.getTrack" => {
                let id = params.get("id").and_then(|v| v.as_str()).context("Missing 'id'")?;
                let track = self.db.get_track(id)?;
                Ok(serde_json::to_value(track)?)
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
                self.audio.player.seek(Duration::from_millis(pos_ms))?;
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
    let active_wallpaper = Arc::new(Mutex::new(initial_wp.clone()));

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

    let ctx = Arc::new(DaemonContext {
        db: db.clone(),
        config: config.clone(),
        governor: governor_arc.clone(),
        active_wallpaper: active_wallpaper.clone(),
        should_quit: should_quit.clone(),
        audio: audio.clone(),
        watcher: watcher_arc.clone(),
        pending_track_change: pending_track_change.clone(),
    });

    // Start IPC Server
    let ipc_server = IpcServer::start(ctx.clone()).context("Failed to start IPC server")?;
    info!("IPC server listening on {}", ipc::PIPE_NAME);

    // Register global media hotkeys (VK_MEDIA_*)
    unsafe {
        let _ = RegisterHotKey(HWND::default(), HOTKEY_MEDIA_NEXT, MOD_NOREPEAT, VK_MEDIA_NEXT_TRACK);
        let _ = RegisterHotKey(HWND::default(), HOTKEY_MEDIA_PREV, MOD_NOREPEAT, VK_MEDIA_PREV_TRACK);
        let _ = RegisterHotKey(HWND::default(), HOTKEY_MEDIA_STOP, MOD_NOREPEAT, VK_MEDIA_STOP);
        let _ = RegisterHotKey(HWND::default(), HOTKEY_MEDIA_PLAY_PAUSE, MOD_NOREPEAT, VK_MEDIA_PLAY_PAUSE);
    }

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
    let mut current_wallpaper_id = initial_wp.as_ref().map(|w| w.id.clone());
    let mut last_external_poll = Instant::now();
    let mut last_external_track_key: Option<String> = None;

    unsafe {
        let mut msg = MSG::default();

        while !should_quit.load(Ordering::SeqCst) {
            // Process Win32 messages
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    should_quit.store(true, Ordering::SeqCst);
                    break;
                } else if msg.message == WM_HOTKEY {
                    let id = msg.wParam.0 as i32;
                    let source = config.lock().unwrap().music.source;
                    match id {
                        HOTKEY_MEDIA_PLAY_PAUSE => {
                            if source == MusicSource::Local {
                                audio.player.toggle_play_pause();
                            } else {
                                let _ = audio.smtc.toggle_play_pause();
                            }
                        }
                        HOTKEY_MEDIA_NEXT => {
                            if source == MusicSource::Local {
                                let _ = audio.player.next();
                            } else {
                                let _ = audio.smtc.skip_next();
                            }
                        }
                        HOTKEY_MEDIA_PREV => {
                            if source == MusicSource::Local {
                                let _ = audio.player.previous();
                            } else {
                                let _ = audio.smtc.skip_previous();
                            }
                        }
                        HOTKEY_MEDIA_STOP => {
                            if source == MusicSource::Local {
                                audio.player.stop();
                            } else {
                                let _ = audio.smtc.pause();
                            }
                        }
                        _ => {}
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

            // Check external SMTC track changes if external source is active
            if last_external_poll.elapsed() >= Duration::from_millis(250) {
                last_external_poll = Instant::now();
                let is_ext = config.lock().unwrap().music.source == MusicSource::External;
                if is_ext {
                    if let Ok(Some(ext_t)) = audio.smtc.get_current_track() {
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
                    let default_wp_id = config.lock().unwrap().active_wallpaper_id.clone();
                    let res = resolve(&track_data.as_context(), db.as_ref(), default_wp_id.as_deref());
                    if let Some(res) = res {
                        let mut active = active_wallpaper.lock().unwrap();
                        let already_active = active.as_ref().map(|a| a.id == res.wallpaper_id).unwrap_or(false);
                        if already_active {
                            info!("Bindings engine: Resolved wallpaper '{}' is already active; skipping reload", res.wallpaper_id);
                        } else {
                            match db.get_wallpaper(&res.wallpaper_id) {
                                Ok(Some(wp)) => {
                                    if Path::new(&wp.processed_path).exists() {
                                        info!("Bindings engine: applying {:?} binding -> switching to wallpaper '{}' ({})",
                                            res.matched_scope, wp.name, wp.id);
                                        *active = Some(wp.clone());
                                        let _ = db.set_kv("active_wallpaper_id", &wp.id);
                                    } else {
                                        warn!("app.warning: Bound wallpaper '{}' file '{}' missing! Falling back to default",
                                            wp.id, wp.processed_path);
                                        if let Some(ref def_id) = default_wp_id {
                                            if active.as_ref().map(|a| &a.id) != Some(def_id) {
                                                if let Ok(Some(def_wp)) = db.get_wallpaper(def_id) {
                                                    *active = Some(def_wp);
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    warn!("app.warning: Bound wallpaper '{}' not found in database! Falling back to default",
                                        res.wallpaper_id);
                                    if let Some(ref def_id) = default_wp_id {
                                        if active.as_ref().map(|a| &a.id) != Some(def_id) {
                                            if let Ok(Some(def_wp)) = db.get_wallpaper(def_id) {
                                                *active = Some(def_wp);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Sync active wallpaper to renderers
            let current_target = active_wallpaper.lock().unwrap().clone();
            if current_target.as_ref().map(|w| &w.id) != current_wallpaper_id.as_ref() {
                if let Some(ref wp) = current_target {
                    info!("Syncing active wallpaper '{}' to {} renderers", wp.name, renderers.len());
                    for (i, r) in renderers.iter_mut().enumerate() {
                        if let Err(e) = r.load(&wp.processed_path) {
                            warn!("Failed to load wallpaper on renderer {i}: {e}");
                        }
                    }
                    current_wallpaper_id = Some(wp.id.clone());
                } else {
                    for r in &mut renderers {
                        let _ = r.unload();
                    }
                    current_wallpaper_id = None;
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

    unsafe {
        let _ = UnregisterHotKey(HWND::default(), HOTKEY_MEDIA_NEXT);
        let _ = UnregisterHotKey(HWND::default(), HOTKEY_MEDIA_PREV);
        let _ = UnregisterHotKey(HWND::default(), HOTKEY_MEDIA_STOP);
        let _ = UnregisterHotKey(HWND::default(), HOTKEY_MEDIA_PLAY_PAUSE);
    }

    // Save final player state to DB KV
    let state = audio.player.get_state();
    if let Ok(json) = serde_json::to_string(&state) {
        let _ = db.set_kv("player_state", &json);
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
