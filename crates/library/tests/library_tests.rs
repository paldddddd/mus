use common::WallpaperItem;
use library::Database;

#[test]
fn test_database_migrations_and_wallpapers_crud() {
    let db = Database::open_in_memory().expect("open_in_memory failed");

    let wp = WallpaperItem {
        id: "wp_test_1".to_string(),
        name: "Test Wallpaper".to_string(),
        kind: "video".to_string(),
        source_path: "C:/test.mp4".to_string(),
        processed_path: "C:/test.mp4".to_string(),
        thumb_path: "C:/test_thumb.webp".to_string(),
        fit: "fill".to_string(),
        fps_cap: Some(30),
        width: Some(1920),
        height: Some(1080),
        duration_ms: Some(15000),
        created_at: 1700000000,
    };

    db.save_wallpaper(&wp).expect("save_wallpaper failed");
    let list = db.list_wallpapers().expect("list_wallpapers failed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "wp_test_1");
    assert_eq!(list[0].kind, "video");

    db.set_kv("active_id", "wp_test_1").expect("set_kv failed");
    assert_eq!(
        db.get_kv("active_id").expect("get_kv failed"),
        Some("wp_test_1".to_string())
    );

    db.delete_wallpaper("wp_test_1").expect("delete_wallpaper failed");
    let list_after = db.list_wallpapers().expect("list_wallpapers failed");
    assert_eq!(list_after.len(), 0);
}

#[test]
fn test_database_tracks_and_playlists_crud() {
    use common::TrackItem;

    let db = Database::open_in_memory().expect("open_in_memory failed");

    let t1 = TrackItem {
        id: "tid_001".to_string(),
        path: "C:/music/track1.flac".to_string(),
        title: Some("Song One".to_string()),
        artist: Some("Artist A".to_string()),
        album: Some("Album X".to_string()),
        duration_ms: Some(215000),
        cover_path: None,
        added_at: 1700000000,
    };

    let t2 = TrackItem {
        id: "tid_002".to_string(),
        path: "C:/music/track2.mp3".to_string(),
        title: Some("Song Two".to_string()),
        artist: Some("Artist B".to_string()),
        album: Some("Album Y".to_string()),
        duration_ms: Some(180000),
        cover_path: Some("C:/covers/cov2.jpg".to_string()),
        added_at: 1700000100,
    };

    db.save_track(&t1).unwrap();
    db.save_track(&t2).unwrap();

    let tracks = db.list_tracks().unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].id, "tid_001");
    assert_eq!(tracks[1].id, "tid_002");

    let found = db.get_track("tid_001").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().title.as_deref(), Some("Song One"));

    let found_by_path = db.get_track_by_path("C:/music/track2.mp3").unwrap();
    assert!(found_by_path.is_some());
    assert_eq!(found_by_path.unwrap().id, "tid_002");

    // Playlists
    let pl_id = db.create_playlist("Favorites").unwrap();
    db.update_playlist(
        pl_id,
        "Favorites (Updated)",
        &["tid_002".to_string(), "tid_001".to_string()],
    )
    .unwrap();

    let pl = db.get_playlist(pl_id).unwrap();
    assert!(pl.is_some());
    let pl_unwrapped = pl.unwrap();
    assert_eq!(pl_unwrapped.name, "Favorites (Updated)");
    assert_eq!(pl_unwrapped.track_ids, vec!["tid_002", "tid_001"]);

    let all_pls = db.list_playlists().unwrap();
    assert_eq!(all_pls.len(), 1);

    db.delete_playlist(pl_id).unwrap();
    assert!(db.get_playlist(pl_id).unwrap().is_none());

    db.delete_track("tid_001").unwrap();
    assert_eq!(db.list_tracks().unwrap().len(), 1);
}

#[test]
fn test_scanner_with_dummy_audio_files() {
    use library::scanner::scan_folders;

    let temp_dir = std::env::temp_dir().join(format!("wallmus_scan_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let file1 = temp_dir.join("test1.mp3");
    let file2 = temp_dir.join("test2.wav");
    let file3 = temp_dir.join("ignored.txt");

    std::fs::write(&file1, b"ID3\x04\x00\x00\x00\x00\x00\x00fake audio content 1").unwrap();
    std::fs::write(&file2, b"RIFFfake wave audio content 2").unwrap();
    std::fs::write(&file3, b"not audio").unwrap();

    let db = Database::open_in_memory().unwrap();
    let folders = vec![temp_dir.to_string_lossy().to_string()];

    let mut scanned_count = 0;
    let summary = scan_folders(&db, &folders, Some(|_prog| {
        scanned_count += 1;
    }))
    .unwrap();

    assert_eq!(summary.added, 2);
    assert_eq!(scanned_count, 2);

    let tracks = db.list_tracks().unwrap();
    assert_eq!(tracks.len(), 2);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

