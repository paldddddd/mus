use common::{MusicConfig, MusicSource};
use library::Database;

#[test]
fn test_stage2_library_and_track_id_contract() {
    let temp_dir = std::env::temp_dir().join(format!("wallmus_stage2_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let song1 = temp_dir.join("song1.mp3");
    let song2 = temp_dir.join("song2.flac");

    std::fs::write(&song1, b"ID3\x04\x00\x00\x00\x00\x00\x00audio bytes here").unwrap();
    std::fs::write(&song2, b"fLaCaudio bytes for flac").unwrap();

    let id1 = library::compute_track_id(&song1).unwrap();
    let id2 = library::compute_track_id(&song2).unwrap();
    assert_ne!(id1, id2);

    let db = Database::open_in_memory().unwrap();
    let summary = library::scanner::scan_folders(
        &db,
        &[temp_dir.to_string_lossy().to_string()],
        None::<fn(library::ScanProgress)>,
    )
    .unwrap();

    assert_eq!(summary.added, 2);
    let tracks = db.list_tracks().unwrap();
    assert_eq!(tracks.len(), 2);

    let found = db.get_track(&id1).unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().path, song1.to_string_lossy().to_string());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_external_track_normalization_contract() {
    use common::normalize::{external_track_key, normalize_name};

    assert_eq!(
        external_track_key("Pink Floyd", "Comfortably Numb - Live 1994"),
        "ext:pink floyd|comfortably numb"
    );
    assert_eq!(
        external_track_key("The Beatles", "Yesterday (Remastered 2009)"),
        "ext:the beatles|yesterday"
    );
    assert_eq!(
        normalize_name("Кино - Группа крови"),
        "кино группа крови"
    );
}

#[test]
fn test_music_config_default_and_source_switch() {
    let mut cfg = MusicConfig::default();
    assert_eq!(cfg.source, MusicSource::Local);
    assert!(cfg.folders.is_empty());

    cfg.source = MusicSource::External;
    assert_eq!(cfg.source, MusicSource::External);
}
