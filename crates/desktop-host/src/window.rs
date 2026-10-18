use crate::attach::{attach, detach, revalidate, AttachHandle};
use anyhow::{bail, Context, Result};
use common::MonitorInfo;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect, LineTo, MoveToEx,
    Rectangle, SelectObject, SetBkMode, SetTextColor, TextOutW, HDC, HGDIOBJ, PAINTSTRUCT,
    PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, PostQuitMessage,
    RegisterClassExW, RegisterWindowMessageW, SetWindowLongPtrW, CS_HREDRAW, CS_VREDRAW,
    GWLP_USERDATA, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_PAINT,
    WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);
const CLASS_NAME: PCWSTR = w!("wallmus_DesktopHostWindow");

/// Rendering mode for the host window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Step 4(a): GDI color fill, rectangles, grid pattern, and text diagnostics.
    GdiPattern,
    /// Step 4(b): WIC static image.
    StaticImage,
    /// Step 4(c): Video child window.
    Video,
}

pub type PaintCallback = Box<dyn Fn(HDC, i32, i32) + Send + Sync>;

/// Host window created for a single monitor.
pub struct DesktopHostWindow {
    pub hwnd: HWND,
    pub monitor: MonitorInfo,
    pub attach_handle: Option<AttachHandle>,
    pub render_mode: RenderMode,
    taskbar_created_msg: u32,
    paint_callback: Option<PaintCallback>,
}

impl DesktopHostWindow {
    /// Set a custom painter (e.g. WIC StaticImageRenderer).
    pub fn set_paint_callback<F>(&mut self, callback: F)
    where
        F: Fn(HDC, i32, i32) + Send + Sync + 'static,
    {
        self.paint_callback = Some(Box::new(callback));
    }

    /// Create a new desktop host window for a specific monitor.
    pub fn create(monitor: MonitorInfo, render_mode: RenderMode) -> Result<Box<Self>> {
        crate::topology::ensure_default_desktop();
        ensure_class_registered()?;

        let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()) }
            .context("Failed to get module handle")?;

        let taskbar_created_msg = unsafe {
            // SAFETY: RegisterWindowMessageW is safe to call with null-terminated string
            RegisterWindowMessageW(w!("TaskbarCreated"))
        };

        // Create window borderless and no-activate
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!("wallmus Host Window"),
                WS_POPUP,
                monitor.x,
                monitor.y,
                monitor.width,
                monitor.height,
                HWND::default(),
                None,
                hinstance,
                None,
            )
        }
        .context("CreateWindowExW failed")?;

        if hwnd.0 as usize == 0 {
            bail!("CreateWindowExW returned null HWND for monitor: {}", monitor.name);
        }

        let mut host = Box::new(Self {
            hwnd,
            monitor: monitor.clone(),
            attach_handle: None,
            render_mode,
            taskbar_created_msg,
            paint_callback: None,
        });

        // Store pointer in GWLP_USERDATA
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *host as *mut Self as isize);
        }

        // Attach to desktop behind icons
        let handle = attach(hwnd, &monitor).context("Failed to attach host window to desktop")?;
        host.attach_handle = Some(handle);

        tracing::info!(
            "Host window created and attached: HWND 0x{:X} for monitor {}",
            hwnd.0 as usize,
            monitor.name
        );

        Ok(host)
    }

    /// Revalidate attachment when desktop shell or monitors reconfigure.
    pub fn revalidate(&mut self) -> Result<()> {
        if let Some(ref mut handle) = self.attach_handle {
            revalidate(handle)?;
        }
        Ok(())
    }
}

impl Drop for DesktopHostWindow {
    fn drop(&mut self) {
        if let Some(handle) = self.attach_handle.take() {
            detach(handle);
        }
        if self.hwnd.0 as usize != 0 {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

fn ensure_class_registered() -> Result<()> {
    if CLASS_REGISTERED.load(Ordering::SeqCst) {
        return Ok(());
    }

    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null()).context("GetModuleHandleW failed")?;
        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(host_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance.into(),
            hIcon: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
            hCursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR::default(),
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: CLASS_NAME,
            hIconSm: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
        };

        let atom = RegisterClassExW(&wnd_class);
        if atom == 0 {
            bail!("RegisterClassExW failed");
        }
    }

    CLASS_REGISTERED.store(true, Ordering::SeqCst);
    Ok(())
}

unsafe extern "system" fn host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DesktopHostWindow;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let host = &mut *ptr;

    if msg == host.taskbar_created_msg {
        tracing::info!("Received TaskbarCreated message. Revalidating desktop attachment...");
        let _ = host.revalidate();
        return LRESULT(0);
    }

    match msg {
        WM_ERASEBKGND => LRESULT(1), // Suppress flicker
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_host_window(host, hdc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            tracing::info!("Received WM_DISPLAYCHANGE. Revalidating attachment...");
            let _ = host.revalidate();
            LRESULT(0)
        }
        WM_DPICHANGED => {
            tracing::info!("Received WM_DPICHANGED. Revalidating attachment...");
            let _ = host.revalidate();
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Paint onto the host window HDC.
fn paint_host_window(host: &DesktopHostWindow, hdc: HDC) {
    if let Some(ref cb) = host.paint_callback {
        cb(hdc, host.monitor.width, host.monitor.height);
        return;
    }

    unsafe {
        let width = host.monitor.width;
        let height = host.monitor.height;
        let rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };

        // Dark Spotify background: RGB(18, 18, 18) = 0x00121212
        let bg_brush = CreateSolidBrush(COLORREF(0x00121212));
        FillRect(hdc, &rect, bg_brush);
        let _ = DeleteObject(HGDIOBJ(bg_brush.0 as _));

        // Draw subtle grid pattern
        let grid_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00282828));
        let old_pen = SelectObject(hdc, HGDIOBJ(grid_pen.0 as _));
        let step = 80;
        let mut x = 0;
        while x < width {
            let _ = MoveToEx(hdc, x, 0, None);
            let _ = LineTo(hdc, x, height);
            x += step;
        }
        let mut y = 0;
        while y < height {
            let _ = MoveToEx(hdc, 0, y, None);
            let _ = LineTo(hdc, width, y);
            y += step;
        }

        // Draw vibrant Spotify Green accent card: RGB(29, 185, 84) = 0x0054B91D in COLORREF (0x00BBGGRR)
        let card_brush = CreateSolidBrush(COLORREF(0x00242424));
        let border_pen = CreatePen(PS_SOLID, 2, COLORREF(0x0054B91D));
        SelectObject(hdc, HGDIOBJ(card_brush.0 as _));
        SelectObject(hdc, HGDIOBJ(border_pen.0 as _));

        let card_w = 560;
        let card_h = 240;
        let card_x = (width - card_w) / 2;
        let card_y = (height - card_h) / 2;
        let _ = Rectangle(hdc, card_x, card_y, card_x + card_w, card_y + card_h);

        // Draw text diagnostics
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x0054B91D)); // Spotify green text

        let title: Vec<u16> = "wallmus Desktop Live Wallpaper Engine (Stage 0: Active)\0"
            .encode_utf16()
            .collect();
        let _ = TextOutW(hdc, card_x + 24, card_y + 24, &title[..title.len() - 1]);

        SetTextColor(hdc, COLORREF(0x00FFFFFF)); // White text
        let line1_str = format!(
            "Monitor: {} ({}x{} @ {} DPI, {:.1}x scale)\0",
            host.monitor.name,
            host.monitor.width,
            host.monitor.height,
            host.monitor.dpi_x,
            host.monitor.scale_factor
        );
        let line1: Vec<u16> = line1_str.encode_utf16().collect();
        let _ = TextOutW(hdc, card_x + 24, card_y + 64, &line1[..line1.len() - 1]);

        let line2_str = format!(
            "Topology: {:?} | HWND: 0x{:X}\0",
            host.attach_handle
                .as_ref()
                .map(|h| &h.topology)
                .unwrap_or(&common::Topology::Classic),
            host.hwnd.0 as usize
        );
        let line2: Vec<u16> = line2_str.encode_utf16().collect();
        let _ = TextOutW(hdc, card_x + 24, card_y + 96, &line2[..line2.len() - 1]);

        SetTextColor(hdc, COLORREF(0x00AAAAAA)); // Gray status
        let line3: Vec<u16> = "Status: Window safely residing BEHIND desktop icons. Icons are fully clickable.\0"
            .encode_utf16()
            .collect();
        let _ = TextOutW(hdc, card_x + 24, card_y + 136, &line3[..line3.len() - 1]);

        let line4: Vec<u16> = "No system wallpapers modified (SPI_SETDESKWALLPAPER untouched).\0"
            .encode_utf16()
            .collect();
        let _ = TextOutW(hdc, card_x + 24, card_y + 168, &line4[..line4.len() - 1]);

        // Cleanup GDI objects
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(grid_pen.0 as _));
        let _ = DeleteObject(HGDIOBJ(card_brush.0 as _));
        let _ = DeleteObject(HGDIOBJ(border_pen.0 as _));
    }
}
