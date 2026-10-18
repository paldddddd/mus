use anyhow::{Context, Result};
use std::env;
use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const APP_NAME: PCWSTR = w!("wallmus");

/// Check if wallmus is configured to launch on Windows startup.
pub fn is_autostart_enabled() -> bool {
    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_READ, &mut hkey).is_ok() {
            let mut val_type = windows::Win32::System::Registry::REG_VALUE_TYPE::default();
            let mut size = 0u32;
            let res = RegQueryValueExW(hkey, APP_NAME, None, Some(&mut val_type), None, Some(&mut size));
            let _ = RegCloseKey(hkey);
            return res.is_ok();
        }
    }
    false
}

/// Enable or disable wallmus startup with Windows.
pub fn set_autostart(enabled: bool) -> Result<()> {
    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_WRITE, &mut hkey)
            .ok()
            .context("Failed to open HKCU Run registry key")?;

        if enabled {
            let current_exe = env::current_exe().context("Failed to get current executable path")?;
            let cmd = format!("\"{}\" --daemon", current_exe.display());
            let wide_val: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();

            let res = RegSetValueExW(
                hkey,
                APP_NAME,
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    wide_val.as_ptr() as *const u8,
                    wide_val.len() * 2,
                )),
            );
            let _ = RegCloseKey(hkey);
            res.ok().context("Failed to set autostart registry value")?;
            tracing::info!("Autostart enabled in HKCU Run");
        } else {
            let _ = RegDeleteValueW(hkey, APP_NAME);
            let _ = RegCloseKey(hkey);
            tracing::info!("Autostart disabled in HKCU Run");
        }
    }
    Ok(())
}
