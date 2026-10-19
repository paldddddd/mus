use common::HotkeysConfig;
use std::collections::HashMap;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

/// High-level actions triggered by hotkeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    TogglePlayback,
    NextTrack,
    PrevTrack,
    ToggleWallpaperPause,
    NextWallpaper,
    ToggleWidget,
    MediaNext,
    MediaPrev,
    MediaStop,
    MediaPlayPause,
}

impl HotkeyAction {
    pub fn id(&self) -> i32 {
        match self {
            Self::TogglePlayback => 2001,
            Self::NextTrack => 2002,
            Self::PrevTrack => 2003,
            Self::ToggleWallpaperPause => 2004,
            Self::NextWallpaper => 2005,
            Self::ToggleWidget => 2006,
            Self::MediaNext => 1001,
            Self::MediaPrev => 1002,
            Self::MediaStop => 1003,
            Self::MediaPlayPause => 1004,
        }
    }

    pub fn from_id(id: i32) -> Option<Self> {
        match id {
            2001 => Some(Self::TogglePlayback),
            2002 => Some(Self::NextTrack),
            2003 => Some(Self::PrevTrack),
            2004 => Some(Self::ToggleWallpaperPause),
            2005 => Some(Self::NextWallpaper),
            2006 => Some(Self::ToggleWidget),
            1001 => Some(Self::MediaNext),
            1002 => Some(Self::MediaPrev),
            1003 => Some(Self::MediaStop),
            1004 => Some(Self::MediaPlayPause),
            _ => None,
        }
    }
}

/// Parses a human-readable hotkey string like "Ctrl+Alt+Space" into (modifiers, virtual_key).
pub fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    let mut mods = MOD_NOREPEAT.0;
    let mut vk = None;

    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => {
                mods |= MOD_CONTROL.0;
            }
            "alt" => {
                mods |= MOD_ALT.0;
            }
            "shift" => {
                mods |= MOD_SHIFT.0;
            }
            "win" | "super" | "meta" => {
                mods |= MOD_WIN.0;
            }
            "space" => vk = Some(0x20),
            "left" => vk = Some(0x25),
            "up" => vk = Some(0x26),
            "right" => vk = Some(0x27),
            "down" => vk = Some(0x28),
            "enter" | "return" => vk = Some(0x0D),
            "escape" | "esc" => vk = Some(0x1B),
            "tab" => vk = Some(0x09),
            "backspace" => vk = Some(0x08),
            "delete" | "del" => vk = Some(0x2E),
            "insert" | "ins" => vk = Some(0x2D),
            "home" => vk = Some(0x24),
            "end" => vk = Some(0x23),
            "pageup" | "pgup" => vk = Some(0x21),
            "pagedown" | "pgdn" => vk = Some(0x22),
            k if k.starts_with('f') && k.len() <= 3 => {
                if let Ok(num) = k[1..].parse::<u32>() {
                    if (1..=24).contains(&num) {
                        vk = Some(0x6F + num);
                    }
                }
            }
            k if k.len() == 1 => {
                let ch = k.chars().next().unwrap().to_ascii_uppercase();
                if ch.is_ascii_alphanumeric() {
                    vk = Some(ch as u32);
                }
            }
            _ => {}
        }
    }

    vk.map(|k| (mods, k))
}

/// Global hotkey registration and lifecycle manager.
pub struct HotkeyManager {
    registered_ids: Vec<i32>,
    status: HashMap<String, bool>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            registered_ids: Vec::new(),
            status: HashMap::new(),
        }
    }

    /// Unregister all active hotkeys.
    pub fn unregister_all(&mut self) {
        unsafe {
            for &id in &self.registered_ids {
                let _ = UnregisterHotKey(HWND::default(), id);
            }
        }
        self.registered_ids.clear();
        self.status.clear();
    }

    /// Register all customizable hotkeys and hardware media keys on the current thread.
    pub fn register_all(&mut self, config: &HotkeysConfig) -> HashMap<String, bool> {
        self.unregister_all();

        let pairs = [
            ("toggle_playback", HotkeyAction::TogglePlayback, &config.toggle_playback),
            ("next_track", HotkeyAction::NextTrack, &config.next_track),
            ("prev_track", HotkeyAction::PrevTrack, &config.prev_track),
            ("toggle_wallpaper_pause", HotkeyAction::ToggleWallpaperPause, &config.toggle_wallpaper_pause),
            ("next_wallpaper", HotkeyAction::NextWallpaper, &config.next_wallpaper),
            ("toggle_widget", HotkeyAction::ToggleWidget, &config.toggle_widget),
        ];

        for (name, action, str_repr) in pairs {
            if let Some((mods, vk)) = parse_hotkey(str_repr) {
                let res = unsafe {
                    RegisterHotKey(
                        HWND::default(),
                        action.id(),
                        HOT_KEY_MODIFIERS(mods),
                        vk,
                    )
                };
                let ok = res.is_ok();
                if ok {
                    self.registered_ids.push(action.id());
                    self.status.insert(name.to_string(), true);
                } else {
                    tracing::warn!(
                        "Failed to register hotkey '{}' ({}): key combination may conflict with another application",
                        name,
                        str_repr
                    );
                    self.status.insert(name.to_string(), false);
                }
            } else {
                tracing::warn!("Invalid hotkey format for '{}': {}", name, str_repr);
                self.status.insert(name.to_string(), false);
            }
        }

        // Register default hardware media keys
        let media_keys = [
            (HotkeyAction::MediaNext, 0xB0),
            (HotkeyAction::MediaPrev, 0xB1),
            (HotkeyAction::MediaStop, 0xB2),
            (HotkeyAction::MediaPlayPause, 0xB3),
        ];
        for (action, vk) in media_keys {
            let res = unsafe {
                RegisterHotKey(
                    HWND::default(),
                    action.id(),
                    MOD_NOREPEAT,
                    vk,
                )
            };
            if res.is_ok() {
                self.registered_ids.push(action.id());
            }
        }

        self.status.clone()
    }

    /// Query the registration success status of configured hotkeys.
    pub fn get_status(&self) -> HashMap<String, bool> {
        self.status.clone()
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        self.unregister_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey_combinations() {
        let (mods, vk) = parse_hotkey("Ctrl+Alt+Space").expect("Must parse Ctrl+Alt+Space");
        assert_eq!(vk, 0x20);
        assert_ne!(mods & MOD_CONTROL.0, 0);
        assert_ne!(mods & MOD_ALT.0, 0);
        assert_ne!(mods & MOD_NOREPEAT.0, 0);

        let (mods_w, vk_w) = parse_hotkey("Ctrl+Shift+W").expect("Must parse Ctrl+Shift+W");
        assert_eq!(vk_w, 'W' as u32);
        assert_ne!(mods_w & MOD_CONTROL.0, 0);
        assert_ne!(mods_w & MOD_SHIFT.0, 0);

        let (_, vk_f5) = parse_hotkey("F5").expect("Must parse F5");
        assert_eq!(vk_f5, 0x74); // VK_F5
    }

    #[test]
    fn test_hotkey_action_id_roundtrip() {
        let actions = [
            HotkeyAction::TogglePlayback,
            HotkeyAction::NextTrack,
            HotkeyAction::PrevTrack,
            HotkeyAction::ToggleWallpaperPause,
            HotkeyAction::NextWallpaper,
            HotkeyAction::ToggleWidget,
            HotkeyAction::MediaNext,
            HotkeyAction::MediaPrev,
            HotkeyAction::MediaStop,
            HotkeyAction::MediaPlayPause,
        ];
        for a in actions {
            let id = a.id();
            let parsed = HotkeyAction::from_id(id).expect("Action roundtrip failed");
            assert_eq!(a, parsed);
        }
    }
}
