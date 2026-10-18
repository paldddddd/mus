use media::{find_tool, probe_media};
use std::path::Path;

#[test]
fn test_find_tools() {
    let ffmpeg = find_tool("ffmpeg");
    assert!(ffmpeg.is_some(), "ffmpeg must be found in deps or PATH");

    let ffprobe = find_tool("ffprobe");
    assert!(ffprobe.is_some(), "ffprobe must be found in deps or PATH");
}

#[test]
fn test_probe_sample_wallpaper() {
    let bmp = Path::new("test_wallpaper.bmp");
    if bmp.exists() {
        let probe = probe_media(bmp).expect("Failed to probe test_wallpaper.bmp");
        assert_eq!(probe.width, 1920);
        assert_eq!(probe.height, 1080);
        println!("Probed test_wallpaper.bmp successfully: {:?}", probe);
    }
}
