use anyhow::{Context, Result};
use common::paths::covers_dir;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use tracing::{debug, warn};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::DataReader;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackStatus {
    Closed,
    Opened,
    Changing,
    Stopped,
    Playing,
    Paused,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album_title: String,
    pub playback_status: PlaybackStatus,
    pub source_app: String,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub cover_path: Option<String>,
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct SmtcManager {
    manager: Option<GlobalSystemMediaTransportControlsSessionManager>,
    cached_track: Arc<Mutex<Option<TrackMetadata>>>,
    should_stop: Arc<AtomicBool>,
    _worker: Option<JoinHandle<()>>,
}

impl SmtcManager {
    pub fn new() -> Self {
        let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            Ok(op) => match op.get() {
                Ok(mgr) => Some(mgr),
                Err(e) => {
                    warn!("Failed to get SMTC SessionManager: {e}");
                    None
                }
            },
            Err(e) => {
                warn!("Failed to request SMTC SessionManager: {e}");
                None
            }
        };

        let cached_track = Arc::new(Mutex::new(None));
        let should_stop = Arc::new(AtomicBool::new(false));

        let worker = if let Some(ref mgr) = manager {
            let mgr_clone = mgr.clone();
            let cache_clone = cached_track.clone();
            let stop_clone = should_stop.clone();

            Some(
                thread::Builder::new()
                    .name("wallmus-smtc-poller".to_string())
                    .spawn(move || {
                        while !stop_clone.load(Ordering::Relaxed) {
                            let track = Self::fetch_track_from_session(&mgr_clone);
                            *cache_clone.lock().unwrap() = track;
                            thread::sleep(Duration::from_millis(300));
                        }
                    })
                    .expect("Failed to spawn SMTC poller thread"),
            )
        } else {
            None
        };

        Self {
            manager,
            cached_track,
            should_stop,
            _worker: worker,
        }
    }

    pub fn get_current_track(&self) -> Result<Option<TrackMetadata>> {
        Ok(self.cached_track.lock().unwrap().clone())
    }

    fn fetch_track_from_session(
        mgr: &GlobalSystemMediaTransportControlsSessionManager,
    ) -> Option<TrackMetadata> {
        let session = mgr.GetCurrentSession().ok()?;

        let source_app = session
            .SourceAppUserModelId()
            .map(|s| s.to_string())
            .unwrap_or_default();

        let playback_status = match session.GetPlaybackInfo() {
            Ok(info) => match info.PlaybackStatus() {
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing) => PlaybackStatus::Playing,
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused) => PlaybackStatus::Paused,
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped) => PlaybackStatus::Stopped,
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Opened) => PlaybackStatus::Opened,
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Changing) => PlaybackStatus::Changing,
                Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed) => PlaybackStatus::Closed,
                _ => PlaybackStatus::Unknown,
            },
            Err(_) => PlaybackStatus::Unknown,
        };

        let media_props = match session.TryGetMediaPropertiesAsync() {
            Ok(op) => match op.get() {
                Ok(props) => props,
                Err(e) => {
                    debug!("Failed to get SMTC media properties: {e}");
                    return None;
                }
            },
            Err(_) => return None,
        };

        let title = media_props.Title().map(|s| s.to_string()).unwrap_or_default();
        let artist = media_props.Artist().map(|s| s.to_string()).unwrap_or_default();
        let album_artist = media_props.AlbumArtist().map(|s| s.to_string()).unwrap_or_default();
        let album_title = media_props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();

        let (position_ms, duration_ms) = if let Ok(timeline) = session.GetTimelineProperties() {
            let pos = timeline.Position().map(|p| (p.Duration / 10_000) as u64).ok();
            let end = timeline.EndTime().map(|e| (e.Duration / 10_000) as u64).ok();
            (pos, end)
        } else {
            (None, None)
        };

        // Extract thumbnail and save to covers_dir() if available
        let cover_path = Self::save_thumbnail(&media_props, &title, &artist);

        Some(TrackMetadata {
            title,
            artist,
            album_artist,
            album_title,
            playback_status,
            source_app,
            position_ms,
            duration_ms,
            cover_path,
        })
    }

    fn save_thumbnail(
        media_props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
        title: &str,
        artist: &str,
    ) -> Option<String> {
        let thumb_ref = media_props.Thumbnail().ok()?;
        let stream = thumb_ref.OpenReadAsync().ok()?.get().ok()?;
        let size = stream.Size().ok()? as usize;
        if size == 0 {
            return None;
        }

        let reader = DataReader::CreateDataReader(&stream).ok()?;
        reader.LoadAsync(size as u32).ok()?.get().ok()?;
        let mut buf = vec![0u8; size];
        reader.ReadBytes(&mut buf).ok()?;

        // Generate filename based on title + artist hash
        let hash = format!("{:x}", md5_hash(format!("{} - {}", title, artist).as_bytes()));
        let cover_file = covers_dir().join(format!("smtc_{hash}.jpg"));

        if !cover_file.exists() {
            if let Ok(mut f) = File::create(&cover_file) {
                let _ = f.write_all(&buf);
            }
        }

        Some(cover_file.to_string_lossy().to_string())
    }

    pub fn play(&self) -> Result<bool> {
        self.with_session(|s| {
            s.TryPlayAsync()?.get().map_err(|e| anyhow::anyhow!("{e}"))
        })
    }

    pub fn pause(&self) -> Result<bool> {
        self.with_session(|s| {
            s.TryPauseAsync()?.get().map_err(|e| anyhow::anyhow!("{e}"))
        })
    }

    pub fn toggle_play_pause(&self) -> Result<bool> {
        self.with_session(|s| {
            s.TryTogglePlayPauseAsync()?.get().map_err(|e| anyhow::anyhow!("{e}"))
        })
    }

    pub fn skip_next(&self) -> Result<bool> {
        self.with_session(|s| {
            s.TrySkipNextAsync()?.get().map_err(|e| anyhow::anyhow!("{e}"))
        })
    }

    pub fn skip_previous(&self) -> Result<bool> {
        self.with_session(|s| {
            s.TrySkipPreviousAsync()?.get().map_err(|e| anyhow::anyhow!("{e}"))
        })
    }

    fn with_session<F>(&self, action: F) -> Result<bool>
    where
        F: FnOnce(&GlobalSystemMediaTransportControlsSession) -> Result<bool>,
    {
        let mgr = self.manager.as_ref().context("SMTC SessionManager is not available")?;
        let session = mgr.GetCurrentSession().context("No active media session")?;
        action(&session)
    }
}

impl Drop for SmtcManager {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }
}

impl Default for SmtcManager {
    fn default() -> Self {
        Self::new()
    }
}

fn md5_hash(data: &[u8]) -> u128 {
    // Simple 128-bit FNV-1a hash for deterministic cover file naming
    let mut hash: u128 = 0x6c62272e07bb014262b821756295c58d;
    for &b in data {
        hash ^= b as u128;
        hash = hash.wrapping_mul(0x1000000000000000000013b);
    }
    hash
}
