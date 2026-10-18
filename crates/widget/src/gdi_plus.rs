use anyhow::{bail, Result};
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::GdiPlus::{
    GdipCreateFont, GdipCreateFontFamilyFromName, GdipCreateFromHDC, GdipCreateSolidFill,
    GdipCreateStringFormat, GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily,
    GdipDeleteGraphics, GdipDeleteStringFormat, GdipDisposeImage, GdipDrawImageRectI,
    GdipDrawString, GdipLoadImageFromFile, GdipSetInterpolationMode, GdipSetSmoothingMode,
    GdipSetStringFormatTrimming, GdipSetTextRenderingHint, GdiplusShutdown, GdiplusStartup,
    FontStyleBold, FontStyleRegular, GdiplusStartupInput, GdiplusStartupOutput, GpBrush,
    GpGraphics, InterpolationModeHighQualityBicubic, RectF, SmoothingModeAntiAlias,
    StringTrimmingEllipsisCharacter, TextRenderingHintAntiAliasGridFit, UnitPoint, Ok as GdiOk,
};

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

pub struct GdiPlusRenderer {
    graphics: *mut GpGraphics,
}

impl GdiPlusRenderer {
    pub fn from_hdc(hdc: HDC) -> Result<Self> {
        let mut graphics = std::ptr::null_mut();
        unsafe {
            let status = GdipCreateFromHDC(hdc, &mut graphics);
            if status != GdiOk || graphics.is_null() {
                bail!("GdipCreateFromHDC failed: {:?}", status);
            }
            let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
            let _ = GdipSetTextRenderingHint(graphics, TextRenderingHintAntiAliasGridFit);
            let _ = GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic);
        }
        Ok(Self { graphics })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &self,
        text: &str,
        font_name: &str,
        size_pt: f32,
        bold: bool,
        argb: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let wide_font: Vec<u16> = font_name.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let mut font_family = std::ptr::null_mut();
            if GdipCreateFontFamilyFromName(PCWSTR(wide_font.as_ptr()), std::ptr::null_mut(), &mut font_family) != GdiOk {
                return;
            }

            let style = if bold { FontStyleBold } else { FontStyleRegular };
            let mut font = std::ptr::null_mut();
            if GdipCreateFont(font_family, size_pt, style.0, UnitPoint, &mut font) != GdiOk {
                let _ = GdipDeleteFontFamily(font_family);
                return;
            }

            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(argb, &mut brush) != GdiOk {
                let _ = GdipDeleteFont(font);
                let _ = GdipDeleteFontFamily(font_family);
                return;
            }

            let layout_rect = RectF {
                X: x,
                Y: y,
                Width: width,
                Height: height,
            };

            let mut string_format = std::ptr::null_mut();
            let _ = GdipCreateStringFormat(0, 0, &mut string_format);
            if !string_format.is_null() {
                let _ = GdipSetStringFormatTrimming(string_format, StringTrimmingEllipsisCharacter);
            }

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

    pub fn draw_image_file(&self, path: &Path, x: i32, y: i32, width: i32, height: i32) -> bool {
        let wide_path: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let mut image = std::ptr::null_mut();
            if GdipLoadImageFromFile(PCWSTR(wide_path.as_ptr()), &mut image) != GdiOk || image.is_null() {
                return false;
            }

            let _ = GdipDrawImageRectI(self.graphics, image, x, y, width, height);
            let _ = GdipDisposeImage(image);
            true
        }
    }
}

impl Drop for GdiPlusRenderer {
    fn drop(&mut self) {
        if !self.graphics.is_null() {
            unsafe {
                let _ = GdipDeleteGraphics(self.graphics);
            }
        }
    }
}
