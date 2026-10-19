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
    painter.play_button_rect = (50.0, 50.0, 30.0, 30.0);
    painter.prev_button_rect = (10.0, 50.0, 30.0, 30.0);
    painter.next_button_rect = (90.0, 50.0, 30.0, 30.0);

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
