use anyhow::Result;
use common::paths::covers_dir;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use tracing::debug;
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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
struct TimelineInterpolator {
    track_key: String,
    last_raw_pos_ms: Option<u64>,
    anchor_pos_ms: u64,
    anchor_instant: Instant,
    was_playing: bool,
}

impl Default for TimelineInterpolator {
    fn default() -> Self {
        Self {
            track_key: String::new(),
            last_raw_pos_ms: None,
            anchor_pos_ms: 0,
            anchor_instant: Instant::now(),
            was_playing: false,
        }
    }
}

impl TimelineInterpolator {
    fn current_pos_ms(&self, duration_ms: Option<u64>) -> u64 {
        let mut pos = if self.was_playing {
            self.anchor_pos_ms
                .saturating_add(self.anchor_instant.elapsed().as_millis() as u64)
        } else {
            self.anchor_pos_ms
        };
        if let Some(dur) = duration_ms {
            if dur > 0 {
                pos = pos.min(dur);
            }
        }
        pos
    }

    fn update_from_poll(
        &mut self,
        track_key: &str,
        raw_pos_ms: Option<u64>,
        duration_ms: Option<u64>,
        last_updated_elapsed_ms: Option<u64>,
        is_playing: bool,
    ) {
        if self.track_key != track_key {
            // New track started or first time seeing this track
            self.track_key = track_key.to_string();
            self.last_raw_pos_ms = raw_pos_ms;
            let base = raw_pos_ms.unwrap_or(0);
            let max_dur = duration_ms.unwrap_or(600_000).max(1);
            if is_playing {
                if let Some(el) = last_updated_elapsed_ms {
                    if el <= max_dur && base.saturating_add(el) <= max_dur {
                        self.anchor_pos_ms = base.saturating_add(el);
                    } else {
                        self.anchor_pos_ms = base;
                    }
                } else {
                    self.anchor_pos_ms = base;
                }
            } else {
                self.anchor_pos_ms = base;
            }
            self.anchor_instant = Instant::now();
            self.was_playing = is_playing;
            return;
        }

        if raw_pos_ms != self.last_raw_pos_ms {
            // Player explicitly updated timeline position (seek, pause, or periodic sync)
            self.last_raw_pos_ms = raw_pos_ms;
            self.anchor_pos_ms = raw_pos_ms.unwrap_or(0);
            self.anchor_instant = Instant::now();
        } else if self.was_playing && !is_playing {
            // Transitioned from Playing -> Paused without SMTC raw position changing
            self.anchor_pos_ms = self.current_pos_ms(duration_ms);
            self.anchor_instant = Instant::now();
        } else if !self.was_playing && is_playing {
            // Transitioned from Paused -> Playing
            self.anchor_instant = Instant::now();
        }

        self.was_playing = is_playing;
    }
}

pub struct SmtcManager {
    cached_track: Arc<Mutex<Option<TrackMetadata>>>,
    interpolator: Arc<Mutex<TimelineInterpolator>>,
    should_stop: Arc<AtomicBool>,
    _worker: Option<JoinHandle<()>>,
}

impl SmtcManager {
    pub fn new() -> Self {
        let cached_track = Arc::new(Mutex::new(None));
        let interpolator = Arc::new(Mutex::new(TimelineInterpolator::default()));
        let should_stop = Arc::new(AtomicBool::new(false));

        let cache_clone = cached_track.clone();
        let interp_clone = interpolator.clone();
        let stop_clone = should_stop.clone();

        let worker = Some(
            thread::Builder::new()
                .name("wallmus-smtc-poller".to_string())
                .spawn(move || {
                    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
                    unsafe {
                        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    }

                    let mut mgr_opt: Option<GlobalSystemMediaTransportControlsSessionManager> = None;

                    while !stop_clone.load(Ordering::Relaxed) {
                        if mgr_opt.is_none() {
                            if let Ok(op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
                                if let Ok(mgr) = op.get() {
                                    mgr_opt = Some(mgr);
                                }
                            }
                        }

                        if let Some(ref mgr) = mgr_opt {
                            let track_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                Self::fetch_track_from_session(mgr, &interp_clone)
                            }));
                            match track_res {
                                Ok(track) => {
                                    let mut g = cache_clone.lock().unwrap_or_else(|e| e.into_inner());
                                    *g = track;
                                }
                                Err(_) => {
                                    debug!("SMTC track polling caught panic/unwind; resetting session manager");
                                    mgr_opt = None;
                                }
                            }
                        }
                        thread::sleep(Duration::from_millis(300));
                    }
                })
                .expect("Failed to spawn SMTC poller thread"),
        );

        Self {
            cached_track,
            interpolator,
            should_stop,
            _worker: worker,
        }
    }

    pub fn get_current_track(&self) -> Result<Option<TrackMetadata>> {
        let mut maybe_track = self.cached_track.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(ref mut t) = maybe_track {
            let interp = self.interpolator.lock().unwrap_or_else(|e| e.into_inner());
            t.position_ms = Some(interp.current_pos_ms(t.duration_ms));
        }
        Ok(maybe_track)
    }

    fn fetch_track_from_session(
        mgr: &GlobalSystemMediaTransportControlsSessionManager,
        interpolator: &Arc<Mutex<TimelineInterpolator>>,
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

        let (raw_pos_ms, duration_ms, last_updated_elapsed_ms) =
            if let Ok(timeline) = session.GetTimelineProperties() {
                let pos = timeline
                    .Position()
                    .ok()
                    .map(|p| (p.Duration.max(0) / 10_000) as u64);
                let end = timeline
                    .EndTime()
                    .ok()
                    .map(|e| (e.Duration.max(0) / 10_000) as u64);
                let elapsed = timeline.LastUpdatedTime().ok().and_then(|dt| {
                    if dt.UniversalTime <= 0 {
                        return None;
                    }
                    // WinRT UniversalTime is 100ns intervals since 1601-01-01 UTC
                    let now_unix_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .ok()?
                        .as_millis() as i64;
                    let last_unix_ms = (dt.UniversalTime / 10_000) - 11_644_473_600_000i64;
                    let diff = now_unix_ms - last_unix_ms;
                    if diff >= 0 && diff <= 3_600_000 {
                        Some(diff as u64)
                    } else {
                        None
                    }
                });
                (pos, end, elapsed)
            } else {
                (None, None, None)
            };

        let track_key = format!("{}|{}", artist, title);
        let is_playing = playback_status == PlaybackStatus::Playing;
        let position_ms = {
            let mut interp = interpolator.lock().unwrap();
            interp.update_from_poll(
                &track_key,
                raw_pos_ms,
                duration_ms,
                last_updated_elapsed_ms,
                is_playing,
            );
            Some(interp.current_pos_ms(duration_ms))
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
        if size == 0 || size > 10 * 1024 * 1024 {
            return None;
        }

        let reader = DataReader::CreateDataReader(&stream).ok()?;
        reader.LoadAsync(size as u32).ok()?.get().ok()?;
        let mut buf = vec![0u8; size];
        reader.ReadBytes(&mut buf).ok()?;

        // Generate filename based on title + artist hash
        let hash = format!("{:x}", md5_hash(format!("{} - {}", title, artist).as_bytes()));
        let tmp_file = covers_dir().join(format!("smtc_{hash}.tmp"));
        let cover_file = covers_dir().join(format!("smtc_{hash}.jpg"));

        let should_write = match std::fs::metadata(&cover_file) {
            Ok(meta) => buf.len() > meta.len() as usize + 2048,
            Err(_) => true,
        };

        if should_write {
            if let Ok(mut f) = File::create(&tmp_file) {
                if f.write_all(&buf).is_ok() {
                    let _ = f.flush();
                    drop(f);
                    let _ = std::fs::rename(&tmp_file, &cover_file);
                }
            }
        }

        Some(cover_file.to_string_lossy().to_string())
    }

    pub fn play(&self) -> Result<bool> {
        Self::dispatch_session_action(|s| {
            if let Ok(op) = s.TryPlayAsync() {
                let _ = op.get();
            }
            Ok(true)
        })
    }

    pub fn pause(&self) -> Result<bool> {
        Self::dispatch_session_action(|s| {
            if let Ok(op) = s.TryPauseAsync() {
                let _ = op.get();
            }
            Ok(true)
        })
    }

    pub fn toggle_play_pause(&self) -> Result<bool> {
        Self::dispatch_session_action(|s| {
            if let Ok(op) = s.TryTogglePlayPauseAsync() {
                let _ = op.get();
            }
            Ok(true)
        })
    }

    pub fn skip_next(&self) -> Result<bool> {
        Self::dispatch_session_action(|s| {
            if let Ok(op) = s.TrySkipNextAsync() {
                let _ = op.get();
            }
            Ok(true)
        })
    }

    pub fn skip_previous(&self) -> Result<bool> {
        Self::dispatch_session_action(|s| {
            if let Ok(op) = s.TrySkipPreviousAsync() {
                let _ = op.get();
            }
            Ok(true)
        })
    }

    pub fn seek(&self, position_ms: u64) -> Result<bool> {
        {
            let mut interp = self.interpolator.lock().unwrap_or_else(|e| e.into_inner());
            interp.anchor_pos_ms = position_ms;
            interp.anchor_instant = Instant::now();
        }
        let ticks = (position_ms as i64).saturating_mul(10_000);
        Self::dispatch_session_action(move |s| {
            if let Ok(op) = s.TryChangePlaybackPositionAsync(ticks) {
                let _ = op.get();
            }
            Ok(true)
        })
    }

    fn dispatch_session_action<F>(action: F) -> Result<bool>
    where
        F: FnOnce(&GlobalSystemMediaTransportControlsSession) -> Result<bool> + Send + 'static,
    {
        std::thread::spawn(move || {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            if let Ok(op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
                if let Ok(mgr) = op.get() {
                    if let Ok(session) = mgr.GetCurrentSession() {
                        let _ = action(&session);
                    }
                }
            }
        });
        Ok(true)
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
