use crate::gdi_plus::{GdiPlusGuard, GdiPlusRenderer};
use crate::painter::{WidgetHitTest, WidgetPainter};
use crate::surface::ArgbSurface;
use anyhow::{bail, Context, Result};
use audio_engine::TrackMetadata;
use common::WidgetConfig;
use std::sync::{Arc, Mutex};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect, RegisterClassExW,
    SendMessageW, UpdateLayeredWindow, HCURSOR, HICON, HTCAPTION, ULW_ALPHA, WM_EXITSIZEMOVE,
    WM_LBUTTONDOWN, WM_NCLBUTTONDOWN, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

pub type ActionCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;
pub type PositionCallback = Arc<dyn Fn(i32, i32) + Send + Sync + 'static>;

pub struct WidgetWindow {
    pub hwnd: HWND,
    _gdi_guard: GdiPlusGuard,
    hdc_screen: HDC,
    hdc_mem: HDC,
    hbitmap: HBITMAP,
    dib_bits: *mut u32,
    surface: ArgbSurface,
    painter: WidgetPainter,
    config: Arc<Mutex<WidgetConfig>>,
    on_action: Option<ActionCallback>,
    on_moved: Option<PositionCallback>,
}

impl WidgetWindow {
    pub fn create(
        config: Arc<Mutex<WidgetConfig>>,
        on_action: Option<ActionCallback>,
        on_moved: Option<PositionCallback>,
    ) -> Result<Self> {
        let gdi_guard = GdiPlusGuard::init().context("Failed to initialize GDI+")?;

        let (x, y, w, h, pinned) = {
            let cfg = config.lock().unwrap();
            (cfg.x, cfg.y, cfg.width, cfg.height, cfg.pinned)
        };

        unsafe {
            let hinstance = GetModuleHandleW(None).context("Failed to get module handle")?;
            let class_name = w!("wallmus_WidgetWindow");

            let wnd_class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                lpfnWndProc: Some(widget_wnd_proc),
                hCursor: HCURSOR::default(),
                hIcon: HICON::default(),
                ..Default::default()
            };

            let _ = RegisterClassExW(&wnd_class);

            let mut ex_style = WS_EX_LAYERED | WS_EX_TOOLWINDOW;
            if pinned {
                ex_style |= WS_EX_TRANSPARENT;
            }

            let hwnd = CreateWindowExW(
                ex_style,
                class_name,
                w!("wallmus Widget"),
                WS_POPUP | WS_VISIBLE,
                x,
                y,
                w as i32,
                h as i32,
                HWND::default(),
                None,
                hinstance,
                None,
            )?;

            if hwnd.0 as usize == 0 {
                bail!("Failed to create widget window");
            }

            // Create memory DC and DIB section for 32-bit ARGB UpdateLayeredWindow
            let hdc_screen = GetDC(HWND::default());
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32), // Top-down DIB
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

            let surface = ArgbSurface::new(w, h);
            let painter = WidgetPainter::new();

            let mut instance = Box::new(Self {
                hwnd,
                _gdi_guard: gdi_guard,
                hdc_screen,
                hdc_mem,
                hbitmap,
                dib_bits: dib_bits as *mut u32,
                surface,
                painter,
                config,
                on_action,
                on_moved,
            });

            // Set user data pointer in HWND for WndProc callback dispatch
            let ptr = &mut *instance as *mut Self as isize;
            windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                ptr,
            );

            Ok(*instance)
        }
    }

    pub fn render_frame(&mut self, track: Option<&TrackMetadata>, spectrum: &[f32]) {
        let (x, y, w, h) = {
            let cfg = self.config.lock().unwrap();
            (cfg.x, cfg.y, cfg.width, cfg.height)
        };

        // 1. Render background card, buttons, progress bar and spectrum into surface
        {
            let cfg_guard = self.config.lock().unwrap();
            self.painter.paint_surface(&mut self.surface, &cfg_guard, track, spectrum);
        }

        // 2. Blit surface pixels to DIB section (hdc_mem)
        unsafe {
            if !self.dib_bits.is_null() && self.surface.pixels.len() == (w * h) as usize {
                std::ptr::copy_nonoverlapping(
                    self.surface.pixels.as_ptr(),
                    self.dib_bits,
                    self.surface.pixels.len(),
                );
            }
        }

        // 3. Render crisp anti-aliased text and album art on top via GDI+
        if let Ok(gdi) = GdiPlusRenderer::from_hdc(self.hdc_mem) {
            let cfg_guard = self.config.lock().unwrap();
            self.painter.paint_foreground(&gdi, &cfg_guard, track);
        }

        // 4. Update layered window with per-pixel alpha blending
        unsafe {
            let pt_dst = POINT { x, y };
            let size_dst = SIZE {
                cx: w as i32,
                cy: h as i32,
            };
            let pt_src = POINT { x: 0, y: 0 };

            let blend = BLENDFUNCTION {
                BlendOp: 0, // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: 1, // AC_SRC_ALPHA (per-pixel alpha)
            };

            let _ = UpdateLayeredWindow(
                self.hwnd,
                self.hdc_screen,
                Some(&pt_dst),
                Some(&size_dst),
                self.hdc_mem,
                Some(&pt_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    pub fn handle_lbutton_down(&mut self, click_x: f32, click_y: f32) {
        let hit = self.painter.hit_test(click_x, click_y);
        match hit {
            WidgetHitTest::PlayPause => {
                if let Some(ref cb) = self.on_action {
                    cb("toggle");
                }
            }
            WidgetHitTest::Previous => {
                if let Some(ref cb) = self.on_action {
                    cb("previous");
                }
            }
            WidgetHitTest::Next => {
                if let Some(ref cb) = self.on_action {
                    cb("next");
                }
            }
            WidgetHitTest::Drag => {
                unsafe {
                    let _ = ReleaseCapture();
                    let _ = SendMessageW(
                        self.hwnd,
                        WM_NCLBUTTONDOWN,
                        WPARAM(HTCAPTION as usize),
                        LPARAM(0),
                    );
                }
            }
            WidgetHitTest::None => {}
        }
    }

    pub fn handle_moved(&mut self) {
        unsafe {
            let mut rect = RECT::default();
            if GetWindowRect(self.hwnd, &mut rect).is_ok() {
                let mut cfg = self.config.lock().unwrap();
                cfg.x = rect.left;
                cfg.y = rect.top;
                if let Some(ref cb) = self.on_moved {
                    cb(rect.left, rect.top);
                }
            }
        }
    }
}

impl Drop for WidgetWindow {
    fn drop(&mut self) {
        unsafe {
            if self.hwnd.0 as usize != 0 {
                let _ = DestroyWindow(self.hwnd);
            }
            if self.hdc_mem.0 as usize != 0 {
                let _ = DeleteDC(self.hdc_mem);
            }
            if self.hbitmap.0 as usize != 0 {
                let _ = DeleteObject(self.hbitmap);
            }
            if self.hdc_screen.0 as usize != 0 {
                let _ = ReleaseDC(HWND::default(), self.hdc_screen);
            }
        }
    }
}

unsafe extern "system" fn widget_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
    );

    if ptr != 0 {
        let widget = &mut *(ptr as *mut WidgetWindow);

        match msg {
            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xFFFF) as i16 as f32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                widget.handle_lbutton_down(x, y);
                return LRESULT(0);
            }
            WM_EXITSIZEMOVE => {
                widget.handle_moved();
                return LRESULT(0);
            }
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}
