use crate::paths::{config_path, ensure_app_dirs};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

/// Governor auto-pause policy settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernorConfig {
    pub enabled: bool,
    pub pause_on_fullscreen: bool,
    pub pause_on_desktop_covered: bool,
    pub pause_on_lock: bool,
    pub pause_on_display_off: bool,
    pub pause_on_battery: bool,
    pub unload_after_minutes: u32,
}

impl Default for GovernorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pause_on_fullscreen: true,
            pause_on_desktop_covered: true,
            pause_on_lock: true,
            pause_on_display_off: true,
            pause_on_battery: false,
            unload_after_minutes: 5,
        }
    }
}

/// Desktop music widget settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetConfig {
    pub enabled: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub theme: String, // "spotify", "pill", "vinyl", "glass"
    pub pinned: bool,
    pub interactive: bool,
    pub visualizer_bars: u32,
    pub opacity: f32,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            // Native widget is an experiment outside the spec; keep it off until the
            // Stage 4 RSS measurement decides between Tauri and native widget.
            enabled: false,
            x: 80,
            y: 80,
            width: 380,
            height: 150,
            scale: 1.0,
            theme: "spotify".to_string(),
            pinned: false,
            interactive: true,
            visualizer_bars: 24,
            opacity: 0.95,
        }
    }
}

/// Which music source drives track events. Only one is active at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MusicSource {
    /// Built-in player of local files.
    #[default]
    Local,
    /// External players via Windows SMTC (Spotify, browser, ...).
    External,
}

/// Music library settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MusicConfig {
    /// Folders scanned recursively and watched for changes.
    #[serde(default)]
    pub folders: Vec<String>,
    /// Active track source. External source is off by default.
    #[serde(default)]
    pub source: MusicSource,
}

/// Global application configuration stored in `%APPDATA%/wallmus/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub autostart: bool,
    pub language: String,
    pub theme: String,
    pub follow_music: bool,
    pub active_wallpaper_id: Option<String>,
    pub fps_limit: u32,
    pub governor: GovernorConfig,
    #[serde(default)]
    pub widget: WidgetConfig,
    #[serde(default)]
    pub music: MusicConfig,
    #[serde(default = "default_crossfade_duration")]
    pub crossfade_duration_ms: u64,
}

fn default_crossfade_duration() -> u64 {
    500
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            autostart: false,
            language: "ru".to_string(),
            theme: "dark".to_string(),
            follow_music: true,
            active_wallpaper_id: None,
            fps_limit: 30,
            crossfade_duration_ms: default_crossfade_duration(),
            governor: GovernorConfig::default(),
            widget: WidgetConfig::default(),
            music: MusicConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load config from file or return default.
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str(&content) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    /// Save configuration to `%APPDATA%/wallmus/config.toml`.
    pub fn save(&self) -> Result<()> {
        ensure_app_dirs()?;
        let path = config_path();
        let content = toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;
        fs::write(path, content).context("Failed to write config.toml")?;
        Ok(())
    }
}
