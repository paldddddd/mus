pub mod player;
pub mod smtc;
pub mod visualizer;
pub mod wallpaper_audio;

pub use player::{LocalPlayer, LocalTrack, RepeatMode};
pub use smtc::{PlaybackStatus, SmtcManager, TrackMetadata};
pub use visualizer::Visualizer;
pub use wallpaper_audio::{WallpaperAudio, WallpaperAudioConfig};

use std::sync::Arc;

pub struct AudioEngine {
    pub smtc: Arc<SmtcManager>,
    pub visualizer: Arc<Visualizer>,
    pub player: Arc<LocalPlayer>,
    pub wallpaper_audio: Arc<WallpaperAudio>,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            smtc: Arc::new(SmtcManager::new()),
            visualizer: Arc::new(Visualizer::new()),
            player: Arc::new(LocalPlayer::new()),
            wallpaper_audio: Arc::new(WallpaperAudio::default()),
        }
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}
