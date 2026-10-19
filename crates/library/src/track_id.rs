//! Stable track identifier: blake3 over the first 1 MiB of the file plus its size.
//!
//! The id does not depend on the file name or location, so bindings survive
//! renames and moves. Any change of content inside the first MiB or of the file
//! size produces a new id.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Number of leading bytes hashed.
pub const TRACK_ID_PREFIX_BYTES: u64 = 1024 * 1024;

/// Compute track id from an open reader of known total size.
pub fn track_id_from_reader<R: Read>(reader: R, total_size: u64) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut limited = reader.take(TRACK_ID_PREFIX_BYTES);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = limited.read(&mut buf).context("Failed to read track data")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    hasher.update(&total_size.to_le_bytes());
    let hash = hasher.finalize();
    // 128 bits of the hash is plenty for a local library and keeps ids short.
    Ok(hash.to_hex()[..32].to_string())
}

/// Compute track id for a file on disk.
pub fn compute_track_id(path: &Path) -> Result<String> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open track file: {}", path.display()))?;
    let size = file
        .metadata()
        .with_context(|| format!("Failed to stat track file: {}", path.display()))?
        .len();
    track_id_from_reader(file, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn same_content_same_id() {
        let data = vec![7u8; 5000];
        let a = track_id_from_reader(Cursor::new(&data), data.len() as u64).unwrap();
        let b = track_id_from_reader(Cursor::new(&data), data.len() as u64).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn changed_prefix_changes_id() {
        let mut data = vec![7u8; 5000];
        let a = track_id_from_reader(Cursor::new(&data), data.len() as u64).unwrap();
        data[10] = 8;
        let b = track_id_from_reader(Cursor::new(&data), data.len() as u64).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn changed_size_changes_id() {
        // Content beyond the first MiB is not hashed, but the size is.
        let mut data = vec![1u8; (TRACK_ID_PREFIX_BYTES + 100) as usize];
        let a = track_id_from_reader(Cursor::new(&data), data.len() as u64).unwrap();
        data.push(1);
        let b = track_id_from_reader(Cursor::new(&data), data.len() as u64).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn tail_change_with_same_size_keeps_id() {
        let mut data = vec![1u8; (TRACK_ID_PREFIX_BYTES + 100) as usize];
        let a = track_id_from_reader(Cursor::new(&data), data.len() as u64).unwrap();
        let last = data.len() - 1;
        data[last] = 2;
        let b = track_id_from_reader(Cursor::new(&data), data.len() as u64).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rename_and_move_keep_id() {
        let dir = std::env::temp_dir().join(format!("wallmus_tid_{}", std::process::id()));
        let sub = dir.join("moved");
        std::fs::create_dir_all(&sub).unwrap();
        let p1 = dir.join("original.mp3");
        std::fs::write(&p1, b"fake audio payload 123").unwrap();
        let id1 = compute_track_id(&p1).unwrap();

        let p2 = sub.join("renamed.mp3");
        std::fs::rename(&p1, &p2).unwrap();
        let id2 = compute_track_id(&p2).unwrap();
        assert_eq!(id1, id2);

        std::fs::write(&p2, b"fake audio payload 124").unwrap();
        let id3 = compute_track_id(&p2).unwrap();
        assert_ne!(id1, id3);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
