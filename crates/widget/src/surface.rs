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
        let r = h / 2.0;
        self.draw_rounded_rect(x, y, w, h, r, bg_col);

        let fill_w = (w * progress.clamp(0.0, 1.0)).max(h);
        self.draw_rounded_rect(x, y, fill_w, h, r, fill_col);
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
        if spectrum.is_empty() {
            return;
        }

        let n = spectrum.len();
        let gap = 2.0f32;
        let bar_width = ((w - gap * (n as f32 - 1.0)) / (n as f32)).max(1.0);

        for (i, &val) in spectrum.iter().enumerate() {
            let bx = x + i as f32 * (bar_width + gap);
            let bar_h = (h * val.clamp(0.05, 1.0)).max(3.0);
            let by = y + h - bar_h;
            self.draw_rounded_rect(bx, by, bar_width, bar_h, bar_width / 2.0, bar_color);
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
