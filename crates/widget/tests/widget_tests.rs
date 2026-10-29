use widget::{ArgbSurface, WidgetHitTest, WidgetPainter};

#[test]
fn test_argb_surface_creation_and_premul() {
    let mut surface = ArgbSurface::new(100, 50);
    assert_eq!(surface.width, 100);
    assert_eq!(surface.height, 50);
    assert_eq!(surface.pixels.len(), 5000);

    // Test premul ARGB: 50% alpha white (A=128, R=255, G=255, B=255)
    let premul = ArgbSurface::premul_argb(128, 255, 255, 255);
    let a = (premul >> 24) & 0xFF;
    let r = (premul >> 16) & 0xFF;
    assert_eq!(a, 128);
    assert_eq!(r, 128); // Pre-multiplied

    surface.blend_pixel(10, 10, premul);
    assert_eq!(surface.pixels[10 * 100 + 10], premul);

    surface.clear();
    assert_eq!(surface.pixels[10 * 100 + 10], 0);
}

#[test]
fn test_rounded_rect_and_border() {
    let mut surface = ArgbSurface::new(200, 100);
    let col = ArgbSurface::premul_argb(255, 29, 185, 84); // Spotify green

    surface.draw_rounded_rect(10.0, 10.0, 180.0, 80.0, 12.0, col);

    // Inside rect should be filled with green
    let center_color = surface.pixels[50 * 200 + 100];
    assert_eq!(center_color, col);

    // Border drawing
    let border_col = ArgbSurface::premul_argb(200, 255, 255, 255);
    surface.draw_rounded_border(10.0, 10.0, 180.0, 80.0, 12.0, border_col);
}

#[test]
fn test_spectrum_bars_drawing() {
    let mut surface = ArgbSurface::new(200, 60);
    let spectrum = vec![0.1, 0.5, 0.8, 1.0, 0.4, 0.2];
    let col = ArgbSurface::premul_argb(255, 0, 242, 254);

    surface.draw_spectrum_bars(10.0, 10.0, 180.0, 40.0, &spectrum, col);

    // At least some pixels should be painted
    let non_zero_count = surface.pixels.iter().filter(|&&p| p != 0).count();
    assert!(non_zero_count > 100, "Spectrum bars should be rendered");
}

#[test]
fn test_widget_painter_hit_testing() {
    let mut painter = WidgetPainter::new();
    painter.layout.play = (50.0, 50.0, 30.0, 30.0);
    painter.layout.prev = (10.0, 50.0, 30.0, 30.0);
    painter.layout.next = (90.0, 50.0, 30.0, 30.0);

    assert_eq!(painter.hit_test(60.0, 60.0), WidgetHitTest::PlayPause);
    assert_eq!(painter.hit_test(20.0, 60.0), WidgetHitTest::Previous);
    assert_eq!(painter.hit_test(100.0, 60.0), WidgetHitTest::Next);
    assert_eq!(painter.hit_test(150.0, 150.0), WidgetHitTest::Drag);
}

#[test]
fn test_themes_geometry_rendering() {
    let mut surface = ArgbSurface::new(380, 150);
    let spectrum = vec![0.5f32; 24];

    // Verify all geometry components render cleanly into surface
    surface.draw_rounded_rect(0.0, 0.0, 380.0, 150.0, 14.0, 0xDE121212);
    surface.draw_circle(48.0, 75.0, 38.0, 0xFF111111);
    surface.draw_playback_icon(150.0, 75.0, 12.0, true, 0xFFFFFFFF);
    surface.draw_playback_icon(150.0, 75.0, 12.0, false, 0xFFFFFFFF);
    surface.draw_step_icon(120.0, 75.0, 12.0, false, 0xFFB3B3B3);
    surface.draw_step_icon(180.0, 75.0, 12.0, true, 0xFFB3B3B3);
    surface.draw_progress_bar(14.0, 120.0, 350.0, 4.0, 0.45, 0x803E3E3E, 0xFF1DB954);
    surface.draw_spectrum_bars(14.0, 130.0, 350.0, 16.0, &spectrum, 0xFF1DB954);

    let painted = surface.pixels.iter().filter(|&&p| p != 0).count();
    assert!(painted > 5000);
}

#[test]
fn test_surface_draw_line_bresenham() {
    let mut surface = ArgbSurface::new(100, 100);
    let color = ArgbSurface::premul_argb(255, 255, 0, 0);

    // Diagonal line from (10, 10) to (90, 90)
    surface.draw_line(10.0, 10.0, 90.0, 90.0, color);

    let painted = surface.pixels.iter().filter(|&&p| p == color).count();
    assert!(painted >= 80, "Diagonal line should paint at least 80 pixels (got {})", painted);

    // Horizontal line
    surface.clear();
    surface.draw_line(0.0, 50.0, 99.0, 50.0, color);
    let painted_h = surface.pixels.iter().filter(|&&p| p == color).count();
    assert_eq!(painted_h, 100, "Horizontal line across 100px should paint exactly 100 pixels");
}

#[test]
fn test_wallpaper_visualizer_config_defaults() {
    let cfg = common::WallpaperVisualizerConfig::default();
    assert!(cfg.enabled);
    assert_eq!(cfg.style, "bars");
    assert_eq!(cfg.bars_count, 48);
    assert_eq!(cfg.height_px, 160);
    assert_eq!(cfg.accent_color, "spotify");
    assert!(cfg.opacity > 0.0 && cfg.opacity <= 1.0);
}

#[test]
fn test_adaptive_vertical_and_horizontal_orientations() {
    use widget::{format_time, Layout, Orientation};

    // Vertical orientation: width < height
    let vert = Layout::compute("spotify", 280.0, 420.0);
    assert_eq!(vert.orientation, Orientation::Vertical);
    assert!(vert.cover.2 > 100.0, "Cover should be large in vertical mode");
    assert!(vert.spectrum.3 > 20.0, "Spectrum should be at the bottom");
    assert!(vert.progress.2 > 150.0, "Progress should span card width");

    // Horizontal orientation: width >= height
    let horiz = Layout::compute("spotify", 480.0, 180.0);
    assert_eq!(horiz.orientation, Orientation::Horizontal);
    assert!(horiz.cover.0 < horiz.title.0, "Cover should be on the left of title");

    // Timecode formatting
    assert_eq!(format_time(1000), "0:01");
    assert_eq!(format_time(170000), "2:50");
    assert_eq!(format_time(3665000), "1:01:05");
}

#[test]
fn test_progress_seek_and_volume_hit_testing() {
    let mut painter = WidgetPainter::new();
    painter.layout.progress = (20.0, 300.0, 200.0, 6.0);
    painter.layout.vol_bar = (180.0, 270.0, 40.0, 6.0);
    painter.layout.vol_icon = (160.0, 270.0, 16.0, 16.0);

    // Hit test progress seek (midway = 50%)
    match painter.hit_test(120.0, 302.0) {
        WidgetHitTest::ProgressSeek(ratio) => {
            assert!((ratio - 0.50).abs() < 0.02, "Ratio should be near 0.50");
        }
        other => panic!("Expected ProgressSeek, got {:?}", other),
    }

    // Hit test volume seek (75%)
    match painter.hit_test(210.0, 272.0) {
        WidgetHitTest::VolumeSeek(ratio) => {
            assert!((ratio - 0.75).abs() < 0.05, "Ratio should be near 0.75");
        }
        other => panic!("Expected VolumeSeek, got {:?}", other),
    }

    // Hit test volume icon (mute toggle)
    assert_eq!(painter.hit_test(165.0, 275.0), WidgetHitTest::VolumeMuteToggle);
}

#[test]
fn test_all_four_visualizer_styles_render() {
    let spectrum = vec![0.2, 0.5, 0.9, 0.7, 0.4, 0.8, 0.3, 0.6];
    let peaks = vec![0.3, 0.6, 0.95, 0.8, 0.5, 0.85, 0.4, 0.7];
    let bar_col = ArgbSurface::premul_argb(220, 0, 245, 212);
    let peak_col = ArgbSurface::premul_argb(255, 255, 255, 255);

    for style in ["bars", "wave", "mirror", "dots"] {
        let mut surface = ArgbSurface::new(240, 60);
        surface.draw_visualizer_styled(style, 10.0, 8.0, 220.0, 44.0, &spectrum, &peaks, bar_col, peak_col);
        let non_zero = surface.pixels.iter().filter(|&&p| p != 0).count();
        assert!(non_zero > 50, "Visualizer style '{}' should paint pixels (painted {})", style, non_zero);
    }
}

#[test]
fn test_lrc_synced_lyrics_parser() {
    let sample_lrc = "[00:12.50] First karaoke line\n[01:05.20] Second line of song\n[02:10.00] Final chorus";
    let parsed = widget::lyrics::parse_lrc(sample_lrc);
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].time_ms, 12_500);
    assert_eq!(parsed[0].text, "First karaoke line");
    assert_eq!(parsed[1].time_ms, 65_200);
    assert_eq!(parsed[1].text, "Second line of song");
    assert_eq!(parsed[2].time_ms, 130_000);
    assert_eq!(parsed[2].text, "Final chorus");
}

#[test]
fn test_wallpaper_visualizer_window_lifecycle() {
    let mon = common::MonitorInfo {
        id: "mon1".to_string(),
        name: "TestMonitor".to_string(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        is_primary: true,
        scale_factor: 1.0,
        dpi_x: 96,
        dpi_y: 96,
    };
    let cfg = common::WallpaperVisualizerConfig {
        enabled: true,
        style: "bars".to_string(),
        bars_count: 32,
        height_px: 120,
        opacity: 0.85,
        accent_color: "spotify".to_string(),
    };

    let mut vis = widget::WallpaperVisualizerWindow::create(mon, &cfg).expect("Create visualizer window");
    let spectrum = vec![0.5; 32];
    vis.update(&spectrum, &cfg, false);
    // test blank
    vis.update(&[], &cfg, true);
}


