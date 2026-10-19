use common::{MonitorInfo, WallpaperItem};
use desktop_host::DesktopHostWindow;
use render::{ActiveLayer, DualLayerHost};
use std::time::Duration;

#[test]
fn test_crossfade_smoothstep_curve() {
    let smoothstep = |t: f32| -> f32 {
        let clamped = t.clamp(0.0, 1.0);
        clamped * clamped * (3.0 - 2.0 * clamped)
    };

    assert_eq!(smoothstep(0.0), 0.0);
    assert_eq!(smoothstep(0.5), 0.5);
    assert_eq!(smoothstep(1.0), 1.0);

    // Verify monotonic increase
    let mut prev = 0.0;
    for step in 0..=100 {
        let t = step as f32 / 100.0;
        let v = smoothstep(t);
        assert!(v >= prev);
        prev = v;
    }
}

#[test]
fn test_desktop_host_window_alpha() {
    desktop_host::ensure_default_desktop();
    let mon = MonitorInfo {
        id: "test_mon".to_string(),
        name: "Test Monitor".to_string(),
        x: 0,
        y: 0,
        width: 800,
        height: 600,
        dpi_x: 96,
        dpi_y: 96,
        scale_factor: 1.0,
        is_primary: true,
    };

    let host = DesktopHostWindow::create(mon, desktop_host::RenderMode::GdiPattern);
    if let Ok(host) = host {
        assert!(host.set_alpha(128).is_ok());
        assert!(host.set_alpha(0).is_ok());
        assert!(host.set_alpha(255).is_ok());
    }
}

#[test]
fn test_dual_layer_host_instant_and_animated_transition() {
    desktop_host::ensure_default_desktop();
    let mon = MonitorInfo {
        id: "test_mon_dual".to_string(),
        name: "Test Monitor Dual".to_string(),
        x: 0,
        y: 0,
        width: 800,
        height: 600,
        dpi_x: 96,
        dpi_y: 96,
        scale_factor: 1.0,
        is_primary: true,
    };

    let host = DualLayerHost::new(mon);
    if let Ok(mut host) = host {
        assert_eq!(host.active_layer, ActiveLayer::A);

        let wp1 = WallpaperItem {
            id: "wp_1".to_string(),
            name: "Wallpaper 1".to_string(),
            kind: "video".to_string(),
            source_path: "C:\\fake\\1.mp4".to_string(),
            processed_path: "C:\\fake\\1.mp4".to_string(),
            thumb_path: "C:\\fake\\1.jpg".to_string(),
            fit: "cover".to_string(),
            fps_cap: Some(30),
            width: Some(1920),
            height: Some(1080),
            duration_ms: Some(10000),
            created_at: 0,
        };

        let wp2 = WallpaperItem {
            id: "wp_2".to_string(),
            name: "Wallpaper 2".to_string(),
            kind: "video".to_string(),
            source_path: "C:\\fake\\2.mp4".to_string(),
            processed_path: "C:\\fake\\2.mp4".to_string(),
            thumb_path: "C:\\fake\\2.jpg".to_string(),
            fit: "cover".to_string(),
            fps_cap: Some(60),
            width: Some(1920),
            height: Some(1080),
            duration_ms: Some(15000),
            created_at: 0,
        };

        // Instant transition (duration = 0)
        let _ = host.start_transition(&wp1, 0);
        assert_eq!(host.current_wallpaper_id.as_deref(), Some("wp_1"));
        assert_eq!(host.active_layer, ActiveLayer::B);

        // Timed transition (duration = 100 ms)
        let _ = host.start_transition(&wp2, 100);
        assert_eq!(host.current_wallpaper_id.as_deref(), Some("wp_2"));

        // While animation is progressing
        let animating = host.update();
        // Wait for animation to finish
        std::thread::sleep(Duration::from_millis(150));
        let finished = !host.update();
        assert!(finished);
        assert_eq!(host.active_layer, ActiveLayer::A);
        let _ = animating;
    }
}
