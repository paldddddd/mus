use crate::Renderer;
use anyhow::{bail, Context, Result};
use std::mem::size_of;
use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, CreateSolidBrush, DeleteDC, DeleteObject,
    FillRect, GetDC, InvalidateRect, ReleaseDC, SelectObject, SetBrushOrgEx, SetStretchBltMode,
    StretchBlt, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE,
    HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};

const GENERIC_READ_FLAG: u32 = 0x80000000;
const PARALLAX_OVERSCAN_PX: i32 = 48;

/// Static image wallpaper renderer powered by WIC (Windows Imaging Component).
/// Caches a pre-scaled overscanned GDI memory bitmap so normal painting and 60 FPS mouse parallax
/// execute via a single hardware-accelerated `BitBlt` with ~0% CPU usage.
pub struct StaticImageRenderer {
    pub hwnd: usize,
    image_width: u32,
    image_height: u32,
    pixels: Vec<u8>,
    target_width: i32,
    target_height: i32,
    loaded: bool,
    mouse_x: f32,
    mouse_y: f32,
    bass_pulse: f32,
    hdc_mem: HDC,
    hbitmap: HBITMAP,
    old_obj: HGDIOBJ,
    cache_width: i32,
    cache_height: i32,
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
            mouse_x: 0.5,
            mouse_y: 0.5,
            bass_pulse: 0.0,
            hdc_mem: HDC::default(),
            hbitmap: HBITMAP::default(),
            old_obj: HGDIOBJ::default(),
            cache_width: 0,
            cache_height: 0,
        }
    }

    fn free_cache_bitmap(&mut self) {
        unsafe {
            if self.hdc_mem.0 as usize != 0 {
                if self.old_obj.0 as usize != 0 {
                    let _ = SelectObject(self.hdc_mem, self.old_obj);
                    self.old_obj = HGDIOBJ::default();
                }
                if self.hbitmap.0 as usize != 0 {
                    let _ = DeleteObject(HGDIOBJ(self.hbitmap.0 as _));
                    self.hbitmap = HBITMAP::default();
                }
                let _ = DeleteDC(self.hdc_mem);
                self.hdc_mem = HDC::default();
            }
            self.cache_width = 0;
            self.cache_height = 0;
        }
    }

    fn rebuild_cache_bitmap(&mut self) {
        self.free_cache_bitmap();
        if !self.loaded
            || self.pixels.is_empty()
            || self.image_width == 0
            || self.image_height == 0
            || self.target_width <= 0
            || self.target_height <= 0
        {
            return;
        }

        let cw = self.target_width + PARALLAX_OVERSCAN_PX * 2;
        let ch = self.target_height + PARALLAX_OVERSCAN_PX * 2;

        unsafe {
            let screen_dc = GetDC(HWND::default());
            let hdc_mem = CreateCompatibleDC(screen_dc);
            if hdc_mem.0 as usize == 0 {
                let _ = ReleaseDC(HWND::default(), screen_dc);
                return;
            }

            let bmi_cache = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: cw,
                    biHeight: -ch, // Top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default()],
            };

            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbm = match CreateDIBSection(screen_dc, &bmi_cache, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) if b.0 as usize != 0 => b,
                _ => {
                    let _ = DeleteDC(hdc_mem);
                    let _ = ReleaseDC(HWND::default(), screen_dc);
                    return;
                }
            };

            let old = SelectObject(hdc_mem, HGDIOBJ(hbm.0 as _));
            let _ = ReleaseDC(HWND::default(), screen_dc);

            let bg_brush = CreateSolidBrush(COLORREF(0x00101014));
            let rect = RECT {
                left: 0,
                top: 0,
                right: cw,
                bottom: ch,
            };
            FillRect(hdc_mem, &rect, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0 as _));

            let _ = SetStretchBltMode(hdc_mem, HALFTONE);
            let _ = SetBrushOrgEx(hdc_mem, 0, 0, Some(std::ptr::null_mut::<POINT>()));

            let img_aspect = self.image_width as f64 / self.image_height as f64;
            let target_aspect = cw as f64 / ch as f64;

            let (dst_x, dst_y, dst_w, dst_h) = if target_aspect > img_aspect {
                let scaled_h = (cw as f64 / img_aspect).round() as i32;
                let offset_y = (ch - scaled_h) / 2;
                (0, offset_y, cw, scaled_h)
            } else {
                let scaled_w = (ch as f64 * img_aspect).round() as i32;
                let offset_x = (cw - scaled_w) / 2;
                (offset_x, 0, scaled_w, ch)
            };

            let bmi_src = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.image_width as i32,
                    biHeight: -(self.image_height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default()],
            };

            let _ = StretchDIBits(
                hdc_mem,
                dst_x,
                dst_y,
                dst_w,
                dst_h,
                0,
                0,
                self.image_width as i32,
                self.image_height as i32,
                Some(self.pixels.as_ptr() as *const _),
                &bmi_src,
                DIB_RGB_COLORS,
                SRCCOPY,
            );

            // Force opaque alpha channel (0xFF) on every pixel in the cached DIB
            if !bits.is_null() {
                let px_slice = std::slice::from_raw_parts_mut(bits as *mut u8, (cw * ch * 4) as usize);
                for px in px_slice.chunks_exact_mut(4) {
                    px[3] = 255;
                }
            }

            self.hdc_mem = hdc_mem;
            self.hbitmap = hbm;
            self.old_obj = old;
            self.cache_width = cw;
            self.cache_height = ch;
        }
    }

    /// Paint the cached image buffer onto the specified HDC with fast BitBlt and parallax offset.
    pub fn paint(&self, hdc: HDC) {
        if !self.loaded {
            return;
        }

        unsafe {
            if self.hdc_mem.0 as usize != 0 && self.cache_width >= self.target_width && self.cache_height >= self.target_height {
                let max_shift_x = (self.cache_width - self.target_width).max(0) as f32;
                let max_shift_y = (self.cache_height - self.target_height).max(0) as f32;
                let base_src_x = (self.mouse_x.clamp(0.0, 1.0) * max_shift_x).round() as i32;
                let base_src_y = (self.mouse_y.clamp(0.0, 1.0) * max_shift_y).round() as i32;

                if self.bass_pulse > 0.005 {
                    // Smooth rhythmic zoom-punch on bass beats (scales up to ~24px overscan crop)
                    let zoom_px = (self.bass_pulse * 24.0).round() as i32;
                    let aspect = self.target_height as f32 / self.target_width as f32;
                    let zoom_py = (zoom_px as f32 * aspect).round() as i32;

                    let crop_x = (base_src_x + zoom_px).clamp(0, self.cache_width - 10);
                    let crop_y = (base_src_y + zoom_py).clamp(0, self.cache_height - 10);
                    let crop_w = (self.target_width - zoom_px * 2).max(10);
                    let crop_h = (self.target_height - zoom_py * 2).max(10);

                    let _ = SetStretchBltMode(hdc, HALFTONE);
                    let _ = StretchBlt(
                        hdc,
                        0,
                        0,
                        self.target_width,
                        self.target_height,
                        self.hdc_mem,
                        crop_x,
                        crop_y,
                        crop_w,
                        crop_h,
                        SRCCOPY,
                    );
                } else {
                    let _ = BitBlt(
                        hdc,
                        0,
                        0,
                        self.target_width,
                        self.target_height,
                        self.hdc_mem,
                        base_src_x,
                        base_src_y,
                        SRCCOPY,
                    );
                }
            }
        }
    }
}

impl Drop for StaticImageRenderer {
    fn drop(&mut self) {
        self.free_cache_bitmap();
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
            self.rebuild_cache_bitmap();

            let hwnd = HWND(self.hwnd as _);
            if hwnd.0 as usize != 0 {
                let _ = InvalidateRect(hwnd, None, false);
            }

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
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        let hwnd = HWND(self.hwnd as _);
        if hwnd.0 as usize != 0 {
            unsafe {
                let _ = InvalidateRect(hwnd, None, false);
            }
        }
        Ok(())
    }

    fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        if self.target_width != width || self.target_height != height {
            self.target_width = width;
            self.target_height = height;
            if self.loaded {
                self.rebuild_cache_bitmap();
            }
        }
        Ok(())
    }

    fn unload(&mut self) -> Result<()> {
        self.free_cache_bitmap();
        self.pixels.clear();
        self.pixels.shrink_to_fit();
        self.image_width = 0;
        self.image_height = 0;
        self.loaded = false;
        Ok(())
    }

    fn update_mouse(&mut self, x: f32, y: f32) {
        if (self.mouse_x - x).abs() > 0.0005 || (self.mouse_y - y).abs() > 0.0005 {
            self.mouse_x = x.clamp(0.0, 1.0);
            self.mouse_y = y.clamp(0.0, 1.0);
            let hwnd = HWND(self.hwnd as _);
            if hwnd.0 as usize != 0 {
                unsafe {
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
        }
    }

    fn update_bass(&mut self, bass: f32) {
        let prev = self.bass_pulse;
        let target = bass.clamp(0.0, 1.0);
        if (target - prev).abs() > 0.004 {
            self.bass_pulse = target;
            let hwnd = HWND(self.hwnd as _);
            if hwnd.0 as usize != 0 {
                unsafe {
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
        }
    }
}
