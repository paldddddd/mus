//! SQLite storage and wallpaper metadata management.

pub mod bindings;
pub mod scanner;
pub mod track_id;
mod tracks;
pub mod watcher;

pub use bindings::{resolve, BindingLookup, ResolutionResult, TrackContext};
pub use scanner::{read_track_metadata, scan_folders, ScanProgress, ScanSummary, AUDIO_EXTENSIONS};
pub use track_id::compute_track_id;
pub use watcher::LibraryWatcher;

use anyhow::{Context, Result};
use common::{db_path, ensure_app_dirs, WallpaperItem};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Lock the connection; a poisoned mutex becomes an error instead of a panic.
    fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("Database mutex poisoned"))
    }
    /// Open the SQLite database at default location `%APPDATA%/wallmus/library.db` and apply migrations.
    pub fn open_default() -> Result<Self> {
        ensure_app_dirs()?;
        let path = db_path();
        Self::open(&path)
    }

    /// Open or create database at given path and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite database at {}", path.display()))?;

        // Enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .context("Failed to enable foreign keys")?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    /// In-memory database for unit testing.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("Failed to open in-memory SQLite")?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Run migrations using PRAGMA user_version.
    pub fn migrate(&self) -> Result<()> {
        let conn = self.conn()?;

        let current_version: u32 = conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))
            .context("Failed to query PRAGMA user_version")?;

        if current_version < 1 {
            conn.execute_batch(
                r#"
                BEGIN TRANSACTION;

                CREATE TABLE IF NOT EXISTS tracks (
                    id TEXT PRIMARY KEY,
                    path TEXT NOT NULL,
                    title TEXT,
                    artist TEXT,
                    album TEXT,
                    duration_ms INTEGER,
                    cover_path TEXT,
                    added_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS wallpapers (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK(kind IN ('image','animated','video','html','shader')),
                    source_path TEXT NOT NULL,
                    processed_path TEXT,
                    thumb_path TEXT,
                    fit TEXT NOT NULL DEFAULT 'fill',
                    fps_cap INTEGER,
                    width INTEGER,
                    height INTEGER,
                    duration_ms INTEGER,
                    created_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS playlists (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS playlist_tracks (
                    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                    position INTEGER NOT NULL,
                    PRIMARY KEY(playlist_id, position)
                );

                CREATE TABLE IF NOT EXISTS bindings (
                    id INTEGER PRIMARY KEY,
                    scope TEXT NOT NULL CHECK(scope IN ('track','album','artist','playlist','external')),
                    key TEXT NOT NULL,
                    wallpaper_id TEXT NOT NULL REFERENCES wallpapers(id) ON DELETE CASCADE,
                    UNIQUE(scope, key)
                );

                CREATE TABLE IF NOT EXISTS kv (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                PRAGMA user_version = 1;

                COMMIT;
                "#,
            )
            .context("Failed to apply migration v1")?;

            tracing::info!("Applied database migration v1");
        }

        Ok(())
    }

    /// Insert or replace wallpaper metadata.
    pub fn save_wallpaper(&self, wp: &WallpaperItem) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            r#"
            INSERT OR REPLACE INTO wallpapers (
                id, name, kind, source_path, processed_path, thumb_path,
                fit, fps_cap, width, height, duration_ms, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                wp.id,
                wp.name,
                wp.kind,
                wp.source_path,
                wp.processed_path,
                wp.thumb_path,
                wp.fit,
                wp.fps_cap,
                wp.width,
                wp.height,
                wp.duration_ms,
                wp.created_at,
            ],
        )
        .context("Failed to save wallpaper")?;
        Ok(())
    }

    /// Retrieve all wallpapers ordered by creation time descending.
    pub fn list_wallpapers(&self) -> Result<Vec<WallpaperItem>> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, name, kind, source_path, processed_path, thumb_path,
                       fit, fps_cap, width, height, duration_ms, created_at
                FROM wallpapers
                ORDER BY created_at DESC
                "#,
            )
            .context("Failed to prepare list_wallpapers query")?;

        let items = stmt
            .query_map([], |row| {
                Ok(WallpaperItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    source_path: row.get(3)?,
                    processed_path: row.get(4)?,
                    thumb_path: row.get(5)?,
                    fit: row.get(6)?,
                    fps_cap: row.get(7)?,
                    width: row.get(8)?,
                    height: row.get(9)?,
                    duration_ms: row.get(10)?,
                    created_at: row.get(11)?,
                })
            })
            .context("Querying wallpapers failed")?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    /// Retrieve single wallpaper by id.
    pub fn get_wallpaper(&self, id: &str) -> Result<Option<WallpaperItem>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, kind, source_path, processed_path, thumb_path,
                   fit, fps_cap, width, height, duration_ms, created_at
            FROM wallpapers
            WHERE id = ?1
            "#,
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(WallpaperItem {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                source_path: row.get(3)?,
                processed_path: row.get(4)?,
                thumb_path: row.get(5)?,
                fit: row.get(6)?,
                fps_cap: row.get(7)?,
                width: row.get(8)?,
                height: row.get(9)?,
                duration_ms: row.get(10)?,
                created_at: row.get(11)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Delete wallpaper by id.
    pub fn delete_wallpaper(&self, id: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM wallpapers WHERE id = ?1", params![id])
            .context("Failed to delete wallpaper")?;
        Ok(())
    }

    /// Get value from KV store.
    pub fn get_kv(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        let res = conn.query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| {
            row.get(0)
        });
        match res {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("get_kv failed: {e}")),
        }
    }

    /// Set value in KV store.
    pub fn set_kv(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .context("set_kv failed")?;
        Ok(())
    }
}
