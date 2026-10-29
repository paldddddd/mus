use desktop_host::{detect_topology, enumerate_monitors, get_os_build};

#[test]
fn test_get_os_build() {
    let (major, _minor, build) = get_os_build();
    assert!(major >= 10, "Expected Windows 10 or newer, got {major}");
    assert!(build > 0, "Expected non-zero build number");
}

#[test]
fn test_detect_topology() {
    let topo = detect_topology();
    assert!(topo.is_ok(), "Topology detection failed: {:?}", topo.err());
    let info = topo.unwrap();
    println!("Detected topology: {:?}", info);
    assert!(info.progman_hwnd != 0, "Progman HWND must not be null");
    assert!(info.target_parent_hwnd != 0, "Target parent HWND must not be null");
}

#[test]
fn test_enumerate_monitors() {
    let monitors = enumerate_monitors();
    assert!(monitors.is_ok(), "Failed to enumerate monitors: {:?}", monitors.err());
    let list = monitors.unwrap();
    assert!(!list.is_empty(), "Must have at least one active monitor");
    for m in &list {
        assert!(m.width > 0, "Monitor width must be positive");
        assert!(m.height > 0, "Monitor height must be positive");
        assert!(m.dpi_x > 0, "Monitor DPI must be positive");
    }
}

#[test]
fn test_inspect_desktop_windows() {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, FindWindowW, GetClassNameW, GetWindowLongW, GetWindowRect,
        IsWindowVisible, SendMessageTimeoutW, GWL_EXSTYLE, GWL_STYLE,
        SMTO_NORMAL,
    };
    use windows::core::{w, PCWSTR};

    desktop_host::ensure_default_desktop();
    unsafe {
        let progman = FindWindowW(w!("Progman"), PCWSTR::null()).unwrap();
        println!("PROGMAN: 0x{:X}", progman.0 as usize);

        let mut child = HWND::default();
        loop {
            child = match FindWindowExW(progman, child, PCWSTR::null(), PCWSTR::null()) {
                Ok(c) if c.0 as usize != 0 => c,
                _ => break,
            };
            let mut class_buf = [0u16; 64];
            let len = GetClassNameW(child, &mut class_buf);
            let class = String::from_utf16_lossy(&class_buf[..len as usize]);
            let mut rect = windows::Win32::Foundation::RECT::default();
            let _ = GetWindowRect(child, &mut rect);
            let style = GetWindowLongW(child, GWL_STYLE) as u32;
            let ex_style = GetWindowLongW(child, GWL_EXSTYLE) as u32;
            let vis = IsWindowVisible(child).as_bool();
            println!("  Child 0x{:X}: class='{}', vis={}, rect=[{},{},{},{}], style=0x{:X}, ex_style=0x{:X}",
                child.0 as usize, class, vis, rect.left, rect.top, rect.right, rect.bottom, style, ex_style);
        }

        // Test sending 0x052C
        println!("Sending 0x052C to Progman...");
        let mut res = 0usize;
        let s_res = SendMessageTimeoutW(progman, 0x052C, WPARAM(0x0000000D), LPARAM(0), SMTO_NORMAL, 1000, Some(&mut res));
        println!("0x052C send result: {:?}, res={}", s_res, res);

        let s_res2 = SendMessageTimeoutW(progman, 0x052C, WPARAM(0x0000000D), LPARAM(1), SMTO_NORMAL, 1000, Some(&mut res));
        println!("0x052C (lparam=1) send result: {:?}, res={}", s_res2, res);

        // Check children again
        child = HWND::default();
        loop {
            child = match FindWindowExW(progman, child, PCWSTR::null(), PCWSTR::null()) {
                Ok(c) if c.0 as usize != 0 => c,
                _ => break,
            };
            let mut class_buf = [0u16; 64];
            let len = GetClassNameW(child, &mut class_buf);
            let class = String::from_utf16_lossy(&class_buf[..len as usize]);
            let vis = IsWindowVisible(child).as_bool();
            println!("  After 0x052C Child 0x{:X}: class='{}', vis={}", child.0 as usize, class, vis);
        }

        // Check top-level WorkerW windows
        extern "system" fn enum_w(hwnd: HWND, _: LPARAM) -> windows::Win32::Foundation::BOOL {
            let mut class_buf = [0u16; 64];
            let len = unsafe { GetClassNameW(hwnd, &mut class_buf) };
            let class = String::from_utf16_lossy(&class_buf[..len as usize]);
            if class == "WorkerW" {
                let vis = unsafe { IsWindowVisible(hwnd).as_bool() };
                let defview = unsafe { FindWindowExW(hwnd, HWND::default(), w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default() };
                let mut rect = windows::Win32::Foundation::RECT::default();
                let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
                println!("  Top WorkerW 0x{:X}: vis={}, defview=0x{:X}, rect=[{},{},{},{}]",
                    hwnd.0 as usize, vis, defview.0 as usize, rect.left, rect.top, rect.right, rect.bottom);
            }
            true.into()
        }
        let _ = windows::Win32::UI::WindowsAndMessaging::EnumWindows(Some(enum_w), LPARAM(0));
    }
}

#[test]
fn test_attach_visibility() {
    use desktop_host::{enumerate_monitors, ensure_default_desktop, DesktopHostWindow, RenderMode};
    use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW};
    use windows::core::{w, PCWSTR};

    ensure_default_desktop();
    let monitors = enumerate_monitors().unwrap();
    let mon = &monitors[0];
    println!("Testing attachment on primary monitor {}x{}...", mon.width, mon.height);

    let host = DesktopHostWindow::create(mon.clone(), RenderMode::GdiPattern).unwrap();
    println!("host attach_handle: {:?}", host.attach_handle);
    std::thread::sleep(std::time::Duration::from_millis(300));

    unsafe {
        let progman = FindWindowW(w!("Progman"), PCWSTR::null()).unwrap();
        let child_worker = FindWindowExW(progman, HWND::default(), w!("WorkerW"), PCWSTR::null()).unwrap_or_default();
        println!("Progman child WorkerW: 0x{:X}", child_worker.0 as usize);

        let hdc_screen = GetDC(HWND::default());
        let center_x = mon.width / 2;
        let center_y = mon.height / 2;
        let pt = windows::Win32::Foundation::POINT { x: center_x, y: center_y };
        let top_wnd = windows::Win32::UI::WindowsAndMessaging::WindowFromPoint(pt);
        let mut class_buf = [0u16; 64];
        let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(top_wnd, &mut class_buf);
        let class = String::from_utf16_lossy(&class_buf[..len as usize]);
        let mut pid = 0u32;
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(top_wnd, Some(&mut pid));
        let mut r = windows::Win32::Foundation::RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(top_wnd, &mut r);
        let vis = windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(top_wnd).as_bool();
        let iconic = windows::Win32::UI::WindowsAndMessaging::IsIconic(top_wnd).as_bool();
        println!("WindowFromPoint at ({}, {}): 0x{:X}, class='{}', pid={}, vis={}, iconic={}, rect=[{},{},{},{}]",
            center_x, center_y, top_wnd.0 as usize, class, pid, vis, iconic, r.left, r.top, r.right, r.bottom);
        ReleaseDC(HWND::default(), hdc_screen);
    }
}
