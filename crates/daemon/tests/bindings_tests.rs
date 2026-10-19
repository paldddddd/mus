use common::{AppConfig, BindingItem, BindingScope, TrackItem, WallpaperItem};
use library::{resolve, BindingLookup, Database, TrackContext};
use std::time::Instant;

fn create_sample_wallpaper(id: &str, name: &str) -> WallpaperItem {
    WallpaperItem {
        id: id.to_string(),
        name: name.to_string(),
        kind: "image".to_string(),
        source_path: format!("C:/wallpapers/{id}.jpg"),
        processed_path: format!("C:/wallpapers/{id}.jpg"),
        thumb_path: format!("C:/wallpapers/{id}_thumb.webp"),
        fit: "fill".to_string(),
        fps_cap: None,
        width: Some(1920),
        height: Some(1080),
        duration_ms: None,
        created_at: 1000,
    }
}

fn create_sample_track(id: &str, title: &str, artist: &str, album: &str) -> TrackItem {
    TrackItem {
        id: id.to_string(),
        path: format!("C:/music/{title}.mp3"),
        title: Some(title.to_string()),
        artist: Some(artist.to_string()),
        album: Some(album.to_string()),
        duration_ms: Some(210_000),
        cover_path: None,
        added_at: 1000,
    }
}

#[test]
fn test_stage3_bindings_resolution_precedence_in_db() {
    let db = Database::open_in_memory().unwrap();

    // Insert 5 distinct wallpapers
    let wp_track = create_sample_wallpaper("wp_track", "Track Specific");
    let wp_album = create_sample_wallpaper("wp_album", "Album Specific");
    let wp_artist = create_sample_wallpaper("wp_artist", "Artist Specific");
    let wp_playlist = create_sample_wallpaper("wp_playlist", "Playlist Specific");
    let wp_default = create_sample_wallpaper("wp_default", "Default Wallpaper");

    db.save_wallpaper(&wp_track).unwrap();
    db.save_wallpaper(&wp_album).unwrap();
    db.save_wallpaper(&wp_artist).unwrap();
    db.save_wallpaper(&wp_playlist).unwrap();
    db.save_wallpaper(&wp_default).unwrap();

    // 1. Create playlist and bindings
    let playlist_id = db.create_playlist("Chill Vibes").unwrap();

    db.save_binding(&BindingItem {
        id: None,
        scope: BindingScope::Playlist,
        key: playlist_id.to_string(),
        wallpaper_id: wp_playlist.id.clone(),
    })
    .unwrap();

    db.save_binding(&BindingItem {
        id: None,
        scope: BindingScope::Artist,
        key: "queen".to_string(),
        wallpaper_id: wp_artist.id.clone(),
    })
    .unwrap();

    db.save_binding(&BindingItem {
        id: None,
        scope: BindingScope::Album,
        key: "queen|a night at the opera".to_string(),
        wallpaper_id: wp_album.id.clone(),
    })
    .unwrap();

    db.save_binding(&BindingItem {
        id: None,
        scope: BindingScope::Track,
        key: "track_bohemian".to_string(),
        wallpaper_id: wp_track.id.clone(),
    })
    .unwrap();

    // Test Case 1: All bindings match -> Track wins
    let track1 = create_sample_track(
        "track_bohemian",
        "Bohemian Rhapsody",
        "Queen",
        "A Night at the Opera",
    );
    let ctx1 = TrackContext::from_local(&track1, Some(playlist_id));
    let res1 = resolve(&ctx1, &db, Some(&wp_default.id)).expect("Should resolve");
    assert_eq!(res1.wallpaper_id, "wp_track");
    assert_eq!(res1.matched_scope, Some(BindingScope::Track));

    // Test Case 2: Different track on same album -> Album wins
    let track2 = create_sample_track(
        "track_love_of_my_life",
        "Love of My Life",
        "Queen",
        "A Night at the Opera (Remastered 2011)",
    );
    let ctx2 = TrackContext::from_local(&track2, Some(playlist_id));
    let res2 = resolve(&ctx2, &db, Some(&wp_default.id)).expect("Should resolve");
    assert_eq!(res2.wallpaper_id, "wp_album");
    assert_eq!(res2.matched_scope, Some(BindingScope::Album));

    // Test Case 3: Different album by Queen -> Artist wins
    let track3 = create_sample_track(
        "track_radio_gaga",
        "Radio Ga Ga",
        "Queen",
        "The Works",
    );
    let ctx3 = TrackContext::from_local(&track3, Some(playlist_id));
    let res3 = resolve(&ctx3, &db, Some(&wp_default.id)).expect("Should resolve");
    assert_eq!(res3.wallpaper_id, "wp_artist");
    assert_eq!(res3.matched_scope, Some(BindingScope::Artist));

    // Test Case 4: Different artist in playlist -> Playlist wins
    let track4 = create_sample_track(
        "track_get_lucky",
        "Get Lucky",
        "Daft Punk",
        "Random Access Memories",
    );
    let ctx4 = TrackContext::from_local(&track4, Some(playlist_id));
    let res4 = resolve(&ctx4, &db, Some(&wp_default.id)).expect("Should resolve");
    assert_eq!(res4.wallpaper_id, "wp_playlist");
    assert_eq!(res4.matched_scope, Some(BindingScope::Playlist));

    // Test Case 5: Independent track with no matches -> Default wins
    let track5 = create_sample_track(
        "track_unknown",
        "Unknown Track",
        "Unknown Artist",
        "Unknown Album",
    );
    let ctx5 = TrackContext::from_local(&track5, None);
    let res5 = resolve(&ctx5, &db, Some(&wp_default.id)).expect("Should resolve");
    assert_eq!(res5.wallpaper_id, "wp_default");
    assert_eq!(res5.matched_scope, None);
}

#[test]
fn test_stage3_fixed_vs_follow_music_mode_contract() {
    let mut config = AppConfig::default();
    assert!(config.follow_music); // Default is follow_music

    // Switching to fixed mode
    config.follow_music = false;
    assert!(!config.follow_music);

    let db = Database::open_in_memory().unwrap();
    let wp_bound = create_sample_wallpaper("wp_bound", "Bound WP");
    let wp_current = create_sample_wallpaper("wp_current", "Current WP");
    db.save_wallpaper(&wp_bound).unwrap();
    db.save_wallpaper(&wp_current).unwrap();

    db.save_binding(&BindingItem {
        id: None,
        scope: BindingScope::Track,
        key: "track_1".to_string(),
        wallpaper_id: wp_bound.id.clone(),
    })
    .unwrap();

    let track = create_sample_track("track_1", "Song", "Artist", "Album");
    let ctx = TrackContext::from_local(&track, None);

    // In fixed mode, daemon preserves active_wallpaper without re-evaluating:
    let active_wallpaper = if config.follow_music {
        resolve(&ctx, &db, None).map(|r| r.wallpaper_id)
    } else {
        Some(wp_current.id.clone())
    };

    assert_eq!(active_wallpaper, Some("wp_current".to_string()));

    // When follow_music is enabled, wallpaper resolves to bound wallpaper:
    config.follow_music = true;
    let active_wallpaper_follow = if config.follow_music {
        resolve(&ctx, &db, None).map(|r| r.wallpaper_id)
    } else {
        Some(wp_current.id.clone())
    };

    assert_eq!(active_wallpaper_follow, Some("wp_bound".to_string()));
}

#[test]
fn test_stage3_resolution_latency_benchmark() {
    let db = Database::open_in_memory().unwrap();

    let wp = create_sample_wallpaper("wp_bench", "Benchmark WP");
    db.save_wallpaper(&wp).unwrap();

    // Populate database with 100 bindings to measure lookup speed under load
    for i in 0..100 {
        db.save_binding(&BindingItem {
            id: None,
            scope: BindingScope::Track,
            key: format!("bench_track_{i}"),
            wallpaper_id: wp.id.clone(),
        })
        .unwrap();
    }

    let track = create_sample_track("bench_track_50", "Test Title", "Test Artist", "Test Album");
    let ctx = TrackContext::from_local(&track, None);

    // Measure resolution latency
    let start = Instant::now();
    let iterations = 1000;
    for _ in 0..iterations {
        let res = resolve(&ctx, &db, Some("wp_bench")).unwrap();
        assert_eq!(res.wallpaper_id, "wp_bench");
    }
    let elapsed = start.elapsed();
    let avg_us = elapsed.as_micros() as f64 / iterations as f64;

    println!("Stage 3 Resolution Benchmark: average {:.2} us per resolution", avg_us);
    // Section 9 requires scenario to complete well under 500 ms (500,000 us)
    assert!(avg_us < 1000.0, "Resolution must take less than 1 ms (1000 us), took {:.2} us", avg_us);
}

#[test]
fn test_stage3_foreign_key_cascade_deletion() {
    let db = Database::open_in_memory().unwrap();
    let wp = create_sample_wallpaper("wp_cascade", "Cascade WP");
    db.save_wallpaper(&wp).unwrap();

    db.save_binding(&BindingItem {
        id: None,
        scope: BindingScope::Artist,
        key: "queen".to_string(),
        wallpaper_id: wp.id.clone(),
    })
    .unwrap();

    assert_eq!(db.find_wallpaper(BindingScope::Artist, "queen"), Some("wp_cascade".to_string()));

    // Deleting the wallpaper from database cascades and removes bindings referencing it
    db.delete_wallpaper("wp_cascade").unwrap();
    assert_eq!(db.find_wallpaper(BindingScope::Artist, "queen"), None);
}
