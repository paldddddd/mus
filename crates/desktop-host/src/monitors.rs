use anyhow::{Context, Result};
use common::MonitorInfo;
use std::mem::size_of;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

/// Enumerate all active display monitors with physical bounds and DPI information.
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
    let mut monitors = Vec::new();
    let monitors_ptr = &mut monitors as *mut Vec<MonitorInfo>;

    unsafe {
        // SAFETY: EnumDisplayMonitors calls our callback for each active monitor.
        // We pass a valid pointer to monitors_ptr as the LPARAM.
        EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(monitor_enum_proc),
            LPARAM(monitors_ptr as isize),
        )
        .ok()
        .context("EnumDisplayMonitors failed")?;
    }

    Ok(monitors)
}

/// Callback for EnumDisplayMonitors.
unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let monitors = &mut *(lparam.0 as *mut Vec<MonitorInfo>);

    let mut mi: MONITORINFOEXW = std::mem::zeroed();
    mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;

    // SAFETY: GetMonitorInfoW fills MONITORINFOEXW for the given HMONITOR.
    if !GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _).as_bool() {
        return true.into();
    }

    let is_primary = (mi.monitorInfo.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY = 1
    let rc = mi.monitorInfo.rcMonitor;
    let width = rc.right - rc.left;
    let height = rc.bottom - rc.top;

    // Extract device name from wide string
    let name_len = mi
        .szDevice
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(mi.szDevice.len());
    let name = String::from_utf16_lossy(&mi.szDevice[..name_len]);

    let mut dpi_x: u32 = 96;
    let mut dpi_y: u32 = 96;
    // SAFETY: Querying effective DPI for this monitor handle.
    let _ = GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
    let scale_factor = dpi_x as f64 / 96.0;

    let id = format!("{}_{}_{}", name, rc.left, rc.top);

    monitors.push(MonitorInfo {
        id,
        name,
        is_primary,
        x: rc.left,
        y: rc.top,
        width,
        height,
        dpi_x,
        dpi_y,
        scale_factor,
    });

    true.into()
}
