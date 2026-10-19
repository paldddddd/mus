use serde::{Deserialize, Serialize};

/// Wallpaper kind representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallpaperKind {
    Image,
    Animated,
    Video,
    Html,
    Shader,
}

impl WallpaperKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Animated => "animated",
            Self::Video => "video",
            Self::Html => "html",
            Self::Shader => "shader",
        }
    }
}

impl std::str::FromStr for WallpaperKind {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "animated" => Self::Animated,
            "video" => Self::Video,
            "html" => Self::Html,
            "shader" => Self::Shader,
            _ => Self::Image,
        })
    }
}

/// Stored wallpaper metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperItem {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source_path: String,
    pub processed_path: String,
    pub thumb_path: String,
    pub fit: String,
    pub fps_cap: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub created_at: i64,
}

/// Track stored in the music library. `id` is the blake3-based track_id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackItem {
    pub id: String,
    pub path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub cover_path: Option<String>,
    pub added_at: i64,
}

impl TrackItem {
    /// Title for display: tag title or file stem.
    pub fn display_title(&self) -> String {
        if let Some(t) = self.title.as_ref().filter(|t| !t.trim().is_empty()) {
            return t.clone();
        }
        std::path::Path::new(&self.path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.clone())
    }
}

/// Playlist with ordered track ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: i64,
    pub name: String,
    pub track_ids: Vec<String>,
}

/// Scope of a music-to-wallpaper binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingScope {
    Track,
    Album,
    Artist,
    Playlist,
    External,
}

impl BindingScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Album => "album",
            Self::Artist => "artist",
            Self::Playlist => "playlist",
            Self::External => "external",
        }
    }
}

impl std::str::FromStr for BindingScope {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "track" => Ok(Self::Track),
            "album" => Ok(Self::Album),
            "artist" => Ok(Self::Artist),
            "playlist" => Ok(Self::Playlist),
            "external" => Ok(Self::External),
            other => anyhow::bail!("Invalid binding scope: '{}'", other),
        }
    }
}

/// Single binding mapping a track/album/artist/playlist/external key to a wallpaper ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingItem {
    pub id: Option<i64>,
    pub scope: BindingScope,
    pub key: String,
    pub wallpaper_id: String,
}
