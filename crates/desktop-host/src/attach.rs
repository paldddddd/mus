use crate::monitors::enumerate_monitors;
use crate::topology::{detect_topology, TopologyInfo};
use anyhow::{bail, Context, Result};
use common::{MonitorInfo, Topology};
use windows::core::w;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetParent, GetWindowLongW, IsWindow, SetParent, SetWindowLongW, SetWindowPos,
    GWL_EXSTYLE, GWL_STYLE, HWND_BOTTOM, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

/// Handle representing an attached desktop host window.
/// Detaches and restores original window hierarchy upon drop or explicit `detach`.
#[derive(Debug)]
pub struct AttachHandle {
    pub host_hwnd: HWND,
    pub original_parent: HWND,
    pub original_style: u32,
    pub original_ex_style: u32,
    pub target_parent: HWND,
    pub topology: Topology,
    pub monitor_id: String,
    pub active: bool,
}

impl Drop for AttachHandle {
    fn drop(&mut self) {
        if self.active {
            detach_internal(self);
        }
    }
}

/// Attach a host window to the desktop wallpaper layer behind desktop icons.
pub fn attach(hwnd: HWND, monitor: &MonitorInfo) -> Result<AttachHandle> {
    if !unsafe { IsWindow(hwnd).as_bool() } {
        bail!("Invalid window handle passed to attach: 0x{:X}", hwnd.0 as usize);
    }

    let topo_info = detect_topology().context("Failed to detect shell topology")?;
    tracing::info!(
        "Attaching host window 0x{:X} on monitor {} ({:?})",
        hwnd.0 as usize,
        monitor.name,
        topo_info.topology
    );

    let original_parent = unsafe { GetParent(hwnd).unwrap_or_default() };
    let original_style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let original_ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;

    let target_parent = HWND(topo_info.target_parent_hwnd as _);
    if !unsafe { IsWindow(target_parent).as_bool() } {
        bail!("Target parent window is invalid: 0x{:X}", target_parent.0 as usize);
    }

    let mut handle = AttachHandle {
        host_hwnd: hwnd,
        original_parent,
        original_style,
        original_ex_style,
        target_parent,
        topology: topo_info.topology.clone(),
        monitor_id: monitor.id.clone(),
        active: true,
    };

    apply_attachment(&mut handle, &topo_info, monitor)?;
    Ok(handle)
}

/// Detach the host window and restore original styles and parenting.
pub fn detach(mut handle: AttachHandle) {
    if handle.active {
        detach_internal(&mut handle);
        handle.active = false;
    }
}

/// Revalidate and reattach if the desktop shell has changed (e.g. Explorer restart, display changes).
pub fn revalidate(handle: &mut AttachHandle) -> Result<()> {
    if !handle.active {
        return Ok(());
    }

    let parent_valid = unsafe { IsWindow(handle.target_parent).as_bool() };
    let current_parent = unsafe { GetParent(handle.host_hwnd).unwrap_or_default() };
    if parent_valid && current_parent == handle.target_parent {
        return Ok(());
    }

    let topo_info = detect_topology().context("detect_topology failed during revalidate")?;

    // If shell parent has changed or is invalid, reapply
    if !parent_valid || topo_info.target_parent_hwnd != handle.target_parent.0 as usize {
        tracing::warn!(
            "Desktop shell structure changed, re-attaching window 0x{:X}...",
            handle.host_hwnd.0 as usize
        );
        handle.target_parent = HWND(topo_info.target_parent_hwnd as _);
        handle.topology = topo_info.topology.clone();
    }

    // Find monitor matching handle
    let monitors = enumerate_monitors().unwrap_or_default();
    let current_mon = monitors.into_iter().find(|m| m.id == handle.monitor_id);

    if let Some(mon) = current_mon {
        apply_attachment(handle, &topo_info, &mon)?;
    } else {
        bail!("Monitor {} not found during revalidate", handle.monitor_id);
    }

    Ok(())
}

fn apply_attachment(
    handle: &mut AttachHandle,
    topo_info: &TopologyInfo,
    monitor: &MonitorInfo,
) -> Result<()> {
    use windows::Win32::Graphics::Gdi::{InvalidateRect, UpdateWindow};
    use windows::Win32::UI::WindowsAndMessaging::SWP_FRAMECHANGED;
    let hwnd = handle.host_hwnd;
    let target_parent = handle.target_parent;

    unsafe {
        let current_parent = GetParent(hwnd).unwrap_or_default();
        let needs_reparent = current_parent != target_parent;

        if needs_reparent {
            // Strip WS_EX_LAYERED BEFORE SetParent so DWM allocates a fresh child redirection
            // surface when WS_EX_LAYERED is added after reparenting into Progman (WS_EX_NOREDIRECTIONBITMAP).
            let pre_ex_style = WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0;
            SetWindowLongW(hwnd, GWL_EXSTYLE, pre_ex_style as i32);

            // Change window style to WS_CHILD (remove WS_POPUP, don't combine)
            let new_style =
                (handle.original_style & !WS_POPUP.0) | WS_CHILD.0 | WS_CLIPSIBLINGS.0 | WS_VISIBLE.0;
            SetWindowLongW(hwnd, GWL_STYLE, new_style as i32);

            // SAFETY: SetParent attaches hwnd to shell desktop host
            let _ = SetParent(hwnd, target_parent);
        } else {
            let new_style =
                (handle.original_style & !WS_POPUP.0) | WS_CHILD.0 | WS_CLIPSIBLINGS.0 | WS_VISIBLE.0;
            SetWindowLongW(hwnd, GWL_STYLE, new_style as i32);
        }

        // Apply WS_EX_LAYERED | WS_EX_TRANSPARENT AFTER SetParent so Windows 11 24H2 DWM
        // creates a valid child redirection surface inside WS_EX_NOREDIRECTIONBITMAP Progman.
        let cur_ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let new_ex_style =
            WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0;
        if cur_ex != new_ex_style || needs_reparent {
            SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex_style as i32);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
                hwnd,
                windows::Win32::Foundation::COLORREF(0),
                255,
                windows::Win32::UI::WindowsAndMessaging::LWA_ALPHA,
            );
        }

        // Calculate screen-to-parent client coordinates
        let mut pt = POINT {
            x: monitor.x,
            y: monitor.y,
        };
        let _ = ScreenToClient(target_parent, &mut pt);

        match topo_info.topology {
            Topology::Raised => {
                // Windows 11 24H2+: SHELLDLL_DefView is direct child of Progman.
                // Ensure SHELLDLL_DefView is at the top of Progman's Z-order.
                let defview = HWND(topo_info.shelldll_defview_hwnd as _);
                if defview.0 as usize != 0 && IsWindow(defview).as_bool() {
                    let _ = SetWindowPos(
                        defview,
                        HWND_TOP,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }

                // Place host window directly behind SHELLDLL_DefView
                let insert_after = if defview.0 as usize != 0 && IsWindow(defview).as_bool() {
                    defview
                } else {
                    HWND_TOP
                };

                SetWindowPos(
                    hwnd,
                    insert_after,
                    pt.x,
                    pt.y,
                    monitor.width,
                    monitor.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
                )
                .ok()
                .context("SetWindowPos failed for Raised topology")?;

                // Push any shell WorkerW inside Progman to HWND_BOTTOM behind our host window
                if let Ok(shell_worker) = FindWindowExW(
                    target_parent,
                    HWND::default(),
                    w!("WorkerW"),
                    windows::core::PCWSTR::null(),
                ) {
                    if shell_worker.0 as usize != 0 && IsWindow(shell_worker).as_bool() {
                        let _ = SetWindowPos(
                            shell_worker,
                            HWND_BOTTOM,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                }
            }
            Topology::Classic => {
                // Windows 10 / Win11 <24H2: host window is child of wallpaper WorkerW.
                SetWindowPos(
                    hwnd,
                    HWND_TOP,
                    pt.x,
                    pt.y,
                    monitor.width,
                    monitor.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
                )
                .ok()
                .context("SetWindowPos failed for Classic topology")?;
            }
            Topology::Unknown(_) => {
                // Fallback attempt
                SetWindowPos(
                    hwnd,
                    HWND_TOP,
                    pt.x,
                    pt.y,
                    monitor.width,
                    monitor.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
                )
                .ok()
                .context("SetWindowPos failed for Unknown topology")?;
            }
        }

        let _ = InvalidateRect(hwnd, None, true);
        let _ = UpdateWindow(hwnd);
    }

    Ok(())
}

fn detach_internal(handle: &mut AttachHandle) {
    unsafe {
        if IsWindow(handle.host_hwnd).as_bool() {
            // Restore WorkerW if hidden
            if let Ok(shell_worker) = FindWindowExW(
                handle.target_parent,
                HWND::default(),
                w!("WorkerW"),
                windows::core::PCWSTR::null(),
            ) {
                if shell_worker.0 as usize != 0 && IsWindow(shell_worker).as_bool() {
                    let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        shell_worker,
                        windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
                    );
                }
            }
            // Restore original parent
            let _ = SetParent(handle.host_hwnd, handle.original_parent);
            SetWindowLongW(handle.host_hwnd, GWL_STYLE, handle.original_style as i32);
            SetWindowLongW(handle.host_hwnd, GWL_EXSTYLE, handle.original_ex_style as i32);
            let _ = SetWindowPos(
                handle.host_hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}
