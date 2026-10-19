use crate::scanner::{read_track_metadata, AUDIO_EXTENSIONS};
use crate::Database;
use anyhow::{Context, Result};
use notify::{recommended_watcher, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Events produced by LibraryWatcher.
#[derive(Debug, Clone)]
pub enum LibraryWatchEvent {
    TrackAdded(String),
    TrackUpdated(String),
    TrackRemoved(String),
}

/// Watches configured music folders for file changes and syncs with the Database.
pub struct LibraryWatcher {
    watcher: Option<RecommendedWatcher>,
    watched_folders: HashSet<PathBuf>,
    pub event_rx: Receiver<LibraryWatchEvent>,
}

impl LibraryWatcher {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let (notify_tx, notify_rx) = channel();
        let (client_tx, client_rx) = channel();

        // Background debouncer thread to process filesystem events without blocking watcher
        let db_clone = db.clone();
        let notify_tx_clone = notify_tx;

        let watcher = recommended_watcher(move |res| {
            let _ = notify_tx_clone.send(res);
        })
        .context("Failed to initialize recommended_watcher")?;

        thread::Builder::new()
            .name("library-watcher".to_string())
            .spawn(move || {
                run_watcher_loop(notify_rx, client_tx, db_clone);
            })
            .context("Failed to spawn library watcher thread")?;

        Ok(Self {
            watcher: Some(watcher),
            watched_folders: HashSet::new(),
            event_rx: client_rx,
        })
    }

    /// Update the set of monitored directories.
    pub fn update_folders(&mut self, folders: &[String]) -> Result<()> {
        let watcher = match self.watcher.as_mut() {
            Some(w) => w,
            None => return Ok(()),
        };

        let new_set: HashSet<PathBuf> = folders
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .collect();

        // Unwatch removed folders
        for old in &self.watched_folders {
            if !new_set.contains(old) {
                if let Err(e) = watcher.unwatch(old) {
                    debug!("Unwatch folder error (ignored): {e}");
                }
            }
        }

        // Watch new folders
        for folder in &new_set {
            if !self.watched_folders.contains(folder) {
                if let Err(e) = watcher.watch(folder, RecursiveMode::Recursive) {
                    warn!("Failed to watch folder {}: {e}", folder.display());
                } else {
                    info!("LibraryWatcher watching: {}", folder.display());
                }
            }
        }

        self.watched_folders = new_set;
        Ok(())
    }
}

fn is_audio_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str())
    } else {
        false
    }
}

fn run_watcher_loop(
    rx: Receiver<notify::Result<Event>>,
    client_tx: Sender<LibraryWatchEvent>,
    db: Arc<Database>,
) {
    while let Ok(event_res) = rx.recv() {
        match event_res {
            Ok(event) => {
                // Short sleep to debounce rapid bursts of events from write/close
                thread::sleep(Duration::from_millis(150));
                // Drain any further duplicate events that arrived in the debounce window
                while let Ok(_extra) = rx.try_recv() {}

                for path in event.paths {
                    if !is_audio_file(&path) {
                        continue;
                    }

                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            if path.exists() {
                                match read_track_metadata(&path) {
                                    Ok(track) => {
                                        let is_new = db.get_track(&track.id).ok().flatten().is_none();
                                        if let Err(e) = db.save_track(&track) {
                                            warn!("Watcher failed to save track {}: {e}", path.display());
                                        } else if is_new {
                                            let _ = client_tx.send(LibraryWatchEvent::TrackAdded(track.id));
                                        } else {
                                            let _ = client_tx.send(LibraryWatchEvent::TrackUpdated(track.id));
                                        }
                                    }
                                    Err(e) => {
                                        debug!("Watcher could not read track {}: {e}", path.display());
                                    }
                                }
                            }
                        }
                        EventKind::Remove(_) => {
                            let path_str = path.to_string_lossy().to_string();
                            if let Ok(Some(track)) = db.get_track_by_path(&path_str) {
                                if let Ok(()) = db.delete_track(&track.id) {
                                    let _ = client_tx.send(LibraryWatchEvent::TrackRemoved(track.id));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                warn!("LibraryWatcher notification error: {e}");
            }
        }
    }
}
