use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub path: String,
    pub duration_ms: u64,
    pub cover_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub queue: Vec<LocalTrack>,
    pub current_index: Option<usize>,
    pub volume: f32,
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
    pub position_ms: u64,
}

pub type TrackChangeCallback = Arc<dyn Fn(&LocalTrack) + Send + Sync>;

pub struct LocalPlayer {
    _stream_running: Arc<AtomicBool>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Arc<Mutex<Option<Sink>>>,
    queue: Arc<Mutex<Vec<LocalTrack>>>,
    current_index: Arc<Mutex<Option<usize>>>,
    volume: Arc<Mutex<f32>>,
    repeat_mode: Arc<Mutex<RepeatMode>>,
    shuffle: Arc<AtomicBool>,
    on_track_changed: Arc<Mutex<Option<TrackChangeCallback>>>,
    rng_seed: Arc<Mutex<u64>>,
    is_playing_cache: Arc<AtomicBool>,
    position_ms_cache: Arc<AtomicU64>,
    active_local: Arc<AtomicBool>,
}

impl LocalPlayer {
    pub fn new() -> Self {
        let (handle_tx, handle_rx) = std::sync::mpsc::channel();
        let stream_running = Arc::new(AtomicBool::new(true));
        let running_clone = stream_running.clone();

        let _ = std::thread::Builder::new()
            .name("audio-stream".to_string())
            .spawn(move || {
                match OutputStream::try_default() {
                    Ok((stream, handle)) => {
                        let _ = handle_tx.send(Some(handle));
                        while running_clone.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        drop(stream);
                    }
                    Err(e) => {
                        warn!("Failed to initialize default audio output stream: {e}");
                        let _ = handle_tx.send(None);
                    }
                }
            });

        let stream_handle = handle_rx.recv().unwrap_or(None);

        let initial_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let is_playing_cache = Arc::new(AtomicBool::new(false));
        let position_ms_cache = Arc::new(AtomicU64::new(0));
        let active_local = Arc::new(AtomicBool::new(false));

        let player = Self {
            _stream_running: stream_running,
            stream_handle,
            sink: Arc::new(Mutex::new(None)),
            queue: Arc::new(Mutex::new(Vec::new())),
            current_index: Arc::new(Mutex::new(None)),
            volume: Arc::new(Mutex::new(0.8)),
            repeat_mode: Arc::new(Mutex::new(RepeatMode::Off)),
            shuffle: Arc::new(AtomicBool::new(false)),
            on_track_changed: Arc::new(Mutex::new(None)),
            rng_seed: Arc::new(Mutex::new(initial_seed)),
            is_playing_cache,
            position_ms_cache,
            active_local,
        };

        player.spawn_auto_advance_monitor();
        player
    }

    pub fn is_active_local(&self) -> bool {
        self.active_local.load(Ordering::SeqCst)
    }

    pub fn set_active_local(&self, active: bool) {
        self.active_local.store(active, Ordering::SeqCst);
    }

    /// Register callback invoked whenever the currently playing track changes.
    pub fn set_on_track_changed<F>(&self, cb: F)
    where
        F: Fn(&LocalTrack) + Send + Sync + 'static,
    {
        *self.on_track_changed.lock().unwrap() = Some(Arc::new(cb));
    }

    /// Background monitor that detects track completion and triggers next track.
    fn spawn_auto_advance_monitor(&self) {
        let sink_arc = self.sink.clone();
        let queue_arc = self.queue.clone();
        let current_idx_arc = self.current_index.clone();
        let repeat_arc = self.repeat_mode.clone();
        let shuffle_arc = self.shuffle.clone();
        let rng_arc = self.rng_seed.clone();
        let running_arc = self._stream_running.clone();
        let stream_handle = self.stream_handle.clone();
        let volume_arc = self.volume.clone();
        let on_change_arc = self.on_track_changed.clone();
        let is_playing_cache = self.is_playing_cache.clone();
        let position_ms_cache = self.position_ms_cache.clone();
        let active_local = self.active_local.clone();

        let _ = std::thread::Builder::new()
            .name("player-monitor".to_string())
            .spawn(move || {
                let mut had_track_playing = false;

                while running_arc.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(100));

                    // Non-blocking update of atomic status & position
                    if let Ok(sink_guard) = sink_arc.try_lock() {
                        if let Some(ref sink) = *sink_guard {
                            let playing = !sink.is_paused() && !sink.empty();
                            is_playing_cache.store(playing, Ordering::Relaxed);
                            let pos = sink.get_pos().as_millis() as u64;
                            position_ms_cache.store(pos, Ordering::Relaxed);
                        } else {
                            is_playing_cache.store(false, Ordering::Relaxed);
                        }
                    }

                    let is_empty = {
                        if let Ok(sink_guard) = sink_arc.try_lock() {
                            if let Some(ref sink) = *sink_guard {
                                sink.empty()
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    let has_index = current_idx_arc.lock().unwrap().is_some();

                    if has_index && !is_empty {
                        had_track_playing = true;
                    }

                    // Track has finished playing naturally
                    if had_track_playing && is_empty {
                        had_track_playing = false;
                        let queue = queue_arc.lock().unwrap();
                        if queue.is_empty() {
                            continue;
                        }

                        let cur = *current_idx_arc.lock().unwrap();
                        let repeat = *repeat_arc.lock().unwrap();
                        let is_shuf = shuffle_arc.load(Ordering::SeqCst);

                        let next_idx = match repeat {
                            RepeatMode::One => cur,
                            RepeatMode::All => {
                                if is_shuf && queue.len() > 1 {
                                    Some(next_random_index(cur, queue.len(), &rng_arc))
                                } else {
                                    cur.map(|i| (i + 1) % queue.len())
                                }
                            }
                            RepeatMode::Off => {
                                if let Some(i) = cur {
                                    if i + 1 < queue.len() {
                                        if is_shuf && queue.len() > 1 {
                                            Some(next_random_index(cur, queue.len(), &rng_arc))
                                        } else {
                                            Some(i + 1)
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }
                        };
                        drop(queue);

                        if let Some(idx) = next_idx {
                            let _ = play_index_internal(
                                idx,
                                &stream_handle,
                                &queue_arc,
                                &sink_arc,
                                &current_idx_arc,
                                &volume_arc,
                                &on_change_arc,
                                &is_playing_cache,
                                &position_ms_cache,
                                &active_local,
                            );
                        }
                    }
                }
            });
    }

    pub fn set_queue(&self, tracks: Vec<LocalTrack>, start_index: Option<usize>) -> Result<()> {
        let mut queue = self.queue.lock().unwrap();
        *queue = tracks;
        drop(queue);

        if let Some(idx) = start_index {
            self.play_index(idx)?;
        }
        Ok(())
    }

    pub fn enqueue(&self, track: LocalTrack) {
        let mut queue = self.queue.lock().unwrap();
        queue.push(track);
    }

    pub fn get_queue(&self) -> Vec<LocalTrack> {
        self.queue.lock().unwrap().clone()
    }

    pub fn clear_queue(&self) {
        self.stop();
        self.queue.lock().unwrap().clear();
    }

    pub fn play_track(&self, track: LocalTrack) -> Result<()> {
        let existing_idx = {
            let mut queue = self.queue.lock().unwrap();
            if let Some(pos) = queue.iter().position(|t| t.id == track.id || t.path == track.path) {
                queue[pos] = track;
                pos
            } else {
                queue.push(track);
                queue.len() - 1
            }
        };
        self.play_index(existing_idx)
    }

    pub fn play_file(&self, path: &Path, title: String, artist: String) -> Result<()> {
        let track = LocalTrack {
            id: format!("local_{}", path.file_stem().unwrap_or_default().to_string_lossy()),
            title,
            artist,
            album: None,
            path: path.to_string_lossy().to_string(),
            duration_ms: 0,
            cover_path: None,
        };
        self.play_track(track)
    }

    pub fn play_index(&self, index: usize) -> Result<()> {
        play_index_internal(
            index,
            &self.stream_handle,
            &self.queue,
            &self.sink,
            &self.current_index,
            &self.volume,
            &self.on_track_changed,
            &self.is_playing_cache,
            &self.position_ms_cache,
            &self.active_local,
        )
    }

    pub fn play(&self) {
        self.active_local.store(true, Ordering::SeqCst);
        let can_resume = {
            let guard = self.sink.lock().unwrap();
            guard.as_ref().map(|s| !s.empty()).unwrap_or(false)
        };
        if can_resume {
            if let Some(ref sink) = *self.sink.lock().unwrap() {
                sink.play();
                self.is_playing_cache.store(true, Ordering::SeqCst);
            }
            return;
        }
        let cur_idx = *self.current_index.lock().unwrap();
        if let Some(idx) = cur_idx {
            let _ = self.play_index(idx);
            return;
        }
        let has_tracks = !self.queue.lock().unwrap().is_empty();
        if has_tracks {
            let _ = self.play_index(0);
        }
    }

    pub fn pause(&self) {
        self.is_playing_cache.store(false, Ordering::SeqCst);
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.pause();
        }
    }

    pub fn toggle_play_pause(&self) {
        if self.is_playing() {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn stop(&self) {
        self.active_local.store(false, Ordering::SeqCst);
        self.is_playing_cache.store(false, Ordering::SeqCst);
        self.position_ms_cache.store(0, Ordering::SeqCst);
        {
            let mut sink_guard = self.sink.lock().unwrap();
            if let Some(old) = sink_guard.take() {
                old.stop();
            }
        }
        *self.current_index.lock().unwrap() = None;
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing_cache.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        *self.volume.lock().unwrap() = clamped;
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.set_volume(clamped);
        }
    }

    pub fn get_volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }

    pub fn seek(&self, pos: Duration) -> Result<()> {
        let ms = pos.as_millis() as u64;
        self.position_ms_cache.store(ms, Ordering::Relaxed);
        if let Ok(sink_guard) = self.sink.try_lock() {
            if let Some(ref sink) = *sink_guard {
                let _ = sink.try_seek(pos);
            }
        }
        Ok(())
    }

    pub fn get_position(&self) -> Duration {
        Duration::from_millis(self.position_ms_cache.load(Ordering::Relaxed))
    }

    pub fn current_track(&self) -> Option<LocalTrack> {
        let idx = (*self.current_index.lock().unwrap())?;
        let queue = self.queue.lock().unwrap();
        queue.get(idx).cloned()
    }

    pub fn next(&self) -> Result<()> {
        let queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(());
        }
        let current = *self.current_index.lock().unwrap();
        let is_shuf = self.shuffle.load(Ordering::SeqCst);
        let repeat = *self.repeat_mode.lock().unwrap();

        let next_idx = match repeat {
            RepeatMode::One => current.unwrap_or(0),
            _ => {
                if is_shuf && queue.len() > 1 {
                    next_random_index(current, queue.len(), &self.rng_seed)
                } else {
                    match current {
                        Some(i) => (i + 1) % queue.len(),
                        None => 0,
                    }
                }
            }
        };

        drop(queue);
        self.play_index(next_idx)
    }

    pub fn previous(&self) -> Result<()> {
        let queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(());
        }
        let current = *self.current_index.lock().unwrap();
        let is_shuf = self.shuffle.load(Ordering::SeqCst);

        let prev_idx = if is_shuf && queue.len() > 1 {
            next_random_index(current, queue.len(), &self.rng_seed)
        } else {
            match current {
                Some(i) => {
                    if i == 0 {
                        queue.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            }
        };

        drop(queue);
        self.play_index(prev_idx)
    }

    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        *self.repeat_mode.lock().unwrap() = mode;
    }

    pub fn get_repeat_mode(&self) -> RepeatMode {
        *self.repeat_mode.lock().unwrap()
    }

    pub fn set_shuffle(&self, shuffle: bool) {
        self.shuffle.store(shuffle, Ordering::SeqCst);
    }

    pub fn is_shuffle(&self) -> bool {
        self.shuffle.load(Ordering::SeqCst)
    }

    /// Snapshot player state for persistence across daemon restarts.
    pub fn get_state(&self) -> PlayerState {
        let queue = self.queue.lock().unwrap().clone();
        let current_index = *self.current_index.lock().unwrap();
        let volume = *self.volume.lock().unwrap();
        let repeat_mode = *self.repeat_mode.lock().unwrap();
        let shuffle = self.shuffle.load(Ordering::SeqCst);
        let position_ms = self.get_position().as_millis() as u64;
        PlayerState {
            queue,
            current_index,
            volume,
            repeat_mode,
            shuffle,
            position_ms,
        }
    }

    /// Restore player state from saved snapshot.
    pub fn load_state(&self, state: PlayerState) {
        self.active_local.store(false, Ordering::SeqCst);
        self.is_playing_cache.store(false, Ordering::SeqCst);
        *self.queue.lock().unwrap() = state.queue;
        *self.volume.lock().unwrap() = state.volume;
        *self.repeat_mode.lock().unwrap() = state.repeat_mode;
        self.shuffle.store(state.shuffle, Ordering::SeqCst);
        *self.current_index.lock().unwrap() = state.current_index;
    }
}

fn next_random_index(current: Option<usize>, len: usize, rng_seed: &Arc<Mutex<u64>>) -> usize {
    let mut seed = rng_seed.lock().unwrap();
    // 64-bit LCG
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let rand_val = (*seed >> 32) as usize;
    let next = rand_val % len;

    if len > 1 && Some(next) == current {
        (next + 1) % len
    } else {
        next
    }
}

fn play_index_internal(
    index: usize,
    stream_handle: &Option<OutputStreamHandle>,
    queue_arc: &Arc<Mutex<Vec<LocalTrack>>>,
    sink_arc: &Arc<Mutex<Option<Sink>>>,
    current_idx_arc: &Arc<Mutex<Option<usize>>>,
    volume_arc: &Arc<Mutex<f32>>,
    on_change_arc: &Arc<Mutex<Option<TrackChangeCallback>>>,
    is_playing_cache: &Arc<AtomicBool>,
    position_ms_cache: &Arc<AtomicU64>,
    active_local: &Arc<AtomicBool>,
) -> Result<()> {
    let handle = stream_handle
        .as_ref()
        .context("Audio output device not available")?;

    let queue = queue_arc.lock().unwrap();
    let track = queue.get(index).cloned().context("Invalid track index")?;
    drop(queue);

    let file = match File::open(&track.path) {
        Ok(f) => f,
        Err(e) => {
            is_playing_cache.store(false, Ordering::SeqCst);
            return Err(anyhow::anyhow!("Failed to open audio file {}: {}", track.path, e));
        }
    };
    let reader = BufReader::new(file);
    let source = match Decoder::new(reader) {
        Ok(s) => s,
        Err(e) => {
            is_playing_cache.store(false, Ordering::SeqCst);
            return Err(anyhow::anyhow!("Failed to decode audio file {}: {}", track.path, e));
        }
    };

    let mut track = track;
    if track.duration_ms == 0 {
        if let Some(dur) = rodio::Source::total_duration(&source) {
            let ms = dur.as_millis() as u64;
            if ms > 0 {
                track.duration_ms = ms;
                if let Some(q_item) = queue_arc.lock().unwrap().get_mut(index) {
                    q_item.duration_ms = ms;
                }
            }
        }
    }

    let new_sink = Sink::try_new(handle).context("Failed to create audio Sink")?;
    let vol = *volume_arc.lock().unwrap();
    new_sink.set_volume(vol);
    new_sink.append(source);
    new_sink.play();

    {
        let mut sink_guard = sink_arc.lock().unwrap();
        if let Some(old) = sink_guard.take() {
            old.stop();
        }
        *sink_guard = Some(new_sink);
    }
    *current_idx_arc.lock().unwrap() = Some(index);
    active_local.store(true, Ordering::SeqCst);
    is_playing_cache.store(true, Ordering::SeqCst);
    position_ms_cache.store(0, Ordering::SeqCst);

    info!("Local player playing track: {} - {}", track.artist, track.title);

    // Invoke track changed callback after releasing all player locks
    let cb_opt = on_change_arc.lock().unwrap().clone();
    if let Some(cb) = cb_opt {
        cb(&track);
    }

    Ok(())
}

impl Drop for LocalPlayer {
    fn drop(&mut self) {
        self._stream_running.store(false, Ordering::SeqCst);
    }
}

impl Default for LocalPlayer {
    fn default() -> Self {
        Self::new()
    }
}
