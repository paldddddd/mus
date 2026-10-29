//! Event-driven Governor engine for managing wallpaper playback state and battery/resource saving.

use anyhow::Result;
use common::{GovernorConfig, MonitorInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND,
    GWL_EXSTYLE, WINEVENT_OUTOFCONTEXT, WS_EX_TOOLWINDOW,
};

/// High-level Governor state for a monitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GovernorState {
    Running,
    Paused(PauseReason),
    Unloaded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PauseReason {
    Manual,
    FullScreenApp,
    DesktopCovered,
    SessionLocked,
    DisplayOff,
    BatterySaver,
}

/// Governor module managing auto-pause and power management.
pub struct Governor {
    config: Arc<Mutex<GovernorConfig>>,
    monitor_states: Arc<Mutex<HashMap<String, GovernorState>>>,
    manual_paused: Arc<AtomicBool>,
    session_locked: Arc<AtomicBool>,
    display_off: Arc<AtomicBool>,
    hook_handle: Option<HWINEVENTHOOK>,
}

// SAFETY: Governor manages its Win32 hook and state safely behind thread-safe primitives.
unsafe impl Send for Governor {}
unsafe impl Sync for Governor {}

// Global reference for WinEventHook callback
static GOVERNOR_EVENT_TRIGGER: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn win_event_proc(
    _h_win_event_hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _dwms_event_time: u32,
) {
    GOVERNOR_EVENT_TRIGGER.store(true, Ordering::SeqCst);
}

impl Governor {
    pub fn new(config: GovernorConfig) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            monitor_states: Arc::new(Mutex::new(HashMap::new())),
            manual_paused: Arc::new(AtomicBool::new(false)),
            session_locked: Arc::new(AtomicBool::new(false)),
            display_off: Arc::new(AtomicBool::new(false)),
            hook_handle: None,
        }
    }

    /// Start event-driven hooks (SetWinEventHook for foreground and window move/minimize).
    pub fn start_hooks(&mut self) -> Result<()> {
        unsafe {
            // Hook foreground window changes, moves/resizes, and minimize/restore events
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_MINIMIZEEND,
                None,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );

            if hook.is_invalid() {
                anyhow::bail!("SetWinEventHook failed");
            }
            self.hook_handle = Some(hook);
            tracing::info!("Governor WinEvent hooks installed successfully");
        }
        Ok(())
    }

    /// Stop event hooks.
    pub fn stop_hooks(&mut self) {
        if let Some(hook) = self.hook_handle.take() {
            unsafe {
                let _ = UnhookWinEvent(hook);
            }
        }
    }

    /// Set manual pause override.
    pub fn set_manual_pause(&self, paused: bool) {
        self.manual_paused.store(paused, Ordering::SeqCst);
    }

    pub fn is_manual_paused(&self) -> bool {
        self.manual_paused.load(Ordering::SeqCst)
    }

    pub fn set_session_locked(&self, locked: bool) {
        self.session_locked.store(locked, Ordering::SeqCst);
    }

    pub fn set_display_off(&self, off: bool) {
        self.display_off.store(off, Ordering::SeqCst);
    }

    /// Update configuration settings.
    pub fn update_config(&self, new_config: GovernorConfig) {
        let mut cfg = self.config.lock().unwrap();
        *cfg = new_config;
    }

    pub fn get_monitor_state(&self, mon_id: &str) -> Option<GovernorState> {
        let states = self.monitor_states.lock().unwrap();
        states.get(mon_id).cloned()
    }

    pub fn set_monitor_state(&self, mon_id: String, state: GovernorState) {
        let mut states = self.monitor_states.lock().unwrap();
        states.insert(mon_id, state);
    }

    /// Check if event hook fired since last check.
    pub fn take_event_trigger(&self) -> bool {
        GOVERNOR_EVENT_TRIGGER.swap(false, Ordering::SeqCst)
    }

    /// Evaluate current state for a given monitor based on active triggers.
    pub fn evaluate_monitor(&self, monitor: &MonitorInfo, our_pid: u32) -> GovernorState {
        // 1. Manual pause always takes precedence
        if self.manual_paused.load(Ordering::SeqCst) {
            return GovernorState::Paused(PauseReason::Manual);
        }

        let cfg = self.config.lock().unwrap().clone();
        if !cfg.enabled {
            return GovernorState::Running;
        }

        // 2. Session lock / remote desktop
        if cfg.pause_on_lock && self.session_locked.load(Ordering::SeqCst) {
            return GovernorState::Paused(PauseReason::SessionLocked);
        }

        // 3. Display off / standby
        if cfg.pause_on_display_off && self.display_off.load(Ordering::SeqCst) {
            return GovernorState::Paused(PauseReason::DisplayOff);
        }

        // 4. Battery / Power saving check
        if cfg.pause_on_battery && is_on_battery() {
            return GovernorState::Paused(PauseReason::BatterySaver);
        }

        // 5. Fullscreen application or game check
        if cfg.pause_on_fullscreen && is_fullscreen_active(monitor, our_pid) {
            return GovernorState::Paused(PauseReason::FullScreenApp);
        }

        // 6. Desktop fully covered by windows check (>= 95% covered)
        if cfg.pause_on_desktop_covered && is_desktop_covered(monitor, our_pid) {
            return GovernorState::Paused(PauseReason::DesktopCovered);
        }

        GovernorState::Running
    }
}

impl Drop for Governor {
    fn drop(&mut self) {
        self.stop_hooks();
    }
}

/// Query whether the system is running on battery.
fn is_on_battery() -> bool {
    unsafe {
        let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
        if GetSystemPowerStatus(&mut status).is_ok() {
            // ACLineStatus: 0 = Offline (Battery), 1 = Online (AC), 255 = Unknown
            return status.ACLineStatus == 0;
        }
    }
    false
}

/// Query whether a full-screen application or D3D game is active on the monitor.
fn is_fullscreen_active(monitor: &MonitorInfo, our_pid: u32) -> bool {
    unsafe {
        // 1. Check Shell User Notification State (only true exclusive D3D fullscreen)
        if let Ok(state) = SHQueryUserNotificationState() {
            if state == QUNS_RUNNING_D3D_FULL_SCREEN {
                return true;
            }
        }

        // 2. Check foreground window bounds
        let fg = GetForegroundWindow();
        if fg.0 as usize == 0 || !IsWindowVisible(fg).as_bool() || IsIconic(fg).as_bool() {
            return false;
        }

        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(fg, Some(&mut process_id));
        if process_id == our_pid {
            return false;
        }

        let mut class_buf = [0u16; 64];
        let len = GetClassNameW(fg, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
        if class_name == "Progman"
            || class_name == "WorkerW"
            || class_name == "SHELLDLL_DefView"
            || class_name == "SysListView32"
            || class_name == "Shell_TrayWnd"
            || class_name == "SecondaryTrayWnd"
            || class_name == "Windows.UI.Core.CoreWindow"
            || class_name == "XamlExplorerHostIslandWindow"
            || class_name.starts_with("wallmus")
        {
            return false;
        }

        let mut title_buf = [0u16; 128];
        let tlen = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(fg, &mut title_buf);
        if tlen > 0 {
            let title = String::from_utf16_lossy(&title_buf[..tlen as usize]);
            if title.starts_with("wallmus") {
                return false;
            }
        }

        let style = GetWindowLongW(fg, windows::Win32::UI::WindowsAndMessaging::GWL_STYLE) as u32;
        let has_caption = (style & windows::Win32::UI::WindowsAndMessaging::WS_CAPTION.0)
            == windows::Win32::UI::WindowsAndMessaging::WS_CAPTION.0;
        let has_thickframe = (style & windows::Win32::UI::WindowsAndMessaging::WS_THICKFRAME.0) != 0;
        let is_maximized = (style & windows::Win32::UI::WindowsAndMessaging::WS_MAXIMIZE.0) != 0;
        if has_caption || has_thickframe || is_maximized {
            // Normal desktop or custom-titlebar window (Chrome, VSCode, Discord, Tauri, etc.)
            // is handled by is_desktop_covered, not FullScreenApp game detection.
            return false;
        }

        let mut rc = RECT::default();
        if GetWindowRect(fg, &mut rc).is_ok() {
            // Only true borderless fullscreen (covers the entire monitor including the taskbar area)
            if rc.left <= monitor.x
                && rc.top <= monitor.y
                && rc.right >= monitor.x + monitor.width
                && rc.bottom >= monitor.y + monitor.height
            {
                return true;
            }
        }
    }

    false
}

/// Query whether desktop on a given monitor is >= 95% covered by normal windows.
fn is_desktop_covered(monitor: &MonitorInfo, our_pid: u32) -> bool {
    struct WindowCoverageCtx {
        monitor_x: i32,
        monitor_y: i32,
        monitor_w: i32,
        monitor_h: i32,
        our_pid: u32,
        is_covered: bool,
    }

    let mut ctx = WindowCoverageCtx {
        monitor_x: monitor.x,
        monitor_y: monitor.y,
        monitor_w: monitor.width,
        monitor_h: monitor.height,
        our_pid,
        is_covered: false,
    };

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut WindowCoverageCtx);

        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return true.into();
        }

        // Check if window is cloaked (e.g. UWP suspended window or virtual desktop background)
        let mut cloaked: u32 = 0;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return true.into();
        }

        // Exclude tool windows and shell windows
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
            return true.into();
        }

        let mut class_buf = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
        if class_name == "Progman" || class_name == "WorkerW" || class_name == "Shell_TrayWnd" {
            return true.into();
        }

        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == ctx.our_pid {
            return true.into();
        }

        let mut title_buf = [0u16; 128];
        let tlen = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut title_buf);
        if tlen > 0 {
            let title = String::from_utf16_lossy(&title_buf[..tlen as usize]);
            if title.starts_with("wallmus") {
                return true.into();
            }
        }

        let mut rc = RECT::default();
        if GetWindowRect(hwnd, &mut rc).is_ok() {
            let intersect_left = rc.left.max(ctx.monitor_x);
            let intersect_top = rc.top.max(ctx.monitor_y);
            let intersect_right = rc.right.min(ctx.monitor_x + ctx.monitor_w);
            let intersect_bottom = rc.bottom.min(ctx.monitor_y + ctx.monitor_h);

            let inter_w = (intersect_right - intersect_left).max(0);
            let inter_h = (intersect_bottom - intersect_top).max(0);
            let inter_area = (inter_w * inter_h) as f64;
            let monitor_area = (ctx.monitor_w * ctx.monitor_h) as f64;

            if inter_area >= monitor_area * 0.95 {
                ctx.is_covered = true;
                return false.into(); // Stop enumeration
            }
        }

        true.into()
    }

    unsafe {
        let _ = EnumWindows(Some(enum_windows_proc), LPARAM(&mut ctx as *mut _ as isize));
    }

    ctx.is_covered
}
