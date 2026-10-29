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

/// Custom user-defined widget preset stored in configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomWidgetPreset {
    pub id: String,
    pub name: String,
    pub theme: String,
    pub visualizer_style: String, // "bars", "wave", "mirror", "dots"
    pub dynamic_accent: bool,
    pub custom_accent: String, // hex like "#00F5D4"
    pub corner_radius: u32,
    pub bg_opacity: f32,
    pub show_lyrics: bool,
    #[serde(default = "default_true")]
    pub show_visualizer: bool,
    pub width: u32,
    pub height: u32,
}

fn default_widget_visualizer_style() -> String {
    "bars".to_string()
}

fn default_true() -> bool {
    true
}

fn default_custom_accent() -> String {
    "#00F5D4".to_string()
}

fn default_corner_radius() -> u32 {
    16
}

fn default_bass_pulse_intensity() -> f32 {
    0.5
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
    #[serde(default = "default_widget_visualizer_style")]
    pub visualizer_style: String, // "bars", "wave", "mirror", "dots"
    #[serde(default = "default_true")]
    pub dynamic_accent: bool,
    #[serde(default = "default_custom_accent")]
    pub custom_accent: String,
    #[serde(default = "default_corner_radius")]
    pub corner_radius: u32,
    #[serde(default = "default_true")]
    pub show_lyrics: bool,
    #[serde(default = "default_true")]
    pub show_visualizer: bool,
    #[serde(default = "default_true")]
    pub magnetic_snap: bool,
    #[serde(default)]
    pub custom_presets: Vec<CustomWidgetPreset>,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
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
            visualizer_style: default_widget_visualizer_style(),
            dynamic_accent: true,
            custom_accent: default_custom_accent(),
            corner_radius: default_corner_radius(),
            show_lyrics: true,
            show_visualizer: true,
            magnetic_snap: true,
            custom_presets: Vec::new(),
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

/// Global keyboard shortcuts configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeysConfig {
    #[serde(default = "default_hotkey_toggle_playback")]
    pub toggle_playback: String,
    #[serde(default = "default_hotkey_next_track")]
    pub next_track: String,
    #[serde(default = "default_hotkey_prev_track")]
    pub prev_track: String,
    #[serde(default = "default_hotkey_toggle_wallpaper_pause")]
    pub toggle_wallpaper_pause: String,
    #[serde(default = "default_hotkey_next_wallpaper")]
    pub next_wallpaper: String,
    #[serde(default = "default_hotkey_toggle_widget")]
    pub toggle_widget: String,
}

fn default_hotkey_toggle_playback() -> String { "Ctrl+Alt+Space".to_string() }
fn default_hotkey_next_track() -> String { "Ctrl+Alt+Right".to_string() }
fn default_hotkey_prev_track() -> String { "Ctrl+Alt+Left".to_string() }
fn default_hotkey_toggle_wallpaper_pause() -> String { "Ctrl+Alt+P".to_string() }
fn default_hotkey_next_wallpaper() -> String { "Ctrl+Alt+W".to_string() }
fn default_hotkey_toggle_widget() -> String { "Ctrl+Alt+M".to_string() }

impl Default for HotkeysConfig {
    fn default() -> Self {
        Self {
            toggle_playback: default_hotkey_toggle_playback(),
            next_track: default_hotkey_next_track(),
            prev_track: default_hotkey_prev_track(),
            toggle_wallpaper_pause: default_hotkey_toggle_wallpaper_pause(),
            next_wallpaper: default_hotkey_next_wallpaper(),
            toggle_widget: default_hotkey_toggle_widget(),
        }
    }
}

/// Live audio visualizer overlay rendered directly above desktop wallpapers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WallpaperVisualizerConfig {
    pub enabled: bool,
    pub style: String, // "bars", "wave", "glow"
    pub bars_count: u32,
    pub height_px: u32,
    pub opacity: f32,
    pub accent_color: String, // "spotify", "cyan_purple", "neon_blue", "fire"
}

impl Default for WallpaperVisualizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            style: "bars".to_string(),
            bars_count: 48,
            height_px: 160,
            opacity: 0.85,
            accent_color: "spotify".to_string(),
        }
    }
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
    #[serde(default)]
    pub hotkeys: HotkeysConfig,
    #[serde(default)]
    pub visualizer: WallpaperVisualizerConfig,
    #[serde(default = "default_mouse_interaction")]
    pub mouse_interaction: bool,
    #[serde(default = "default_bass_pulse_intensity")]
    pub bass_pulse_intensity: f32,
    #[serde(default = "default_wallpaper_change_mode")]
    pub wallpaper_change_mode: String, // "random", "sequential", "off"
    #[serde(default = "default_false")]
    pub respect_bindings_in_auto_change: bool,
}

fn default_crossfade_duration() -> u64 {
    500
}

fn default_mouse_interaction() -> bool {
    true
}

fn default_wallpaper_change_mode() -> String {
    "random".to_string()
}

fn default_false() -> bool {
    false
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            autostart: false,
            language: "ru".to_string(),
            theme: "dark".to_string(),
            follow_music: true,
            active_wallpaper_id: None,
            fps_limit: 0,
            crossfade_duration_ms: default_crossfade_duration(),
            governor: GovernorConfig::default(),
            widget: WidgetConfig::default(),
            music: MusicConfig::default(),
            hotkeys: HotkeysConfig::default(),
            visualizer: WallpaperVisualizerConfig::default(),
            mouse_interaction: default_mouse_interaction(),
            bass_pulse_intensity: default_bass_pulse_intensity(),
            wallpaper_change_mode: default_wallpaper_change_mode(),
            respect_bindings_in_auto_change: default_false(),
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
