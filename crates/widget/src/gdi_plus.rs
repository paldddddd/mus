use anyhow::{bail, Result};
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Graphics::GdiPlus::{
    FillModeAlternate, FontStyleBold, FontStyleRegular, GdipAddPathArc,
    GdipAddPathEllipse, GdipClosePathFigure, GdipCreateBitmapFromScan0, GdipCreateFont,
    GdipCreateFontFamilyFromName, GdipCreatePath, GdipCreateSolidFill, GdipCreateStringFormat,
    GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily, GdipDeleteGraphics,
    GdipDeletePath, GdipDeleteStringFormat, GdipDisposeImage, GdipDrawImageRectRect, GdipDrawString,
    GdipGetImageGraphicsContext, GdipGetImageHeight, GdipGetImageWidth, GdipLoadImageFromFile,
    GdipMeasureString, GdipResetClip, GdipResetWorldTransform, GdipRotateWorldTransform,
    GdipSetClipPath, GdipSetClipRect, GdipSetInterpolationMode, GdipSetPixelOffsetMode,
    GdipSetSmoothingMode, GdipSetStringFormatAlign, GdipSetStringFormatFlags, GdipSetStringFormatLineAlign,
    GdipSetStringFormatTrimming, GdipSetTextRenderingHint, GdipTranslateWorldTransform,
    GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, GdiplusStartupOutput, GpBitmap, GpBrush, GpGraphics,
    GpImage, GpPath, InterpolationModeHighQualityBicubic, MatrixOrderAppend, Ok as GdiOk,
    PixelOffsetModeHighQuality, RectF, SmoothingModeAntiAlias, StringAlignment, StringAlignmentCenter,
    StringTrimmingEllipsisCharacter, TextRenderingHintAntiAlias, UnitPixel, CombineModeReplace,
};

/// `PixelFormat32bppPARGB` (not exported as a constant by the `windows` crate).
const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;
/// `StringFormatFlagsNoWrap`
const STRING_FORMAT_FLAGS_NO_WRAP: i32 = 0x0000_1000;

pub struct GdiPlusGuard {
    token: usize,
}

impl GdiPlusGuard {
    pub fn init() -> Result<Self> {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: false.into(),
            SuppressExternalCodecs: false.into(),
        };
        let mut token = 0usize;
        let mut output = GdiplusStartupOutput::default();

        unsafe {
            let status = GdiplusStartup(&mut token, &input, &mut output);
            if status != GdiOk {
                bail!("GdiplusStartup failed with status: {:?}", status);
            }
        }
        Ok(Self { token })
    }
}

impl Drop for GdiPlusGuard {
    fn drop(&mut self) {
        unsafe {
            GdiplusShutdown(self.token);
        }
    }
}

/// A decoded image kept in memory so it is not re-read from disk on every frame.
pub struct CachedImage {
    pub path: String,
    pub image: *mut GpImage,
    pub width: f32,
    pub height: f32,
}

impl CachedImage {
    pub fn load(path: &Path) -> Option<Self> {
        let wide_path: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let mut image = std::ptr::null_mut();
            if GdipLoadImageFromFile(PCWSTR(wide_path.as_ptr()), &mut image) != GdiOk
                || image.is_null()
            {
                return None;
            }
            let (mut w, mut h) = (0u32, 0u32);
            let _ = GdipGetImageWidth(image, &mut w);
            let _ = GdipGetImageHeight(image, &mut h);
            if w == 0 || h == 0 {
                let _ = GdipDisposeImage(image);
                return None;
            }
            Some(Self {
                path: path.to_string_lossy().into_owned(),
                image,
                width: w as f32,
                height: h as f32,
            })
        }
    }
}

impl Drop for CachedImage {
    fn drop(&mut self) {
        if !self.image.is_null() {
            unsafe {
                let _ = GdipDisposeImage(self.image);
            }
        }
    }
}

/// Horizontal text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

static SEGOE_UI_WIDE: &[u16] = &[
    'S' as u16, 'e' as u16, 'g' as u16, 'o' as u16, 'e' as u16, ' ' as u16, 'U' as u16, 'I' as u16, 0,
];

pub struct GdiPlusRenderer {
    bitmap: *mut GpBitmap,
    graphics: *mut GpGraphics,
}

impl GdiPlusRenderer {
    /// Wrap an existing top-down 32bpp premultiplied ARGB buffer (e.g. a DIB section).
    ///
    /// # Safety
    /// `bits` must point to `width * height` writable `u32` pixels that outlive the renderer.
    pub unsafe fn from_bits(bits: *mut u32, width: u32, height: u32) -> Result<Self> {
        let mut bitmap = std::ptr::null_mut();
        let status = GdipCreateBitmapFromScan0(
            width as i32,
            height as i32,
            (width * 4) as i32,
            PIXEL_FORMAT_32BPP_PARGB,
            Some(bits as *const u8),
            &mut bitmap,
        );
        if status != GdiOk || bitmap.is_null() {
            bail!("GdipCreateBitmapFromScan0 failed: {:?}", status);
        }
        let mut graphics = std::ptr::null_mut();
        let status = GdipGetImageGraphicsContext(bitmap as *mut GpImage, &mut graphics);
        if status != GdiOk || graphics.is_null() {
            let _ = GdipDisposeImage(bitmap as *mut GpImage);
            bail!("GdipGetImageGraphicsContext failed: {:?}", status);
        }
        let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let _ = GdipSetTextRenderingHint(graphics, TextRenderingHintAntiAlias);
        let _ = GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic);
        Ok(Self { bitmap, graphics })
    }

    /// Draw single-line text with ellipsis trimming. `size_px` is the font em-size in pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &self,
        text: &str,
        size_px: f32,
        bold: bool,
        argb: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        align: TextAlign,
    ) {
        self.draw_text_marquee(text, size_px, bold, argb, x, y, width, height, 0.0, align);
    }

    /// Measure text width in pixels without rendering.
    pub fn measure_text_width(&self, text: &str, size_px: f32, bold: bool) -> f32 {
        if text.is_empty() || size_px < 1.0 {
            return 0.0;
        }
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let mut font_family = std::ptr::null_mut();
            if GdipCreateFontFamilyFromName(
                PCWSTR(SEGOE_UI_WIDE.as_ptr()),
                std::ptr::null_mut(),
                &mut font_family,
            ) != GdiOk {
                return 0.0;
            }
            let style = if bold { FontStyleBold } else { FontStyleRegular };
            let mut font = std::ptr::null_mut();
            if GdipCreateFont(font_family, size_px, style.0, UnitPixel, &mut font) != GdiOk {
                let _ = GdipDeleteFontFamily(font_family);
                return 0.0;
            }
            let layout_rect = RectF { X: 0.0, Y: 0.0, Width: 10000.0, Height: 200.0 };
            let mut bounding_box = RectF::default();
            let _ = GdipMeasureString(
                self.graphics,
                PCWSTR(wide_text.as_ptr()),
                wide_text.len() as i32,
                font,
                &layout_rect,
                std::ptr::null(),
                &mut bounding_box,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            let _ = GdipDeleteFont(font);
            let _ = GdipDeleteFontFamily(font_family);
            bounding_box.Width
        }
    }

    /// Draw single-line text with smooth marquee scrolling if width exceeds container.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_marquee(
        &self,
        text: &str,
        size_px: f32,
        bold: bool,
        argb: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        scroll_offset: f32,
        align: TextAlign,
    ) {
        if text.is_empty() || width <= 1.0 || height <= 1.0 || size_px < 1.0 {
            return;
        }
        let wide_text: Vec<u16> = text.encode_utf16().collect();

        unsafe {
            let mut font_family = std::ptr::null_mut();
            if GdipCreateFontFamilyFromName(
                PCWSTR(SEGOE_UI_WIDE.as_ptr()),
                std::ptr::null_mut(),
                &mut font_family,
            ) != GdiOk
            {
                return;
            }

            let style = if bold { FontStyleBold } else { FontStyleRegular };
            let mut font = std::ptr::null_mut();
            if GdipCreateFont(font_family, size_px, style.0, UnitPixel, &mut font) != GdiOk {
                let _ = GdipDeleteFontFamily(font_family);
                return;
            }

            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(argb, &mut brush) != GdiOk {
                let _ = GdipDeleteFont(font);
                let _ = GdipDeleteFontFamily(font_family);
                return;
            }

            let mut string_format = std::ptr::null_mut();
            let _ = GdipCreateStringFormat(0, 0, &mut string_format);
            if !string_format.is_null() {
                let _ = GdipSetStringFormatFlags(string_format, STRING_FORMAT_FLAGS_NO_WRAP);
                let _ = GdipSetStringFormatLineAlign(string_format, StringAlignmentCenter);
                if scroll_offset > 0.0 {
                    let _ = GdipSetStringFormatAlign(string_format, StringAlignment(0));
                } else {
                    let h_align = match align {
                        TextAlign::Left => StringAlignment(0),
                        TextAlign::Center => StringAlignmentCenter,
                        TextAlign::Right => StringAlignment(2),
                    };
                    let _ = GdipSetStringFormatAlign(string_format, h_align);
                    let _ = GdipSetStringFormatTrimming(string_format, StringTrimmingEllipsisCharacter);
                }
            }

            if scroll_offset > 0.0 {
                let _ = GdipSetClipRect(self.graphics, x, y, width, height, CombineModeReplace);
                let layout_rect = RectF {
                    X: x - scroll_offset,
                    Y: y,
                    Width: 10000.0,
                    Height: height,
                };
                let _ = GdipDrawString(
                    self.graphics,
                    PCWSTR(wide_text.as_ptr()),
                    wide_text.len() as i32,
                    font,
                    &layout_rect,
                    string_format,
                    brush as *const GpBrush,
                );
                let _ = GdipResetClip(self.graphics);
            } else {
                let layout_rect = RectF { X: x, Y: y, Width: width, Height: height };
                let _ = GdipDrawString(
                    self.graphics,
                    PCWSTR(wide_text.as_ptr()),
                    wide_text.len() as i32,
                    font,
                    &layout_rect,
                    string_format,
                    brush as *const GpBrush,
                );
            }

            if !string_format.is_null() {
                let _ = GdipDeleteStringFormat(string_format);
            }
            let _ = GdipDeleteBrush(brush as _);
            let _ = GdipDeleteFont(font);
            let _ = GdipDeleteFontFamily(font_family);
        }
    }

    /// Fill a rounded rectangle (or a circle when `circle == true`) with the image,
    /// center-cropped to cover the target area. High-quality bicubic resampling.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_image_shape(
        &self,
        img: &CachedImage,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        circle: bool,
        rotation_deg: f32,
    ) -> bool {
        if width < 2.0 || height < 2.0 || img.width <= 1.0 || img.height <= 1.0 || img.image.is_null() {
            return false;
        }
        unsafe {
            let mut path: *mut GpPath = std::ptr::null_mut();
            if GdipCreatePath(FillModeAlternate, &mut path) != GdiOk || path.is_null() {
                return false;
            }
            if circle {
                let _ = GdipAddPathEllipse(path, x, y, width, height);
            } else {
                let d = (radius * 2.0).min(width).min(height).max(1.0);
                let _ = GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
                let _ = GdipAddPathArc(path, x + width - d, y, d, d, 270.0, 90.0);
                let _ = GdipAddPathArc(path, x + width - d, y + height - d, d, d, 0.0, 90.0);
                let _ = GdipAddPathArc(path, x, y + height - d, d, d, 90.0, 90.0);
            }
            let _ = GdipClosePathFigure(path);

            // Clip the graphics output to the exact path
            let _ = GdipSetClipPath(self.graphics, path, CombineModeReplace);
            let _ = GdipSetInterpolationMode(self.graphics, InterpolationModeHighQualityBicubic);
            let _ = GdipSetPixelOffsetMode(self.graphics, PixelOffsetModeHighQuality);

            // Compute center-cropped target dimensions
            let s = (width / img.width).max(height / img.height).max(0.001);
            let dw = img.width * s;
            let dh = img.height * s;
            let dx = x + (width - dw) / 2.0;
            let dy = y + (height - dh) / 2.0;

            if rotation_deg != 0.0 {
                let (cx, cy) = (x + width / 2.0, y + height / 2.0);
                let _ = GdipTranslateWorldTransform(self.graphics, cx, cy, MatrixOrderAppend);
                let _ = GdipRotateWorldTransform(self.graphics, rotation_deg, MatrixOrderAppend);
                let _ = GdipTranslateWorldTransform(self.graphics, -cx, -cy, MatrixOrderAppend);
            }

            let ok = GdipDrawImageRectRect(
                self.graphics,
                img.image,
                dx,
                dy,
                dw,
                dh,
                0.0,
                0.0,
                img.width,
                img.height,
                UnitPixel,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
            ) == GdiOk;

            if rotation_deg != 0.0 {
                let _ = GdipResetWorldTransform(self.graphics);
            }
            let _ = GdipResetClip(self.graphics);
            let _ = GdipDeletePath(path);
            ok
        }
    }

    /// Draw multi-line wrapped text inside `(x, y, width, height)`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_wrapped(
        &self,
        text: &str,
        size_px: f32,
        bold: bool,
        argb: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        align: TextAlign,
    ) {
        if text.is_empty() || width <= 1.0 || height <= 1.0 || size_px < 1.0 {
            return;
        }
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let mut font_family = std::ptr::null_mut();
            if GdipCreateFontFamilyFromName(
                PCWSTR(SEGOE_UI_WIDE.as_ptr()),
                std::ptr::null_mut(),
                &mut font_family,
            ) != GdiOk {
                return;
            }
            let style = if bold { FontStyleBold } else { FontStyleRegular };
            let mut font = std::ptr::null_mut();
            if GdipCreateFont(font_family, size_px, style.0, UnitPixel, &mut font) != GdiOk {
                let _ = GdipDeleteFontFamily(font_family);
                return;
            }
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(argb, &mut brush) != GdiOk {
                let _ = GdipDeleteFont(font);
                let _ = GdipDeleteFontFamily(font_family);
                return;
            }
            let mut string_format = std::ptr::null_mut();
            let _ = GdipCreateStringFormat(0, 0, &mut string_format);
            if !string_format.is_null() {
                let _ = GdipSetStringFormatLineAlign(string_format, StringAlignmentCenter);
                let h_align = match align {
                    TextAlign::Left => StringAlignment(0),
                    TextAlign::Center => StringAlignmentCenter,
                    TextAlign::Right => StringAlignment(2),
                };
                let _ = GdipSetStringFormatAlign(string_format, h_align);
                let _ = GdipSetStringFormatTrimming(string_format, StringTrimmingEllipsisCharacter);
            }
            let layout_rect = RectF { X: x, Y: y, Width: width, Height: height };
            let _ = GdipDrawString(
                self.graphics,
                PCWSTR(wide_text.as_ptr()),
                wide_text.len() as i32,
                font,
                &layout_rect,
                string_format,
                brush as *const GpBrush,
            );
            if !string_format.is_null() {
                let _ = GdipDeleteStringFormat(string_format);
            }
            let _ = GdipDeleteBrush(brush as _);
            let _ = GdipDeleteFont(font);
            let _ = GdipDeleteFontFamily(font_family);
        }
    }

    /// Measure wrapped text size inside `max_width`.
    pub fn measure_text_wrapped(&self, text: &str, size_px: f32, bold: bool, max_width: f32) -> (f32, f32) {
        if text.is_empty() || size_px < 1.0 || max_width < 1.0 {
            return (0.0, 0.0);
        }
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let mut font_family = std::ptr::null_mut();
            if GdipCreateFontFamilyFromName(
                PCWSTR(SEGOE_UI_WIDE.as_ptr()),
                std::ptr::null_mut(),
                &mut font_family,
            ) != GdiOk {
                return (0.0, 0.0);
            }
            let style = if bold { FontStyleBold } else { FontStyleRegular };
            let mut font = std::ptr::null_mut();
            if GdipCreateFont(font_family, size_px, style.0, UnitPixel, &mut font) != GdiOk {
                let _ = GdipDeleteFontFamily(font_family);
                return (0.0, 0.0);
            }
            let layout_rect = RectF { X: 0.0, Y: 0.0, Width: max_width, Height: 10000.0 };
            let mut bounding_box = RectF::default();
            let _ = GdipMeasureString(
                self.graphics,
                PCWSTR(wide_text.as_ptr()),
                wide_text.len() as i32,
                font,
                &layout_rect,
                std::ptr::null(),
                &mut bounding_box,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            let _ = GdipDeleteFont(font);
            let _ = GdipDeleteFontFamily(font_family);
            (bounding_box.Width, bounding_box.Height)
        }
    }
}

impl Drop for GdiPlusRenderer {
    fn drop(&mut self) {
        unsafe {
            if !self.graphics.is_null() {
                let _ = GdipDeleteGraphics(self.graphics);
            }
            if !self.bitmap.is_null() {
                let _ = GdipDisposeImage(self.bitmap as *mut GpImage);
            }
        }
    }
}
