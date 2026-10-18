use crate::Renderer;
use anyhow::{bail, Context, Result};
use std::mem::size_of;
use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, FillRect, SetStretchBltMode, StretchDIBits, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, COLORONCOLOR, DIB_RGB_COLORS, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};

const GENERIC_READ_FLAG: u32 = 0x80000000;

/// Static image wallpaper renderer powered by WIC (Windows Imaging Component).
/// Draws onto a window HDC once, consuming 0% CPU and minimal RAM.
pub struct StaticImageRenderer {
    pub hwnd: usize,
    image_width: u32,
    image_height: u32,
    pixels: Vec<u8>,
    target_width: i32,
    target_height: i32,
    loaded: bool,
}

// SAFETY: StaticImageRenderer encapsulates image buffer and targets HWND which is manipulated via thread-safe Win32 APIs.
unsafe impl Send for StaticImageRenderer {}
unsafe impl Sync for StaticImageRenderer {}

impl StaticImageRenderer {
    pub fn new(hwnd: HWND, target_width: i32, target_height: i32) -> Self {
        Self {
            hwnd: hwnd.0 as usize,
            image_width: 0,
            image_height: 0,
            pixels: Vec::new(),
            target_width,
            target_height,
            loaded: false,
        }
    }

    /// Paint the cached image buffer onto the specified HDC using proper aspect fit.
    pub fn paint(&self, hdc: HDC) {
        if !self.loaded || self.pixels.is_empty() || self.image_width == 0 || self.image_height == 0 {
            return;
        }

        unsafe {
            // Fill background with black
            let bg_brush = CreateSolidBrush(COLORREF(0x00000000));
            let rect = RECT {
                left: 0,
                top: 0,
                right: self.target_width,
                bottom: self.target_height,
            };
            FillRect(hdc, &rect, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0 as _));

            // High-quality stretching
            SetStretchBltMode(hdc, COLORONCOLOR);

            // Compute aspect fill/fit coordinates
            let img_aspect = self.image_width as f64 / self.image_height as f64;
            let target_aspect = self.target_width as f64 / self.target_height as f64;

            let (dst_x, dst_y, dst_w, dst_h) = if target_aspect > img_aspect {
                // Scale to target width, crop height
                let scaled_h = (self.target_width as f64 / img_aspect).round() as i32;
                let offset_y = (self.target_height - scaled_h) / 2;
                (0, offset_y, self.target_width, scaled_h)
            } else {
                // Scale to target height, crop width
                let scaled_w = (self.target_height as f64 * img_aspect).round() as i32;
                let offset_x = (self.target_width - scaled_w) / 2;
                (offset_x, 0, scaled_w, self.target_height)
            };

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.image_width as i32,
                    biHeight: -(self.image_height as i32), // Top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default()],
            };

            let _ = StretchDIBits(
                hdc,
                dst_x,
                dst_y,
                dst_w,
                dst_h,
                0,
                0,
                self.image_width as i32,
                self.image_height as i32,
                Some(self.pixels.as_ptr() as *const _),
                &bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }
}

impl Renderer for StaticImageRenderer {
    fn load(&mut self, source: &str) -> Result<()> {
        let path = Path::new(source);
        if !path.exists() {
            bail!("Image file not found: {}", source);
        }

        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .context("Failed to create WICImagingFactory")?;

            let source_hstring = HSTRING::from(source);
            let decoder = factory
                .CreateDecoderFromFilename(
                    PCWSTR::from_raw(source_hstring.as_ptr()),
                    None,
                    windows::Win32::Foundation::GENERIC_ACCESS_RIGHTS(GENERIC_READ_FLAG),
                    WICDecodeMetadataCacheOnDemand,
                )
                .with_context(|| format!("WIC failed to decode image: {}", source))?;

            let frame = decoder.GetFrame(0).context("Failed to read first image frame")?;

            let converter = factory
                .CreateFormatConverter()
                .context("Failed to create WICFormatConverter")?;

            converter
                .Initialize(
                    &frame,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )
                .context("WIC format conversion failed")?;

            let mut width = 0u32;
            let mut height = 0u32;
            converter.GetSize(&mut width, &mut height).context("Failed to get image size")?;

            let stride = width * 4;
            let buffer_size = (stride * height) as usize;
            let mut buffer = vec![0u8; buffer_size];

            converter
                .CopyPixels(std::ptr::null(), stride, &mut buffer)
                .context("Failed to copy WIC bitmap pixels")?;

            self.image_width = width;
            self.image_height = height;
            self.pixels = buffer;
            self.loaded = true;

            tracing::info!(
                "Loaded static wallpaper via WIC: {} ({}x{})",
                source,
                width,
                height
            );
        }

        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        // Static image has 0% CPU; no-op
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        // Static image has 0% CPU; no-op
        Ok(())
    }

    fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        self.target_width = width;
        self.target_height = height;
        Ok(())
    }

    fn unload(&mut self) -> Result<()> {
        self.pixels.clear();
        self.pixels.shrink_to_fit();
        self.image_width = 0;
        self.image_height = 0;
        self.loaded = false;
        Ok(())
    }
}
