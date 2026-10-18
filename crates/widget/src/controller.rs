use crate::window::{ActionCallback, PositionCallback, WidgetWindow};
use anyhow::Result;
use audio_engine::AudioEngine;
use common::WidgetConfig;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct WidgetController {
    window: WidgetWindow,
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
            last_render: Instant::now(),
            last_track_fetch: Instant::now() - Duration::from_secs(5),
            cached_track: None,
        })
    }

    pub fn update(&mut self, is_paused: bool) {
        // If Governor paused the system, do not render to save CPU & battery
        if is_paused {
            return;
        }

        let now = Instant::now();

        // Refresh track metadata every 500ms
        if now.duration_since(self.last_track_fetch) >= Duration::from_millis(500) {
            self.last_track_fetch = now;
            self.cached_track = self.audio.smtc.get_current_track().unwrap_or(None);
            if self.cached_track.is_none() {
                if let Some(local) = self.audio.player.current_track() {
                    self.cached_track = Some(audio_engine::TrackMetadata {
                        title: local.title,
                        artist: local.artist,
                        album_artist: String::new(),
                        album_title: String::new(),
                        playback_status: if self.audio.player.is_playing() {
                            audio_engine::PlaybackStatus::Playing
                        } else {
                            audio_engine::PlaybackStatus::Paused
                        },
                        source_app: "wallmus".to_string(),
                        position_ms: None,
                        duration_ms: Some(local.duration_ms),
                        cover_path: None,
                    });
                }
            }
        }

        // Render at ~30 FPS (every 33ms)
        if now.duration_since(self.last_render) >= Duration::from_millis(33) {
            self.last_render = now;
            let bars = {
                let cfg = self.config.lock().unwrap();
                cfg.visualizer_bars as usize
            };
            let spectrum = self.audio.visualizer.get_spectrum(bars);
            self.window.render_frame(self.cached_track.as_ref(), &spectrum);
        }
    }

    pub fn set_theme(&mut self, theme: &str) {
        let mut cfg = self.config.lock().unwrap();
        cfg.theme = theme.to_string();
    }
}
