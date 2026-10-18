use audio_engine::{
    LocalPlayer, LocalTrack, RepeatMode, SmtcManager, Visualizer, WallpaperAudio,
    WallpaperAudioConfig,
};

#[test]
fn test_smtc_manager_initialization() {
    let smtc = SmtcManager::new();
    let track = smtc.get_current_track();
    assert!(track.is_ok(), "SMTC query should not fail: {:?}", track.err());
}

#[test]
fn test_visualizer_creation_and_spectrum() {
    let visualizer = Visualizer::new();

    for num_bars in [16, 32, 64] {
        let spectrum = visualizer.get_spectrum(num_bars);
        assert_eq!(spectrum.len(), num_bars);
        for &val in &spectrum {
            assert!(
                (0.0..=1.0).contains(&val),
                "Spectrum value {} should be between 0.0 and 1.0",
                val
            );
        }
    }

    visualizer.set_paused(true);
    let spectrum_paused = visualizer.get_spectrum(32);
    assert_eq!(spectrum_paused.len(), 32);

    visualizer.set_paused(false);
}

#[test]
fn test_wallpaper_audio_fade_and_mute() {
    let audio = WallpaperAudio::new(WallpaperAudioConfig {
        volume: 75,
        muted: false,
        crossfade_ms: 100,
    });

    assert!((audio.get_effective_volume() - 0.75).abs() < 0.01);
    assert!(!audio.is_muted());

    audio.set_muted(true);
    assert!(audio.is_muted());
    assert_eq!(audio.get_effective_volume(), 0.0);

    audio.set_muted(false);
    assert!(!audio.is_muted());
    assert!((audio.get_effective_volume() - 0.75).abs() < 0.01);

    audio.set_volume(50);
    assert!((audio.get_effective_volume() - 0.50).abs() < 0.01);

    audio.fade_out();
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert_eq!(audio.get_effective_volume(), 0.0);

    audio.fade_in();
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!((audio.get_effective_volume() - 0.50).abs() < 0.05);
}

#[test]
fn test_local_player_queue_and_state() {
    let player = LocalPlayer::new();

    assert!(!player.is_playing());
    assert!((player.get_volume() - 0.8).abs() < 0.01);

    player.set_volume(0.4);
    assert!((player.get_volume() - 0.4).abs() < 0.01);

    assert_eq!(player.get_repeat_mode(), RepeatMode::Off);
    player.set_repeat_mode(RepeatMode::All);
    assert_eq!(player.get_repeat_mode(), RepeatMode::All);

    assert!(!player.is_shuffle());
    player.set_shuffle(true);
    assert!(player.is_shuffle());

    let tracks = vec![
        LocalTrack {
            id: "t1".to_string(),
            title: "Track 1".to_string(),
            artist: "Artist 1".to_string(),
            path: "C:\\fake\\track1.mp3".to_string(),
            duration_ms: 180000,
        },
        LocalTrack {
            id: "t2".to_string(),
            title: "Track 2".to_string(),
            artist: "Artist 2".to_string(),
            path: "C:\\fake\\track2.mp3".to_string(),
            duration_ms: 210000,
        },
    ];

    let _ = player.set_queue(tracks, None);
    assert!(!player.is_playing());
}
