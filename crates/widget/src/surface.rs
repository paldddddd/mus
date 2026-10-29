/// A 32-bit pre-multiplied ARGB pixel buffer for UpdateLayeredWindow.
pub struct ArgbSurface {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>, // 0xAARRGGBB in pre-multiplied alpha
}

impl ArgbSurface {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        }
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x >= 0 && (x as u32) < self.width && y >= 0 && (y as u32) < self.height {
            let idx = (y as u32 * self.width + x as u32) as usize;
            self.pixels[idx] = color;
        }
    }

    #[inline(always)]
    pub fn blend_pixel(&mut self, x: i32, y: i32, src_color: u32) {
        if x < 0 || (x as u32) >= self.width || y < 0 || (y as u32) >= self.height {
            return;
        }
        let sa = (src_color >> 24) & 0xFF;
        if sa == 0 {
            return;
        }
        if sa == 255 {
            let idx = (y as u32 * self.width + x as u32) as usize;
            self.pixels[idx] = src_color;
            return;
        }

        let idx = (y as u32 * self.width + x as u32) as usize;
        let dst = self.pixels[idx];

        let inv_sa = 255 - sa;

        let sr = (src_color >> 16) & 0xFF;
        let sg = (src_color >> 8) & 0xFF;
        let sb = src_color & 0xFF;

        let dr = (dst >> 16) & 0xFF;
        let dg = (dst >> 8) & 0xFF;
        let db = dst & 0xFF;
        let da = (dst >> 24) & 0xFF;

        let out_a = sa + (da * inv_sa + 127) / 255;
        let out_r = sr + (dr * inv_sa + 127) / 255;
        let out_g = sg + (dg * inv_sa + 127) / 255;
        let out_b = sb + (db * inv_sa + 127) / 255;

        self.pixels[idx] = (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b;
    }

    /// Helper to produce pre-multiplied ARGB color
    pub fn premul_argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
        let fa = a as u32;
        let pr = (r as u32 * fa + 127) / 255;
        let pg = (g as u32 * fa + 127) / 255;
        let pb = (b as u32 * fa + 127) / 255;
        (fa << 24) | (pr << 16) | (pg << 8) | pb
    }

    /// Draw a rounded rectangle with smooth anti-aliased corners and pre-multiplied color
    pub fn draw_rounded_rect(
        &mut self,
        rx: f32,
        ry: f32,
        rw: f32,
        rh: f32,
        radius: f32,
        color: u32,
    ) {
        let x0 = rx.max(0.0) as i32;
        let y0 = ry.max(0.0) as i32;
        let x1 = (rx + rw).min(self.width as f32) as i32;
        let y1 = (ry + rh).min(self.height as f32) as i32;

        let r = radius.max(1.0);
        let left_inner = rx + r;
        let right_inner = rx + rw - r;
        let top_inner = ry + r;
        let bottom_inner = ry + rh - r;

        let sa = (color >> 24) & 0xFF;
        let sr = (color >> 16) & 0xFF;
        let sg = (color >> 8) & 0xFF;
        let sb = color & 0xFF;

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;

                // Determine distance from nearest corner center
                let mut dist = 0.0f32;
                if px < left_inner && py < top_inner {
                    dist = ((px - left_inner).powi(2) + (py - top_inner).powi(2)).sqrt() - r;
                } else if px > right_inner && py < top_inner {
                    dist = ((px - right_inner).powi(2) + (py - top_inner).powi(2)).sqrt() - r;
                } else if px < left_inner && py > bottom_inner {
                    dist = ((px - left_inner).powi(2) + (py - bottom_inner).powi(2)).sqrt() - r;
                } else if px > right_inner && py > bottom_inner {
                    dist = ((px - right_inner).powi(2) + (py - bottom_inner).powi(2)).sqrt() - r;
                }

                if dist <= 0.0 {
                    self.blend_pixel(x, y, color);
                } else if dist < 1.0 {
                    // Anti-aliasing edge
                    let edge_alpha = ((1.0 - dist) * (sa as f32)) as u8;
                    let edge_color = Self::premul_argb(edge_alpha, sr as u8, sg as u8, sb as u8);
                    self.blend_pixel(x, y, edge_color);
                }
            }
        }
    }

    /// Draw a subtle 1px border around a rounded rectangle
    pub fn draw_rounded_border(
        &mut self,
        rx: f32,
        ry: f32,
        rw: f32,
        rh: f32,
        radius: f32,
        border_color: u32,
    ) {
        let x0 = rx.max(0.0) as i32;
        let y0 = ry.max(0.0) as i32;
        let x1 = (rx + rw).min(self.width as f32) as i32;
        let y1 = (ry + rh).min(self.height as f32) as i32;

        let r = radius.max(1.0);
        let left_inner = rx + r;
        let right_inner = rx + rw - r;
        let top_inner = ry + r;
        let bottom_inner = ry + rh - r;

        let sa = (border_color >> 24) & 0xFF;
        let sr = (border_color >> 16) & 0xFF;
        let sg = (border_color >> 8) & 0xFF;
        let sb = border_color & 0xFF;

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;

                let dist = if px < left_inner && py < top_inner {
                    ((px - left_inner).powi(2) + (py - top_inner).powi(2)).sqrt() - r
                } else if px > right_inner && py < top_inner {
                    ((px - right_inner).powi(2) + (py - top_inner).powi(2)).sqrt() - r
                } else if px < left_inner && py > bottom_inner {
                    ((px - left_inner).powi(2) + (py - bottom_inner).powi(2)).sqrt() - r
                } else if px > right_inner && py > bottom_inner {
                    ((px - right_inner).powi(2) + (py - bottom_inner).powi(2)).sqrt() - r
                } else {
                    let d_left = px - rx;
                    let d_right = (rx + rw) - px;
                    let d_top = py - ry;
                    let d_bottom = (ry + rh) - py;
                    -d_left.min(d_right).min(d_top).min(d_bottom)
                };

                // Border thickness ~ 1.2px
                let border_dist = dist.abs();
                if border_dist < 1.0 {
                    let edge_alpha = ((1.0 - border_dist) * (sa as f32)) as u8;
                    let edge_color = Self::premul_argb(edge_alpha, sr as u8, sg as u8, sb as u8);
                    self.blend_pixel(x, y, edge_color);
                }
            }
        }
    }

    /// Draw a filled circle (e.g. for play button or vinyl disc)
    pub fn draw_circle(&mut self, cx: f32, cy: f32, radius: f32, color: u32) {
        let x0 = (cx - radius - 1.0).max(0.0) as i32;
        let y0 = (cy - radius - 1.0).max(0.0) as i32;
        let x1 = (cx + radius + 1.0).min(self.width as f32) as i32;
        let y1 = (cy + radius + 1.0).min(self.height as f32) as i32;

        let sa = (color >> 24) & 0xFF;
        let sr = (color >> 16) & 0xFF;
        let sg = (color >> 8) & 0xFF;
        let sb = color & 0xFF;

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let dist = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - radius;

                if dist <= 0.0 {
                    self.blend_pixel(x, y, color);
                } else if dist < 1.0 {
                    let edge_alpha = ((1.0 - dist) * (sa as f32)) as u8;
                    let edge_color = Self::premul_argb(edge_alpha, sr as u8, sg as u8, sb as u8);
                    self.blend_pixel(x, y, edge_color);
                }
            }
        }
    }

    /// Draw a line using Bresenham's algorithm
    pub fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: u32) {
        let mut x = x0.round() as i32;
        let mut y = y0.round() as i32;
        let x2 = x1.round() as i32;
        let y2 = y1.round() as i32;

        let dx = (x2 - x).abs();
        let dy = -(y2 - y).abs();
        let sx = if x < x2 { 1 } else { -1 };
        let sy = if y < y2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.blend_pixel(x, y, color);
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a horizontal progress bar
    #[allow(clippy::too_many_arguments)]
    pub fn draw_progress_bar(&mut self, x: f32, y: f32, w: f32, h: f32, progress: f32, bg_col: u32, fill_col: u32) {
        self.draw_progress_bar_with_knob(x, y, w, h, progress, bg_col, fill_col, fill_col, false);
    }

    /// Draw a horizontal progress bar with an optional glowing scrubber knob
    #[allow(clippy::too_many_arguments)]
    pub fn draw_progress_bar_with_knob(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        progress: f32,
        bg_col: u32,
        fill_col: u32,
        knob_col: u32,
        show_knob: bool,
    ) {
        let r = h / 2.0;
        self.draw_rounded_rect(x, y, w, h, r, bg_col);

        let clamped = progress.clamp(0.0, 1.0);
        let fill_w = (w * clamped).max(h);
        self.draw_rounded_rect(x, y, fill_w, h, r, fill_col);

        if show_knob && w > 10.0 {
            let kx = (x + w * clamped).clamp(x + r, x + w - r);
            let ky = y + h / 2.0;
            let kr = (h * 1.35).clamp(3.5, 6.5);
            self.draw_circle(kx, ky, kr, knob_col);
        }
    }

    /// Draw speaker volume icon
    pub fn draw_speaker_icon(&mut self, cx: f32, cy: f32, size: f32, volume: f32, is_muted: bool, color: u32) {
        let u = size / 10.0;
        // Speaker body (box)
        self.draw_rounded_rect(cx - u * 3.5, cy - u * 1.8, u * 2.4, u * 3.6, 1.0, color);
        // Speaker horn (trapezoid)
        let x0 = (cx - u * 1.1) as i32;
        let x1 = (cx + u * 1.5) as i32;
        for px in x0..=x1 {
            let t = if x1 > x0 { (px - x0) as f32 / (x1 - x0) as f32 } else { 0.0 };
            let half_h = u * 1.8 + u * 2.0 * t;
            self.draw_line(px as f32, cy - half_h, px as f32, cy + half_h, color);
        }

        if is_muted || volume <= 0.01 {
            // Mute cross
            let mx = cx + u * 3.2;
            let sz = u * 1.8;
            self.draw_line(mx - sz, cy - sz, mx + sz, cy + sz, color);
            self.draw_line(mx - sz, cy + sz, mx + sz, cy - sz, color);
        } else {
            // Sound wave arcs
            let a1_x = cx + u * 2.8;
            self.draw_line(a1_x, cy - u * 2.0, a1_x + u * 0.8, cy, color);
            self.draw_line(a1_x + u * 0.8, cy, a1_x, cy + u * 2.0, color);

            if volume > 0.5 {
                let a2_x = cx + u * 4.2;
                self.draw_line(a2_x, cy - u * 3.2, a2_x + u * 1.2, cy, color);
                self.draw_line(a2_x + u * 1.2, cy, a2_x, cy + u * 3.2, color);
            }
        }
    }

    /// Draw equalizer / spectrum visualizer bars
    pub fn draw_spectrum_bars(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        spectrum: &[f32],
        bar_color: u32,
    ) {
        self.draw_spectrum_bars_with_peaks(x, y, w, h, spectrum, &[], bar_color, bar_color);
    }

    /// Draw equalizer / spectrum visualizer bars with floating peak caps
    pub fn draw_spectrum_bars_with_peaks(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        spectrum: &[f32],
        peaks: &[f32],
        bar_color: u32,
        peak_color: u32,
    ) {
        if spectrum.is_empty() {
            return;
        }

        let n = spectrum.len();
        let gap = 2.0f32;
        let bar_width = ((w - gap * (n as f32 - 1.0)) / (n as f32)).max(1.0);
        let active_h = (h - 4.0).max(1.0);

        for (i, &val) in spectrum.iter().enumerate() {
            let bx = x + i as f32 * (bar_width + gap);
            let bar_h = (3.0 + active_h * val.clamp(0.0, 1.0)).min(h);
            let by = y + h - bar_h;
            self.draw_rounded_rect(bx, by, bar_width, bar_h, (bar_width / 2.0).min(3.0), bar_color);

            if let Some(&p) = peaks.get(i) {
                if p > 0.03 {
                    let peak_h = (3.0 + active_h * p.clamp(0.0, 1.0)).min(h);
                    let peak_y = (y + h - peak_h - 2.5).max(y);
                    self.draw_rounded_rect(bx, peak_y, bar_width, 2.0, 1.0, peak_color);
                }
            }
        }
    }

    /// Dispatch visualizer rendering based on style ("bars", "wave", "mirror", "dots").
    #[allow(clippy::too_many_arguments)]
    pub fn draw_visualizer_styled(
        &mut self,
        style: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        spectrum: &[f32],
        peaks: &[f32],
        bar_color: u32,
        peak_color: u32,
    ) {
        match style {
            "wave" => self.draw_spectrum_wave(x, y, w, h, spectrum, bar_color, peak_color),
            "mirror" => self.draw_spectrum_mirror(x, y, w, h, spectrum, peaks, bar_color, peak_color),
            "dots" => self.draw_spectrum_dots(x, y, w, h, spectrum, peaks, bar_color, peak_color),
            _ => self.draw_spectrum_bars_with_peaks(x, y, w, h, spectrum, peaks, bar_color, peak_color),
        }
    }

    /// Draw smooth Neon Waveform visualizer with glowing gradient fill underneath.
    pub fn draw_spectrum_wave(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        spectrum: &[f32],
        fill_color: u32,
        crest_color: u32,
    ) {
        if spectrum.is_empty() || w < 4.0 || h < 4.0 {
            return;
        }
        let n = spectrum.len();
        let x0 = x.max(0.0) as i32;
        let x1 = (x + w).min(self.width as f32) as i32;
        let base_y = (y + h).min(self.height as f32) as i32;
        let active_h = (h - 3.0).max(1.0);

        // Extract un-premultiplied components roughly from crest_color / fill_color
        let fa = ((fill_color >> 24) & 0xFF) as f32;
        let fr = ((fill_color >> 16) & 0xFF) as f32;
        let fg = ((fill_color >> 8) & 0xFF) as f32;
        let fb = (fill_color & 0xFF) as f32;

        let mut prev_wy: Option<f32> = None;

        for px in x0..x1 {
            let t = ((px as f32 - x) / w.max(1.0)).clamp(0.0, 1.0);
            let pos = t * (n.saturating_sub(1)) as f32;
            let idx = pos.floor() as usize;
            let frac = pos - idx as f32;
            // Smooth cosine interpolation between adjacent frequency bins
            let mu = (1.0 - (frac * std::f32::consts::PI).cos()) * 0.5;
            let v0 = spectrum.get(idx).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            let v1 = spectrum
                .get((idx + 1).min(n.saturating_sub(1)))
                .copied()
                .unwrap_or(v0)
                .clamp(0.0, 1.0);
            let val = v0 * (1.0 - mu) + v1 * mu;

            let wave_h = 2.0 + active_h * val;
            let wy = (y + h - wave_h).clamp(y, y + h);

            // Fill gradient under crest
            let top_py = wy.floor() as i32;
            let col_span = (base_y - top_py).max(1) as f32;
            for py in top_py..base_y {
                let v_ratio = 1.0 - ((py - top_py) as f32 / col_span);
                let alpha_scale = (v_ratio * v_ratio * 0.65).clamp(0.05, 0.75);
                let a = (fa * alpha_scale) as u8;
                let r = (fr * alpha_scale) as u8;
                let g = (fg * alpha_scale) as u8;
                let b = (fb * alpha_scale) as u8;
                let col = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                self.blend_pixel(px, py, col);
            }

            // Draw bright 2px crest line
            if let Some(p_wy) = prev_wy {
                self.draw_line((px - 1) as f32, p_wy, px as f32, wy, crest_color);
                self.draw_line((px - 1) as f32, p_wy + 1.0, px as f32, wy + 1.0, fill_color);
            } else {
                self.blend_pixel(px, wy as i32, crest_color);
            }
            prev_wy = Some(wy);
        }
    }

    /// Draw symmetric Mirror Spectrum visualizer centered vertically.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_spectrum_mirror(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        spectrum: &[f32],
        peaks: &[f32],
        bar_color: u32,
        peak_color: u32,
    ) {
        if spectrum.is_empty() {
            return;
        }
        let n = spectrum.len();
        let gap = 2.0f32;
        let bar_width = ((w - gap * (n as f32 - 1.0)) / (n as f32)).max(1.0);
        let mid_y = y + h * 0.5;
        let max_half = (h * 0.5 - 1.0).max(1.0);

        for (i, &val) in spectrum.iter().enumerate() {
            let bx = x + i as f32 * (bar_width + gap);
            let half_h = (1.5 + max_half * val.clamp(0.0, 1.0)).min(h * 0.5);
            let by = mid_y - half_h;
            self.draw_rounded_rect(
                bx,
                by,
                bar_width,
                half_h * 2.0,
                (bar_width * 0.5).min(3.0),
                bar_color,
            );

            if let Some(&p) = peaks.get(i) {
                if p > 0.06 {
                    let p_half = (1.5 + max_half * p.clamp(0.0, 1.0)).min(h * 0.5);
                    let top_peak = (mid_y - p_half - 2.0).max(y);
                    let bot_peak = (mid_y + p_half + 0.5).min(y + h - 1.5);
                    self.draw_rounded_rect(bx, top_peak, bar_width, 1.5, 0.75, peak_color);
                    self.draw_rounded_rect(bx, bot_peak, bar_width, 1.5, 0.75, peak_color);
                }
            }
        }
    }

    /// Draw retro-futuristic LED Dot Matrix visualizer.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_spectrum_dots(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        spectrum: &[f32],
        peaks: &[f32],
        bar_color: u32,
        peak_color: u32,
    ) {
        if spectrum.is_empty() {
            return;
        }
        let n = spectrum.len();
        let gap_x = 2.5f32;
        let col_w = ((w - gap_x * (n as f32 - 1.0)) / (n as f32)).max(2.0);
        let rows = ((h / 4.2).floor() as usize).clamp(3, 8);
        let gap_y = 1.5f32;
        let dot_h = ((h - gap_y * (rows as f32 - 1.0)) / (rows as f32)).max(1.5);
        let dim_dot = Self::premul_argb(20, 255, 255, 255);

        for (i, &val) in spectrum.iter().enumerate() {
            let bx = x + i as f32 * (col_w + gap_x);
            let lit_rows = (val.clamp(0.0, 1.0) * rows as f32).ceil() as usize;
            let peak_row = peaks
                .get(i)
                .map(|&p| (p.clamp(0.0, 1.0) * (rows.saturating_sub(1)) as f32).round() as usize)
                .unwrap_or(0);

            for r in 0..rows {
                let dy = y + h - (r + 1) as f32 * dot_h - r as f32 * gap_y;
                let color = if r < lit_rows {
                    if r + 1 >= rows {
                        peak_color
                    } else {
                        bar_color
                    }
                } else if r == peak_row && peak_row > 0 {
                    peak_color
                } else {
                    dim_dot
                };
                self.draw_rounded_rect(bx, dy, col_w, dot_h, (dot_h * 0.5).min(2.0), color);
            }
        }
    }

    /// Draw playback icon (Play triangle or Pause bars)
    pub fn draw_playback_icon(&mut self, cx: f32, cy: f32, size: f32, is_playing: bool, color: u32) {
        if is_playing {
            // Draw Pause (2 vertical bars)
            let bar_w = size * 0.28;
            let bar_h = size * 0.85;
            let gap = size * 0.24;

            let x1 = cx - gap / 2.0 - bar_w;
            let x2 = cx + gap / 2.0;
            let y = cy - bar_h / 2.0;

            self.draw_rounded_rect(x1, y, bar_w, bar_h, 2.0, color);
            self.draw_rounded_rect(x2, y, bar_w, bar_h, 2.0, color);
        } else {
            // Draw Play (right-pointing triangle)
            let half = size * 0.45;
            let left = cx - half * 0.7;
            let right = cx + half;
            let top = cy - half;
            let bottom = cy + half;

            let x0 = left as i32;
            let x1 = (right + 1.0) as i32;
            let y0 = top as i32;
            let y1 = (bottom + 1.0) as i32;

            for y in y0..y1 {
                let py = y as f32 + 0.5;
                for x in x0..x1 {
                    let px = x as f32 + 0.5;
                    if px >= left && px <= right {
                        let ratio = (px - left) / (right - left);
                        let edge_top = cy - half * (1.0 - ratio);
                        let edge_bottom = cy + half * (1.0 - ratio);

                        if py >= edge_top && py <= edge_bottom {
                            self.blend_pixel(x, y, color);
                        }
                    }
                }
            }
        }
    }

    /// Draw previous or next track icon
    pub fn draw_step_icon(&mut self, cx: f32, cy: f32, size: f32, is_next: bool, color: u32) {
        let half = size * 0.4;
        let bar_w = 2.0f32;

        if is_next {
            // Right arrow + vertical bar
            let left = cx - half;
            let right = cx + half * 0.5;
            for y in (cy - half) as i32..=(cy + half) as i32 {
                let py = y as f32;
                for x in left as i32..=right as i32 {
                    let px = x as f32;
                    let ratio = (px - left) / (right - left);
                    if (py - cy).abs() <= half * (1.0 - ratio) {
                        self.blend_pixel(x, y, color);
                    }
                }
            }
            self.draw_rounded_rect(right + 2.0, cy - half, bar_w, half * 2.0, 1.0, color);
        } else {
            // Vertical bar + Left arrow
            let left = cx - half * 0.5;
            let right = cx + half;
            self.draw_rounded_rect(left - bar_w - 2.0, cy - half, bar_w, half * 2.0, 1.0, color);
            for y in (cy - half) as i32..=(cy + half) as i32 {
                let py = y as f32;
                for x in left as i32..=right as i32 {
                    let px = x as f32;
                    let ratio = (right - px) / (right - left);
                    if (py - cy).abs() <= half * (1.0 - ratio) {
                        self.blend_pixel(x, y, color);
                    }
                }
            }
        }
    }
}
