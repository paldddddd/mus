use crate::Renderer;
use anyhow::{bail, Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use windows::Win32::Foundation::HWND;

/// Out-of-process HTML5/WebGL wallpaper renderer powered by WebView2.
/// Runs in a dedicated child process to isolate memory, GPU, and crashes from the daemon.
pub struct HtmlRenderer {
    parent_hwnd: HWND,
    width: i32,
    height: i32,
    child_process: Option<Child>,
    current_source: Option<String>,
    is_paused: bool,
}

unsafe impl Send for HtmlRenderer {}
unsafe impl Sync for HtmlRenderer {}

impl HtmlRenderer {
    /// Create a new HTML wallpaper renderer bound to a parent window.
    pub fn new(parent_hwnd: HWND, width: i32, height: i32) -> Self {
        Self {
            parent_hwnd,
            width,
            height,
            child_process: None,
            current_source: None,
            is_paused: false,
        }
    }

    /// Locate the `wallmus-htmlhost.exe` binary.
    fn find_htmlhost_exe() -> Result<PathBuf> {
        // 1. Next to current executable
        if let Ok(cur_exe) = env::current_exe() {
            if let Some(parent) = cur_exe.parent() {
                let candidate = parent.join("wallmus-htmlhost.exe");
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }

        // 2. In target/debug or target/release
        let debug_cand = PathBuf::from("target/debug/wallmus-htmlhost.exe");
        if debug_cand.exists() {
            return Ok(debug_cand);
        }
        let rel_cand = PathBuf::from("target/release/wallmus-htmlhost.exe");
        if rel_cand.exists() {
            return Ok(rel_cand);
        }

        // 3. Current working directory
        let cwd_cand = PathBuf::from("wallmus-htmlhost.exe");
        if cwd_cand.exists() {
            return Ok(cwd_cand);
        }

        bail!("wallmus-htmlhost.exe not found next to executable or in target/debug");
    }
}

impl Renderer for HtmlRenderer {
    fn load(&mut self, source: &str) -> Result<()> {
        self.unload()?;

        let url = if source.starts_with("http://") || source.starts_with("https://") || source.starts_with("file://") {
            source.to_string()
        } else {
            let path = Path::new(source);
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let path_str = canonical.to_string_lossy().replace('\\', "/");
            format!("file:///{}", path_str.trim_start_matches('/'))
        };

        let exe_path = Self::find_htmlhost_exe()?;
        let parent_val = self.parent_hwnd.0 as usize;

        tracing::info!(
            "Spawning out-of-process HTML wallpaper host for HWND 0x{:X} -> {}",
            parent_val,
            url
        );

        let child = Command::new(&exe_path)
            .arg("--parent")
            .arg(parent_val.to_string())
            .arg("--url")
            .arg(&url)
            .arg("--width")
            .arg(self.width.to_string())
            .arg("--height")
            .arg(self.height.to_string())
            .spawn()
            .with_context(|| format!("Failed to spawn {}", exe_path.display()))?;

        self.child_process = Some(child);
        self.current_source = Some(source.to_string());
        self.is_paused = false;

        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        if !self.is_paused {
            self.is_paused = true;
            // When paused, child remains attached; optional thread suspension can be triggered
            tracing::debug!("Paused HTML wallpaper on HWND 0x{:X}", self.parent_hwnd.0 as usize);
        }
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        if self.is_paused {
            self.is_paused = false;
            tracing::debug!("Resumed HTML wallpaper on HWND 0x{:X}", self.parent_hwnd.0 as usize);
        }
        Ok(())
    }

    fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn unload(&mut self) -> Result<()> {
        if let Some(mut child) = self.child_process.take() {
            tracing::info!("Terminating HTML wallpaper host process (pid {})", child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
        self.current_source = None;
        self.is_paused = false;
        Ok(())
    }
}

impl Drop for HtmlRenderer {
    fn drop(&mut self) {
        let _ = self.unload();
    }
}
