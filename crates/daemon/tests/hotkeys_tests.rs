use common::HotkeysConfig;

#[test]
fn test_hotkeys_config_defaults_and_serde() {
    let def = HotkeysConfig::default();
    assert_eq!(def.toggle_playback, "Ctrl+Alt+Space");
    assert_eq!(def.next_track, "Ctrl+Alt+Right");
    assert_eq!(def.prev_track, "Ctrl+Alt+Left");
    assert_eq!(def.toggle_wallpaper_pause, "Ctrl+Alt+P");
    assert_eq!(def.next_wallpaper, "Ctrl+Alt+W");
    assert_eq!(def.toggle_widget, "Ctrl+Alt+M");

    // Serialize to JSON and parse back
    let json = serde_json::to_string(&def).unwrap();
    let parsed: HotkeysConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(def, parsed);
}

#[test]
fn test_hotkeys_config_custom_values() {
    let custom_json = r#"{
        "toggle_playback": "Ctrl+Shift+Space",
        "next_track": "Alt+Right",
        "prev_track": "Alt+Left",
        "toggle_wallpaper_pause": "Ctrl+Shift+P",
        "next_wallpaper": "Ctrl+Shift+N",
        "toggle_widget": "Ctrl+Shift+W"
    }"#;

    let parsed: HotkeysConfig = serde_json::from_str(custom_json).unwrap();
    assert_eq!(parsed.toggle_playback, "Ctrl+Shift+Space");
    assert_eq!(parsed.next_track, "Alt+Right");
    assert_eq!(parsed.next_wallpaper, "Ctrl+Shift+N");
}
