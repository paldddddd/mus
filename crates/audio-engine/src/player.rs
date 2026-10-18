use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub path: String,
    pub duration_ms: u64,
}

pub struct LocalPlayer {
    _stream_running: Arc<AtomicBool>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Arc<Mutex<Option<Sink>>>,
    queue: Arc<Mutex<Vec<LocalTrack>>>,
    current_index: Arc<Mutex<Option<usize>>>,
    volume: Arc<Mutex<f32>>,
    repeat_mode: Arc<Mutex<RepeatMode>>,
    shuffle: Arc<AtomicBool>,
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
                        while running_clone.load(std::sync::atomic::Ordering::SeqCst) {
                            std::thread::sleep(std::time::Duration::from_millis(100));
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

        Self {
            _stream_running: stream_running,
            stream_handle,
            sink: Arc::new(Mutex::new(None)),
            queue: Arc::new(Mutex::new(Vec::new())),
            current_index: Arc::new(Mutex::new(None)),
            volume: Arc::new(Mutex::new(0.8)),
            repeat_mode: Arc::new(Mutex::new(RepeatMode::Off)),
            shuffle: Arc::new(AtomicBool::new(false)),
        }
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

    pub fn play_file(&self, path: &Path, title: String, artist: String) -> Result<()> {
        let track = LocalTrack {
            id: format!("local_{}", path.file_stem().unwrap_or_default().to_string_lossy()),
            title,
            artist,
            path: path.to_string_lossy().to_string(),
            duration_ms: 0,
        };

        let mut queue = self.queue.lock().unwrap();
        queue.clear();
        queue.push(track);
        drop(queue);

        self.play_index(0)
    }

    pub fn play_index(&self, index: usize) -> Result<()> {
        let handle = self
            .stream_handle
            .as_ref()
            .context("Audio output device not available")?;

        let queue = self.queue.lock().unwrap();
        let track = queue.get(index).cloned().context("Invalid track index")?;
        drop(queue);

        let file = File::open(&track.path)
            .with_context(|| format!("Failed to open audio file: {}", track.path))?;
        let reader = BufReader::new(file);
        let source = Decoder::new(reader)
            .with_context(|| format!("Failed to decode audio file: {}", track.path))?;

        let new_sink = Sink::try_new(handle).context("Failed to create audio Sink")?;
        let vol = *self.volume.lock().unwrap();
        new_sink.set_volume(vol);
        new_sink.append(source);
        new_sink.play();

        let mut sink_guard = self.sink.lock().unwrap();
        if let Some(old) = sink_guard.take() {
            old.stop();
        }
        *sink_guard = Some(new_sink);
        *self.current_index.lock().unwrap() = Some(index);

        info!("Local player playing track: {} - {}", track.artist, track.title);
        Ok(())
    }

    pub fn play(&self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.play();
        }
    }

    pub fn pause(&self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.pause();
        }
    }

    pub fn toggle_play_pause(&self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            if sink.is_paused() {
                sink.play();
            } else {
                sink.pause();
            }
        }
    }

    pub fn stop(&self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.stop();
        }
        *self.current_index.lock().unwrap() = None;
    }

    pub fn is_playing(&self) -> bool {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            !sink.is_paused() && !sink.empty()
        } else {
            false
        }
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
        let current = self.current_index.lock().unwrap();
        let next_idx = match *current {
            Some(i) => (i + 1) % queue.len(),
            None => 0,
        };
        drop(current);
        drop(queue);
        self.play_index(next_idx)
    }

    pub fn previous(&self) -> Result<()> {
        let queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(());
        }
        let current = self.current_index.lock().unwrap();
        let prev_idx = match *current {
            Some(i) => {
                if i == 0 {
                    queue.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        drop(current);
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
        use std::sync::atomic::Ordering;
        self.shuffle.store(shuffle, Ordering::SeqCst);
    }

    pub fn is_shuffle(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.shuffle.load(Ordering::SeqCst)
    }
}

impl Drop for LocalPlayer {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        self._stream_running.store(false, Ordering::SeqCst);
    }
}

impl Default for LocalPlayer {
    fn default() -> Self {
        Self::new()
    }
}
