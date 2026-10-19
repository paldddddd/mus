//! Render abstraction and implementations for static images and video live wallpapers.

pub mod crossfade;
pub mod mpv;
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
}

pub use crossfade::{ActiveLayer, DualLayerHost};
pub use mpv::{get_mpv_version, MpvRenderer};
pub use static_image::StaticImageRenderer;
