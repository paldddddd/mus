use crate::window::{ActionCallback, PositionCallback, WidgetWindow};
use anyhow::Result;
use audio_engine::AudioEngine;
use common::WidgetConfig;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct WidgetController {
    window: Box<WidgetWindow>,
    audio: Arc<AudioEngine>,
    config: Arc<Mutex<WidgetConfig>>,
    last_render: Instant,
    last_track_fetch: Instant,
    cached_track: Option<audio_engine::TrackMetadata>,
}

impl WidgetController {
    pub fn new(
        config: Arc<Mutex<WidgetConfig>>,
        audio: Arc<AudioEngine>,
        on_action: Option<ActionCallback>,
        on_moved: Option<PositionCallback>,
    ) -> Result<Self> {
        let window = WidgetWindow::create(config.clone(), on_action, on_moved)?;

        Ok(Self {
            window,
            audio,
            config,
            last_render: Instant::now() - Duration::from_secs(1),
            last_track_fetch: Instant::now() - Duration::from_secs(5),
            cached_track: None,
        })
    }

    pub fn is_valid(&self) -> bool {
        self.window.is_valid()
    }

    pub fn update(&mut self, is_fullscreen_paused: bool) {
        if is_fullscreen_paused {
            return;
        }
        let now = Instant::now();

        // Refresh track metadata every 250ms, prioritizing active LocalPlayer over stale SMTC sessions
        if now.duration_since(self.last_track_fetch) >= Duration::from_millis(250) {
            self.last_track_fetch = now;
            let local_playing = self.audio.player.is_playing();
            let local_track = self.audio.player.current_track();

            if local_playing && local_track.is_some() {
                let local = local_track.unwrap();
                let pos_ms = self.audio.player.get_position().as_millis() as u64;
                self.cached_track = Some(audio_engine::TrackMetadata {
                    title: local.title,
                    artist: local.artist,
                    album_artist: String::new(),
                    album_title: local.album.unwrap_or_default(),
                    playback_status: audio_engine::PlaybackStatus::Playing,
                    source_app: "wallmus".to_string(),
                    position_ms: Some(pos_ms),
                    duration_ms: Some(local.duration_ms),
                    cover_path: local.cover_path,
                });
            } else {
                let smtc_track = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.audio.smtc.get_current_track()
                }))
                .ok()
                .and_then(|r| r.ok())
                .flatten();

                let smtc_playing = smtc_track
                    .as_ref()
                    .map(|t| t.playback_status == audio_engine::PlaybackStatus::Playing)
                    .unwrap_or(false);

                if smtc_playing {
                    self.audio.player.set_active_local(false);
                    self.cached_track = smtc_track;
                } else if self.audio.player.is_active_local() {
                    if let Some(local) = local_track {
                        let pos_ms = self.audio.player.get_position().as_millis() as u64;
                        self.cached_track = Some(audio_engine::TrackMetadata {
                            title: local.title,
                            artist: local.artist,
                            album_artist: String::new(),
                            album_title: local.album.unwrap_or_default(),
                            playback_status: if local_playing {
                                audio_engine::PlaybackStatus::Playing
                            } else {
                                audio_engine::PlaybackStatus::Paused
                            },
                            source_app: "wallmus".to_string(),
                            position_ms: Some(pos_ms),
                            duration_ms: Some(local.duration_ms),
                            cover_path: local.cover_path,
                        });
                    } else {
                        self.cached_track = smtc_track;
                    }
                } else {
                    self.cached_track = smtc_track;
                }
            }
        } else if let Some(ref mut cached) = self.cached_track {
            // Keep local player position and status updated every frame
            if cached.source_app == "wallmus" {
                cached.position_ms = Some(self.audio.player.get_position().as_millis() as u64);
                cached.playback_status = if self.audio.player.is_playing() {
                    audio_engine::PlaybackStatus::Playing
                } else {
                    audio_engine::PlaybackStatus::Paused
                };
            }
        }

        // Render at smooth ~60 FPS (every 16ms)
        if now.duration_since(self.last_render) >= Duration::from_millis(16) {
            self.last_render = now;
            let bars = {
                let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
                cfg.visualizer_bars as usize
            };

            let spectrum = self.audio.visualizer.get_spectrum(bars);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.window.render_frame(self.cached_track.as_ref(), &spectrum);
            }));
        }
    }

    pub fn apply_config(&mut self, new_cfg: &WidgetConfig) {
        let (old_x, old_y, old_w, old_h, old_pinned) = {
            let mut cfg = self.config.lock().unwrap();
            let old = (cfg.x, cfg.y, cfg.width, cfg.height, cfg.pinned);
            *cfg = new_cfg.clone();
            old
        };

        if new_cfg.pinned != old_pinned {
            self.window.set_pinned(new_cfg.pinned);
        }
        if new_cfg.width != old_w || new_cfg.height != old_h || new_cfg.width != self.window.surface.width || new_cfg.height != self.window.surface.height {
            self.window.animate_resize(new_cfg.width, new_cfg.height);
        }
        if new_cfg.x != old_x || new_cfg.y != old_y {
            self.window.set_position(new_cfg.x, new_cfg.y);
        }
        self.last_render = Instant::now() - Duration::from_secs(1);
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.pinned = pinned;
        }
        self.window.set_pinned(pinned);
    }

    pub fn set_theme(&mut self, theme: &str) {
        let (w, h) = {
            let mut cfg = self.config.lock().unwrap();
            cfg.theme = theme.to_string();
            let h = crate::painter::preferred_height(theme, cfg.width);
            cfg.height = h;
            (cfg.width, h)
        };
        self.window.animate_resize(w, h);
    }

    pub fn cycle_theme(&mut self) -> (String, u32, u32) {
        let (next_theme, w, h) = {
            let mut cfg = self.config.lock().unwrap();
            let next = match cfg.theme.as_str() {
                "spotify" => "aurora",
                "aurora" => "vinyl",
                "vinyl" => "glass",
                _ => "spotify",
            };
            cfg.theme = next.to_string();
            let h = crate::painter::preferred_height(next, cfg.width);
            cfg.height = h;
            (next.to_string(), cfg.width, h)
        };
        self.window.animate_resize(w, h);
        self.last_render = Instant::now() - Duration::from_secs(1);
        (next_theme, w, h)
    }

    pub fn cycle_size(&mut self) {
        self.window.cycle_size();
    }
}
