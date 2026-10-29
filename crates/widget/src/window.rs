use crate::gdi_plus::{GdiPlusGuard, GdiPlusRenderer};
use crate::painter::{WidgetHitTest, WidgetPainter, MIN_H, MIN_W};
use crate::surface::ArgbSurface;
use anyhow::{bail, Context, Result};
use audio_engine::TrackMetadata;
use common::WidgetConfig;
use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetWindowRect, LoadCursorW, RegisterClassExW, SetWindowPos, TrackPopupMenu,
    UpdateLayeredWindow, HICON, IDC_ARROW, MF_SEPARATOR, MF_STRING,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, TPM_RETURNCMD, TPM_RIGHTBUTTON, ULW_ALPHA,
    WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_RBUTTONUP, WM_SIZING, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_POPUP, WS_VISIBLE,
};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_MOVING: u32 = 0x0216;

pub const SNAP_LEFT: u8 = 1 << 0;
pub const SNAP_RIGHT: u8 = 1 << 1;
pub const SNAP_TOP: u8 = 1 << 2;
pub const SNAP_BOTTOM: u8 = 1 << 3;

pub type ActionCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;
pub type PositionCallback = Arc<dyn Fn(i32, i32, u32, u32) + Send + Sync + 'static>;

pub struct WidgetWindow {
    pub hwnd: HWND,
    _gdi_guard: GdiPlusGuard,
    hdc_screen: HDC,
    hdc_mem: HDC,
    hbitmap: HBITMAP,
    dib_bits: *mut u32,
    pub surface: ArgbSurface,
    pub painter: WidgetPainter,
    config: Arc<Mutex<WidgetConfig>>,
    on_action: Option<ActionCallback>,
    on_moved: Option<PositionCallback>,
    last_track: Option<TrackMetadata>,
    last_spectrum: Vec<f32>,
    target_size: Option<(f32, f32)>,
    target_pos: Option<(f32, f32)>,
    current_pos: (f32, f32),
    snap_neon_glow: f32,
    snapped_edges: u8,
    is_seeking: bool,
    is_vol_seeking: bool,
    pub is_dragging: bool,
    pub drag_start_cursor: (i32, i32),
    pub drag_start_pos: (i32, i32),
    scrub_ratio: f32,
    snap_fx: crate::snap_fx::SnapFx,
}

impl WidgetWindow {
    pub fn create(
        config: Arc<Mutex<WidgetConfig>>,
        on_action: Option<ActionCallback>,
        on_moved: Option<PositionCallback>,
    ) -> Result<Box<Self>> {
        let gdi_guard = GdiPlusGuard::init().context("Failed to initialize GDI+")?;

        let (mut x, mut y, w, h, pinned) = {
            let cfg = config.lock().unwrap();
            (cfg.x, cfg.y, cfg.width.max(MIN_W), cfg.height.max(MIN_H), cfg.pinned)
        };

        unsafe {
            // Clamp initial widget coordinates to visible monitor work area so it never spawns off-screen
            let hmon = windows::Win32::Graphics::Gdi::MonitorFromPoint(
                POINT { x, y },
                windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
            );
            let mut mi = windows::Win32::Graphics::Gdi::MONITORINFO {
                cbSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::MONITORINFO>() as u32,
                ..Default::default()
            };
            if windows::Win32::Graphics::Gdi::GetMonitorInfoW(hmon, &mut mi).as_bool() {
                let work = mi.rcWork;
                let max_x = (work.right - w as i32).max(work.left);
                let max_y = (work.bottom - h as i32).max(work.top);
                x = x.clamp(work.left, max_x);
                y = y.clamp(work.top, max_y);
                let mut cfg = config.lock().unwrap();
                cfg.x = x;
                cfg.y = y;
            }
        }

        unsafe {
            let hinstance = GetModuleHandleW(None).context("Failed to get module handle")?;
            let class_name = w!("wallmus_WidgetWindow");

            let wnd_class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                lpfnWndProc: Some(widget_wnd_proc),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hIcon: HICON::default(),
                ..Default::default()
            };

            let _ = RegisterClassExW(&wnd_class);

            let mut ex_style = WS_EX_LAYERED
                | WS_EX_TOOLWINDOW
                | windows::Win32::UI::WindowsAndMessaging::WS_EX_NOACTIVATE;
            if pinned {
                ex_style |= windows::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST;
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

            if !pinned {
                let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::HWND_NOTOPMOST,
                    0,
                    0,
                    0,
                    0,
                    windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                        | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                        | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
                );
            }

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
                last_track: None,
                last_spectrum: vec![0.0; 24],
                target_size: None,
                target_pos: None,
                current_pos: (x as f32, y as f32),
                snap_neon_glow: 0.0,
                snapped_edges: 0,
                is_seeking: false,
                is_vol_seeking: false,
                is_dragging: false,
                drag_start_cursor: (0, 0),
                drag_start_pos: (0, 0),
                scrub_ratio: 0.0,
                snap_fx: crate::snap_fx::SnapFx::new(),
            });

            // Set user data pointer in HWND for WndProc callback dispatch
            let ptr = &mut *instance as *mut Self as isize;
            windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                ptr,
            );

            // Immediately render initial frame so WS_EX_LAYERED window is never blank/black
            instance.render_frame(None, &[0.0; 24]);

            Ok(instance)
        }
    }

    pub fn is_valid(&self) -> bool {
        unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(self.hwnd).as_bool() }
    }

    pub fn animate_resize(&mut self, new_w: u32, new_h: u32) {
        let tw = new_w.clamp(MIN_W, 1200) as f32;
        let th = new_h.clamp(MIN_H, 700) as f32;
        self.target_size = Some((tw, th));
    }

    pub fn render_cached_frame(&mut self) {
        let track = self.last_track.clone();
        let spectrum = self.last_spectrum.clone();
        self.render_frame_inner(track.as_ref(), &spectrum);
    }

    pub fn render_frame(&mut self, track: Option<&TrackMetadata>, spectrum: &[f32]) {
        self.last_track = track.cloned();
        self.last_spectrum = spectrum.to_vec();

        // Step smooth animated size transition if active
        if let Some((tw, th)) = self.target_size {
            let cw = self.surface.width as f32;
            let ch = self.surface.height as f32;
            let dw = tw - cw;
            let dh = th - ch;
            if dw.abs() <= 1.5 && dh.abs() <= 1.5 {
                self.target_size = None;
                self.resize(tw.round() as u32, th.round() as u32);
            } else {
                let next_w = (cw + dw * 0.22).round() as u32;
                let next_h = (ch + dh * 0.22).round() as u32;
                self.resize(next_w, next_h);
            }
        }

        // Smooth magnetic snap docking lerp (zero teleportation)
        if let Some((tx, ty)) = self.target_pos {
            let (cx, cy) = self.current_pos;
            let dx = tx - cx;
            let dy = ty - cy;
            if dx.abs() <= 0.8 && dy.abs() <= 0.8 {
                self.target_pos = None;
                self.current_pos = (tx, ty);
                let final_x = tx.round() as i32;
                let final_y = ty.round() as i32;
                self.set_position(final_x, final_y);
                if let Some(ref cb) = self.on_moved {
                    cb(final_x, final_y, self.surface.width, self.surface.height);
                }
            } else {
                let next_x = cx + dx * 0.32;
                let next_y = cy + dy * 0.32;
                self.current_pos = (next_x, next_y);
                self.set_position(next_x.round() as i32, next_y.round() as i32);
            }
        }

        // Decay neon snap edge glow
        if self.snap_neon_glow > 0.0 {
            self.snap_neon_glow = (self.snap_neon_glow - 0.035).max(0.0);
            self.painter.snap_neon_glow = self.snap_neon_glow;
            if self.snap_neon_glow == 0.0 {
                self.snapped_edges = 0;
                self.painter.snapped_edges = 0;
            }
        }

        self.render_frame_inner(track, spectrum);
    }

    fn render_frame_inner(&mut self, track: Option<&TrackMetadata>, spectrum: &[f32]) {
        // Keep in-flight magnetic attraction particles animating (dt-based, any call rate)
        self.snap_fx.tick(self.hwnd);
        self.painter.is_seeking = self.is_seeking;
        self.painter.scrub_ratio = self.scrub_ratio;
        let (x, y) = {
            let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
            (cfg.x, cfg.y)
        };
        let w = self.surface.width;
        let h = self.surface.height;
        if w == 0 || h == 0 || self.dib_bits.is_null() {
            return;
        }

        // 1. Render background card, buttons, progress bar and spectrum into surface
        {
            let cfg_guard = self.config.lock().unwrap_or_else(|e| e.into_inner());
            self.painter.paint_surface(&mut self.surface, &cfg_guard, track, spectrum);
        }

        // 2. Blit surface pixels to DIB section (hdc_mem)
        unsafe {
            if self.surface.pixels.len() == (w * h) as usize {
                std::ptr::copy_nonoverlapping(
                    self.surface.pixels.as_ptr(),
                    self.dib_bits,
                    self.surface.pixels.len(),
                );
            }
        }

        // 3. Render crisp anti-aliased text and album art on top via GDI+
        unsafe {
            if let Ok(gdi) = GdiPlusRenderer::from_bits(self.dib_bits, w, h) {
                let cfg_guard = self.config.lock().unwrap_or_else(|e| e.into_inner());
                self.painter.paint_foreground(&gdi, &cfg_guard, track);
            }
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

    pub fn resize(&mut self, new_w: u32, new_h: u32) {
        let new_w = new_w.max(MIN_W);
        let new_h = new_h.max(MIN_H);

        let (x, y) = {
            let mut cfg = self.config.lock().unwrap();
            cfg.width = new_w;
            cfg.height = new_h;
            (cfg.x, cfg.y)
        };

        unsafe {
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: new_w as i32,
                    biHeight: -(new_h as i32), // Top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut dib_bits: *mut std::ffi::c_void = std::ptr::null_mut();
            if let Ok(new_hbitmap) = CreateDIBSection(
                self.hdc_mem,
                &bmi,
                DIB_RGB_COLORS,
                &mut dib_bits,
                None,
                0,
            ) {
                let old_hbitmap = self.hbitmap;
                SelectObject(self.hdc_mem, new_hbitmap);
                if old_hbitmap.0 as usize != 0 {
                    let _ = DeleteObject(old_hbitmap);
                }
                self.hbitmap = new_hbitmap;
                self.dib_bits = dib_bits as *mut u32;
                self.surface = ArgbSurface::new(new_w, new_h);

                let _ = SetWindowPos(
                    self.hwnd,
                    HWND::default(),
                    x,
                    y,
                    new_w as i32,
                    new_h as i32,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
                );
            }
        }
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE,
            };
            let insert_after = if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST };
            let _ = SetWindowPos(
                self.hwnd,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    pub fn set_position(&mut self, new_x: i32, new_y: i32) {
        self.current_pos = (new_x as f32, new_y as f32);
        let (w, h) = {
            let mut cfg = self.config.lock().unwrap();
            cfg.x = new_x;
            cfg.y = new_y;
            (cfg.width, cfg.height)
        };
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND::default(),
                new_x,
                new_y,
                w as i32,
                h as i32,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }

    pub fn cycle_size(&mut self) {
        let cur_w = self.surface.width;
        let cur_h = self.surface.height;
        let (next_w, next_h) = if cur_w < cur_h {
            if cur_w < 290 {
                (300, 420) // Medium Vertical
            } else if cur_w < 350 {
                (360, 500) // Large Vertical
            } else {
                (260, 360) // Compact Vertical
            }
        } else if cur_w < 350 {
            (380, 150) // Medium Horizontal
        } else if cur_w < 440 {
            (480, 190) // Large Horizontal
        } else if cur_w < 560 {
            (600, 240) // Extra Large Horizontal
        } else {
            (320, 130) // Compact Horizontal
        };

        self.animate_resize(next_w, next_h);
        let (x, y) = {
            let cfg = self.config.lock().unwrap();
            (cfg.x, cfg.y)
        };
        if let Some(ref cb) = self.on_moved {
            cb(x, y, next_w, next_h);
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
            WidgetHitTest::VolumeUp => {
                if let Some(ref cb) = self.on_action {
                    cb("volume_up");
                }
            }
            WidgetHitTest::VolumeDown => {
                if let Some(ref cb) = self.on_action {
                    cb("volume_down");
                }
            }
            WidgetHitTest::ProgressSeek(ratio) => {
                self.is_seeking = true;
                self.scrub_ratio = ratio;
                unsafe {
                    let _ = SetCapture(self.hwnd);
                }
                self.render_cached_frame();
            }
            WidgetHitTest::VolumeSeek(ratio) => {
                self.is_vol_seeking = true;
                self.painter.volume = ratio;
                self.painter.is_muted = false;
                unsafe {
                    let _ = SetCapture(self.hwnd);
                }
                if let Some(ref cb) = self.on_action {
                    cb(&format!("volume:{}", ratio));
                }
                self.render_cached_frame();
            }
            WidgetHitTest::VolumeMuteToggle => {
                self.painter.is_muted = !self.painter.is_muted;
                if let Some(ref cb) = self.on_action {
                    cb(&format!("volume_mute:{}", self.painter.is_muted));
                }
                self.render_cached_frame();
            }
            WidgetHitTest::ToggleLyricsExpand => {
                self.painter.lyrics_expanded = !self.painter.lyrics_expanded;
                self.render_cached_frame();
            }
            WidgetHitTest::OpenSettings => {
                if let Some(ref cb) = self.on_action {
                    cb("settings");
                }
            }
            WidgetHitTest::ResizeGrip => {
                self.cycle_size();
            }
            WidgetHitTest::Drag | WidgetHitTest::None => {
                self.is_dragging = true;
                unsafe {
                    let mut pt = POINT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                    self.drag_start_cursor = (pt.x, pt.y);
                    let mut rect = RECT::default();
                    let _ = GetWindowRect(self.hwnd, &mut rect);
                    self.drag_start_pos = (rect.left, rect.top);
                    let _ = SetCapture(self.hwnd);
                }
                self.target_pos = None;
                self.snap_neon_glow = 0.0;
                self.snapped_edges = 0;
                self.painter.snap_neon_glow = 0.0;
                self.painter.snapped_edges = 0;
            }
        }
    }

    pub fn handle_rbutton_up(&mut self, screen_x: i32, screen_y: i32) {
        unsafe {
            if let Ok(menu) = CreatePopupMenu() {
                let _ = AppendMenuW(menu, MF_STRING, 101, w!("Открыть настройки"));
                let _ = AppendMenuW(menu, MF_STRING, 102, w!("Пауза / Возобновить обои"));
                let _ = AppendMenuW(menu, MF_STRING, 103, w!("Следующие обои"));
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(menu, MF_STRING, 104, w!("Изменить размер виджета (S / M / L / XL)"));
                let _ = AppendMenuW(menu, MF_STRING, 105, w!("Сменить тему (Spotify / Aurora / Vinyl / Glass)"));
                let _ = AppendMenuW(menu, MF_STRING, 109, w!("Сменить эквалайзер (Линии / Волна / Зеркало / Точки)"));
                let (is_pinned, show_lyrics) = {
                    let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
                    (cfg.pinned, cfg.show_lyrics)
                };
                if show_lyrics {
                    let exp_text = if self.painter.lyrics_expanded {
                        w!("Обычный размер текста песни (вернуть обложку)")
                    } else {
                        w!("Увеличить текст песни на виджете [TXT]")
                    };
                    let _ = AppendMenuW(menu, MF_STRING, 111, exp_text);
                }
                let lyr_text = if show_lyrics {
                    w!("Отключить текст песни (Lyrics)")
                } else {
                    w!("Включить текст песни (Lyrics)")
                };
                let _ = AppendMenuW(menu, MF_STRING, 110, lyr_text);
                let pin_text = if is_pinned {
                    w!("Открепить от верха (Обычное окно)")
                } else {
                    w!("Закрепить поверх всех окон")
                };
                let _ = AppendMenuW(menu, MF_STRING, 108, pin_text);
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(menu, MF_STRING, 106, w!("Скрыть виджет"));
                let _ = AppendMenuW(menu, MF_STRING, 107, w!("Закрыть wallmus (Выход)"));

                let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(self.hwnd);
                let cmd = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    screen_x,
                    screen_y,
                    0,
                    self.hwnd,
                    None,
                );
                let _ = DestroyMenu(menu);

                if cmd.0 == 111 {
                    self.painter.lyrics_expanded = !self.painter.lyrics_expanded;
                    self.render_cached_frame();
                } else if let Some(ref cb) = self.on_action {
                    match cmd.0 {
                        101 => cb("settings"),
                        102 => cb("toggle_wallpaper_pause"),
                        103 => cb("next_wallpaper"),
                        104 => self.cycle_size(),
                        105 => cb("toggle_theme"),
                        109 => cb("toggle_visualizer_style"),
                        110 => cb("toggle_lyrics"),
                        108 => cb("toggle_pinned"),
                        106 => cb("close"),
                        107 => cb("quit"),
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn perform_edge_snap(&mut self) {
        let snap_enabled = {
            let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
            cfg.magnetic_snap
        };
        let shift_held = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x10) < 0 };

        let mut rect = RECT::default();
        unsafe {
            if GetWindowRect(self.hwnd, &mut rect).is_ok() {
                let cur_x = rect.left as f32;
                let cur_y = rect.top as f32;
                self.current_pos = (cur_x, cur_y);
                {
                    let mut cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
                    cfg.x = rect.left;
                    cfg.y = rect.top;
                }

                if snap_enabled && !shift_held {
                    let w = rect.right - rect.left;
                    let h = rect.bottom - rect.top;
                    let hmon = windows::Win32::Graphics::Gdi::MonitorFromWindow(
                        self.hwnd,
                        windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
                    );
                    let mut mi = windows::Win32::Graphics::Gdi::MONITORINFO {
                        cbSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    if windows::Win32::Graphics::Gdi::GetMonitorInfoW(hmon, &mut mi).as_bool() {
                        let work = mi.rcWork;
                        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd).max(96);
                        let snap_threshold = ((24.0 * dpi as f32) / 96.0).round() as i32;
                        let mut snap_x = rect.left;
                        let mut snap_y = rect.top;
                        let mut snapped = 0u8;

                        // Magnetic snap to 4 monitor edges (zero center snapping)
                        if (rect.left - work.left).abs() <= snap_threshold {
                            snap_x = work.left;
                            snapped |= SNAP_LEFT;
                        } else if (rect.right - work.right).abs() <= snap_threshold {
                            snap_x = work.right - w;
                            snapped |= SNAP_RIGHT;
                        }

                        if (rect.top - work.top).abs() <= snap_threshold {
                            snap_y = work.top;
                            snapped |= SNAP_TOP;
                        } else if (rect.bottom - work.bottom).abs() <= snap_threshold {
                            snap_y = work.bottom - h;
                            snapped |= SNAP_BOTTOM;
                        }

                        if snapped != 0 {
                            self.target_pos = Some((snap_x as f32, snap_y as f32));
                            self.snap_neon_glow = 1.0;
                            self.snapped_edges = snapped;
                            self.painter.snap_neon_glow = 1.0;
                            self.painter.snapped_edges = snapped;
                        } else {
                            self.snapped_edges = 0;
                            self.painter.snapped_edges = 0;
                        }
                    }
                }
            }
        }
        self.handle_moved();
    }

    pub fn handle_moved(&mut self) {
        unsafe {
            let mut rect = RECT::default();
            if GetWindowRect(self.hwnd, &mut rect).is_ok() {
                if rect.left <= -10000 || rect.top <= -10000 {
                    return;
                }
                let w = (rect.right - rect.left).clamp(MIN_W as i32, 1600) as u32;
                let h = (rect.bottom - rect.top).clamp(MIN_H as i32, 900) as u32;
                if w != self.surface.width || h != self.surface.height {
                    self.resize(w, h);
                }
                if self.target_pos.is_none() {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.x = rect.left;
                    cfg.y = rect.top;
                    cfg.width = w;
                    cfg.height = h;
                    if let Some(ref cb) = self.on_moved {
                        cb(rect.left, rect.top, w, h);
                    }
                }
            }
        }
    }
}

impl Drop for WidgetWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                self.hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                0,
            );
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
            windows::Win32::UI::WindowsAndMessaging::WM_CLOSE => {
                // Prevent accidental Alt+F4 or external close from destroying widget HWND
                return LRESULT(0);
            }
            windows::Win32::UI::WindowsAndMessaging::WM_NCLBUTTONDBLCLK => {
                // Prevent double-clicking HTCAPTION from maximizing widget to fullscreen
                return LRESULT(0);
            }
            windows::Win32::UI::WindowsAndMessaging::WM_MOUSEACTIVATE => {
                // MA_NOACTIVATE (3) so clicking widget controls never steals focus
                return LRESULT(3);
            }
            windows::Win32::UI::WindowsAndMessaging::WM_NCHITTEST => {
                if widget.is_seeking || widget.is_vol_seeking || widget.is_dragging {
                    return LRESULT(windows::Win32::UI::WindowsAndMessaging::HTCLIENT as isize);
                }
                let x_screen = (lparam.0 & 0xFFFF) as i16 as i32;
                let y_screen = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let mut pt = POINT { x: x_screen, y: y_screen };
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                let hit = widget.painter.hit_test(pt.x as f32, pt.y as f32);
                widget.painter.hover = hit;
                return match hit {
                    WidgetHitTest::PlayPause
                    | WidgetHitTest::Previous
                    | WidgetHitTest::Next
                    | WidgetHitTest::VolumeUp
                    | WidgetHitTest::VolumeDown
                    | WidgetHitTest::ProgressSeek(_)
                    | WidgetHitTest::VolumeSeek(_)
                    | WidgetHitTest::VolumeMuteToggle
                    | WidgetHitTest::ToggleLyricsExpand
                    | WidgetHitTest::OpenSettings
                    | WidgetHitTest::Drag
                    | WidgetHitTest::None => {
                        LRESULT(windows::Win32::UI::WindowsAndMessaging::HTCLIENT as isize)
                    }
                    WidgetHitTest::ResizeGrip => {
                        LRESULT(windows::Win32::UI::WindowsAndMessaging::HTBOTTOMRIGHT as isize)
                    }
                };
            }
            WM_MOUSEMOVE => {
                let x = (lparam.0 & 0xFFFF) as i16 as f32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if widget.is_dragging {
                    let mut pt = POINT::default();
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                    }
                    let dx = pt.x - widget.drag_start_cursor.0;
                    let dy = pt.y - widget.drag_start_cursor.1;
                    let new_x = widget.drag_start_pos.0 + dx;
                    let new_y = widget.drag_start_pos.1 + dy;
                    widget.set_position(new_x, new_y);

                    // Detect edge proximity during drag to stream snap attraction particles
                    let snap_enabled = {
                        let cfg = widget.config.lock().unwrap_or_else(|e| e.into_inner());
                        cfg.magnetic_snap
                    };
                    let shift_held = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x10) < 0 };
                    if snap_enabled && !shift_held {
                        let w = widget.surface.width as i32;
                        let h = widget.surface.height as i32;
                        let hmon = windows::Win32::Graphics::Gdi::MonitorFromWindow(
                            widget.hwnd,
                            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
                        );
                        let mut mi = windows::Win32::Graphics::Gdi::MONITORINFO {
                            cbSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::MONITORINFO>() as u32,
                            ..Default::default()
                        };
                        if windows::Win32::Graphics::Gdi::GetMonitorInfoW(hmon, &mut mi).as_bool() {
                            let work = mi.rcWork;
                            let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(widget.hwnd).max(96);
                            let proximity_threshold = ((68.0 * dpi as f32) / 96.0).round() as i32;
                            let mut approaching = 0u8;

                            if (new_x - work.left).abs() <= proximity_threshold {
                                approaching |= SNAP_LEFT;
                            } else if (new_x + w - work.right).abs() <= proximity_threshold {
                                approaching |= SNAP_RIGHT;
                            }

                            if (new_y - work.top).abs() <= proximity_threshold {
                                approaching |= SNAP_TOP;
                            } else if (new_y + h - work.bottom).abs() <= proximity_threshold {
                                approaching |= SNAP_BOTTOM;
                            }

                            widget.painter.approaching_edges = approaching;

                            // Feed the out-of-window particle stream (gap between widget and screen edge)
                            let widget_rect = RECT { left: new_x, top: new_y, right: new_x + w, bottom: new_y + h };
                            let corner = widget.painter.layout.card_radius;
                            let accent = widget.painter.dynamic_accent;
                            widget.snap_fx.set_target(
                                widget_rect,
                                work,
                                approaching,
                                proximity_threshold as f32,
                                corner,
                                accent,
                            );
                        }
                    } else {
                        widget.painter.approaching_edges = 0;
                        widget.snap_fx.clear_target();
                    }

                    widget.render_cached_frame();
                    return LRESULT(0);
                }
                if widget.is_seeking {
                    let (px, _py, pw, _ph) = widget.painter.layout.progress;
                    if pw > 0.0 {
                        widget.scrub_ratio = ((x - px) / pw).clamp(0.0, 1.0);
                        widget.render_cached_frame();
                    }
                    return LRESULT(0);
                }
                if widget.is_vol_seeking {
                    let (vx, _vy, vw, _vh) = widget.painter.layout.vol_bar;
                    if vw > 0.0 {
                        let ratio = ((x - vx) / vw).clamp(0.0, 1.0);
                        widget.painter.volume = ratio;
                        widget.painter.is_muted = false;
                        if let Some(ref cb) = widget.on_action {
                            cb(&format!("volume:{}", ratio));
                        }
                        widget.render_cached_frame();
                    }
                    return LRESULT(0);
                }
                widget.painter.hover = widget.painter.hit_test(x, y);
                return LRESULT(0);
            }
            WM_MOUSELEAVE => {
                if !widget.is_seeking && !widget.is_vol_seeking {
                    widget.painter.hover = WidgetHitTest::None;
                }
                return LRESULT(0);
            }
            WM_LBUTTONDOWN => {
                let x = (lparam.0 & 0xFFFF) as i16 as f32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                widget.handle_lbutton_down(x, y);
                return LRESULT(0);
            }
            WM_LBUTTONUP => {
                if widget.is_dragging {
                    widget.is_dragging = false;
                    widget.painter.approaching_edges = 0;
                    widget.snap_fx.clear_target();
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    widget.perform_edge_snap();
                    widget.render_cached_frame();
                    return LRESULT(0);
                }
                if widget.is_seeking {
                    widget.is_seeking = false;
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    let ratio = widget.scrub_ratio;
                    if let Some(ref track) = widget.last_track {
                        if let Some(dur) = track.duration_ms {
                            if dur > 0 {
                                let target_ms = (ratio * dur as f32).round() as u64;
                                if let Some(ref cb) = widget.on_action {
                                    cb(&format!("seek:{}", target_ms));
                                }
                            }
                        }
                    }
                    widget.render_cached_frame();
                    return LRESULT(0);
                }
                if widget.is_vol_seeking {
                    widget.is_vol_seeking = false;
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    return LRESULT(0);
                }
                return LRESULT(0);
            }
            WM_ENTERSIZEMOVE => {
                widget.target_pos = None;
                widget.snap_neon_glow = 0.0;
                widget.snapped_edges = 0;
                widget.painter.snap_neon_glow = 0.0;
                widget.painter.snapped_edges = 0;
                return LRESULT(0);
            }
            WM_MOVING => {
                // Completely free cursor movement with zero sticky traps or hard-clamped coordinates
                return LRESULT(0);
            }
            WM_SIZING => {
                let rect = &mut *(lparam.0 as *mut RECT);
                let w = (rect.right - rect.left).clamp(MIN_W as i32, 1200);
                let h = (rect.bottom - rect.top).clamp(MIN_H as i32, 700);
                rect.right = rect.left + w;
                rect.bottom = rect.top + h;
                widget.painter.is_resizing = true;
                widget.target_size = None;
                if w as u32 != widget.surface.width || h as u32 != widget.surface.height {
                    widget.resize(w as u32, h as u32);
                }
                widget.render_cached_frame();
                return LRESULT(1);
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16;
                if delta > 0 {
                    if let Some(ref cb) = widget.on_action {
                        cb("volume_up");
                    }
                } else if delta < 0 {
                    if let Some(ref cb) = widget.on_action {
                        cb("volume_down");
                    }
                }
                return LRESULT(0);
            }
            WM_RBUTTONUP => {
                let mut pt = POINT::default();
                let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                widget.handle_rbutton_up(pt.x, pt.y);
                return LRESULT(0);
            }
            0x0215 /* WM_CAPTURECHANGED */ => {
                if widget.is_dragging {
                    widget.is_dragging = false;
                    widget.perform_edge_snap();
                    widget.render_cached_frame();
                }
                return LRESULT(0);
            }
            WM_EXITSIZEMOVE => {
                let was_resizing = widget.painter.is_resizing;
                widget.painter.is_resizing = false;

                if !was_resizing {
                    widget.perform_edge_snap();
                } else {
                    widget.handle_moved();
                    widget.render_cached_frame();
                }
                return LRESULT(0);
            }
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}
