use crate::Renderer;
use anyhow::{bail, Context, Result};
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_ulong, c_void};
use std::path::Path;
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

type MpvHandlePtr = *mut c_void;

type MpvCreateFn = unsafe extern "C" fn() -> MpvHandlePtr;
type MpvInitializeFn = unsafe extern "C" fn(ctx: MpvHandlePtr) -> c_int;
type MpvTerminateDestroyFn = unsafe extern "C" fn(ctx: MpvHandlePtr);
type MpvSetOptionStringFn =
    unsafe extern "C" fn(ctx: MpvHandlePtr, name: *const c_char, data: *const c_char) -> c_int;
type MpvSetPropertyStringFn =
    unsafe extern "C" fn(ctx: MpvHandlePtr, name: *const c_char, data: *const c_char) -> c_int;
type MpvCommandStringFn =
    unsafe extern "C" fn(ctx: MpvHandlePtr, args: *const c_char) -> c_int;
type MpvClientApiVersionFn = unsafe extern "C" fn() -> c_ulong;
type MpvErrorStringFn = unsafe extern "C" fn(error: c_int) -> *const c_char;

struct MpvVTable {
    create: MpvCreateFn,
    initialize: MpvInitializeFn,
    terminate_destroy: MpvTerminateDestroyFn,
    set_option_string: MpvSetOptionStringFn,
    set_property_string: MpvSetPropertyStringFn,
    command_string: MpvCommandStringFn,
    client_api_version: MpvClientApiVersionFn,
    error_string: MpvErrorStringFn,
}

static MPV_VTABLE: OnceLock<MpvVTable> = OnceLock::new();

fn try_load_vtable() -> Option<MpvVTable> {
    unsafe {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            let exe_clean = exe
                .to_string_lossy()
                .strip_prefix(r"\\?\")
                .map(std::path::PathBuf::from)
                .unwrap_or(exe);
            if let Some(dir) = exe_clean.parent() {
                candidates.push(dir.join("libmpv-2.dll"));
                candidates.push(dir.join("mpv-2.dll"));
                candidates.push(dir.join("wallmus\\libmpv-2.dll"));
                candidates.push(dir.join("deps\\libmpv-2.dll"));
                if let Some(p1) = dir.parent() {
                    candidates.push(p1.join("libmpv-2.dll"));
                    candidates.push(p1.join("deps\\libmpv-2.dll"));
                    candidates.push(p1.join("dist\\libmpv-2.dll"));
                    candidates.push(p1.join("dist\\wallmus\\libmpv-2.dll"));
                    if let Some(p2) = p1.parent() {
                        candidates.push(p2.join("deps\\libmpv-2.dll"));
                        candidates.push(p2.join("dist\\libmpv-2.dll"));
                        candidates.push(p2.join("dist\\wallmus\\libmpv-2.dll"));
                    }
                }
            }
        }
        candidates.push("libmpv-2.dll".into());
        candidates.push("mpv-2.dll".into());
        candidates.push("deps\\libmpv-2.dll".into());
        candidates.push("dist\\libmpv-2.dll".into());
        candidates.push("dist\\wallmus\\libmpv-2.dll".into());

        let mut hmod = HMODULE::default();
        for path in &candidates {
            let wide: Vec<u16> = path
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            if let Ok(m) = LoadLibraryW(PCWSTR(wide.as_ptr())) {
                if !m.is_invalid() {
                    tracing::info!("Loaded libmpv from: {}", path.display());
                    hmod = m;
                    break;
                }
            }
        }

        if hmod.is_invalid() {
            tracing::warn!("Could not load libmpv-2.dll from any candidate path: {:?}", candidates);
            return None;
        }

        let load = |name: &CStr| -> Option<*const ()> {
            let proc = GetProcAddress(hmod, windows::core::PCSTR(name.as_ptr() as _));
            if proc.is_none() {
                tracing::warn!("libmpv symbol missing: {:?}", name);
            }
            proc.map(|p| p as _)
        };

        let vtable = MpvVTable {
            create: std::mem::transmute::<*const (), MpvCreateFn>(load(c"mpv_create")?),
            initialize: std::mem::transmute::<*const (), MpvInitializeFn>(load(c"mpv_initialize")?),
            terminate_destroy: std::mem::transmute::<*const (), MpvTerminateDestroyFn>(load(c"mpv_terminate_destroy")?),
            set_option_string: std::mem::transmute::<*const (), MpvSetOptionStringFn>(load(c"mpv_set_option_string")?),
            set_property_string: std::mem::transmute::<*const (), MpvSetPropertyStringFn>(load(c"mpv_set_property_string")?),
            command_string: std::mem::transmute::<*const (), MpvCommandStringFn>(load(c"mpv_command_string")?),
            client_api_version: std::mem::transmute::<*const (), MpvClientApiVersionFn>(load(c"mpv_client_api_version")?),
            error_string: std::mem::transmute::<*const (), MpvErrorStringFn>(load(c"mpv_error_string")?),
        };

        Some(vtable)
    }
}

fn get_vtable() -> Option<&'static MpvVTable> {
    if let Some(vt) = MPV_VTABLE.get() {
        return Some(vt);
    }
    let loaded = try_load_vtable()?;
    let _ = MPV_VTABLE.set(loaded);
    MPV_VTABLE.get()
}

/// Query libmpv version string if library is present.
pub fn get_mpv_version() -> Option<String> {
    let vt = get_vtable()?;
    let ver = unsafe { (vt.client_api_version)() };
    let major = ver >> 16;
    let minor = ver & 0xFFFF;
    Some(format!("libmpv {}.{} (API v{})", major, minor, ver))
}

/// MpvRenderer for playing video live wallpapers (mp4, webm, mkv).
pub struct MpvRenderer {
    handle: MpvHandlePtr,
    target_hwnd: usize,
    width: i32,
    height: i32,
    loaded: bool,
    mouse_pos: (f32, f32),
    bass_pulse: f32,
}

// SAFETY: MpvRenderer controls its handle and HWND via thread-safe C API.
unsafe impl Send for MpvRenderer {}
unsafe impl Sync for MpvRenderer {}

impl MpvRenderer {
    pub fn new(target_hwnd: usize, width: i32, height: i32) -> Result<Self> {
        let _ = get_vtable().context("libmpv dynamic library not found")?;
        Ok(Self {
            handle: std::ptr::null_mut(),
            target_hwnd,
            width,
            height,
            loaded: false,
            mouse_pos: (0.5, 0.5),
            bass_pulse: 0.0,
        })
    }

    fn check_err(&self, code: c_int, action: &str) -> Result<()> {
        if code >= 0 {
            return Ok(());
        }
        let vt = get_vtable().unwrap();
        let c_msg = unsafe { (vt.error_string)(code) };
        let msg = unsafe { CStr::from_ptr(c_msg) }.to_string_lossy();
        bail!("mpv error during '{}': {} (code {})", action, msg, code);
    }

    fn sync_viewport(&self) {
        if !self.loaded || self.handle.is_null() {
            return;
        }
        if let Some(vt) = get_vtable() {
            let (x, y) = self.mouse_pos;
            let pan_x = (0.5 - x.clamp(0.0, 1.0)) * 0.035;
            let pan_y = (0.5 - y.clamp(0.0, 1.0)) * 0.035;
            let zoom = 0.05 + self.bass_pulse.clamp(0.0, 1.0) * 0.065;
            if let (Ok(cx), Ok(cy), Ok(cz)) = (
                std::ffi::CString::new(format!("{:.4}", pan_x)),
                std::ffi::CString::new(format!("{:.4}", pan_y)),
                std::ffi::CString::new(format!("{:.4}", zoom)),
            ) {
                unsafe {
                    let _ = (vt.set_property_string)(self.handle, c"video-zoom".as_ptr(), cz.as_ptr());
                    let _ = (vt.set_property_string)(self.handle, c"video-pan-x".as_ptr(), cx.as_ptr());
                    let _ = (vt.set_property_string)(self.handle, c"video-pan-y".as_ptr(), cy.as_ptr());
                }
            }
        }
    }
}

impl Drop for MpvRenderer {
    fn drop(&mut self) {
        let _ = self.unload();
    }
}

impl Renderer for MpvRenderer {
    fn load(&mut self, source: &str) -> Result<()> {
        self.unload()?;

        let path = Path::new(source);
        if !path.exists() {
            bail!("Video file not found: {}", source);
        }

        let vt = get_vtable().context("libmpv not available")?;

        let ctx = unsafe { (vt.create)() };
        if ctx.is_null() {
            bail!("mpv_create returned null");
        }
        self.handle = ctx;

        // Configure options strictly according to Section 6.4(в):
        // wid: target window
        let wid_str = std::ffi::CString::new(self.target_hwnd.to_string())?;
        unsafe {
            (vt.set_option_string)(ctx, c"wid".as_ptr(), wid_str.as_ptr());
            // hwdec d3d11va
            (vt.set_option_string)(ctx, c"hwdec".as_ptr(), c"d3d11va".as_ptr());
            // loop-file=inf
            (vt.set_option_string)(ctx, c"loop-file".as_ptr(), c"inf".as_ptr());
            // mute sound (wallpaper must be silent)
            (vt.set_option_string)(ctx, c"mute".as_ptr(), c"yes".as_ptr());
            (vt.set_option_string)(ctx, c"ao".as_ptr(), c"null".as_ptr());
            // disable OSC / OSD
            (vt.set_option_string)(ctx, c"osc".as_ptr(), c"no".as_ptr());
            (vt.set_option_string)(ctx, c"osd-level".as_ptr(), c"0".as_ptr());
            // disable input handling
            (vt.set_option_string)(ctx, c"input-default-bindings".as_ptr(), c"no".as_ptr());
            (vt.set_option_string)(ctx, c"input-vo-keyboard".as_ptr(), c"no".as_ptr());
            // Set gpu-api
            (vt.set_option_string)(ctx, c"gpu-api".as_ptr(), c"d3d11".as_ptr());
        }

        let init_code = unsafe { (vt.initialize)(ctx) };
        self.check_err(init_code, "initialize")?;

        // Load video file
        let cmd = format!("loadfile \"{}\" replace", source.replace('\\', "/"));
        let c_cmd = std::ffi::CString::new(cmd)?;
        let load_code = unsafe { (vt.command_string)(ctx, c_cmd.as_ptr()) };
        self.check_err(load_code, "loadfile")?;

        self.loaded = true;
        tracing::info!(
            "Loaded video wallpaper in MpvRenderer: {} (wid: 0x{:X})",
            source,
            self.target_hwnd
        );

        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        if !self.loaded || self.handle.is_null() {
            return Ok(());
        }
        let vt = get_vtable().context("libmpv not available")?;
        unsafe {
            let res = (vt.set_property_string)(self.handle, c"pause".as_ptr(), c"yes".as_ptr());
            self.check_err(res, "pause=yes")?;
        }
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        if !self.loaded || self.handle.is_null() {
            return Ok(());
        }
        let vt = get_vtable().context("libmpv not available")?;
        unsafe {
            let res = (vt.set_property_string)(self.handle, c"pause".as_ptr(), c"no".as_ptr());
            self.check_err(res, "pause=no")?;
        }
        Ok(())
    }

    fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn unload(&mut self) -> Result<()> {
        if !self.handle.is_null() {
            if let Some(vt) = get_vtable() {
                unsafe {
                    (vt.terminate_destroy)(self.handle);
                }
            }
            self.handle = std::ptr::null_mut();
        }
        self.loaded = false;
        Ok(())
    }

    fn update_mouse(&mut self, x: f32, y: f32) {
        if (self.mouse_pos.0 - x).abs() > 0.0005 || (self.mouse_pos.1 - y).abs() > 0.0005 {
            self.mouse_pos = (x, y);
            self.sync_viewport();
        }
    }

    fn update_bass(&mut self, bass: f32) {
        let prev = self.bass_pulse;
        let target = bass.clamp(0.0, 1.0);
        if (target - prev).abs() > 0.004 {
            self.bass_pulse = target;
            self.sync_viewport();
        }
    }
}
