//! Responsive widget painter.
//!
//! Automatically adapts between Vertical (W < H) and Horizontal (W >= H) modes.
//! Every element (cover, text, buttons, progress, spectrum, volume) is laid out proportionally
//! from the window dimensions so the widget scales smoothly to any size.

use crate::gdi_plus::{CachedImage, GdiPlusRenderer, TextAlign};
use crate::surface::ArgbSurface;
use audio_engine::{PlaybackStatus, TrackMetadata};
use common::WidgetConfig;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidgetHitTest {
    None,
    Drag,
    PlayPause,
    Previous,
    Next,
    VolumeUp,
    VolumeDown,
    ProgressSeek(f32),
    VolumeSeek(f32),
    VolumeMuteToggle,
    ToggleLyricsExpand,
    OpenSettings,
    ResizeGrip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

/// (x, y, width, height)
pub type Rect = (f32, f32, f32, f32);
const ZERO: Rect = (0.0, 0.0, 0.0, 0.0);

/// Minimum / maximum widget size in physical pixels.
pub const MIN_W: u32 = 240;
pub const MIN_H: u32 = 64;
pub const MAX_W: u32 = 1600;
pub const MAX_H: u32 = 1000;

#[derive(Clone, Copy)]
struct Theme {
    /// Card background (straight ARGB, converted to premultiplied when drawn)
    bg: (u8, u8, u8, u8),
    border: (u8, u8, u8, u8),
    /// Accent color, opaque 0xAARRGGBB
    accent: u32,
    /// Icon color on top of the accent (play button glyph)
    on_accent: u32,
    title: u32,
    subtitle: u32,
    icon: u32,
    pill: bool,
    circle_cover: bool,
    /// Corner radius relative to the scale unit
    radius: f32,
}

fn theme_for(name: &str) -> Theme {
    match name {
        "aurora" => Theme {
            bg: (232, 10, 14, 28),
            border: (130, 168, 85, 247), // Electric neon violet border
            accent: 0xFF00F5D4,          // Cyber mint
            on_accent: 0xFF050811,
            title: 0xFFFFFFFF,
            subtitle: 0xFFC4B5FD,        // Soft lavender
            icon: 0xFFE2E8F0,
            pill: false,
            circle_cover: false,
            radius: 20.0,
        },
        "vinyl" => Theme {
            bg: (235, 15, 17, 22),
            border: (50, 212, 175, 55),
            accent: 0xFFD4AF37,
            on_accent: 0xFF1A1410,
            title: 0xFFF3E9D2,
            subtitle: 0xFFC9A94F,
            icon: 0xFFE0C878,
            pill: false,
            circle_cover: true,
            radius: 18.0,
        },
        "glass" => Theme {
            bg: (165, 10, 16, 26),
            border: (120, 0, 245, 212),
            accent: 0xFF00F5D4,
            on_accent: 0xFF04141A,
            title: 0xFFFFFFFF,
            subtitle: 0xFF9DEBFF,
            icon: 0xFFCFF7FF,
            pill: false,
            circle_cover: false,
            radius: 20.0,
        },
        _ => Theme {
            // Modern Obsidian Glass / Cyber Mint default
            bg: (226, 12, 16, 23),
            border: (48, 255, 255, 255),
            accent: 0xFF00F5D4,
            on_accent: 0xFF0B0F14,
            title: 0xFFFFFFFF,
            subtitle: 0xFF94A3B8,
            icon: 0xFFCBD5E1,
            pill: false,
            circle_cover: false,
            radius: 18.0,
        },
    }
}

/// Convert a straight (non-premultiplied) opaque-ish ARGB to premultiplied with a new alpha.
fn premul(argb: u32, alpha: u8) -> u32 {
    let src_a = ((argb >> 24) & 0xFF) as u32;
    let a = (alpha as u32 * src_a / 255) as u8;
    ArgbSurface::premul_argb(
        a,
        ((argb >> 16) & 0xFF) as u8,
        ((argb >> 8) & 0xFF) as u8,
        (argb & 0xFF) as u8,
    )
}

fn center(r: Rect) -> (f32, f32) {
    (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0)
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    (
        lerp_f32(a.0, b.0, t),
        lerp_f32(a.1, b.1, t),
        lerp_f32(a.2, b.2, t),
        lerp_f32(a.3, b.3, t),
    )
}

/// Computed geometry for one frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct Layout {
    pub orientation: Orientation,
    pub scale: f32,
    pub pill: bool,
    pub card_radius: f32,
    pub cover: Rect,
    pub cover_radius: f32,
    pub cover_circle: bool,
    pub title: Rect,
    pub artist: Rect,
    pub title_px: f32,
    pub artist_px: f32,
    pub lyric_cur: Rect,
    pub lyric_next: Rect,
    pub lyric_third: Rect,
    pub lyric_cur_px: f32,
    pub lyric_next_px: f32,
    pub lyrics_btn: Rect,
    pub text_align: TextAlign,
    pub spectrum: Rect,
    pub progress: Rect,
    pub time_cur: Rect,
    pub time_dur: Rect,
    pub timecode_px: f32,
    pub prev: Rect,
    pub play: Rect,
    pub next: Rect,
    pub vol_icon: Rect,
    pub vol_bar: Rect,
    pub vol_down: Rect,
    pub vol_up: Rect,
    pub settings: Rect,
    pub grip: Rect,
}

impl Layout {
    /// Compute a responsive layout for the given theme, size, lyrics visibility, visualizer visibility, and expanded lyrics factor (0.0..=1.0).
    pub fn compute_ex(
        theme_name: &str,
        w: f32,
        h: f32,
        show_lyrics: bool,
        show_visualizer: bool,
        lyrics_expand: f32,
    ) -> Self {
        let theme = theme_for(theme_name);
        if w < h {
            Self::vertical(w, h, &theme, show_lyrics, show_visualizer, lyrics_expand)
        } else {
            Self::horizontal(w, h, &theme, show_lyrics, show_visualizer, lyrics_expand)
        }
    }

    /// Compute a responsive layout for the given theme and size.
    pub fn compute(theme_name: &str, w: f32, h: f32) -> Self {
        Self::compute_ex(theme_name, w, h, true, true, 0.0)
    }

    /// Vertical Layout (w < h): strictly faithful to user's mockup, with 2-line synced lyrics
    /// (current line highlighted + next line) and smooth on-widget "Expand Lyrics" mode where
    /// the cover shifts to the side/corner inside the widget's territory.
    fn vertical(
        w: f32,
        h: f32,
        theme: &Theme,
        show_lyrics: bool,
        show_visualizer: bool,
        lyrics_expand: f32,
    ) -> Self {
        let t = if show_lyrics { lyrics_expand.clamp(0.0, 1.0) } else { 0.0 };
        let smooth_t = t * t * (3.0 - 2.0 * t);
        let s = (h / 380.0).min(w / 280.0).clamp(0.5, 3.0);
        let pad = (12.0 * s).clamp(8.0, 20.0);
        let grip_sz = (22.0 * s).max(18.0);
        let lyr_btn_w = (54.0 * s).clamp(44.0, 76.0);
        let lyr_btn_h = (20.0 * s).clamp(18.0, 26.0);
        let lyrics_btn = if show_lyrics {
            (w - pad - lyr_btn_w, pad * 0.8, lyr_btn_w, lyr_btn_h)
        } else {
            ZERO
        };

        // 1. Bottom audio visualizer: occupies bottom ~16% of card height (if enabled)
        let (_spec_h, spectrum) = if show_visualizer {
            let sh = (h * 0.16).clamp(26.0 * s, 56.0 * s);
            (sh, (pad * 0.5, h - sh - 2.0 * s, w - pad, sh))
        } else {
            (0.0, ZERO)
        };

        // 2. Full-width seek line above the spectrum
        let bar_h = (4.0 * s).clamp(3.0, 6.0);
        let prog_y = if show_visualizer {
            spectrum.1 - 13.0 * s
        } else {
            h - pad - 8.0 * s
        };
        let prog_w = w - pad * 2.0;
        let progress = (pad, prog_y, prog_w, bar_h);

        // 3. Timestamps & volume row above the seek line (settings button cleanly placed on far right)
        let tc_h = (14.0 * s).max(12.0);
        let tc_w = (38.0 * s).max(30.0);
        let row_y = prog_y - tc_h - 4.0 * s;
        let settings_sz = (16.0 * s).clamp(14.0, 20.0);
        let settings = (w - pad - settings_sz, row_y + (tc_h - settings_sz) / 2.0, settings_sz, settings_sz);
        let time_cur = (pad, row_y, tc_w, tc_h);
        let time_dur = (settings.0 - tc_w - 5.0 * s, row_y, tc_w, tc_h);

        let vol_w = (34.0 * s).clamp(22.0, 48.0);
        let vol_icon_sz = (13.0 * s).max(11.0);
        let vol_bar = (time_dur.0 - vol_w - 5.0 * s, row_y + (tc_h - 4.0 * s) / 2.0, vol_w, 4.0 * s);
        let vol_icon = (vol_bar.0 - vol_icon_sz - 4.0 * s, row_y, vol_icon_sz, tc_h);

        // 4. Transport buttons row above timestamps
        let play_d = (30.0 * s).clamp(24.0, 42.0);
        let step_d = play_d * 0.75;
        let gap = 12.0 * s;
        let ctrl_y = row_y - play_d - 6.0 * s;
        let play = ((w - play_d) / 2.0, ctrl_y, play_d, play_d);
        let prev = (play.0 - step_d - gap, ctrl_y + (play_d - step_d) / 2.0, step_d, step_d);
        let next = (play.0 + play_d + gap, ctrl_y + (play_d - step_d) / 2.0, step_d, step_d);

        // --- NORMAL VERTICAL GEOMETRY (t = 0.0) ---
        let title_px_norm = (14.5 * s).clamp(11.5, 22.0);
        let artist_px_norm = (11.2 * s).clamp(9.5, 16.5);
        let lyr_cur_px_norm = (11.5 * s).clamp(9.5, 14.5);
        let lyr_next_px_norm = (9.8 * s).clamp(8.2, 12.0);

        let lyrics_block_h = if show_lyrics {
            lyr_cur_px_norm * 1.42 + lyr_next_px_norm * 1.38 + 6.0 * s
        } else {
            0.0
        };
        let text_block_h = title_px_norm * 1.32 + artist_px_norm * 1.28 + lyrics_block_h + 8.0 * s;
        let top_header_h = if show_lyrics { lyr_btn_h + 8.0 * s } else { 0.0 };
        let avail_cover_h = (ctrl_y - pad - top_header_h - text_block_h - 8.0 * s).max(44.0);
        let cover_sz_norm = (w - pad * 2.4).min(avail_cover_h).max(44.0);
        let cover_norm = ((w - cover_sz_norm) / 2.0, pad + top_header_h, cover_sz_norm, cover_sz_norm);

        let title_y_norm = cover_norm.1 + cover_norm.3 + 5.0 * s;
        let title_norm = (pad, title_y_norm, w - pad * 2.0, title_px_norm * 1.32);
        let artist_norm = (pad, title_norm.1 + title_norm.3 + 1.0 * s, w - pad * 2.0, artist_px_norm * 1.28);
        let lyr_cur_norm = if show_lyrics {
            (pad, artist_norm.1 + artist_norm.3 + 4.0 * s, w - pad * 2.0, lyr_cur_px_norm * 1.45)
        } else {
            ZERO
        };
        let lyr_next_norm = if show_lyrics {
            (pad, lyr_cur_norm.1 + lyr_cur_norm.3 + 2.0 * s, w - pad * 2.0, lyr_next_px_norm * 1.38)
        } else {
            ZERO
        };

        // --- EXPANDED LYRICS VERTICAL GEOMETRY (t = 1.0) ---
        // Cover shifts to top-left corner inside the widget's territory; title & artist sit beside it;
        // Center of widget becomes a large multi-line karaoke lyrics stage!
        let cover_sz_exp = (54.0 * s).clamp(40.0, 76.0);
        let cover_exp = (pad, pad, cover_sz_exp, cover_sz_exp);
        let hdr_x = pad + cover_sz_exp + 10.0 * s;
        let hdr_right = if show_lyrics { lyrics_btn.0 - 8.0 * s } else { w - pad };
        let hdr_w = (hdr_right - hdr_x).max(30.0);
        let title_px_exp = (13.5 * s).clamp(11.0, 19.0);
        let artist_px_exp = (11.0 * s).clamp(9.0, 15.5);
        let title_exp = (hdr_x, pad + 6.0 * s, hdr_w, title_px_exp * 1.32);
        let artist_exp = (hdr_x, title_exp.1 + title_exp.3 + 2.0 * s, hdr_w, artist_px_exp * 1.28);

        let stage_top = cover_exp.1 + cover_exp.3 + 10.0 * s;
        let stage_h = (ctrl_y - stage_top - 6.0 * s).max(48.0);
        let lyr_cur_px_exp = (14.5 * s).clamp(12.0, 18.0);
        let lyr_next_px_exp = (12.5 * s).clamp(10.5, 15.5);
        let gap_between = (8.0 * s).clamp(6.0, 12.0);
        let cur_h_exp = ((stage_h - gap_between) * 0.38).clamp(34.0, 68.0);
        let next_h_exp = (stage_h - cur_h_exp - gap_between).max(30.0);
        let lyr_cur_exp = (pad, stage_top, w - pad * 2.0, cur_h_exp);
        let lyr_next_exp = (pad, stage_top + cur_h_exp + gap_between, w - pad * 2.0, next_h_exp);
        let lyr_third_exp = ZERO;

        Self {
            orientation: Orientation::Vertical,
            scale: s,
            pill: false,
            card_radius: theme.radius * s,
            cover: lerp_rect(cover_norm, cover_exp, smooth_t),
            cover_radius: lerp_f32(12.0 * s, 9.0 * s, smooth_t),
            cover_circle: theme.circle_cover,
            title: lerp_rect(title_norm, title_exp, smooth_t),
            artist: lerp_rect(artist_norm, artist_exp, smooth_t),
            title_px: lerp_f32(title_px_norm, title_px_exp, smooth_t),
            artist_px: lerp_f32(artist_px_norm, artist_px_exp, smooth_t),
            lyric_cur: if show_lyrics { lerp_rect(lyr_cur_norm, lyr_cur_exp, smooth_t) } else { ZERO },
            lyric_next: if show_lyrics { lerp_rect(lyr_next_norm, lyr_next_exp, smooth_t) } else { ZERO },
            lyric_third: if show_lyrics && smooth_t > 0.45 { lyr_third_exp } else { ZERO },
            lyric_cur_px: lerp_f32(lyr_cur_px_norm, lyr_cur_px_exp, smooth_t),
            lyric_next_px: lerp_f32(lyr_next_px_norm, lyr_next_px_exp, smooth_t),
            lyrics_btn,
            text_align: if smooth_t > 0.5 { TextAlign::Left } else { TextAlign::Center },
            spectrum,
            progress,
            time_cur,
            time_dur,
            timecode_px: (10.5 * s).clamp(9.0, 14.0),
            prev,
            play,
            next,
            vol_icon,
            vol_bar,
            vol_down: ZERO,
            vol_up: ZERO,
            settings,
            grip: (w - grip_sz, h - grip_sz, grip_sz, grip_sz),
        }
    }

    /// Horizontal Layout (w >= h): widescreen modern card with 1:1 cover on left,
    /// 2-line synced lyrics (current highlighted + next line), and on-widget "Expand Lyrics"
    /// button that smoothly shifts the cover to the left/top corner inside the widget's territory.
    fn horizontal(
        w: f32,
        h: f32,
        theme: &Theme,
        show_lyrics: bool,
        show_visualizer: bool,
        lyrics_expand: f32,
    ) -> Self {
        let t = if show_lyrics { lyrics_expand.clamp(0.0, 1.0) } else { 0.0 };
        let smooth_t = t * t * (3.0 - 2.0 * t);
        let s = (h / 150.0).min(w / 340.0).clamp(0.45, 3.0);
        let pad = (10.0 * s).clamp(6.0, 18.0);
        let grip_sz = (22.0 * s).max(18.0);
        let lyr_btn_w = (50.0 * s).clamp(40.0, 72.0);
        let lyr_btn_h = (18.0 * s).clamp(16.0, 24.0);

        // 1. Cover on the left: shifts to compact side/corner badge when lyrics_expand is active
        let cover_sz_norm = (h - pad * 2.0).max(30.0);
        let cover_norm = (pad, pad, cover_sz_norm, cover_sz_norm);
        let cover_sz_exp = (cover_sz_norm * 0.62).max(28.0);
        let cover_exp = (pad * 0.8, pad * 0.8, cover_sz_exp, cover_sz_exp);
        let cover = lerp_rect(cover_norm, cover_exp, smooth_t);

        // 2. Right content column: expands leftward as the cover shifts to the side
        let rx_norm = pad + cover_sz_norm + 12.0 * s;
        let rx_exp = pad * 0.8 + cover_sz_exp + 10.0 * s;
        let rx = lerp_f32(rx_norm, rx_exp, smooth_t);
        let rw = (w - rx - pad).max(40.0);

        let lyrics_btn = if show_lyrics {
            (rx + rw - lyr_btn_w, pad * 0.75, lyr_btn_w, lyr_btn_h)
        } else {
            ZERO
        };

        // 3. Title & Artist header
        let top_btns_left = if show_lyrics { lyrics_btn.0 - 8.0 * s } else { rx + rw };
        let title_px = lerp_f32((14.0 * s).clamp(11.0, 21.0), (12.5 * s).clamp(10.0, 18.0), smooth_t);
        let artist_px = lerp_f32((11.0 * s).clamp(9.0, 16.0), (10.2 * s).clamp(8.5, 14.5), smooth_t);
        let title = (rx, pad * 0.75, (top_btns_left - rx).max(20.0), title_px * 1.28);
        let artist = (rx, title.1 + title.3 + 1.0 * s, (top_btns_left - rx).max(20.0), artist_px * 1.25);
        let header_bottom = artist.1 + artist.3;

        // 4. Spectrum & Progress bar at bottom of right column (if enabled)
        let (_spec_h, spectrum) = if show_visualizer {
            let sh = lerp_f32((16.0 * s).clamp(10.0, 24.0), (13.0 * s).clamp(9.0, 18.0), smooth_t);
            (sh, (rx, h - pad * 0.85 - sh, rw, sh))
        } else {
            (0.0, ZERO)
        };

        let bar_h = (3.5 * s).clamp(3.0, 5.0);
        let prog_y = if show_visualizer {
            spectrum.1 - 4.5 * s - bar_h
        } else {
            h - pad * 0.85 - bar_h
        };
        let progress = (rx, prog_y, rw, bar_h);

        // 5. Timestamps & Volume & Settings row (cleanly placed ABOVE the progress bar)
        let tc_h = (12.0 * s).clamp(10.0, 14.0);
        let tc_w = (32.0 * s).clamp(24.0, 42.0);
        let time_y = prog_y - tc_h - 3.0 * s;

        let settings_sz = (14.0 * s).clamp(12.0, 18.0);
        let settings = (rx + rw - settings_sz, time_y + (tc_h - settings_sz) / 2.0, settings_sz, settings_sz);
        let time_cur = (rx, time_y, tc_w, tc_h);
        let time_dur = (settings.0 - tc_w - 4.0 * s, time_y, tc_w, tc_h);

        let vol_icon_sz = (12.0 * s).clamp(10.0, 14.0);
        let vol_w = (34.0 * s).clamp(20.0, 48.0);
        let vol_bar = (
            time_dur.0 - vol_w - 6.0 * s,
            time_y + (tc_h - 3.5 * s) / 2.0,
            vol_w,
            3.5 * s,
        );
        let vol_icon = (
            vol_bar.0 - vol_icon_sz - 4.0 * s,
            time_y + (tc_h - vol_icon_sz) / 2.0,
            vol_icon_sz,
            vol_icon_sz,
        );

        // 6. Transport buttons row (Play/Pause, Prev, Next)
        // Positioned strictly ABOVE the timestamps row with a dedicated vertical gap!
        let play_d_norm = (24.0 * s).clamp(20.0, 32.0);
        let step_d_norm = play_d_norm * 0.74;
        let gap_norm = (8.0 * s).clamp(6.0, 14.0);

        let ctrl_y_norm = if show_lyrics {
            time_y - play_d_norm - 5.0 * s
        } else {
            header_bottom + (time_y - header_bottom - play_d_norm) * 0.5
        };

        let total_ctrl_w = step_d_norm * 2.0 + play_d_norm + gap_norm * 2.0;
        let cx_start = rx + (rw - total_ctrl_w) * 0.5;
        let prev_norm = (cx_start, ctrl_y_norm + (play_d_norm - step_d_norm) / 2.0, step_d_norm, step_d_norm);
        let play_norm = (cx_start + step_d_norm + gap_norm, ctrl_y_norm, play_d_norm, play_d_norm);
        let next_norm = (
            play_norm.0 + play_d_norm + gap_norm,
            ctrl_y_norm + (play_d_norm - step_d_norm) / 2.0,
            step_d_norm,
            step_d_norm,
        );

        // In expanded horizontal mode, transport buttons tuck neatly beneath the shifted cover on the left!
        let play_d_exp = (22.0 * s).clamp(18.0, 28.0);
        let step_d_exp = play_d_exp * 0.74;
        let gap_exp = 3.5 * s;
        let left_col_cx = cover_exp.0 + cover_exp.2 * 0.5;
        let ctrl_y_exp = (cover_exp.1 + cover_exp.3 + 8.0 * s).min(h - pad - play_d_exp);
        let play_exp = (left_col_cx - play_d_exp * 0.5, ctrl_y_exp, play_d_exp, play_d_exp);
        let prev_exp = (
            play_exp.0 - step_d_exp - gap_exp,
            ctrl_y_exp + (play_d_exp - step_d_exp) / 2.0,
            step_d_exp,
            step_d_exp,
        );
        let next_exp = (
            play_exp.0 + play_d_exp + gap_exp,
            ctrl_y_exp + (play_d_exp - step_d_exp) / 2.0,
            step_d_exp,
            step_d_exp,
        );

        let prev = lerp_rect(prev_norm, prev_exp, smooth_t);
        let play = lerp_rect(play_norm, play_exp, smooth_t);
        let next = lerp_rect(next_norm, next_exp, smooth_t);

        // 7. Lyrics:
        // In normal mode: sits between header_bottom and ctrl_y_norm!
        // In expanded mode: spans the large middle stage of the right column!
        let lyr_cur_px_norm = (11.5 * s).clamp(9.5, 14.5);
        let lyr_next_px_norm = (9.8 * s).clamp(8.2, 12.0);
        let lyr_cur_px_exp = (14.5 * s).clamp(12.0, 18.0);
        let lyr_next_px_exp = (12.0 * s).clamp(10.0, 15.0);

        let lyr_cur_px = lerp_f32(lyr_cur_px_norm, lyr_cur_px_exp, smooth_t);
        let lyr_next_px = lerp_f32(lyr_next_px_norm, lyr_next_px_exp, smooth_t);

        let lyr_space_top_norm = header_bottom + 2.0 * s;
        let lyr_space_h_norm = (ctrl_y_norm - lyr_space_top_norm - 2.0 * s).max(0.0);

        let (cur_h_norm, next_h_norm) = if lyr_space_h_norm >= 24.0 * s {
            let ch = (lyr_space_h_norm * 0.52).clamp(11.0, 20.0);
            let nh = (lyr_space_h_norm - ch - 2.0 * s).max(9.0);
            (ch, nh)
        } else {
            (lyr_space_h_norm, 0.0)
        };

        let lyr_cur_norm = if show_lyrics && cur_h_norm > 0.0 {
            (rx, lyr_space_top_norm, rw, cur_h_norm)
        } else {
            ZERO
        };
        let lyr_next_norm = if show_lyrics && next_h_norm > 0.0 {
            (rx, lyr_space_top_norm + cur_h_norm + 2.0 * s, rw, next_h_norm)
        } else {
            ZERO
        };

        let stage_top_exp = header_bottom + 3.0 * s;
        let stage_h_exp = (time_y - stage_top_exp - 3.0 * s).max(28.0);
        let cur_h_exp = (stage_h_exp * 0.54).max(18.0);
        let next_h_exp = (stage_h_exp - cur_h_exp - 2.0 * s).max(14.0);
        let rw_exp = (w - rx_exp - pad).max(40.0);

        let lyr_cur_exp = if show_lyrics {
            (rx_exp, stage_top_exp, rw_exp, cur_h_exp)
        } else {
            ZERO
        };
        let lyr_next_exp = if show_lyrics {
            (rx_exp, stage_top_exp + cur_h_exp + 2.0 * s, rw_exp, next_h_exp)
        } else {
            ZERO
        };

        let lyric_cur = lerp_rect(lyr_cur_norm, lyr_cur_exp, smooth_t);
        let lyric_next = lerp_rect(lyr_next_norm, lyr_next_exp, smooth_t);

        Self {
            orientation: Orientation::Horizontal,
            scale: s,
            pill: false,
            card_radius: theme.radius * s,
            cover,
            cover_radius: lerp_f32(10.0 * s, 8.0 * s, smooth_t),
            cover_circle: theme.circle_cover,
            title,
            artist,
            title_px,
            artist_px,
            lyric_cur,
            lyric_next,
            lyric_third: ZERO,
            lyric_cur_px: lyr_cur_px,
            lyric_next_px: lyr_next_px,
            lyrics_btn,
            text_align: TextAlign::Left,
            spectrum,
            progress,
            time_cur,
            time_dur,
            timecode_px: (10.0 * s).clamp(8.5, 13.0),
            prev,
            play,
            next,
            vol_icon,
            vol_bar,
            vol_down: ZERO,
            vol_up: ZERO,
            settings,
            grip: (w - grip_sz, h - grip_sz, grip_sz, grip_sz),
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> WidgetHitTest {
        let inside = |r: Rect| r.2 > 0.0 && r.3 > 0.0 && x >= r.0 && x <= r.0 + r.2 && y >= r.1 && y <= r.1 + r.3;

        if inside(self.grip) {
            return WidgetHitTest::ResizeGrip;
        }
        if inside(self.settings) {
            return WidgetHitTest::OpenSettings;
        }
        if inside(self.lyrics_btn) || inside(self.lyric_cur) || inside(self.lyric_next) {
            return WidgetHitTest::ToggleLyricsExpand;
        }

        // Progress bar hit-test with comfortable expanded 24px vertical height
        let (px, py, pw, ph) = self.progress;
        if pw > 0.0 && x >= px && x <= px + pw && y >= py - 10.0 && y <= py + ph + 10.0 {
            let ratio = ((x - px) / pw).clamp(0.0, 1.0);
            return WidgetHitTest::ProgressSeek(ratio);
        }

        // Volume bar hit-test with expanded height
        let (vx, vy, vw, vh) = self.vol_bar;
        if vw > 0.0 && x >= vx && x <= vx + vw && y >= vy - 8.0 && y <= vy + vh + 8.0 {
            let ratio = ((x - vx) / vw).clamp(0.0, 1.0);
            return WidgetHitTest::VolumeSeek(ratio);
        }

        if inside(self.vol_icon) {
            return WidgetHitTest::VolumeMuteToggle;
        }

        if inside(self.play) {
            return WidgetHitTest::PlayPause;
        }
        if inside(self.prev) {
            return WidgetHitTest::Previous;
        }
        if inside(self.next) {
            return WidgetHitTest::Next;
        }
        if inside(self.vol_down) {
            return WidgetHitTest::VolumeDown;
        }
        if inside(self.vol_up) {
            return WidgetHitTest::VolumeUp;
        }

        // In vertical mode, clicking directly on the cover artwork toggles Play/Pause
        if self.orientation == Orientation::Vertical && inside(self.cover) {
            return WidgetHitTest::PlayPause;
        }

        WidgetHitTest::Drag
    }
}

/// Preferred height for a theme at a given width.
pub fn preferred_height(_theme_name: &str, width: u32) -> u32 {
    let h = if width < 340 {
        width as f32 * 1.35 // Vertical mode
    } else {
        width as f32 * 0.40 // Horizontal mode
    };
    (h.round() as u32).clamp(MIN_H, MAX_H)
}

pub fn format_time(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins >= 60 {
        let hrs = mins / 60;
        let rem_mins = mins % 60;
        format!("{}:{:02}:{:02}", hrs, rem_mins, secs)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}

pub fn parse_hex_color(hex: &str, fallback: u32) -> u32 {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        if let Ok(rgb) = u32::from_str_radix(clean, 16) {
            return 0xFF000000 | rgb;
        }
    }
    fallback
}

#[derive(Clone, Debug)]
pub struct SnapParticle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
}

pub struct WidgetPainter {
    pub layout: Layout,
    /// Button under the mouse cursor (for hover highlight)
    pub hover: WidgetHitTest,
    pub is_resizing: bool,
    pub lyrics_expanded: bool,
    pub lyrics_expand_anim: f32,
    pub vinyl_angle: f32,
    pub smoothed_spectrum: Vec<f32>,
    pub peak_caps: Vec<f32>,
    pub volume: f32,
    pub is_muted: bool,
    pub dynamic_accent: u32,
    pub target_accent: u32,
    pub title_scroll: f32,
    pub title_pause: u32,
    pub artist_scroll: f32,
    pub artist_pause: u32,
    pub lyric_scroll: f32,
    pub lyric_pause: u32,
    pub last_title: String,
    pub last_artist: String,
    pub last_lyric: String,
    pub lyrics: crate::lyrics::LyricsManager,
    pub snap_neon_glow: f32,
    pub snapped_edges: u8,
    pub approaching_edges: u8,
    pub snap_particles: Vec<SnapParticle>,
    pub is_seeking: bool,
    pub scrub_ratio: f32,
    pub lyric_prev_line: String,
    pub lyric_trans: f32,
    cover_cache: Option<CachedImage>,
    prev_cover_cache: Option<CachedImage>,
    pub cover_crossfade: f32,
    failed_cover: Option<String>,
}

impl WidgetPainter {
    pub fn new() -> Self {
        Self {
            layout: Layout::default(),
            hover: WidgetHitTest::None,
            is_resizing: false,
            lyrics_expanded: false,
            lyrics_expand_anim: 0.0,
            vinyl_angle: 0.0,
            smoothed_spectrum: Vec::new(),
            peak_caps: Vec::new(),
            volume: 0.8,
            is_muted: false,
            dynamic_accent: 0xFF00F5D4,
            target_accent: 0xFF00F5D4,
            title_scroll: 0.0,
            title_pause: 0,
            artist_scroll: 0.0,
            artist_pause: 0,
            lyric_scroll: 0.0,
            lyric_pause: 0,
            last_title: String::new(),
            last_artist: String::new(),
            last_lyric: String::new(),
            lyrics: crate::lyrics::LyricsManager::new(),
            snap_neon_glow: 0.0,
            snapped_edges: 0,
            approaching_edges: 0,
            snap_particles: Vec::new(),
            is_seeking: false,
            scrub_ratio: 0.0,
            lyric_prev_line: String::new(),
            lyric_trans: 1.0,
            cover_cache: None,
            prev_cover_cache: None,
            cover_crossfade: 1.0,
            failed_cover: None,
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> WidgetHitTest {
        self.layout.hit_test(x, y)
    }

    /// Draw background, shapes, buttons, progress, volume, and spectrum into the ARGB surface.
    pub fn paint_surface(
        &mut self,
        surface: &mut ArgbSurface,
        config: &WidgetConfig,
        track: Option<&TrackMetadata>,
        raw_spectrum: &[f32],
    ) {
        surface.clear();
        let w = surface.width as f32;
        let h = surface.height as f32;
        let theme = theme_for(&config.theme);

        let target_exp = if config.show_lyrics && self.lyrics_expanded { 1.0 } else { 0.0 };
        let diff_exp = target_exp - self.lyrics_expand_anim;
        if diff_exp.abs() < 0.005 {
            self.lyrics_expand_anim = target_exp;
        } else {
            self.lyrics_expand_anim += diff_exp * 0.22;
        }

        self.layout = Layout::compute_ex(
            &config.theme,
            w,
            h,
            config.show_lyrics,
            config.show_visualizer,
            self.lyrics_expand_anim,
        );
        if !theme.pill && config.corner_radius > 0 {
            self.layout.card_radius = (config.corner_radius as f32 * self.layout.scale).min(w.min(h) * 0.5);
        }
        let l = self.layout;
        let s = l.scale;

        // Track FFT spectrum (instant attack, clean organic fall, floating peak caps)
        let n = raw_spectrum.len().max(8);
        if self.smoothed_spectrum.len() != n {
            self.smoothed_spectrum = vec![0.0; n];
            self.peak_caps = vec![0.0; n];
        }
        for (i, &raw) in raw_spectrum.iter().enumerate() {
            let prev = self.smoothed_spectrum[i];
            let target = if raw > prev {
                prev + (raw - prev) * 0.85
            } else {
                (prev * 0.88 - 0.005).max(0.0)
            };
            self.smoothed_spectrum[i] = target;

            let prev_peak = self.peak_caps[i];
            let peak_target = if target >= prev_peak {
                target
            } else {
                (prev_peak - 0.012).max(0.0)
            };
            self.peak_caps[i] = peak_target;
        }

        let is_playing = track
            .map(|t| t.playback_status == PlaybackStatus::Playing)
            .unwrap_or(false);
        if is_playing {
            self.vinyl_angle = (self.vinyl_angle + 0.65) % 360.0;
        }

        // 1. Card background + micro-border
        // Smoothly interpolate accent color toward either extracted cover accent or custom preset accent
        let desired_accent = if config.dynamic_accent {
            self.target_accent
        } else {
            parse_hex_color(&config.custom_accent, theme.accent)
        };
        self.dynamic_accent = crate::palette::PaletteExtractor::lerp_color(
            self.dynamic_accent,
            desired_accent,
            0.08,
        );
        let active_accent = self.dynamic_accent;

        let opacity = config.opacity.clamp(0.3, 1.0);
        let (ba, br, bg, bb) = theme.bg;
        let bg_col = ArgbSurface::premul_argb((ba as f32 * opacity) as u8, br, bg, bb);
        let (oa, or, og, ob) = theme.border;
        let border_col = if self.is_resizing {
            active_accent
        } else {
            ArgbSurface::premul_argb(oa, or, og, ob)
        };
        surface.draw_rounded_rect(0.0, 0.0, w, h, l.card_radius, bg_col);
        surface.draw_rounded_border(0.0, 0.0, w, h, l.card_radius, border_col);
        if self.is_resizing {
            surface.draw_rounded_border(
                1.0,
                1.0,
                (w - 2.0).max(2.0),
                (h - 2.0).max(2.0),
                (l.card_radius - 1.0).max(0.0),
                premul(active_accent, 180),
            );
        }

        // 1b. Magnetic snap docking neon edge glow (follows card rounded corners perfectly)
        if self.snap_neon_glow > 0.01 && self.snapped_edges != 0 {
            let glow = self.snap_neon_glow.clamp(0.0, 1.0);
            let alpha_halo = (90.0 * glow) as u8;
            let alpha_core = (235.0 * glow) as u8;
            let col_halo = premul(active_accent, alpha_halo);
            let col_core = premul(active_accent, alpha_core);

            // 1. Sleek rounded pulse around the entire card contour:
            surface.draw_rounded_border(0.0, 0.0, w, h, l.card_radius, col_halo);
            surface.draw_rounded_border(1.0, 1.0, (w - 2.0).max(2.0), (h - 2.0).max(2.0), (l.card_radius - 1.0).max(0.0), col_core);

            // 2. Focused glowing edge accents strictly within the rounded straight spans:
            let cr = l.card_radius.max(6.0);
            if self.snapped_edges & crate::window::SNAP_LEFT != 0 {
                let seg_h = (h - cr * 2.0).max(10.0);
                surface.draw_rounded_rect(1.5, cr, 3.5, seg_h, 1.7, col_core);
            }
            if self.snapped_edges & crate::window::SNAP_RIGHT != 0 {
                let seg_h = (h - cr * 2.0).max(10.0);
                surface.draw_rounded_rect(w - 5.0, cr, 3.5, seg_h, 1.7, col_core);
            }
            if self.snapped_edges & crate::window::SNAP_TOP != 0 {
                let seg_w = (w - cr * 2.0).max(10.0);
                surface.draw_rounded_rect(cr, 1.5, seg_w, 3.5, 1.7, col_core);
            }
            if self.snapped_edges & crate::window::SNAP_BOTTOM != 0 {
                let seg_w = (w - cr * 2.0).max(10.0);
                surface.draw_rounded_rect(cr, h - 5.0, seg_w, 3.5, 1.7, col_core);
            }
        }

        // Soft magnetic attraction edge glow
        if self.approaching_edges != 0 {
            let col_edge = premul(active_accent, 90);
            let cr = l.card_radius.max(6.0);
            if self.approaching_edges & crate::window::SNAP_LEFT != 0 {
                surface.draw_rounded_rect(0.5, cr, 2.8, (h - cr * 2.0).max(10.0), 1.4, col_edge);
            }
            if self.approaching_edges & crate::window::SNAP_RIGHT != 0 {
                surface.draw_rounded_rect(w - 3.3, cr, 2.8, (h - cr * 2.0).max(10.0), 1.4, col_edge);
            }
            if self.approaching_edges & crate::window::SNAP_TOP != 0 {
                surface.draw_rounded_rect(cr, 0.5, (w - cr * 2.0).max(10.0), 2.8, 1.4, col_edge);
            }
            if self.approaching_edges & crate::window::SNAP_BOTTOM != 0 {
                surface.draw_rounded_rect(cr, h - 3.3, (w - cr * 2.0).max(10.0), 2.8, 1.4, col_edge);
            }
        }

        // 2. Cover placeholder / vinyl disc (real artwork is drawn by GDI+ on top)
        let (cx, cy) = center(l.cover);
        if config.theme == "vinyl" {
            let r = l.cover.2 / 2.0;
            surface.draw_circle(cx, cy, r, 0xFF0D0D0D);
            let groove = premul(0xFFFFFFFF, 14);
            let mut gr = r * 0.95;
            while gr > r * 0.45 {
                surface.draw_circle(cx, cy, gr, groove);
                surface.draw_circle(cx, cy, gr - 1.0, 0xFF0D0D0D);
                gr -= (3.0 * s).max(2.0);
            }
            if !self.has_cover(track) {
                surface.draw_circle(cx, cy, r * 0.36, active_accent);
            }
        } else if !self.has_cover(track) {
            let ph = premul(active_accent, 255);
            if l.cover_circle {
                surface.draw_circle(cx, cy, l.cover.2 / 2.0, ph);
            } else {
                surface.draw_rounded_rect(l.cover.0, l.cover.1, l.cover.2, l.cover.3, l.cover_radius, ph);
            }
            // Music-note glyph
            let u = l.cover.2 / 10.0;
            surface.draw_circle(cx - u * 1.2, cy + u * 1.8, u * 1.15, theme.on_accent);
            surface.draw_rounded_rect(cx - u * 0.25, cy - u * 2.6, u * 0.6, u * 4.4, u * 0.3, theme.on_accent);
            surface.draw_rounded_rect(cx - u * 0.25, cy - u * 2.6, u * 2.2, u * 0.8, u * 0.3, theme.on_accent);
        }

        // 3. Audio visualizer spectrum (Bars, Wave, Mirror, or Dots)
        if config.show_visualizer && l.spectrum.2 > 0.0 {
            let n_bars = ((l.spectrum.2 / (7.0 * s)) as usize).clamp(8, self.smoothed_spectrum.len().max(8));
            let bars: Vec<f32> = resample(&self.smoothed_spectrum, n_bars);
            let peaks: Vec<f32> = resample(&self.peak_caps, n_bars);
            let bar_color = premul(active_accent, 225);
            let peak_color = premul(0xFFFFFFFF, 240);
            surface.draw_visualizer_styled(
                &config.visualizer_style,
                l.spectrum.0,
                l.spectrum.1,
                l.spectrum.2,
                l.spectrum.3,
                &bars,
                &peaks,
                bar_color,
                peak_color,
            );
        }

        // 4. Progress bar with glowing scrubber knob
        if l.progress.2 > 0.0 {
            let progress = if self.is_seeking {
                self.scrub_ratio
            } else {
                track
                    .and_then(|t| match (t.position_ms, t.duration_ms) {
                        (Some(p), Some(d)) if d > 0 => Some((p as f32 / d as f32).clamp(0.0, 1.0)),
                        _ => None,
                    })
                    .unwrap_or(0.0)
            };
            surface.draw_progress_bar_with_knob(
                l.progress.0,
                l.progress.1,
                l.progress.2,
                l.progress.3,
                progress,
                premul(0xFFFFFFFF, 35),
                active_accent,
                0xFFFFFFFF,
                true,
            );
        }

        // 5. Volume controls
        if l.vol_icon.2 > 0.0 {
            let (vx, vy) = center(l.vol_icon);
            let hover_vol = matches!(self.hover, WidgetHitTest::VolumeMuteToggle);
            let vol_col = if hover_vol { active_accent } else { theme.icon };
            surface.draw_speaker_icon(vx, vy, l.vol_icon.3 * 0.9, self.volume, self.is_muted, vol_col);
        }
        if l.vol_bar.2 > 0.0 {
            let vol_level = if self.is_muted { 0.0 } else { self.volume };
            surface.draw_progress_bar(
                l.vol_bar.0,
                l.vol_bar.1,
                l.vol_bar.2,
                l.vol_bar.3,
                vol_level,
                premul(0xFFFFFFFF, 35),
                premul(theme.icon, 200),
            );
        }

        // 6. Transport controls (Play/Pause, Previous, Next)
        let hover_bg = premul(0xFFFFFFFF, 36);
        for (rect, kind, next_dir) in [
            (l.prev, WidgetHitTest::Previous, false),
            (l.next, WidgetHitTest::Next, true),
        ] {
            if rect.2 > 0.0 {
                let (bx, by) = center(rect);
                if self.hover == kind {
                    surface.draw_circle(bx, by, rect.2 / 2.0, hover_bg);
                }
                surface.draw_step_icon(bx, by, rect.2 * 0.5, next_dir, theme.icon);
            }
        }
        if l.play.2 > 0.0 {
            let (bx, by) = center(l.play);
            let grow = if self.hover == WidgetHitTest::PlayPause { 1.07 } else { 1.0 };
            surface.draw_circle(bx, by, l.play.2 / 2.0 * grow, active_accent);
            let icon_dx = if is_playing { 0.0 } else { l.play.2 * 0.04 };
            surface.draw_playback_icon(bx + icon_dx, by, l.play.2 * 0.42, is_playing, theme.on_accent);
        }

        // Volume - / + buttons (used in pill mode)
        for (rect, kind, plus) in [
            (l.vol_down, WidgetHitTest::VolumeDown, false),
            (l.vol_up, WidgetHitTest::VolumeUp, true),
        ] {
            if rect.2 <= 0.0 {
                continue;
            }
            let (bx, by) = center(rect);
            let alpha = if self.hover == kind { 60 } else { 26 };
            surface.draw_circle(bx, by, rect.2 / 2.0, premul(0xFFFFFFFF, alpha));
            let len = rect.2 * 0.42;
            let th = (rect.2 * 0.09).max(1.5);
            surface.draw_rounded_rect(bx - len / 2.0, by - th / 2.0, len, th, th / 2.0, theme.icon);
            if plus {
                surface.draw_rounded_rect(bx - th / 2.0, by - len / 2.0, th, len, th / 2.0, theme.icon);
            }
        }

        // 7. Highlight pill behind active lyric line + On-Widget "Expand Lyrics" button + Settings "⋯"
        if l.lyric_cur.2 > 0.0 && config.show_lyrics {
            let pad = 12.0 * s;
            let is_exp = self.lyrics_expand_anim > 0.3;
            let hl_alpha = if is_exp { 26 } else { 22 };
            let pad_x = if is_exp { 2.0 * s } else { 4.0 * s };
            let pad_y = if is_exp { 2.0 * s } else { 1.0 * s };
            let box_x = (l.lyric_cur.0 - pad_x).max(pad * 0.7);
            let box_w = (l.lyric_cur.2 + pad_x * 2.0).min(w - box_x - pad * 0.7);
            let box_y = l.lyric_cur.1 - pad_y;
            let box_h = l.lyric_cur.3 + pad_y * 2.0;
            let rad = (8.0 * s).clamp(6.0, 12.0);
            surface.draw_rounded_rect(
                box_x,
                box_y,
                box_w,
                box_h,
                rad,
                premul(active_accent, hl_alpha),
            );
            if is_exp {
                surface.draw_rounded_border(
                    box_x,
                    box_y,
                    box_w,
                    box_h,
                    rad,
                    premul(active_accent, 42),
                );
            }
        }

        if l.lyrics_btn.2 > 0.0 {
            let is_hov = self.hover == WidgetHitTest::ToggleLyricsExpand;
            let radius = l.lyrics_btn.3 * 0.5; // Perfect smooth pill
            if self.lyrics_expanded {
                // Active state: bright neon pill
                let bg_col = premul(active_accent, if is_hov { 255 } else { 220 });
                surface.draw_rounded_rect(
                    l.lyrics_btn.0,
                    l.lyrics_btn.1,
                    l.lyrics_btn.2,
                    l.lyrics_btn.3,
                    radius,
                    bg_col,
                );
            } else {
                // Inactive state: frosted glass with delicate glowing neon border
                let bg_alpha = if is_hov { 42 } else { 20 };
                let border_alpha = if is_hov { 230 } else { 110 };
                surface.draw_rounded_rect(
                    l.lyrics_btn.0,
                    l.lyrics_btn.1,
                    l.lyrics_btn.2,
                    l.lyrics_btn.3,
                    radius,
                    premul(0xFFFFFFFF, bg_alpha),
                );
                surface.draw_rounded_border(
                    l.lyrics_btn.0,
                    l.lyrics_btn.1,
                    l.lyrics_btn.2,
                    l.lyrics_btn.3,
                    radius,
                    premul(active_accent, border_alpha),
                );
            }
        }

        if l.settings.2 > 0.0 {
            let (bx, by) = center(l.settings);
            let is_hov = self.hover == WidgetHitTest::OpenSettings;
            if is_hov {
                surface.draw_circle(bx, by, l.settings.2 * 0.54, hover_bg);
            }
            let dot = (1.5 * s).max(1.1);
            let col = if is_hov { active_accent } else { premul(0xFFFFFFFF, 160) };
            for k in [-1.0f32, 0.0, 1.0] {
                surface.draw_circle(bx + k * 4.2 * s, by, dot, col);
            }
        }

        // 8. Resize grip dots (cleanly in corner, no numbers!)
        if l.grip.2 > 0.0 {
            let col = if self.is_resizing {
                theme.accent
            } else {
                premul(0xFFFFFFFF, if self.hover == WidgetHitTest::ResizeGrip { 230 } else { 110 })
            };
            let step = 4.2 * s;
            let (gx, gy) = (w - 7.0 * s, h - 7.0 * s);
            let dot = (1.5 * s).max(1.2);
            surface.draw_circle(gx, gy, dot, col);
            surface.draw_circle(gx - step, gy, dot, col);
            surface.draw_circle(gx, gy - step, dot, col);
            surface.draw_circle(gx - step * 2.0, gy, dot, col);
            surface.draw_circle(gx - step, gy - step, dot, col);
            surface.draw_circle(gx, gy - step * 2.0, dot, col);
        }

        // NOTE: Live resize number badge has been completely removed per user request!
    }

    /// Draw anti-aliased text and the album artwork on top of the surface via GDI+.
    pub fn paint_foreground(
        &mut self,
        gdi: &GdiPlusRenderer,
        config: &WidgetConfig,
        track: Option<&TrackMetadata>,
    ) {
        let theme = theme_for(&config.theme);
        let l = self.layout;
        let s = l.scale;

        // Album artwork (cached in memory; bicubic high-resolution rendering with smooth crossfade)
        self.refresh_cover(track);
        if self.cover_crossfade < 1.0 {
            self.cover_crossfade = (self.cover_crossfade + 0.08).min(1.0);
        } else if self.prev_cover_cache.is_some() {
            self.prev_cover_cache = None;
        }

        if let Some(ref prev_img) = self.prev_cover_cache {
            if config.theme != "vinyl" && self.cover_crossfade < 1.0 {
                let _ = gdi.draw_image_shape(
                    prev_img, l.cover.0, l.cover.1, l.cover.2, l.cover.3, l.cover_radius, l.cover_circle, 0.0,
                );
            }
        }

        if let Some(ref img) = self.cover_cache {
            if config.theme == "vinyl" {
                let d = l.cover.2 * 0.62;
                let (cx, cy) = center(l.cover);
                let _ = gdi.draw_image_shape(img, cx - d / 2.0, cy - d / 2.0, d, d, 0.0, true, self.vinyl_angle);
            } else {
                let t = self.cover_crossfade.clamp(0.0, 1.0);
                let smooth_t = t * t * (3.0 - 2.0 * t);
                let scale_f = if self.prev_cover_cache.is_some() {
                    0.92 + 0.08 * smooth_t
                } else {
                    1.0
                };
                let cw = l.cover.2 * scale_f;
                let ch = l.cover.3 * scale_f;
                let cx = l.cover.0 + (l.cover.2 - cw) * 0.5;
                let cy = l.cover.1 + (l.cover.3 - ch) * 0.5;
                let _ = gdi.draw_image_shape(
                    img, cx, cy, cw, ch, l.cover_radius * scale_f, l.cover_circle, 0.0,
                );
            }
        }

        let (title, artist) = match track {
            Some(t) => (
                if t.title.is_empty() { "Без названия" } else { t.title.as_str() },
                if t.artist.is_empty() { "Неизвестный исполнитель" } else { t.artist.as_str() },
            ),
            None => ("Ничего не играет", "Включите музыку в любом плеере"),
        };

        self.lyrics.on_track_changed(title, artist);
        let dur_ms = track.and_then(|t| t.duration_ms).unwrap_or(0);
        let cur_ms = if self.is_seeking {
            (self.scrub_ratio * dur_ms as f32).round() as u64
        } else {
            track.and_then(|t| t.position_ms).unwrap_or(0)
        };

        // Reset scroll offset if track title or artist changed
        if self.last_title != title {
            self.last_title = title.to_string();
            self.title_scroll = 0.0;
            self.title_pause = 0;
        }
        if self.last_artist != artist {
            self.last_artist = artist.to_string();
            self.artist_scroll = 0.0;
            self.artist_pause = 0;
        }

        // Smooth Marquee for long titles
        let title_w = gdi.measure_text_width(title, l.title_px, true);
        let avail_title_w = l.title.2;
        if title_w > avail_title_w + 4.0 {
            if self.title_pause < 75 {
                self.title_pause += 1;
            } else {
                self.title_scroll += 0.8;
                if self.title_scroll > title_w - avail_title_w + 40.0 {
                    self.title_scroll = 0.0;
                    self.title_pause = 0;
                }
            }
        } else {
            self.title_scroll = 0.0;
        }

        let artist_w = gdi.measure_text_width(artist, l.artist_px, false);
        let avail_artist_w = l.artist.2;
        if artist_w > avail_artist_w + 4.0 {
            if self.artist_pause < 75 {
                self.artist_pause += 1;
            } else {
                self.artist_scroll += 0.7;
                if self.artist_scroll > artist_w - avail_artist_w + 30.0 {
                    self.artist_scroll = 0.0;
                    self.artist_pause = 0;
                }
            }
        } else {
            self.artist_scroll = 0.0;
        }

        // Render Title & Artist
        gdi.draw_text_marquee(
            title,
            l.title_px,
            true,
            theme.title,
            l.title.0,
            l.title.1,
            l.title.2,
            l.title.3,
            self.title_scroll,
            l.text_align,
        );
        gdi.draw_text_marquee(
            artist,
            l.artist_px,
            false,
            theme.subtitle,
            l.artist.0,
            l.artist.1,
            l.artist.2,
            l.artist.3,
            self.artist_scroll,
            l.text_align,
        );

        // Render On-Widget Lyrics Expand button label ("♪ Текст")
        if l.lyrics_btn.2 > 0.0 {
            let is_hov = self.hover == WidgetHitTest::ToggleLyricsExpand;
            let btn_txt_col = if self.lyrics_expanded {
                theme.on_accent
            } else if is_hov {
                0xFFFFFFFF
            } else {
                crate::palette::PaletteExtractor::lerp_color(0xFFFFFFFF, self.dynamic_accent, 0.4)
            };
            let btn_px = (9.4 * l.scale).clamp(8.5, 12.0);
            gdi.draw_text(
                "♪ Текст",
                btn_px,
                true,
                btn_txt_col,
                l.lyrics_btn.0,
                l.lyrics_btn.1 + (l.lyrics_btn.3 - btn_px * 1.25) * 0.5,
                l.lyrics_btn.2,
                l.lyrics_btn.3,
                TextAlign::Center,
            );
        }

        // Render 2-Line (or 3-Line Expanded) Synced Lyrics:
        // Current line highlighted (bright bold) + Next line below it!
        if config.show_lyrics && l.lyric_cur.2 > 0.0 {
            let (cur_line, upcoming_lines) = match self.lyrics.current_and_upcoming(cur_ms, 4) {
                Some((c, up)) => (format!("♪ {}", c), up),
                None => {
                    if track.is_some() {
                        (
                            format!("♪ {} — {}", artist, title),
                            vec!["Синхронный текст песни (LRCLIB)".to_string()],
                        )
                    } else {
                        (
                            "♪ Текст песни появится при воспроизведении".to_string(),
                            vec!["Включите любой трек".to_string()],
                        )
                    }
                }
            };
            let next_line = upcoming_lines.first().cloned().unwrap_or_else(|| "···".to_string());

            // Detect lyric line transition
            if self.last_lyric != cur_line {
                self.lyric_prev_line = self.last_lyric.clone();
                self.last_lyric = cur_line.clone();
                self.lyric_trans = 0.0;
                self.lyric_scroll = 0.0;
                self.lyric_pause = 0;
            } else if self.lyric_trans < 1.0 {
                self.lyric_trans = (self.lyric_trans + 0.09).min(1.0);
            }

            // Dynamic Auto-Fit: scale down font size if line exceeds available width
            let avail_w = l.lyric_cur.2;
            let mut cur_font_px = l.lyric_cur_px;
            if avail_w > 50.0 {
                let init_w = gdi.measure_text_width(&cur_line, cur_font_px, true);
                if init_w > avail_w {
                    let scale_f = (avail_w / init_w).clamp(0.68, 1.0);
                    cur_font_px = (cur_font_px * scale_f).max(9.0);
                }
            }

            let cur_w = gdi.measure_text_width(&cur_line, cur_font_px, true);
            if cur_w > avail_w + 4.0 {
                if self.lyric_pause < 50 {
                    self.lyric_pause += 1;
                } else {
                    self.lyric_scroll += 0.85;
                    if self.lyric_scroll > cur_w - avail_w + 32.0 {
                        self.lyric_scroll = 0.0;
                        self.lyric_pause = 0;
                    }
                }
            } else {
                self.lyric_scroll = 0.0;
            }

            let mut next_font_px = l.lyric_next_px;
            if avail_w > 50.0 && !next_line.is_empty() {
                let init_nw = gdi.measure_text_width(&next_line, next_font_px, false);
                if init_nw > avail_w {
                    let scale_f = (avail_w / init_nw).clamp(0.70, 1.0);
                    next_font_px = (next_font_px * scale_f).max(8.5);
                }
            }

            let cur_col = crate::palette::PaletteExtractor::lerp_color(
                0xFFFFFFFF,
                self.dynamic_accent,
                0.35,
            );
            let next_col = premul(theme.subtitle, 205);
            let lyr_align = if l.orientation == Orientation::Vertical {
                TextAlign::Center
            } else {
                TextAlign::Left
            };

            if self.lyrics_expanded {
                // Multi-line adaptive font sizing for expanded mode:
                // Text expands into multiple lines and dynamically fits available stage height
                let avail_w = l.lyric_cur.2;
                let avail_h = l.lyric_cur.3;
                let mut exp_font_px = l.lyric_cur_px;
                while exp_font_px > 10.0 {
                    let (_, mh) = gdi.measure_text_wrapped(&cur_line, exp_font_px, true, avail_w);
                    if mh <= avail_h + 1.0 {
                        break;
                    }
                    exp_font_px -= 0.5;
                }

                let t = self.lyric_trans.clamp(0.0, 1.0);
                let ease = 1.0 - (1.0 - t).powi(3);

                if self.lyric_trans < 1.0 && !self.lyric_prev_line.is_empty() {
                    let out_alpha = ((1.0 - ease) * 220.0) as u8;
                    let out_y = l.lyric_cur.1 - (8.0 * l.scale) * ease;
                    gdi.draw_text_wrapped(
                        &self.lyric_prev_line,
                        exp_font_px * 0.96,
                        true,
                        premul(cur_col, out_alpha),
                        l.lyric_cur.0,
                        out_y,
                        l.lyric_cur.2,
                        l.lyric_cur.3,
                        lyr_align,
                    );

                    let in_alpha = ((0.30 + 0.70 * ease) * 255.0) as u8;
                    let in_y = l.lyric_cur.1 + (8.0 * l.scale) * (1.0 - ease);
                    let in_col = crate::palette::PaletteExtractor::lerp_color(next_col, cur_col, ease);
                    gdi.draw_text_wrapped(
                        &cur_line,
                        exp_font_px,
                        true,
                        premul(in_col, in_alpha),
                        l.lyric_cur.0,
                        in_y,
                        l.lyric_cur.2,
                        l.lyric_cur.3,
                        lyr_align,
                    );
                } else {
                    gdi.draw_text_wrapped(
                        &cur_line,
                        exp_font_px,
                        true,
                        cur_col,
                        l.lyric_cur.0,
                        l.lyric_cur.1,
                        l.lyric_cur.2,
                        l.lyric_cur.3,
                        lyr_align,
                    );
                }

                // Render up to 3 to 4 upcoming lines below the active line
                let start_y = l.lyric_cur.1 + l.lyric_cur.3 + 8.0 * s;
                let bottom_limit = (l.prev.1 - 6.0 * s).max(start_y + 20.0);
                let avail_upcoming_h = bottom_limit - start_y;

                let max_allowed = if l.orientation == Orientation::Vertical {
                    if avail_upcoming_h >= 84.0 * s {
                        4
                    } else if avail_upcoming_h >= 54.0 * s {
                        3
                    } else {
                        2
                    }
                } else {
                    if avail_upcoming_h >= 48.0 * s { 2 } else { 1 }
                };

                let active_upcoming: Vec<&String> = upcoming_lines
                    .iter()
                    .filter(|s| !s.is_empty())
                    .take(max_allowed)
                    .collect();

                let n_lines = active_upcoming.len().max(1);
                let line_slot_h = (avail_upcoming_h / n_lines as f32).clamp(16.0 * s, 32.0 * s);

                for (idx, line_text) in active_upcoming.iter().enumerate() {
                    let (font_mult, alpha) = match idx {
                        0 => (1.00, 225), // Next immediate line: crisp soft white
                        1 => (0.92, 160), // 2nd upcoming line
                        2 => (0.86, 115), // 3rd upcoming line
                        _ => (0.80, 75),  // 4th upcoming line
                    };

                    let mut next_font = (l.lyric_next_px * font_mult).clamp(9.0, 16.0);
                    while next_font > 8.5 {
                        let (_, mh) = gdi.measure_text_wrapped(line_text, next_font, false, l.lyric_next.2);
                        if mh <= line_slot_h + 1.5 {
                            break;
                        }
                        next_font -= 0.5;
                    }

                    let float_offset = if self.lyric_trans < 1.0 {
                        (6.0 * l.scale) * (1.0 - ease)
                    } else {
                        0.0
                    };
                    let line_y = start_y + idx as f32 * line_slot_h + float_offset;

                    let next_col = premul(0xFFFFFFFF, alpha);
                    gdi.draw_text_wrapped(
                        line_text,
                        next_font,
                        false,
                        next_col,
                        l.lyric_next.0,
                        line_y,
                        l.lyric_next.2,
                        line_slot_h,
                        lyr_align,
                    );
                }
            } else {
                // Compact Single-Line Marquee & Rising Line Transition
                let line_gap = if l.lyric_next.2 > 0.0 {
                    l.lyric_next.1 - l.lyric_cur.1
                } else {
                    cur_font_px * 1.5 + 4.0 * s
                };

                let t = self.lyric_trans.clamp(0.0, 1.0);
                let ease = 1.0 - (1.0 - t).powi(3);

                if self.lyric_trans < 1.0 && !self.lyric_prev_line.is_empty() {
                    let out_alpha = ((1.0 - ease) * 225.0) as u8;
                    let out_y = l.lyric_cur.1 - line_gap * 0.85 * ease;
                    let out_font = cur_font_px * (1.0 - 0.08 * ease);
                    gdi.draw_text_marquee(
                        &self.lyric_prev_line,
                        out_font,
                        true,
                        premul(cur_col, out_alpha),
                        l.lyric_cur.0,
                        out_y,
                        l.lyric_cur.2,
                        l.lyric_cur.3,
                        0.0,
                        lyr_align,
                    );

                    let in_alpha = ((0.35 + 0.65 * ease) * 255.0) as u8;
                    let in_y = l.lyric_cur.1 + line_gap * (1.0 - ease);
                    let in_col = crate::palette::PaletteExtractor::lerp_color(
                        next_col,
                        cur_col,
                        ease,
                    );
                    let in_font = cur_font_px * (0.92 + 0.08 * ease);
                    gdi.draw_text_marquee(
                        &cur_line,
                        in_font,
                        true,
                        premul(in_col, in_alpha),
                        l.lyric_cur.0,
                        in_y,
                        l.lyric_cur.2,
                        l.lyric_cur.3,
                        self.lyric_scroll,
                        lyr_align,
                    );

                    if l.lyric_next.2 > 0.0 && !next_line.is_empty() {
                        let next_y = l.lyric_next.1 + line_gap * 0.70 * (1.0 - ease);
                        let next_alpha = (ease * 205.0) as u8;
                        gdi.draw_text(
                            &next_line,
                            next_font_px,
                            false,
                            premul(theme.subtitle, next_alpha),
                            l.lyric_next.0,
                            next_y,
                            l.lyric_next.2,
                            l.lyric_next.3,
                            lyr_align,
                        );
                    }
                } else {
                    gdi.draw_text_marquee(
                        &cur_line,
                        cur_font_px,
                        true,
                        cur_col,
                        l.lyric_cur.0,
                        l.lyric_cur.1,
                        l.lyric_cur.2,
                        l.lyric_cur.3,
                        self.lyric_scroll,
                        lyr_align,
                    );
                    if l.lyric_next.2 > 0.0 && !next_line.is_empty() {
                        gdi.draw_text(
                            &next_line,
                            next_font_px,
                            false,
                            next_col,
                            l.lyric_next.0,
                            l.lyric_next.1,
                            l.lyric_next.2,
                            l.lyric_next.3,
                            lyr_align,
                        );
                    }
                }
            }
        }

        // Render Timestamps
        let cur_str = format_time(cur_ms);
        let dur_str = if dur_ms > 0 { format_time(dur_ms) } else { "--:--".to_string() };

        let tc_col = premul(theme.subtitle, 210);
        if l.time_cur.2 > 0.0 {
            gdi.draw_text(&cur_str, l.timecode_px, false, tc_col, l.time_cur.0, l.time_cur.1, l.time_cur.2, l.time_cur.3, TextAlign::Left);
        }
        if l.time_dur.2 > 0.0 {
            gdi.draw_text(&dur_str, l.timecode_px, false, tc_col, l.time_dur.0, l.time_dur.1, l.time_dur.2, l.time_dur.3, TextAlign::Right);
        }

        // NOTE: Live resize number badge has been completely removed per user request!
    }

    fn has_cover(&self, track: Option<&TrackMetadata>) -> bool {
        match (track.and_then(|t| t.cover_path.as_deref()), &self.cover_cache) {
            (Some(p), Some(img)) => img.path == p,
            _ => false,
        }
    }

    fn refresh_cover(&mut self, track: Option<&TrackMetadata>) {
        let wanted = track.and_then(|t| t.cover_path.clone());
        match wanted {
            None => {
                self.cover_cache = None;
                self.prev_cover_cache = None;
                self.failed_cover = None;
                self.target_accent = 0xFF00F5D4;
            }
            Some(ref p) if self.failed_cover.as_deref() == Some(p) => {}
            Some(ref p) => {
                let need_reload = match &self.cover_cache {
                    Some(img) => &img.path != p,
                    None => true,
                };
                if need_reload {
                    let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        CachedImage::load(Path::new(&p))
                    }))
                    .ok()
                    .flatten();

                    match loaded {
                        Some(img) => {
                            unsafe {
                                if let Some(acc) = crate::palette::PaletteExtractor::extract_accent(
                                    img.image,
                                    img.width as u32,
                                    img.height as u32,
                                ) {
                                    self.target_accent = acc;
                                }
                            }
                            if self.cover_cache.is_some() {
                                self.prev_cover_cache = self.cover_cache.take();
                                self.cover_crossfade = 0.0;
                            }
                            self.cover_cache = Some(img);
                            self.failed_cover = None;
                        }
                        None => {
                            self.cover_cache = None;
                            self.failed_cover = Some(p.clone());
                            self.target_accent = 0xFF00F5D4;
                        }
                    }
                }
            }
        }
    }
}

impl Default for WidgetPainter {
    fn default() -> Self {
        Self::new()
    }
}

/// Resample a spectrum to `n` bars (bucket max).
fn resample(src: &[f32], n: usize) -> Vec<f32> {
    if src.is_empty() || n == 0 {
        return vec![0.0; n];
    }
    (0..n)
        .map(|i| {
            let a = i * src.len() / n;
            let b = ((i + 1) * src.len() / n).max(a + 1).min(src.len());
            src[a..b].iter().cloned().fold(0.0f32, f32::max)
        })
        .collect()
}
