//! Media probing, thumbnail generation, and video transcoding via FFmpeg/FFprobe.

pub mod importer;

pub use importer::{
    find_tool, generate_thumbnail, import_wallpaper, probe_media, transcode_animated_to_mp4,
    MediaProbe,
};
