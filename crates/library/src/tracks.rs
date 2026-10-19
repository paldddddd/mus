use crate::Database;
use anyhow::{Context, Result};
use common::{PlaylistItem, TrackItem};
use rusqlite::params;

impl Database {
    /// Insert or update a track in the library.
    pub fn save_track(&self, track: &TrackItem) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            r#"
            INSERT OR REPLACE INTO tracks (
                id, path, title, artist, album, duration_ms, cover_path, added_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                track.id,
                track.path,
                track.title,
                track.artist,
                track.album,
                track.duration_ms.map(|d| d as i64),
                track.cover_path,
                track.added_at,
            ],
        )
        .context("Failed to save track")?;
        Ok(())
    }

    /// Retrieve track by track_id.
    pub fn get_track(&self, id: &str) -> Result<Option<TrackItem>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, path, title, artist, album, duration_ms, cover_path, added_at
            FROM tracks WHERE id = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let dur: Option<i64> = row.get(5)?;
            Ok(Some(TrackItem {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                artist: row.get(3)?,
                album: row.get(4)?,
                duration_ms: dur.map(|d| d as u64),
                cover_path: row.get(6)?,
                added_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Retrieve track by path.
    pub fn get_track_by_path(&self, path: &str) -> Result<Option<TrackItem>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, path, title, artist, album, duration_ms, cover_path, added_at
            FROM tracks WHERE path = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            let dur: Option<i64> = row.get(5)?;
            Ok(Some(TrackItem {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                artist: row.get(3)?,
                album: row.get(4)?,
                duration_ms: dur.map(|d| d as u64),
                cover_path: row.get(6)?,
                added_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// List all tracks in the library ordered by artist, album, title.
    pub fn list_tracks(&self) -> Result<Vec<TrackItem>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, path, title, artist, album, duration_ms, cover_path, added_at
            FROM tracks
            ORDER BY COALESCE(artist, '') ASC, COALESCE(album, '') ASC, COALESCE(title, path) ASC
            "#,
        )?;

        let items = stmt
            .query_map([], |row| {
                let dur: Option<i64> = row.get(5)?;
                Ok(TrackItem {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(2)?,
                    artist: row.get(3)?,
                    album: row.get(4)?,
                    duration_ms: dur.map(|d| d as u64),
                    cover_path: row.get(6)?,
                    added_at: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    /// Delete track by track_id.
    pub fn delete_track(&self, id: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM tracks WHERE id = ?1", params![id])
            .context("Failed to delete track")?;
        Ok(())
    }

    /// Delete tracks whose file path no longer exists.
    pub fn delete_missing_tracks(&self, active_paths: &[String]) -> Result<usize> {
        let all_tracks = self.list_tracks()?;
        let set: std::collections::HashSet<&str> = active_paths.iter().map(|s| s.as_str()).collect();
        let mut deleted = 0;
        let conn = self.conn()?;
        for track in all_tracks {
            if !set.contains(track.path.as_str()) && !std::path::Path::new(&track.path).exists() {
                conn.execute("DELETE FROM tracks WHERE id = ?1", params![track.id])?;
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    /// Create a new playlist.
    pub fn create_playlist(&self, name: &str) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute("INSERT INTO playlists (name) VALUES (?1)", params![name])
            .context("Failed to create playlist")?;
        Ok(conn.last_insert_rowid())
    }

    /// List all playlists with their ordered track ids.
    pub fn list_playlists(&self) -> Result<Vec<PlaylistItem>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, name FROM playlists ORDER BY id ASC")?;
        let playlists: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut result = Vec::with_capacity(playlists.len());
        for (id, name) in playlists {
            let mut track_stmt = conn.prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position ASC",
            )?;
            let tracks: Vec<String> = track_stmt
                .query_map(params![id], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            result.push(PlaylistItem {
                id,
                name,
                track_ids: tracks,
            });
        }
        Ok(result)
    }

    /// Retrieve a single playlist by id.
    pub fn get_playlist(&self, id: i64) -> Result<Option<PlaylistItem>> {
        let conn = self.conn()?;
        let res = conn.query_row(
            "SELECT id, name FROM playlists WHERE id = ?1",
            params![id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        );

        match res {
            Ok((pid, name)) => {
                let mut track_stmt = conn.prepare(
                    "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position ASC",
                )?;
                let tracks: Vec<String> = track_stmt
                    .query_map(params![pid], |row| row.get(0))?
                    .filter_map(|r| r.ok())
                    .collect();
                Ok(Some(PlaylistItem {
                    id: pid,
                    name,
                    track_ids: tracks,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to query playlist: {e}")),
        }
    }

    /// Update playlist name and track ordering.
    pub fn update_playlist(&self, id: i64, name: &str, track_ids: &[String]) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        tx.execute(
            "UPDATE playlists SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
            params![id],
        )?;

        for (pos, tid) in track_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
                params![id, tid, pos as i64],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Delete playlist by id.
    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])
            .context("Failed to delete playlist")?;
        Ok(())
    }
}
