//! Render abstraction and implementations for static images and video live wallpapers.

pub mod crossfade;
pub mod html;
pub mod mpv;
pub mod shader;
pub mod static_image;

use anyhow::Result;

/// Core interface for live wallpaper rendering on a single monitor.
pub trait Renderer: Send {
    /// Load a media source (file path, URI, etc.).
    fn load(&mut self, source: &str) -> Result<()>;

    /// Pause rendering / playback.
    fn pause(&mut self) -> Result<()>;

    /// Resume rendering / playback.
    fn resume(&mut self) -> Result<()>;

    /// Update target render dimensions.
    fn resize(&mut self, width: i32, height: i32) -> Result<()>;

    /// Unload resources and release memory.
    fn unload(&mut self) -> Result<()>;

    /// Update interactive cursor position (normalized 0.0 .. 1.0) if supported by renderer.
    fn update_mouse(&mut self, _x: f32, _y: f32) {}

    /// Update audio bass energy / kick (0.0 .. 1.0) for rhythmic bounce/zoom effects.
    fn update_bass(&mut self, _bass: f32) {}
}

pub use crossfade::{ActiveLayer, DualLayerHost};
pub use html::HtmlRenderer;
pub use mpv::{get_mpv_version, MpvRenderer};
pub use shader::{ShaderRenderer, ShaderUniforms, DEFAULT_WGSL_AURORA_WAVES, DEFAULT_WGSL_CYBER_TUNNEL};
pub use static_image::StaticImageRenderer;
