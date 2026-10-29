//! Dynamic palette extraction and color interpolation for wallmus widget.

use windows::Win32::Graphics::GdiPlus::{
    GdipCreateBitmapFromScan0, GdipDeleteGraphics, GdipDisposeImage, GdipDrawImageRectRectI,
    GdipGetImageGraphicsContext, GdipSetInterpolationMode, GpImage,
    InterpolationModeBilinear, Ok as GdiOk, UnitPixel,
};

/// 32bpp premultiplied ARGB pixel format constant for GDI+
const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;

pub struct PaletteExtractor;

impl PaletteExtractor {
    /// Extracts a vibrant dominant accent color from a GDI+ image thumbnail.
    /// Returns 0xAARRGGBB opaque color.
    pub unsafe fn extract_accent(image: *mut GpImage, w: u32, h: u32) -> Option<u32> {
        if image.is_null() || w == 0 || h == 0 {
            return None;
        }

        // Fast 24x24 memory bitmap thumbnail (576 pixels)
        const SAMPLE_SZ: i32 = 24;
        let mut pixels = vec![0u32; (SAMPLE_SZ * SAMPLE_SZ) as usize];
        let mut bitmap = std::ptr::null_mut();
        let status = GdipCreateBitmapFromScan0(
            SAMPLE_SZ,
            SAMPLE_SZ,
            SAMPLE_SZ * 4,
            PIXEL_FORMAT_32BPP_PARGB,
            Some(pixels.as_mut_ptr() as *mut u8),
            &mut bitmap,
        );
        if status != GdiOk || bitmap.is_null() {
            return None;
        }

        let mut graphics = std::ptr::null_mut();
        if GdipGetImageGraphicsContext(bitmap as *mut GpImage, &mut graphics) != GdiOk || graphics.is_null() {
            let _ = GdipDisposeImage(bitmap as *mut GpImage);
            return None;
        }

        let _ = GdipSetInterpolationMode(graphics, InterpolationModeBilinear);
        let _ = GdipDrawImageRectRectI(
            graphics,
            image,
            0,
            0,
            SAMPLE_SZ,
            SAMPLE_SZ,
            0,
            0,
            w as i32,
            h as i32,
            UnitPixel,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
        );

        let _ = GdipDeleteGraphics(graphics);
        let _ = GdipDisposeImage(bitmap as *mut GpImage);

        // Find the most vibrant, readable accent color
        let mut best_color: Option<u32> = None;
        let mut max_score = 0.0f32;

        for &pixel in &pixels {
            let a = (pixel >> 24) & 0xFF;
            if a < 128 {
                continue;
            }
            let r = ((pixel >> 16) & 0xFF) as f32;
            let g = ((pixel >> 8) & 0xFF) as f32;
            let b = (pixel & 0xFF) as f32;

            let max_c = r.max(g).max(b);
            let min_c = r.min(g).min(b);
            let delta = max_c - min_c;
            let sat = if max_c > 0.0 { delta / max_c } else { 0.0 };
            let val = max_c / 255.0;

            // Discard too dark (< 22%), too white (> 95% with low sat), or grayscale (< 18% sat)
            if val < 0.22 || (val > 0.94 && sat < 0.3) || sat < 0.18 {
                continue;
            }

            // Score color by vibrance (saturation * brightness)
            let score = sat * 1.6 + val * 0.9;
            if score > max_score {
                max_score = score;
                // Boost brightness & saturation slightly for clean neon glow
                let boost_r = ((r * 1.12).clamp(0.0, 255.0)) as u32;
                let boost_g = ((g * 1.12).clamp(0.0, 255.0)) as u32;
                let boost_b = ((b * 1.12).clamp(0.0, 255.0)) as u32;
                best_color = Some(0xFF000000 | (boost_r << 16) | (boost_g << 8) | boost_b);
            }
        }

        best_color
    }

    /// Smoothly interpolates (Lerp) between two ARGB colors.
    #[inline]
    pub fn lerp_color(from: u32, to: u32, t: f32) -> u32 {
        let t = t.clamp(0.0, 1.0);
        let a1 = ((from >> 24) & 0xFF) as f32;
        let r1 = ((from >> 16) & 0xFF) as f32;
        let g1 = ((from >> 8) & 0xFF) as f32;
        let b1 = (from & 0xFF) as f32;

        let a2 = ((to >> 24) & 0xFF) as f32;
        let r2 = ((to >> 16) & 0xFF) as f32;
        let g2 = ((to >> 8) & 0xFF) as f32;
        let b2 = (to & 0xFF) as f32;

        let a = (a1 + (a2 - a1) * t).round() as u32;
        let r = (r1 + (r2 - r1) * t).round() as u32;
        let g = (g1 + (g2 - g1) * t).round() as u32;
        let b = (b1 + (b2 - b1) * t).round() as u32;

        (a << 24) | (r << 16) | (g << 8) | b
    }
}
