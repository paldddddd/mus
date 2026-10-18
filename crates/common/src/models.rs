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
