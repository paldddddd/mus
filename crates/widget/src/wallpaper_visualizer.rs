use crate::surface::ArgbSurface;
use anyhow::{bail, Context, Result};
use common::{MonitorInfo, WallpaperVisualizerConfig};
use desktop_host::{attach, detach, revalidate, AttachHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, UpdateLayeredWindow, HCURSOR,
    HICON, ULW_ALPHA, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_POPUP,
};

static VISUALIZER_CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);
const VISUALIZER_CLASS_NAME: PCWSTR = w!("wallmus_WallpaperVisualizer");

/// Transparent layered window rendering live audio visualizer overlay directly over wallpapers.
pub struct WallpaperVisualizerWindow {
    pub hwnd: HWND,
    pub monitor: MonitorInfo,
    attach_handle: Option<AttachHandle>,
    surface: ArgbSurface,
    hdc_screen: HDC,
    hdc_mem: HDC,
    hbitmap: HBITMAP,
    dib_bits: *mut u32,
    decayed_bars: Vec<f32>,
    peak_caps: Vec<f32>,
    was_blank: bool,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
}

// SAFETY: HWND and GDI handles are managed and updated safely
unsafe impl Send for WallpaperVisualizerWindow {}
unsafe impl Sync for WallpaperVisualizerWindow {}

impl WallpaperVisualizerWindow {
    /// Create wallpaper visualizer overlay window for a given monitor.
    pub fn create(monitor: MonitorInfo, config: &WallpaperVisualizerConfig) -> Result<Box<Self>> {
        desktop_host::ensure_default_desktop();
        ensure_class_registered()?;

        let width = monitor.width as u32;
        let height = config.height_px.clamp(40, 480);
        let x = monitor.x;
        let y = monitor.y + monitor.height - height as i32;

        unsafe {
            let hinstance = GetModuleHandleW(PCWSTR::null())
                .context("Failed to get module handle for wallpaper visualizer")?;

            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                VISUALIZER_CLASS_NAME,
                w!("wallmus Wallpaper Visualizer"),
                WS_POPUP,
                x,
                y,
                width as i32,
                height as i32,
                HWND::default(),
                None,
                hinstance,
                None,
            )?;

            if hwnd.0 as usize == 0 {
                bail!("CreateWindowExW failed for wallpaper visualizer");
            }

            // Create memory DC and DIB section for 32-bit ARGB UpdateLayeredWindow
            let hdc_screen = GetDC(HWND::default());
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // Top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut dib_bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbitmap = CreateDIBSection(
                hdc_mem,
                &bmi,
                DIB_RGB_COLORS,
                &mut dib_bits,
                None,
                0,
            )?;

            SelectObject(hdc_mem, hbitmap);

            let surface = ArgbSurface::new(width, height);

            // Attach behind desktop icons (above wallpaper layers)
            let handle = attach(hwnd, &monitor).context("Failed to attach visualizer to desktop")?;

            tracing::info!(
                "Created wallpaper visualizer overlay on monitor {} ({}x{}) HWND 0x{:X}",
                monitor.name,
                width,
                height,
                hwnd.0 as usize
            );

            Ok(Box::new(Self {
                hwnd,
                monitor: monitor.clone(),
                attach_handle: Some(handle),
                surface,
                hdc_screen,
                hdc_mem,
                hbitmap,
                dib_bits: dib_bits as *mut u32,
                decayed_bars: vec![0.0f32; config.bars_count as usize],
                peak_caps: vec![0.0f32; config.bars_count as usize],
                was_blank: true,
                width,
                height,
                x,
                y,
            }))
        }
    }

    /// Render visualizer frame according to audio spectrum and active configuration.
    pub fn update(&mut self, spectrum: &[f32], config: &WallpaperVisualizerConfig, is_paused: bool) {
        if !config.enabled || is_paused {
            if !self.was_blank {
                self.clear_frame(config.opacity);
                self.was_blank = true;
            }
            return;
        }

        let num_bars = (config.bars_count as usize).clamp(16, 128);
        if self.decayed_bars.len() != num_bars {
            self.decayed_bars = vec![0.0f32; num_bars];
            self.peak_caps = vec![0.0f32; num_bars];
        }

        // Resample input spectrum to target bars count
        let resampled = resample_spectrum(spectrum, num_bars);

        let mut has_energy = false;
        for i in 0..num_bars {
            let energy = resampled.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            if energy > 0.01 {
                has_energy = true;
            }
            // Fast attack, smooth decay
            let prev = self.decayed_bars[i];
            let target = energy.max(prev * 0.84);
            self.decayed_bars[i] = target;

            // Peak cap drop with gravity
            let peak_prev = self.peak_caps[i];
            let peak_target = target.max(peak_prev - 0.012);
            self.peak_caps[i] = peak_target;
        }

        if !has_energy && self.decayed_bars.iter().all(|&v| v < 0.005) {
            if !self.was_blank {
                self.clear_frame(config.opacity);
                self.was_blank = true;
            }
            return;
        }

        self.was_blank = false;
        self.surface.clear();

        let w = self.width as f32;
        let h = self.height as f32;

        match config.style.as_str() {
            "wave" => self.paint_wave_style(w, h, &config.accent_color),
            "glow" => self.paint_glow_style(w, h, &config.accent_color),
            _ => self.paint_bars_style(w, h, &config.accent_color),
        }

        self.commit_frame(config.opacity);
    }

    fn paint_bars_style(&mut self, w: f32, h: f32, accent: &str) {
        let n = self.decayed_bars.len();
        if n == 0 {
            return;
        }

        let total_padding = w * 0.08; // 8% total horizontal margins
        let available_w = w - total_padding * 2.0;
        let bar_width = (available_w / n as f32) * 0.72;
        let bar_gap = (available_w / n as f32) * 0.28;

        for i in 0..n {
            let val = self.decayed_bars[i];
            let peak = self.peak_caps[i];
            let bar_h = (val * (h - 18.0)).max(2.0);
            let x = total_padding + i as f32 * (bar_width + bar_gap);
            let y = h - bar_h;

            let color = get_accent_color(accent, i, n, 230);
            let peak_color = get_accent_color(accent, i, n, 255);

            // Draw equalizer bar with rounded top
            self.surface.draw_rounded_rect(x, y, bar_width, bar_h, 3.0, color);

            // Draw floating peak cap dot
            let peak_y = (h - (peak * (h - 18.0)) - 6.0).max(2.0);
            self.surface.draw_rounded_rect(x, peak_y, bar_width, 2.5, 1.0, peak_color);
        }
    }

    fn paint_wave_style(&mut self, w: f32, h: f32, accent: &str) {
        let n = self.decayed_bars.len();
        if n < 2 {
            return;
        }

        let step_x = w / (n - 1) as f32;
        let wave_color = get_accent_color(accent, n / 2, n, 240);

        let mut prev_x = 0.0f32;
        let mut prev_y = h - (self.decayed_bars[0] * (h - 12.0));

        for i in 1..n {
            let curr_x = i as f32 * step_x;
            let curr_y = h - (self.decayed_bars[i] * (h - 12.0));

            self.surface.draw_line(prev_x, prev_y, curr_x, curr_y, wave_color);
            // Draw subtle thickness
            self.surface.draw_line(prev_x, prev_y + 1.0, curr_x, curr_y + 1.0, wave_color);

            prev_x = curr_x;
            prev_y = curr_y;
        }
    }

    fn paint_glow_style(&mut self, w: f32, h: f32, accent: &str) {
        // Average low frequencies (bass) for ambient horizon pulse
        let bass_energy = self.decayed_bars.iter().take(8).sum::<f32>() / 8.0f32.max(1.0);
        let glow_h = (bass_energy * h).clamp(4.0, h);
        let glow_color = get_accent_color(accent, 0, 1, ((bass_energy * 200.0) as u8).max(40));

        self.surface.draw_rounded_rect(0.0, h - glow_h, w, glow_h, 0.0, glow_color);
    }

    fn clear_frame(&mut self, opacity: f32) {
        self.surface.clear();
        self.commit_frame(opacity);
    }

    fn commit_frame(&mut self, opacity: f32) {
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.surface.pixels.as_ptr(),
                self.dib_bits,
                (self.width * self.height) as usize,
            );

            let pt_src = POINT { x: 0, y: 0 };
            let pt_dst = POINT {
                x: self.x,
                y: self.y,
            };
            let size = SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };

            let blend = BLENDFUNCTION {
                BlendOp: 0, // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: (opacity.clamp(0.0, 1.0) * 255.0) as u8,
                AlphaFormat: 1, // AC_SRC_ALPHA
            };

            let _ = UpdateLayeredWindow(
                self.hwnd,
                self.hdc_screen,
                Some(&pt_dst),
                Some(&size),
                self.hdc_mem,
                Some(&pt_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    /// Revalidate attachment when monitors or shell change.
    pub fn revalidate(&mut self) -> Result<()> {
        if let Some(ref mut handle) = self.attach_handle {
            revalidate(handle)?;
        }
        Ok(())
    }
}

impl Drop for WallpaperVisualizerWindow {
    fn drop(&mut self) {
        if let Some(handle) = self.attach_handle.take() {
            detach(handle);
        }
        unsafe {
            if self.hwnd.0 as usize != 0 {
                let _ = DestroyWindow(self.hwnd);
            }
            if !self.hbitmap.is_invalid() {
                let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(self.hbitmap.0 as _));
            }
            if !self.hdc_mem.is_invalid() {
                let _ = DeleteDC(self.hdc_mem);
            }
            if !self.hdc_screen.is_invalid() {
                let _ = ReleaseDC(HWND::default(), self.hdc_screen);
            }
        }
    }
}

unsafe extern "system" fn visualizer_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn ensure_class_registered() -> Result<()> {
    if VISUALIZER_CLASS_REGISTERED.load(Ordering::SeqCst) {
        return Ok(());
    }

    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null())
            .context("Failed to get module handle for visualizer class")?;

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            hInstance: hinstance.into(),
            lpszClassName: VISUALIZER_CLASS_NAME,
            lpfnWndProc: Some(visualizer_wnd_proc),
            hCursor: HCURSOR::default(),
            hIcon: HICON::default(),
            ..Default::default()
        };

        let _ = RegisterClassExW(&wnd_class);
    }

    VISUALIZER_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    Ok(())
}

fn resample_spectrum(input: &[f32], target_len: usize) -> Vec<f32> {
    if input.is_empty() {
        return vec![0.0; target_len];
    }
    if input.len() == target_len {
        return input.to_vec();
    }

    let mut out = Vec::with_capacity(target_len);
    for i in 0..target_len {
        let t = i as f32 / target_len as f32;
        let src_idx = (t * input.len() as f32) as usize;
        let val = input.get(src_idx).copied().unwrap_or(0.0);
        out.push(val);
    }
    out
}

fn get_accent_color(accent: &str, index: usize, total: usize, alpha: u8) -> u32 {
    let t = if total > 1 {
        index as f32 / (total - 1) as f32
    } else {
        0.5
    };

    match accent {
        "cyan_purple" => {
            // Cyan (0x06, 0xB6, 0xD4) to Violet (0x8B, 0x5C, 0xF6)
            let r = (6.0 + t * (139.0 - 6.0)) as u8;
            let g = (182.0 + t * (92.0 - 182.0)) as u8;
            let b = (212.0 + t * (246.0 - 212.0)) as u8;
            ArgbSurface::premul_argb(alpha, r, g, b)
        }
        "neon_blue" => {
            // Royal Blue (0x25, 0x63, 0xEB) to Sky (0x38, 0xBD, 0xF8)
            let r = (37.0 + t * (56.0 - 37.0)) as u8;
            let g = (99.0 + t * (189.0 - 99.0)) as u8;
            let b = (235.0 + t * (248.0 - 235.0)) as u8;
            ArgbSurface::premul_argb(alpha, r, g, b)
        }
        "fire" => {
            // Amber (0xF5, 0x9E, 0x0B) to Crimson (0xEF, 0x44, 0x44)
            let r = (245.0 + t * (239.0 - 245.0)) as u8;
            let g = (158.0 + t * (68.0 - 158.0)) as u8;
            let b = (11.0 + t * (68.0 - 11.0)) as u8;
            ArgbSurface::premul_argb(alpha, r, g, b)
        }
        _ => {
            // Spotify neon green (0x1D, 0xB9, 0x54) to bright lime (0x1E, 0xD7, 0x60)
            let r = (29.0 + t * (30.0 - 29.0)) as u8;
            let g = (185.0 + t * (215.0 - 185.0)) as u8;
            let b = (84.0 + t * (96.0 - 84.0)) as u8;
            ArgbSurface::premul_argb(alpha, r, g, b)
        }
    }
}
