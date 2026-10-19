use anyhow::{bail, Context, Result};
use std::env;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassExW,
    TranslateMessage, HCURSOR, HICON, MSG, WNDCLASSEXW, WS_CHILD, WS_VISIBLE,
};
use raw_window_handle::{
    HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    Win32WindowHandle, WindowsDisplayHandle,
};
use std::num::NonZeroIsize;

struct HwndHost(HWND);

impl HasWindowHandle for HwndHost {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, HandleError> {
        let non_zero = NonZeroIsize::new(self.0.0 as isize).ok_or(HandleError::Unavailable)?;
        let handle = Win32WindowHandle::new(non_zero);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(RawWindowHandle::Win32(handle))) }
    }
}

impl HasDisplayHandle for HwndHost {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, HandleError> {
        unsafe { Ok(raw_window_handle::DisplayHandle::borrow_raw(RawDisplayHandle::Windows(WindowsDisplayHandle::new()))) }
    }
}

unsafe extern "system" fn host_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let mut parent_hwnd_val: usize = 0;
    let mut url = String::new();
    let mut width: i32 = 1920;
    let mut height: i32 = 1080;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--parent" => {
                if let Some(val) = args.get(i + 1) {
                    parent_hwnd_val = val.parse().unwrap_or(0);
                    i += 1;
                }
            }
            "--url" => {
                if let Some(val) = args.get(i + 1) {
                    url = val.clone();
                    i += 1;
                }
            }
            "--width" => {
                if let Some(val) = args.get(i + 1) {
                    width = val.parse().unwrap_or(1920);
                    i += 1;
                }
            }
            "--height" => {
                if let Some(val) = args.get(i + 1) {
                    height = val.parse().unwrap_or(1080);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if parent_hwnd_val == 0 || url.is_empty() {
        bail!("Usage: wallmus-htmlhost --parent <HWND> --url <URL> [--width <W>] [--height <H>]");
    }

    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null())
            .context("Failed to get module handle")?;

        let class_name = w!("wallmus_HtmlHostWindow");
        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(host_wnd_proc),
            hCursor: HCURSOR::default(),
            hIcon: HICON::default(),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wnd_class);

        let parent_hwnd = HWND(parent_hwnd_val as *mut _);
        let child_hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
            class_name,
            w!("wallmus HTML Child"),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            width,
            height,
            parent_hwnd,
            None,
            hinstance,
            None,
        )?;

        let host = HwndHost(child_hwnd);
        let _webview = wry::WebViewBuilder::new()
            .with_url(&url)
            .build_as_child(&host)?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}
