use crate::gdi_plus::GdiPlusRenderer;
use crate::surface::ArgbSurface;
use audio_engine::{PlaybackStatus, TrackMetadata};
use common::WidgetConfig;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetHitTest {
    None,
    Drag,
    PlayPause,
    Previous,
    Next,
}

pub struct WidgetPainter {
    pub play_button_rect: (f32, f32, f32, f32),
    pub prev_button_rect: (f32, f32, f32, f32),
    pub next_button_rect: (f32, f32, f32, f32),
    pub marquee_offset: f32,
    pub vinyl_angle: f32,
}

impl WidgetPainter {
    pub fn new() -> Self {
        Self {
            play_button_rect: (0.0, 0.0, 0.0, 0.0),
            prev_button_rect: (0.0, 0.0, 0.0, 0.0),
            next_button_rect: (0.0, 0.0, 0.0, 0.0),
            marquee_offset: 0.0,
            vinyl_angle: 0.0,
        }
    }

    pub fn paint(
        &mut self,
        surface: &mut ArgbSurface,
        gdi: &GdiPlusRenderer,
        config: &WidgetConfig,
        track: Option<&TrackMetadata>,
        spectrum: &[f32],
    ) {
        self.paint_surface(surface, config, track, spectrum);
        self.paint_foreground(gdi, config, track);
    }

    pub fn paint_surface(
        &mut self,
        surface: &mut ArgbSurface,
        config: &WidgetConfig,
        track: Option<&TrackMetadata>,
        spectrum: &[f32],
    ) {
        surface.clear();

        let w = surface.width as f32;
        let h = surface.height as f32;

        match config.theme.as_str() {
            "pill" => self.paint_pill_surface(surface, w, h, track, spectrum),
            "vinyl" => self.paint_vinyl_surface(surface, w, h, track, spectrum),
            "glass" => self.paint_glass_surface(surface, w, h, track, spectrum),
            _ => self.paint_spotify_surface(surface, w, h, track, spectrum),
        }
    }

    pub fn paint_foreground(
        &mut self,
        gdi: &GdiPlusRenderer,
        config: &WidgetConfig,
        track: Option<&TrackMetadata>,
    ) {
        let w = config.width as f32;
        let h = config.height as f32;

        match config.theme.as_str() {
            "pill" => self.paint_pill_foreground(gdi, w, h, track),
            "vinyl" => self.paint_vinyl_foreground(gdi, w, h, track),
            "glass" => self.paint_glass_foreground(gdi, w, h, track),
            _ => self.paint_spotify_foreground(gdi, w, h, track),
        }
    }

    fn paint_spotify_surface(
        &mut self,
        surface: &mut ArgbSurface,
        w: f32,
        h: f32,
        track: Option<&TrackMetadata>,
        spectrum: &[f32],
    ) {
        // 1. Sleek Spotify Dark Card Background (92% opacity #121212)
        let bg_color = ArgbSurface::premul_argb(235, 18, 18, 18);
        let border_color = ArgbSurface::premul_argb(50, 255, 255, 255);
        surface.draw_rounded_rect(0.0, 0.0, w, h, 14.0, bg_color);
        surface.draw_rounded_border(0.0, 0.0, w, h, 14.0, border_color);

        // 2. Album Cover Art Background Placeholder
        let cover_size = 64.0f32;
        let cover_x = 14.0f32;
        let cover_y = 14.0f32;

        let ph_color = ArgbSurface::premul_argb(240, 29, 185, 84);
        surface.draw_rounded_rect(cover_x, cover_y, cover_size, cover_size, 8.0, ph_color);
        surface.draw_circle(cover_x + 22.0, cover_y + 44.0, 6.0, 0xFF121212);
        surface.draw_circle(cover_x + 42.0, cover_y + 36.0, 6.0, 0xFF121212);
        surface.draw_rounded_rect(cover_x + 26.0, cover_y + 20.0, 4.0, 26.0, 1.0, 0xFF121212);
        surface.draw_rounded_rect(cover_x + 46.0, cover_y + 14.0, 4.0, 24.0, 1.0, 0xFF121212);
        surface.draw_rounded_rect(cover_x + 26.0, cover_y + 14.0, 24.0, 6.0, 1.0, 0xFF121212);
        surface.draw_rounded_border(cover_x, cover_y, cover_size, cover_size, 8.0, border_color);

        // 3. Interactive Playback Controls
        let text_x = cover_x + cover_size + 14.0;
        let is_playing = track.map(|t| t.playback_status == PlaybackStatus::Playing).unwrap_or(false);
        let btn_y = 56.0f32;
        let prev_x = text_x + 4.0;
        let play_x = prev_x + 28.0;
        let next_x = play_x + 32.0;

        self.prev_button_rect = (prev_x - 8.0, btn_y - 8.0, 22.0, 22.0);
        surface.draw_step_icon(prev_x + 3.0, btn_y + 3.0, 12.0, false, 0xFFB3B3B3);

        self.play_button_rect = (play_x - 4.0, btn_y - 6.0, 24.0, 24.0);
        let play_bg = ArgbSurface::premul_argb(255, 29, 185, 84);
        surface.draw_circle(play_x + 8.0, btn_y + 6.0, 11.0, play_bg);
        surface.draw_playback_icon(play_x + 8.0, btn_y + 6.0, 9.0, is_playing, 0xFF000000);

        self.next_button_rect = (next_x - 8.0, btn_y - 8.0, 22.0, 22.0);
        surface.draw_step_icon(next_x + 3.0, btn_y + 3.0, 12.0, true, 0xFFB3B3B3);

        // 4. Timeline Progress Bar
        let bar_x = 14.0f32;
        let bar_y = 86.0f32;
        let bar_w = w - 28.0;
        let bar_h = 3.5f32;

        let progress = if let Some(t) = track {
            match (t.position_ms, t.duration_ms) {
                (Some(p), Some(d)) if d > 0 => p as f32 / d as f32,
                _ => 0.0,
            }
        } else {
            0.0
        };

        let prog_bg = ArgbSurface::premul_argb(120, 80, 80, 80);
        let prog_fill = ArgbSurface::premul_argb(255, 29, 185, 84);
        surface.draw_progress_bar(bar_x, bar_y, bar_w, bar_h, progress, prog_bg, prog_fill);

        // 5. Spectrum Visualizer
        let spec_y = 96.0f32;
        let spec_h = (h - spec_y - 10.0).max(10.0);
        let spec_col = ArgbSurface::premul_argb(220, 29, 185, 84);
        surface.draw_spectrum_bars(bar_x, spec_y, bar_w, spec_h, spectrum, spec_col);
    }

    fn paint_spotify_foreground(
        &mut self,
        gdi: &GdiPlusRenderer,
        w: f32,
        _h: f32,
        track: Option<&TrackMetadata>,
    ) {
        let cover_size = 64.0f32;
        let cover_x = 14.0f32;
        let cover_y = 14.0f32;

        if let Some(t) = track {
            if let Some(ref cp) = t.cover_path {
                let path = Path::new(cp);
                if path.exists() {
                    let _ = gdi.draw_image_file(path, cover_x as i32, cover_y as i32, cover_size as i32, cover_size as i32);
                }
            }
        }

        let text_x = cover_x + cover_size + 14.0;
        let text_max_w = (w - text_x - 14.0).max(50.0);

        let title = track.map(|t| t.title.as_str()).unwrap_or("wallmus Music");
        let artist = track.map(|t| t.artist.as_str()).unwrap_or("Ready to play");

        gdi.draw_text(title, "Segoe UI", 10.5, true, 0xFFFFFFFF, text_x, 14.0, text_max_w, 20.0);
        gdi.draw_text(artist, "Segoe UI", 8.5, false, 0xFFB3B3B3, text_x, 34.0, text_max_w, 18.0);
    }

    fn paint_pill_surface(
        &mut self,
        surface: &mut ArgbSurface,
        w: f32,
        h: f32,
        track: Option<&TrackMetadata>,
        spectrum: &[f32],
    ) {
        let r = (h / 2.0).min(30.0);
        let bg_color = ArgbSurface::premul_argb(225, 24, 28, 36);
        let border_color = ArgbSurface::premul_argb(70, 255, 255, 255);
        surface.draw_rounded_rect(0.0, 0.0, w, h, r, bg_color);
        surface.draw_rounded_border(0.0, 0.0, w, h, r, border_color);

        let pad = 8.0f32;
        let cover_size = (h - pad * 2.0).max(20.0);
        let is_playing = track.map(|t| t.playback_status == PlaybackStatus::Playing).unwrap_or(false);

        // Mini cover placeholder
        surface.draw_rounded_rect(pad, pad, cover_size, cover_size, (r - 4.0).max(4.0), 0xFF1DB954);

        // Compact play button
        let play_x = w - 70.0;
        let play_y = h / 2.0;
        self.play_button_rect = (play_x - 12.0, play_y - 12.0, 24.0, 24.0);
        surface.draw_circle(play_x, play_y, 11.0, 0xFF1DB954);
        surface.draw_playback_icon(play_x, play_y, 9.0, is_playing, 0xFF000000);

        // Mini visualizer on right
        let spec_x = w - 44.0;
        let spec_w = 34.0;
        surface.draw_spectrum_bars(spec_x, 12.0, spec_w, h - 24.0, &spectrum[..spectrum.len().min(8)], 0xFF1DB954);
    }

    fn paint_pill_foreground(
        &mut self,
        gdi: &GdiPlusRenderer,
        w: f32,
        h: f32,
        track: Option<&TrackMetadata>,
    ) {
        let pad = 8.0f32;
        let cover_size = (h - pad * 2.0).max(20.0);

        if let Some(t) = track {
            if let Some(ref cp) = t.cover_path {
                let path = Path::new(cp);
                if path.exists() {
                    let _ = gdi.draw_image_file(path, pad as i32, pad as i32, cover_size as i32, cover_size as i32);
                }
            }
        }

        let title = track.map(|t| t.title.as_str()).unwrap_or("wallmus");
        let artist = track.map(|t| t.artist.as_str()).unwrap_or("Ready to play");
        let text_x = pad + cover_size + 10.0;
        let text_w = (w - text_x - 90.0).max(40.0);

        gdi.draw_text(title, "Segoe UI", 10.0, true, 0xFFFFFFFF, text_x, 10.0, text_w, 20.0);
        gdi.draw_text(artist, "Segoe UI", 8.5, false, 0xFFB3B3B3, text_x, 32.0, text_w, 18.0);
    }

    fn paint_vinyl_surface(
        &mut self,
        surface: &mut ArgbSurface,
        w: f32,
        h: f32,
        track: Option<&TrackMetadata>,
        spectrum: &[f32],
    ) {
        let bg_color = ArgbSurface::premul_argb(225, 16, 16, 20);
        let border_color = ArgbSurface::premul_argb(50, 255, 255, 255);
        surface.draw_rounded_rect(0.0, 0.0, w, h, 14.0, bg_color);
        surface.draw_rounded_border(0.0, 0.0, w, h, 14.0, border_color);

        let disc_cx = 48.0f32;
        let disc_cy = h / 2.0;
        let disc_r = 38.0f32;

        surface.draw_circle(disc_cx, disc_cy, disc_r, 0xFF111111);
        surface.draw_circle(disc_cx, disc_cy, disc_r - 4.0, 0xFF181818);
        surface.draw_circle(disc_cx, disc_cy, disc_r - 10.0, 0xFF141414);
        surface.draw_circle(disc_cx, disc_cy, disc_r - 16.0, 0xFF1A1A1A);
        surface.draw_circle(disc_cx, disc_cy, 15.0, 0xFFD4AF37);
        surface.draw_circle(disc_cx, disc_cy, 4.0, 0xFF000000);

        let is_playing = track.map(|t| t.playback_status == PlaybackStatus::Playing).unwrap_or(false);
        let text_x = 98.0f32;
        let btn_y = 66.0f32;

        self.prev_button_rect = (text_x, btn_y - 6.0, 20.0, 20.0);
        surface.draw_step_icon(text_x + 8.0, btn_y + 4.0, 11.0, false, 0xFFD4AF37);

        self.play_button_rect = (text_x + 28.0, btn_y - 6.0, 24.0, 24.0);
        surface.draw_circle(text_x + 38.0, btn_y + 5.0, 11.0, 0xFFD4AF37);
        surface.draw_playback_icon(text_x + 38.0, btn_y + 5.0, 9.0, is_playing, 0xFF000000);

        self.next_button_rect = (text_x + 58.0, btn_y - 6.0, 20.0, 20.0);
        surface.draw_step_icon(text_x + 68.0, btn_y + 4.0, 11.0, true, 0xFFD4AF37);

        surface.draw_spectrum_bars(text_x, 98.0, w - text_x - 14.0, h - 110.0, spectrum, 0xFFD4AF37);
    }

    fn paint_vinyl_foreground(
        &mut self,
        gdi: &GdiPlusRenderer,
        w: f32,
        _h: f32,
        track: Option<&TrackMetadata>,
    ) {
        let text_x = 98.0f32;
        let text_w = w - text_x - 14.0;
        let title = track.map(|t| t.title.as_str()).unwrap_or("Vinyl Record");
        let artist = track.map(|t| t.artist.as_str()).unwrap_or("Analog Warmth");

        gdi.draw_text(title, "Segoe UI", 11.0, true, 0xFFE0E0E0, text_x, 16.0, text_w, 22.0);
        gdi.draw_text(artist, "Segoe UI", 9.0, false, 0xFFD4AF37, text_x, 38.0, text_w, 18.0);
    }

    fn paint_glass_surface(
        &mut self,
        surface: &mut ArgbSurface,
        w: f32,
        h: f32,
        track: Option<&TrackMetadata>,
        spectrum: &[f32],
    ) {
        let bg_color = ArgbSurface::premul_argb(180, 20, 28, 38);
        let border_color = ArgbSurface::premul_argb(180, 0, 242, 254);
        surface.draw_rounded_rect(0.0, 0.0, w, h, 16.0, bg_color);
        surface.draw_rounded_border(0.0, 0.0, w, h, 16.0, border_color);

        let cover_size = 60.0f32;
        surface.draw_rounded_rect(16.0, 16.0, cover_size, cover_size, 10.0, 0xFF00F2FE);

        let is_playing = track.map(|t| t.playback_status == PlaybackStatus::Playing).unwrap_or(false);
        let text_x = 88.0f32;
        let btn_y = 66.0f32;

        self.play_button_rect = (text_x + 28.0, btn_y - 6.0, 24.0, 24.0);
        surface.draw_circle(text_x + 38.0, btn_y + 5.0, 11.0, 0xFF00F2FE);
        surface.draw_playback_icon(text_x + 38.0, btn_y + 5.0, 9.0, is_playing, 0xFF000000);

        surface.draw_spectrum_bars(16.0, 98.0, w - 32.0, h - 112.0, spectrum, 0xFF00F2FE);
    }

    fn paint_glass_foreground(
        &mut self,
        gdi: &GdiPlusRenderer,
        w: f32,
        _h: f32,
        track: Option<&TrackMetadata>,
    ) {
        let text_x = 88.0f32;
        let text_w = w - text_x - 16.0;
        let title = track.map(|t| t.title.as_str()).unwrap_or("Glassmorphic");
        let artist = track.map(|t| t.artist.as_str()).unwrap_or("Aesthetic");

        gdi.draw_text(title, "Segoe UI", 11.0, true, 0xFFFFFFFF, text_x, 16.0, text_w, 22.0);
        gdi.draw_text(artist, "Segoe UI", 9.0, false, 0xFF00F2FE, text_x, 38.0, text_w, 18.0);
    }

    pub fn hit_test(&self, x: f32, y: f32) -> WidgetHitTest {
        let in_rect = |(rx, ry, rw, rh): (f32, f32, f32, f32)| {
            x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
        };

        if in_rect(self.play_button_rect) {
            WidgetHitTest::PlayPause
        } else if in_rect(self.prev_button_rect) {
            WidgetHitTest::Previous
        } else if in_rect(self.next_button_rect) {
            WidgetHitTest::Next
        } else {
            WidgetHitTest::Drag
        }
    }
}

impl Default for WidgetPainter {
    fn default() -> Self {
        Self::new()
    }
}
