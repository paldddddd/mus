use anyhow::{bail, Result};
use common::{Topology, WindowTreeItem};
use serde::{Deserialize, Serialize};
use std::mem::size_of;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::StationsAndDesktops::{
    GetThreadDesktop, GetUserObjectInformationW, OpenDesktopW, SetThreadDesktop,
    DESKTOP_CONTROL_FLAGS, UOI_NAME,
};
use windows::Win32::System::SystemInformation::OSVERSIONINFOEXW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, FindWindowExW, FindWindowW, GetClassNameW,
    GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextW, IsWindow, IsWindowVisible,
    SendMessageTimeoutW, GWL_EXSTYLE, GWL_STYLE, GW_HWNDNEXT, SMTO_NORMAL,
    WS_EX_NOREDIRECTIONBITMAP,
};

/// Check whether the current thread is bound to the interactive `Default` desktop.
pub fn is_on_default_desktop() -> bool {
    unsafe {
        if let Ok(desk) = GetThreadDesktop(GetCurrentThreadId()) {
            let mut desk_buf = [0u16; 256];
            let mut desk_needed = 0;
            let _ = GetUserObjectInformationW(
                HANDLE(desk.0 as _),
                UOI_NAME,
                Some(desk_buf.as_mut_ptr() as *mut _),
                (desk_buf.len() * 2) as u32,
                Some(&mut desk_needed),
            );
            let desk_len = (desk_needed as usize / 2).saturating_sub(1);
            let desk_name = String::from_utf16_lossy(&desk_buf[..desk_len]);
            let clean_name = desk_name.trim_matches('\0').trim();
            return clean_name.eq_ignore_ascii_case("default");
        }
    }
    true
}

/// Ensure current thread is bound to WinSta0\Default desktop so it can interact with Explorer.
pub fn ensure_default_desktop() {
    unsafe {
        if let Ok(desk) = GetThreadDesktop(GetCurrentThreadId()) {
            let mut desk_buf = [0u16; 256];
            let mut desk_needed = 0;
            let _ = GetUserObjectInformationW(
                HANDLE(desk.0 as _),
                UOI_NAME,
                Some(desk_buf.as_mut_ptr() as *mut _),
                (desk_buf.len() * 2) as u32,
                Some(&mut desk_needed),
            );
            let desk_len = (desk_needed as usize / 2).saturating_sub(1);
            let desk_name = String::from_utf16_lossy(&desk_buf[..desk_len]);
            let clean_name = desk_name.trim_matches('\0').trim();
            if !clean_name.eq_ignore_ascii_case("default") {
                match OpenDesktopW(
                    w!("Default"),
                    DESKTOP_CONTROL_FLAGS(0),
                    false,
                    0x10000000,
                ) {
                    Ok(default_desk) => {
                        if let Err(e) = SetThreadDesktop(default_desk) {
                            tracing::warn!("Failed to switch thread desktop from '{clean_name}' to Default: {e}");
                        } else {
                            tracing::info!("Switched thread desktop from '{clean_name}' to Default");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to OpenDesktopW('Default'): {e}");
                    }
                }
            }
        }
    }
}

/// Spawn an executable explicitly attached to `winsta0\default` so its UI windows and WebView2
/// child processes are always visible on the user's real interactive desktop even if the parent
/// was started inside an isolated desktop/job sandbox.
pub fn spawn_on_default_desktop(exe: &std::path::Path, args: &[&str]) -> Result<()> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateProcessW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW,
    };

    let mut cmd = format!("\"{}\"", exe.display());
    for a in args {
        cmd.push(' ');
        if a.contains(' ') {
            cmd.push_str(&format!("\"{a}\""));
        } else {
            cmd.push_str(a);
        }
    }
    let mut cmd_wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
    let mut desktop_wide: Vec<u16> = "winsta0\\default"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let work_dir_wide: Option<Vec<u16>> = exe
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect());

    unsafe {
        let mut si = STARTUPINFOW::default();
        si.cb = size_of::<STARTUPINFOW>() as u32;
        si.lpDesktop = PWSTR(desktop_wide.as_mut_ptr());

        let mut pi = PROCESS_INFORMATION::default();
        let cur_dir = work_dir_wide
            .as_ref()
            .map(|v| PCWSTR(v.as_ptr()))
            .unwrap_or(PCWSTR::null());

        CreateProcessW(
            PCWSTR::null(),
            PWSTR(cmd_wide.as_mut_ptr()),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            cur_dir,
            &si,
            &mut pi,
        )?;

        if !pi.hThread.is_invalid() {
            let _ = CloseHandle(pi.hThread);
        }
        if !pi.hProcess.is_invalid() {
            let _ = CloseHandle(pi.hProcess);
        }
    }
    Ok(())
}

/// Details discovered about the desktop shell topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyInfo {
    pub topology: Topology,
    pub progman_hwnd: usize,
    pub shelldll_defview_hwnd: usize,
    pub target_parent_hwnd: usize,
    pub insert_after_hwnd: Option<usize>,
    pub os_build: u32,
    pub details: String,
}

/// Retrieve true Windows OS build via ntdll RtlGetVersion (for logging and diagnostics).
pub fn get_os_build() -> (u32, u32, u32) {
    unsafe {
        // SAFETY: Loading ntdll module handle (already loaded in every Windows process)
        // and retrieving RtlGetVersion.
        let ntdll = GetModuleHandleW(w!("ntdll.dll")).unwrap_or_default();
        if !ntdll.is_invalid() {
            let proc = GetProcAddress(ntdll, windows::core::s!("RtlGetVersion"));
            if let Some(func) = proc {
                type RtlGetVersionFn = unsafe extern "system" fn(*mut OSVERSIONINFOEXW) -> i32;
                let rtl_get_version: RtlGetVersionFn = std::mem::transmute(func);
                let mut os_info: OSVERSIONINFOEXW = std::mem::zeroed();
                os_info.dwOSVersionInfoSize = size_of::<OSVERSIONINFOEXW>() as u32;
                if rtl_get_version(&mut os_info) == 0 {
                    return (
                        os_info.dwMajorVersion,
                        os_info.dwMinorVersion,
                        os_info.dwBuildNumber,
                    );
                }
            }
        }
    }
    (10, 0, 0)
}

/// Detect the shell topology strictly by examining the structure and styles of desktop windows.
pub fn detect_topology() -> Result<TopologyInfo> {
    ensure_default_desktop();
    let (_, _, os_build) = get_os_build();

    // 1. Locate Progman
    // SAFETY: Calling FindWindowW with static class name "Progman".
    let progman = unsafe { FindWindowW(w!("Progman"), PCWSTR::null()) }
        .map_err(|e| anyhow::anyhow!("FindWindowW(Progman) error: {e}"))?;

    if progman.0 as usize == 0 || !unsafe { IsWindow(progman).as_bool() } {
        bail!("Could not find desktop Progman window");
    }

    // Check Progman extended styles
    let progman_ex_style = unsafe { GetWindowLongW(progman, GWL_EXSTYLE) } as u32;
    let has_no_redirection = (progman_ex_style & WS_EX_NOREDIRECTIONBITMAP.0) != 0;

    // Check if SHELLDLL_DefView and child WorkerW already exist in Progman before sending 0x052C
    let existing_defview = unsafe {
        FindWindowExW(progman, HWND::default(), w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default()
    };
    let existing_worker = unsafe {
        FindWindowExW(progman, HWND::default(), w!("WorkerW"), PCWSTR::null()).unwrap_or_default()
    };

    if existing_defview.0 as usize == 0 || existing_worker.0 as usize == 0 {
        // Send 0x052C to Progman once to trigger shell worker separation
        let mut send_res: usize = 0;
        unsafe {
            let _ = SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0x0000000D),
                LPARAM(0),
                SMTO_NORMAL,
                100,
                Some(&mut send_res),
            );
            let _ = SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0x0000000D),
                LPARAM(1),
                SMTO_NORMAL,
                100,
                Some(&mut send_res),
            );
        }
    }

    // 2. Check if SHELLDLL_DefView is a direct child of Progman
    let defview_in_progman = unsafe {
        FindWindowExW(progman, HWND::default(), w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default()
    };

    if defview_in_progman.0 as usize != 0 && unsafe { IsWindow(defview_in_progman).as_bool() } {
        // In Windows 11 24H2+, SHELLDLL_DefView is directly inside Progman.
        // Look for child WorkerW inside Progman if any exists
        let child_worker = unsafe {
            FindWindowExW(progman, HWND::default(), w!("WorkerW"), PCWSTR::null()).unwrap_or_default()
        };

        let details = format!(
            "Progman hwnd: 0x{:X}, ex_style: 0x{:X} (WS_EX_NOREDIRECTIONBITMAP: {}), SHELLDLL_DefView directly in Progman (hwnd: 0x{:X}), child WorkerW: 0x{:X}",
            progman.0 as usize,
            progman_ex_style,
            has_no_redirection,
            defview_in_progman.0 as usize,
            child_worker.0 as usize
        );

        return Ok(TopologyInfo {
            topology: Topology::Raised,
            progman_hwnd: progman.0 as usize,
            shelldll_defview_hwnd: defview_in_progman.0 as usize,
            target_parent_hwnd: progman.0 as usize,
            // In Raised topology, host window will be positioned behind SHELLDLL_DefView
            insert_after_hwnd: Some(defview_in_progman.0 as usize),
            os_build,
            details,
        });
    }

    // 3. Otherwise, check Classic topology:
    // Send 0x052C to Progman to ensure WorkerW is spawned by the shell.
    let mut send_res: usize = 0;
    unsafe {
        // SAFETY: Sending undocumented 0x052C message to Progman to split WorkerW hierarchy.
        let _ = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0x0000000D),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            Some(&mut send_res),
        );
    }

    // Search for WorkerW windows to find the one with SHELLDLL_DefView and the one behind it
    struct ClassicWorkerSearch {
        defview_hwnd: HWND,
        defview_workerw: HWND,
        wallpaper_workerw: HWND,
    }

    let mut search = ClassicWorkerSearch {
        defview_hwnd: HWND::default(),
        defview_workerw: HWND::default(),
        wallpaper_workerw: HWND::default(),
    };

    let search_ptr = &mut search as *mut ClassicWorkerSearch;

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut ClassicWorkerSearch);

        // Check if this window has class "WorkerW"
        let mut class_buf = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);

        if class_name == "WorkerW" {
            let defview = FindWindowExW(hwnd, HWND::default(), w!("SHELLDLL_DefView"), PCWSTR::null())
                .unwrap_or_default();
            if defview.0 as usize != 0 && IsWindow(defview).as_bool() {
                search.defview_workerw = hwnd;
                search.defview_hwnd = defview;

                // The next WorkerW in the z-order is typically the wallpaper host
                if let Ok(next) = GetWindow(hwnd, GW_HWNDNEXT) {
                    let mut next_class = [0u16; 64];
                    let next_len = GetClassNameW(next, &mut next_class);
                    let next_name = String::from_utf16_lossy(&next_class[..next_len as usize]);
                    if next_name == "WorkerW" {
                        search.wallpaper_workerw = next;
                    }
                }
            }
        }
        true.into()
    }

    unsafe {
        // SAFETY: EnumWindows calls our callback for each top-level window.
        let _ = EnumWindows(Some(enum_windows_proc), LPARAM(search_ptr as isize));
    }

    if search.wallpaper_workerw.0 as usize != 0 {
        let details = format!(
            "Progman: 0x{:X}, DefView WorkerW: 0x{:X}, DefView: 0x{:X}, Wallpaper WorkerW: 0x{:X}",
            progman.0 as usize,
            search.defview_workerw.0 as usize,
            search.defview_hwnd.0 as usize,
            search.wallpaper_workerw.0 as usize
        );

        return Ok(TopologyInfo {
            topology: Topology::Classic,
            progman_hwnd: progman.0 as usize,
            shelldll_defview_hwnd: search.defview_hwnd.0 as usize,
            target_parent_hwnd: search.wallpaper_workerw.0 as usize,
            insert_after_hwnd: None,
            os_build,
            details,
        });
    }

    // 4. Fallback if classic WorkerW wasn't found or topology is unknown
    let details = format!(
        "Unknown topology: Progman hwnd: 0x{:X}, ex_style: 0x{:X}, defview_in_progman: 0x{:X}",
        progman.0 as usize, progman_ex_style, defview_in_progman.0 as usize
    );

    Ok(TopologyInfo {
        topology: Topology::Unknown(details.clone()),
        progman_hwnd: progman.0 as usize,
        shelldll_defview_hwnd: defview_in_progman.0 as usize,
        target_parent_hwnd: progman.0 as usize,
        insert_after_hwnd: None,
        os_build,
        details,
    })
}

/// Enumerate all desktop shell-related windows for diagnostic logging.
pub fn collect_shell_windows() -> Vec<WindowTreeItem> {
    ensure_default_desktop();
    let mut items = Vec::new();
    let progman = unsafe { FindWindowW(w!("Progman"), PCWSTR::null()) }.unwrap_or_default();
    if progman.0 as usize != 0 {
        if let Some(item) = get_window_item(progman, HWND::default()) {
            items.push(item);
        }
        collect_child_windows(progman, &mut items);
    }

    // Also collect all WorkerW windows
    let items_ptr = &mut items as *mut Vec<WindowTreeItem>;
    unsafe extern "system" fn enum_workerw(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let items = &mut *(lparam.0 as *mut Vec<WindowTreeItem>);
        let mut class_buf = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
        if class_name == "WorkerW" {
            if let Some(item) = get_window_item(hwnd, HWND::default()) {
                items.push(item);
            }
            collect_child_windows(hwnd, items);
        }
        true.into()
    }
    unsafe {
        let _ = EnumWindows(Some(enum_workerw), LPARAM(items_ptr as isize));
    }

    items
}

struct ChildContext {
    parent: HWND,
    items: *mut Vec<WindowTreeItem>,
}

fn collect_child_windows(parent: HWND, items: &mut Vec<WindowTreeItem>) {
    let ctx = ChildContext {
        parent,
        items: items as *mut _,
    };

    unsafe extern "system" fn enum_child(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &*(lparam.0 as *const ChildContext);
        let items = &mut *ctx.items;
        if let Some(item) = get_window_item(hwnd, ctx.parent) {
            items.push(item);
        }
        true.into()
    }

    unsafe {
        let _ = EnumChildWindows(parent, Some(enum_child), LPARAM(&ctx as *const _ as isize));
    }
}

fn get_window_item(hwnd: HWND, parent: HWND) -> Option<WindowTreeItem> {
    unsafe {
        let mut class_buf = [0u16; 128];
        let c_len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..c_len as usize]);

        let mut text_buf = [0u16; 128];
        let t_len = GetWindowTextW(hwnd, &mut text_buf);
        let window_text = String::from_utf16_lossy(&text_buf[..t_len as usize]);

        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let is_visible = IsWindowVisible(hwnd).as_bool();

        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);

        Some(WindowTreeItem {
            hwnd: hwnd.0 as usize,
            parent_hwnd: parent.0 as usize,
            class_name,
            window_text,
            style,
            ex_style,
            is_visible,
            rect: [rect.left, rect.top, rect.right, rect.bottom],
        })
    }
}
