use crate::track_id::compute_track_id;
use crate::Database;
use anyhow::{Context, Result};
use common::paths::covers_dir;
use common::TrackItem;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::MimeType;
use lofty::prelude::Accessor;
use lofty::probe::Probe;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;
use tracing::{debug, warn};
use walkdir::WalkDir;

/// Standard supported audio file extensions.
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "wav", "m4a", "aac"];

/// Progress information emitted during scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub path: String,
}

/// Summary metrics of a scan run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanSummary {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub errors: usize,
}

/// Read track metadata and embedded cover art from an audio file.
pub fn read_track_metadata(path: &Path) -> Result<TrackItem> {
    let track_id = compute_track_id(path)
        .with_context(|| format!("Failed to compute track_id for {}", path.display()))?;

    let tagged_file = Probe::open(path)
        .with_context(|| format!("Failed to probe audio file {}", path.display()))?
        .read();

    let (title, artist, album, duration_ms, cover_path) = match tagged_file {
        Ok(tagged) => {
            let duration_ms = Some(tagged.properties().duration().as_millis() as u64);
            let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

            let (title, artist, album) = if let Some(t) = tag {
                (
                    t.title().map(|s| s.to_string()),
                    t.artist().map(|s| s.to_string()),
                    t.album().map(|s| s.to_string()),
                )
            } else {
                (None, None, None)
            };

            // Extract picture if available
            let cover_path = tag.and_then(|t| {
                t.pictures().first().map(|pic| {
                    let ext = match pic.mime_type() {
                        Some(MimeType::Png) => "png",
                        _ => "jpg",
                    };
                    let hash = blake3::hash(pic.data()).to_hex()[..16].to_string();
                    let cover_file = covers_dir().join(format!("track_{hash}.{ext}"));
                    if !cover_file.exists() {
                        if let Ok(mut f) = File::create(&cover_file) {
                            let _ = f.write_all(pic.data());
                        }
                    }
                    cover_file.to_string_lossy().to_string()
                })
            });

            (title, artist, album, duration_ms, cover_path)
        }
        Err(e) => {
            debug!("Could not read audio tags from {}: {e}", path.display());
            (None, None, None, None, None)
        }
    };

    let fallback_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    let final_title = title
        .filter(|s| !s.trim().is_empty())
        .or(fallback_title);

    let added_at = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    Ok(TrackItem {
        id: track_id,
        path: path.to_string_lossy().to_string(),
        title: final_title,
        artist,
        album,
        duration_ms,
        cover_path,
        added_at,
    })
}

/// Recursively scan music folders and synchronize with database.
pub fn scan_folders<F>(db: &Database, folders: &[String], mut on_progress: Option<F>) -> Result<ScanSummary>
where
    F: FnMut(ScanProgress),
{
    let mut files_to_scan = Vec::new();

    for folder in folders {
        let p = Path::new(folder);
        if !p.is_dir() {
            continue;
        }

        for entry in WalkDir::new(p).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if AUDIO_EXTENSIONS.contains(&ext_lower.as_str()) {
                        files_to_scan.push(entry.path().to_path_buf());
                    }
                }
            }
        }
    }

    let total = files_to_scan.len();
    let mut summary = ScanSummary::default();
    let mut active_paths = Vec::with_capacity(total);

    for (idx, path) in files_to_scan.into_iter().enumerate() {
        let path_str = path.to_string_lossy().to_string();
        active_paths.push(path_str.clone());

        if let Some(cb) = on_progress.as_mut() {
            cb(ScanProgress {
                current: idx + 1,
                total,
                path: path_str.clone(),
            });
        }

        match read_track_metadata(&path) {
            Ok(track) => {
                match db.get_track(&track.id) {
                    Ok(Some(existing)) => {
                        if existing.path != track.path {
                            // File moved or renamed
                            if let Err(e) = db.save_track(&track) {
                                warn!("Failed to update moved track {}: {e}", path.display());
                                summary.errors += 1;
                            } else {
                                summary.updated += 1;
                            }
                        } else {
                            summary.unchanged += 1;
                        }
                    }
                    Ok(None) => {
                        if let Err(e) = db.save_track(&track) {
                            warn!("Failed to insert track {}: {e}", path.display());
                            summary.errors += 1;
                        } else {
                            summary.added += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Database error querying track {}: {e}", path.display());
                        summary.errors += 1;
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read metadata for {}: {e}", path.display());
                summary.errors += 1;
            }
        }
    }

    // Clean up tracks from db that were deleted from disk
    if let Ok(removed) = db.delete_missing_tracks(&active_paths) {
        summary.removed = removed;
    }

    Ok(summary)
}
