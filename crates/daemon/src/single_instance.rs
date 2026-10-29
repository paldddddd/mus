use anyhow::Result;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};

/// RAII Guard ensuring only a single instance of wallmus runs concurrently.
pub struct SingleInstanceGuard {
    handle: HANDLE,
    is_first_instance: bool,
}

impl SingleInstanceGuard {
    pub fn acquire(name: &str) -> Result<Self> {
        let wide_name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let handle = CreateMutexW(None, true, windows::core::PCWSTR(wide_name.as_ptr()))
                .map_err(|e| anyhow::anyhow!("CreateMutexW error: {e}"))?;

            let already_exists = GetLastError() == ERROR_ALREADY_EXISTS;

            Ok(Self {
                handle,
                is_first_instance: !already_exists,
            })
        }
    }

    pub fn is_primary(&self) -> bool {
        self.is_first_instance
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            unsafe {
                if self.is_first_instance {
                    let _ = ReleaseMutex(self.handle);
                }
                let _ = CloseHandle(self.handle);
            }
        }
    }
}
