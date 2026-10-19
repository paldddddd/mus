//! Low-level desktop mouse hook (`WH_MOUSE_LL`) for interactive wallpapers.
//!
//! Per project specification (Section 14 & Stage 5 Item 5):
//! - Hook is installed ONLY when the desktop is visible (`GovernorState::Running`).
//! - When the desktop is covered by windows or fullscreen games, the hook is completely removed (`UnhookWindowsHookEx`).
//! - Overhead is minimal: atomic write of (x, y) into static atomics (under 10 nanoseconds), zero allocations.
//! - Normalized (0.0 .. 1.0) coordinates are forwarded to active wallpapers (e.g. WGSL shaders, WebGL).

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSLLHOOKSTRUCT,
    WH_MOUSE_LL, WM_MOUSEMOVE,
};

static HOOK_HANDLE: AtomicIsize = AtomicIsize::new(0);
static MOUSE_X: AtomicI32 = AtomicI32::new(0);
static MOUSE_Y: AtomicI32 = AtomicI32::new(0);
static MOUSE_MOVED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn mouse_ll_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 == WM_MOUSEMOVE as usize && lparam.0 != 0 {
        let hook_struct = *(lparam.0 as *const MSLLHOOKSTRUCT);
        MOUSE_X.store(hook_struct.pt.x, Ordering::Relaxed);
        MOUSE_Y.store(hook_struct.pt.y, Ordering::Relaxed);
        MOUSE_MOVED.store(true, Ordering::Relaxed);
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Manager responsible for dynamically attaching and detaching the `WH_MOUSE_LL` hook.
pub struct MouseHookManager {
    is_active: bool,
}

impl Default for MouseHookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseHookManager {
    /// Create a new mouse hook manager in the detached state.
    pub fn new() -> Self {
        Self { is_active: false }
    }

    /// Dynamically enable or disable the hook based on desktop visibility and user configuration.
    pub fn set_enabled(&mut self, enable: bool) {
        if enable && !self.is_active {
            unsafe {
                let hook = SetWindowsHookExW(
                    WH_MOUSE_LL,
                    Some(mouse_ll_proc),
                    HINSTANCE::default(),
                    0,
                );
                match hook {
                    Ok(h) => {
                        HOOK_HANDLE.store(h.0 as isize, Ordering::SeqCst);
                        self.is_active = true;
                        tracing::info!("MouseHook: WH_MOUSE_LL installed (desktop active)");
                    }
                    Err(e) => {
                        tracing::warn!("MouseHook: Failed to install WH_MOUSE_LL hook: {e}");
                    }
                }
            }
        } else if !enable && self.is_active {
            let handle_val = HOOK_HANDLE.swap(0, Ordering::SeqCst);
            if handle_val != 0 {
                unsafe {
                    let _ = UnhookWindowsHookEx(HHOOK(handle_val as *mut std::ffi::c_void));
                }
                self.is_active = false;
                tracing::info!("MouseHook: WH_MOUSE_LL unhooked (desktop inactive/covered)");
            }
        }
    }

    /// Whether the low-level hook is currently installed and active.
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Retrieve the current cursor coordinates if the mouse moved since the last poll.
    /// Resets the moved flag atomically.
    pub fn poll_position() -> Option<(i32, i32)> {
        if MOUSE_MOVED.swap(false, Ordering::Relaxed) {
            Some((
                MOUSE_X.load(Ordering::Relaxed),
                MOUSE_Y.load(Ordering::Relaxed),
            ))
        } else {
            None
        }
    }
}

impl Drop for MouseHookManager {
    fn drop(&mut self) {
        self.set_enabled(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_hook_state_and_polling() {
        let mut mgr = MouseHookManager::new();
        assert!(!mgr.is_active());

        // Test atomic coordinate update simulation
        MOUSE_X.store(350, Ordering::Relaxed);
        MOUSE_Y.store(600, Ordering::Relaxed);
        MOUSE_MOVED.store(true, Ordering::Relaxed);

        let pos = MouseHookManager::poll_position();
        assert_eq!(pos, Some((350, 600)));

        // Flag should be reset now
        assert_eq!(MouseHookManager::poll_position(), None);

        // Disabling when already inactive should be safe and idempotent
        mgr.set_enabled(false);
        assert!(!mgr.is_active());
    }
}
