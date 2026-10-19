//! Wallpaper-to-music binding resolution engine and database CRUD.
//!
//! Prioritizes:
//! 1. Track ID (local) or `ext:<artist>|<title>` (external)
//! 2. Album (`<artist>|<album>` normalized)
//! 3. Artist (`<artist>` normalized)
//! 4. Playlist (`<playlist_id>`)
//! 5. Default wallpaper (fallback)

use crate::Database;
use anyhow::{Context, Result};
use common::normalize::{album_key, external_track_key, normalize_name};
use common::{BindingItem, BindingScope, TrackItem};
use std::collections::HashMap;

/// Trait abstracting binding lookup for pure resolver testing and DB lookup.
pub trait BindingLookup {
    fn find_wallpaper(&self, scope: BindingScope, key: &str) -> Option<String>;
}

impl BindingLookup for HashMap<(BindingScope, String), String> {
    fn find_wallpaper(&self, scope: BindingScope, key: &str) -> Option<String> {
        self.get(&(scope, key.to_string())).cloned()
    }
}

/// Information about a track playing locally or externally.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackContext<'a> {
    /// Local Blake3-based track ID.
    pub track_id: Option<&'a str>,
    /// Artist name.
    pub artist: Option<&'a str>,
    /// Track title.
    pub title: Option<&'a str>,
    /// Album title.
    pub album: Option<&'a str>,
    /// Active playlist ID, if playing from a playlist.
    pub playlist_id: Option<i64>,
    /// True if track comes from Windows SMTC / external player.
    pub is_external: bool,
}

impl<'a> TrackContext<'a> {
    /// Create context from a local library track and optional playlist ID.
    pub fn from_local(track: &'a TrackItem, playlist_id: Option<i64>) -> Self {
        Self {
            track_id: Some(&track.id),
            artist: track.artist.as_deref(),
            title: track.title.as_deref(),
            album: track.album.as_deref(),
            playlist_id,
            is_external: false,
        }
    }

    /// Create context from an external media session (SMTC).
    pub fn from_external(
        artist: &'a str,
        title: &'a str,
        album: Option<&'a str>,
    ) -> Self {
        Self {
            track_id: None,
            artist: Some(artist),
            title: Some(title),
            album,
            playlist_id: None,
            is_external: true,
        }
    }
}

/// The result of resolving a wallpaper for a track, including the matched scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionResult {
    /// Wallpaper ID to display.
    pub wallpaper_id: String,
    /// Which scope matched. None if fell back to default wallpaper.
    pub matched_scope: Option<BindingScope>,
}

/// Pure resolution function adhering to Section 9 precedence:
/// Track/External -> Album -> Artist -> Playlist -> Default Wallpaper.
pub fn resolve<L: BindingLookup + ?Sized>(
    ctx: &TrackContext<'_>,
    lookup: &L,
    default_wallpaper_id: Option<&str>,
) -> Option<ResolutionResult> {
    // 1. Specific Track / External
    if ctx.is_external {
        let artist = ctx.artist.unwrap_or_default();
        let title = ctx.title.unwrap_or_default();
        let key = external_track_key(artist, title);
        if let Some(wp) = lookup.find_wallpaper(BindingScope::External, &key) {
            return Some(ResolutionResult {
                wallpaper_id: wp,
                matched_scope: Some(BindingScope::External),
            });
        }
    } else if let Some(id) = ctx.track_id {
        if let Some(wp) = lookup.find_wallpaper(BindingScope::Track, id) {
            return Some(ResolutionResult {
                wallpaper_id: wp,
                matched_scope: Some(BindingScope::Track),
            });
        }
    }

    // 2. Album
    if let (Some(artist), Some(album)) = (ctx.artist, ctx.album) {
        if !artist.trim().is_empty() && !album.trim().is_empty() {
            let key = album_key(artist, album);
            if let Some(wp) = lookup.find_wallpaper(BindingScope::Album, &key) {
                return Some(ResolutionResult {
                    wallpaper_id: wp,
                    matched_scope: Some(BindingScope::Album),
                });
            }
        }
    }

    // 3. Artist
    if let Some(artist) = ctx.artist {
        let norm_artist = normalize_name(artist);
        if !norm_artist.is_empty() {
            if let Some(wp) = lookup.find_wallpaper(BindingScope::Artist, &norm_artist) {
                return Some(ResolutionResult {
                    wallpaper_id: wp,
                    matched_scope: Some(BindingScope::Artist),
                });
            }
        }
    }

    // 4. Playlist
    if let Some(pid) = ctx.playlist_id {
        let key = pid.to_string();
        if let Some(wp) = lookup.find_wallpaper(BindingScope::Playlist, &key) {
            return Some(ResolutionResult {
                wallpaper_id: wp,
                matched_scope: Some(BindingScope::Playlist),
            });
        }
    }

    // 5. Default wallpaper
    default_wallpaper_id.map(|wp| ResolutionResult {
        wallpaper_id: wp.to_string(),
        matched_scope: None,
    })
}

impl Database {
    /// Save or replace a binding in SQLite.
    pub fn save_binding(&self, binding: &BindingItem) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            r#"
            INSERT OR REPLACE INTO bindings (scope, key, wallpaper_id)
            VALUES (?1, ?2, ?3)
            "#,
            rusqlite::params![binding.scope.as_str(), binding.key, binding.wallpaper_id],
        )
        .context("Failed to save binding")?;
        Ok(())
    }

    /// Retrieve a binding by scope and key.
    pub fn get_binding(&self, scope: BindingScope, key: &str) -> Result<Option<BindingItem>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, scope, key, wallpaper_id
            FROM bindings
            WHERE scope = ?1 AND key = ?2
            "#,
        )?;
        let mut rows = stmt.query(rusqlite::params![scope.as_str(), key])?;
        if let Some(row) = rows.next()? {
            let scope_str: String = row.get(1)?;
            let scope_parsed = scope_str.parse().unwrap_or(scope);
            Ok(Some(BindingItem {
                id: Some(row.get(0)?),
                scope: scope_parsed,
                key: row.get(2)?,
                wallpaper_id: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Retrieve just the wallpaper ID for a given binding scope and key.
    pub fn get_binding_wallpaper(&self, scope: BindingScope, key: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        let res = conn.query_row(
            "SELECT wallpaper_id FROM bindings WHERE scope = ?1 AND key = ?2",
            rusqlite::params![scope.as_str(), key],
            |row| row.get(0),
        );
        match res {
            Ok(w) => Ok(Some(w)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("get_binding_wallpaper failed: {e}")),
        }
    }

    /// List all bindings.
    pub fn list_bindings(&self) -> Result<Vec<BindingItem>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, scope, key, wallpaper_id
            FROM bindings
            ORDER BY id ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            let scope_str: String = row.get(1)?;
            let scope_parsed = scope_str.parse().unwrap_or(BindingScope::Track);
            Ok(BindingItem {
                id: Some(row.get(0)?),
                scope: scope_parsed,
                key: row.get(2)?,
                wallpaper_id: row.get(3)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// Delete a binding by scope and key. Returns true if a row was deleted.
    pub fn delete_binding(&self, scope: BindingScope, key: &str) -> Result<bool> {
        let conn = self.conn()?;
        let count = conn.execute(
            "DELETE FROM bindings WHERE scope = ?1 AND key = ?2",
            rusqlite::params![scope.as_str(), key],
        )?;
        Ok(count > 0)
    }

    /// Delete a binding by primary key ID. Returns true if a row was deleted.
    pub fn delete_binding_by_id(&self, id: i64) -> Result<bool> {
        let conn = self.conn()?;
        let count = conn.execute(
            "DELETE FROM bindings WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(count > 0)
    }
}

impl BindingLookup for Database {
    fn find_wallpaper(&self, scope: BindingScope, key: &str) -> Option<String> {
        self.get_binding_wallpaper(scope, key).ok().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolution_precedence() {
        let mut lookup = HashMap::new();
        lookup.insert(
            (BindingScope::Track, "track_123".to_string()),
            "wp_track".to_string(),
        );
        lookup.insert(
            (BindingScope::Album, "queen|a night at the opera".to_string()),
            "wp_album".to_string(),
        );
        lookup.insert(
            (BindingScope::Artist, "queen".to_string()),
            "wp_artist".to_string(),
        );
        lookup.insert(
            (BindingScope::Playlist, "42".to_string()),
            "wp_playlist".to_string(),
        );

        let default_wp = "wp_default";

        // 1. With all matches available, Track ID wins
        let ctx = TrackContext {
            track_id: Some("track_123"),
            artist: Some("Queen"),
            title: Some("Bohemian Rhapsody"),
            album: Some("A Night at the Opera"),
            playlist_id: Some(42),
            is_external: false,
        };
        let res = resolve(&ctx, &lookup, Some(default_wp)).unwrap();
        assert_eq!(res.wallpaper_id, "wp_track");
        assert_eq!(res.matched_scope, Some(BindingScope::Track));

        // 2. Without track match, Album wins
        let ctx_no_track = TrackContext {
            track_id: Some("track_999"),
            artist: Some("Queen"),
            title: Some("Love of My Life"),
            album: Some("A Night at the Opera (2011 Remaster)"),
            playlist_id: Some(42),
            is_external: false,
        };
        let res = resolve(&ctx_no_track, &lookup, Some(default_wp)).unwrap();
        assert_eq!(res.wallpaper_id, "wp_album");
        assert_eq!(res.matched_scope, Some(BindingScope::Album));

        // 3. Without track & album, Artist wins
        let ctx_artist_only = TrackContext {
            track_id: Some("track_999"),
            artist: Some("Queen"),
            title: Some("Radio Ga Ga"),
            album: Some("The Works"),
            playlist_id: Some(42),
            is_external: false,
        };
        let res = resolve(&ctx_artist_only, &lookup, Some(default_wp)).unwrap();
        assert_eq!(res.wallpaper_id, "wp_artist");
        assert_eq!(res.matched_scope, Some(BindingScope::Artist));

        // 4. Without track/album/artist, Playlist wins
        let ctx_playlist_only = TrackContext {
            track_id: Some("track_999"),
            artist: Some("Daft Punk"),
            title: Some("Get Lucky"),
            album: Some("RAM"),
            playlist_id: Some(42),
            is_external: false,
        };
        let res = resolve(&ctx_playlist_only, &lookup, Some(default_wp)).unwrap();
        assert_eq!(res.wallpaper_id, "wp_playlist");
        assert_eq!(res.matched_scope, Some(BindingScope::Playlist));

        // 5. Without any matches, Default wins
        let ctx_none = TrackContext {
            track_id: Some("track_999"),
            artist: Some("Unknown"),
            title: Some("Unknown"),
            album: None,
            playlist_id: None,
            is_external: false,
        };
        let res = resolve(&ctx_none, &lookup, Some(default_wp)).unwrap();
        assert_eq!(res.wallpaper_id, "wp_default");
        assert_eq!(res.matched_scope, None);

        // 6. Without default and no match, returns None
        assert!(resolve(&ctx_none, &lookup, None).is_none());
    }

    #[test]
    fn test_external_track_resolution() {
        let mut lookup = HashMap::new();
        let key = external_track_key("Linkin Park", "In the End");
        lookup.insert((BindingScope::External, key), "wp_lp".to_string());
        lookup.insert(
            (BindingScope::Artist, "linkin park".to_string()),
            "wp_lp_artist".to_string(),
        );

        let ctx = TrackContext::from_external("Linkin Park", "In the End (2020 Remaster)", None);
        let res = resolve(&ctx, &lookup, None).unwrap();
        assert_eq!(res.wallpaper_id, "wp_lp");
        assert_eq!(res.matched_scope, Some(BindingScope::External));

        // Fallback to artist for external track if specific track not found
        let ctx2 = TrackContext::from_external("Linkin Park", "Numb", None);
        let res2 = resolve(&ctx2, &lookup, None).unwrap();
        assert_eq!(res2.wallpaper_id, "wp_lp_artist");
        assert_eq!(res2.matched_scope, Some(BindingScope::Artist));
    }

    #[test]
    fn test_database_bindings_crud() {
        let db = Database::open_in_memory().unwrap();

        // Need dummy wallpapers to satisfy foreign key constraint
        let dummy_wp = common::WallpaperItem {
            id: "wp_neon".to_string(),
            name: "Neon Lights".to_string(),
            kind: "image".to_string(),
            source_path: "neon.jpg".to_string(),
            processed_path: "neon.jpg".to_string(),
            thumb_path: "neon_thumb.webp".to_string(),
            fit: "fill".to_string(),
            fps_cap: None,
            width: Some(1920),
            height: Some(1080),
            duration_ms: None,
            created_at: 1000,
        };
        db.save_wallpaper(&dummy_wp).unwrap();

        let dummy_wp2 = common::WallpaperItem {
            id: "wp_space".to_string(),
            name: "Space Nebula".to_string(),
            kind: "video".to_string(),
            source_path: "space.mp4".to_string(),
            processed_path: "space.mp4".to_string(),
            thumb_path: "space_thumb.webp".to_string(),
            fit: "fill".to_string(),
            fps_cap: None,
            width: Some(1920),
            height: Some(1080),
            duration_ms: Some(15000),
            created_at: 1001,
        };
        db.save_wallpaper(&dummy_wp2).unwrap();

        // 1. Save binding
        let binding = BindingItem {
            id: None,
            scope: BindingScope::Artist,
            key: "daft punk".to_string(),
            wallpaper_id: "wp_neon".to_string(),
        };
        db.save_binding(&binding).unwrap();

        // 2. Get binding
        let fetched = db
            .get_binding(BindingScope::Artist, "daft punk")
            .unwrap()
            .expect("Binding should exist");
        assert_eq!(fetched.wallpaper_id, "wp_neon");
        assert_eq!(fetched.scope, BindingScope::Artist);
        assert_eq!(fetched.key, "daft punk");

        // 3. Test find_wallpaper via BindingLookup trait
        let resolved = db.find_wallpaper(BindingScope::Artist, "daft punk");
        assert_eq!(resolved, Some("wp_neon".to_string()));

        // 4. Update binding (replace)
        let updated = BindingItem {
            id: None,
            scope: BindingScope::Artist,
            key: "daft punk".to_string(),
            wallpaper_id: "wp_space".to_string(),
        };
        db.save_binding(&updated).unwrap();
        assert_eq!(
            db.find_wallpaper(BindingScope::Artist, "daft punk"),
            Some("wp_space".to_string())
        );

        // 5. List bindings
        let list = db.list_bindings().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].wallpaper_id, "wp_space");

        // 6. Delete binding
        let deleted = db.delete_binding(BindingScope::Artist, "daft punk").unwrap();
        assert!(deleted);
        assert_eq!(db.find_wallpaper(BindingScope::Artist, "daft punk"), None);
    }
}
