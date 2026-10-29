//! Magnetic snap attraction effect.
//!
//! Renders a stream of accent-colored particles in the *gap between the widget and the
//! screen edge* (outside the widget's own surface) using a dedicated transparent,
//! click-through layered window placed directly beneath the widget in Z-order.
//! Particles are emitted uniformly along the widget's facing edge and accelerate into
//! the screen edge, where they are absorbed into a soft glowing seam.

use crate::surface::ArgbSurface;
use crate::window::{SNAP_BOTTOM, SNAP_LEFT, SNAP_RIGHT, SNAP_TOP};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, SetWindowPos, ShowWindow,
    UpdateLayeredWindow, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE,
    ULW_ALPHA, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_POPUP,
};

static FX_CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);
const FX_CLASS_NAME: PCWSTR = w!("wallmus_SnapFx");

/// Max particles alive at once (keeps CPU usage tiny).
const MAX_PARTICLES: usize = 160;

#[derive(Clone, Copy)]
struct Particle {
    /// Screen-space position (px)
    x: f32,
    y: f32,
    /// Velocity (px / s)
    vx: f32,
    vy: f32,
    /// Which screen edge this particle is being pulled into
    edge: u8,
    size: f32,
    /// 0..1 fade-in ramp
    age: f32,
    alive: bool,
}

/// Geometry describing the current attraction target.
#[derive(Clone, Copy, Default)]
struct Target {
    widget: RECT,
    work: RECT,
    edges: u8,
    threshold: f32,
    corner: f32,
}

pub struct SnapFx {
    hwnd: HWND,
    hdc_screen: HDC,
    hdc_mem: HDC,
    hbitmap: HBITMAP,
    dib_bits: *mut u32,
    surface: ArgbSurface,
    /// Window origin in screen space
    origin: (i32, i32),
    particles: Vec<Particle>,
    target: Option<Target>,
    /// Last known geometry (kept after release so in-flight particles finish their trip)
    last_target: Target,
    accent: u32,
    rng: u32,
    spawn_acc: f32,
    /// Per-edge seam glow intensity (L, R, T, B)
    seam: [f32; 4],
    last_tick: Instant,
    shown: bool,
}

impl SnapFx {
    pub fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
            .unwrap_or(0x9E37_79B9)
            | 1;
        Self {
            hwnd: HWND::default(),
            hdc_screen: HDC::default(),
            hdc_mem: HDC::default(),
            hbitmap: HBITMAP::default(),
            dib_bits: std::ptr::null_mut(),
            surface: ArgbSurface::new(1, 1),
            origin: (0, 0),
            particles: Vec::with_capacity(MAX_PARTICLES),
            target: None,
            last_target: Target::default(),
            accent: 0xFF00F5D4,
            rng: seed,
            spawn_acc: 0.0,
            seam: [0.0; 4],
            last_tick: Instant::now(),
            shown: false,
        }
    }

    #[inline]
    fn rand(&mut self) -> f32 {
        // xorshift32 — uniform, non-repeating sequence (fixes particles spawning at identical points)
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Update the attraction target while the widget is dragged near screen edges.
    pub fn set_target(&mut self, widget: RECT, work: RECT, edges: u8, threshold: f32, corner: f32, accent: u32) {
        self.accent = accent;
        if edges == 0 {
            self.target = None;
            return;
        }
        let t = Target { widget, work, edges, threshold: threshold.max(1.0), corner };
        self.target = Some(t);
        self.last_target = t;
    }

    pub fn clear_target(&mut self) {
        self.target = None;
    }

    pub fn is_active(&self) -> bool {
        self.target.is_some() || self.particles.iter().any(|p| p.alive) || self.seam.iter().any(|&s| s > 0.01)
    }

    /// Distance between the widget's side and the screen edge for a given edge flag.
    fn gap(t: &Target, edge: u8) -> f32 {
        match edge {
            SNAP_LEFT => (t.widget.left - t.work.left) as f32,
            SNAP_RIGHT => (t.work.right - t.widget.right) as f32,
            SNAP_TOP => (t.widget.top - t.work.top) as f32,
            _ => (t.work.bottom - t.widget.bottom) as f32,
        }
    }

    fn edge_index(edge: u8) -> usize {
        match edge {
            SNAP_LEFT => 0,
            SNAP_RIGHT => 1,
            SNAP_TOP => 2,
            _ => 3,
        }
    }

    fn spawn(&mut self, t: &Target, edge: u8) {
        if self.particles.iter().filter(|p| p.alive).count() >= MAX_PARTICLES {
            return;
        }
        let corner = t.corner.max(6.0);
        let (wl, wt, wr, wb) = (
            t.widget.left as f32,
            t.widget.top as f32,
            t.widget.right as f32,
            t.widget.bottom as f32,
        );
        let r_along = self.rand();
        let r_off = self.rand();
        let r_spd = self.rand();
        let r_lat = self.rand();
        let r_size = self.rand();

        // Uniformly distributed along the straight span of the facing side (rounded corners excluded)
        let (x, y, vx, vy) = match edge {
            SNAP_LEFT | SNAP_RIGHT => {
                let span = (wb - wt - corner * 2.0).max(4.0);
                let y = wt + corner + r_along * span;
                let out = if edge == SNAP_LEFT { -1.0 } else { 1.0 };
                let x = if edge == SNAP_LEFT { wl } else { wr } + out * (1.0 + r_off * 6.0);
                (x, y, out * (40.0 + r_spd * 60.0), (r_lat - 0.5) * 30.0)
            }
            _ => {
                let span = (wr - wl - corner * 2.0).max(4.0);
                let x = wl + corner + r_along * span;
                let out = if edge == SNAP_TOP { -1.0 } else { 1.0 };
                let y = if edge == SNAP_TOP { wt } else { wb } + out * (1.0 + r_off * 6.0);
                (x, y, (r_lat - 0.5) * 30.0, out * (40.0 + r_spd * 60.0))
            }
        };

        let p = Particle { x, y, vx, vy, edge, size: 1.1 + r_size * 1.6, age: 0.0, alive: true };
        if let Some(slot) = self.particles.iter_mut().find(|p| !p.alive) {
            *slot = p;
        } else {
            self.particles.push(p);
        }
    }

    /// Advance the simulation and redraw. Safe to call at any frequency (dt-based).
    pub fn tick(&mut self, widget_hwnd: HWND) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().clamp(0.0, 0.05);
        self.last_tick = now;

        if !self.is_active() {
            if self.shown {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
                self.shown = false;
            }
            return;
        }
        if dt <= 0.0 {
            return;
        }

        // --- Emission ---
        if let Some(t) = self.target {
            for edge in [SNAP_LEFT, SNAP_RIGHT, SNAP_TOP, SNAP_BOTTOM] {
                if t.edges & edge == 0 {
                    continue;
                }
                let gap = Self::gap(&t, edge);
                if gap < 3.0 {
                    continue;
                }
                // Closer to the edge → denser stream (30..140 particles/s)
                let closeness = (1.0 - gap / t.threshold).clamp(0.0, 1.0);
                self.spawn_acc += dt * (30.0 + 110.0 * closeness);
                while self.spawn_acc >= 1.0 {
                    self.spawn_acc -= 1.0;
                    self.spawn(&t, edge);
                }
                let idx = Self::edge_index(edge);
                self.seam[idx] = self.seam[idx].max(0.25 + 0.55 * closeness);
            }
        }

        // --- Physics: gravity-like pull into the screen edge, lateral damping ---
        let lt = self.last_target;
        let mut absorbed = [0u32; 4];
        for p in self.particles.iter_mut().filter(|p| p.alive) {
            p.age = (p.age + dt * 6.0).min(1.0);
            let (dist, pull_x, pull_y) = match p.edge {
                SNAP_LEFT => (p.x - lt.work.left as f32, -1.0, 0.0),
                SNAP_RIGHT => (lt.work.right as f32 - p.x, 1.0, 0.0),
                SNAP_TOP => (p.y - lt.work.top as f32, 0.0, -1.0),
                _ => (lt.work.bottom as f32 - p.y, 0.0, 1.0),
            };
            // Acceleration grows as the particle nears the edge (magnetic feel)
            let accel = 900.0 + 5200.0 / (1.0 + dist.max(0.0) * 0.08);
            p.vx += pull_x * accel * dt;
            p.vy += pull_y * accel * dt;
            // Damp sideways motion so streams look like they're being sucked in
            if pull_x != 0.0 {
                p.vy *= 1.0 - (4.0 * dt).min(1.0);
            } else {
                p.vx *= 1.0 - (4.0 * dt).min(1.0);
            }
            p.x += p.vx * dt;
            p.y += p.vy * dt;

            let reached = match p.edge {
                SNAP_LEFT => p.x <= lt.work.left as f32,
                SNAP_RIGHT => p.x >= lt.work.right as f32,
                SNAP_TOP => p.y <= lt.work.top as f32,
                _ => p.y >= lt.work.bottom as f32,
            };
            if reached {
                p.alive = false;
                absorbed[Self::edge_index(p.edge)] += 1;
            }
        }
        for i in 0..4 {
            self.seam[i] = (self.seam[i] + absorbed[i] as f32 * 0.04).min(1.0);
            let decay = if self.target.map(|t| t.edges & [SNAP_LEFT, SNAP_RIGHT, SNAP_TOP, SNAP_BOTTOM][i] != 0).unwrap_or(false) {
                1.2
            } else {
                3.0
            };
            self.seam[i] = (self.seam[i] - decay * dt).max(0.0);
        }

        self.render(widget_hwnd);
    }

    /// Ensure the FX window exists and covers the widget plus a margin large enough to reach the edge.
    fn ensure_window(&mut self, widget_hwnd: HWND) -> bool {
        let t = self.last_target;
        let margin = (t.threshold + 24.0).ceil() as i32;
        let ww = t.widget.right - t.widget.left;
        let wh = t.widget.bottom - t.widget.top;
        let fw = (ww + margin * 2).max(1);
        let fh = (wh + margin * 2).max(1);
        let ox = t.widget.left - margin;
        let oy = t.widget.top - margin;

        unsafe {
            if self.hwnd.0.is_null() {
                if !register_class() {
                    return false;
                }
                let Ok(hinstance) = GetModuleHandleW(PCWSTR::null()) else { return false };
                let Ok(hwnd) = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    FX_CLASS_NAME,
                    w!("wallmus Snap FX"),
                    WS_POPUP,
                    ox,
                    oy,
                    fw,
                    fh,
                    HWND::default(),
                    None,
                    hinstance,
                    None,
                ) else {
                    return false;
                };
                self.hwnd = hwnd;
                self.hdc_screen = GetDC(HWND::default());
                self.hdc_mem = CreateCompatibleDC(self.hdc_screen);
            }

            if self.surface.width != fw as u32 || self.surface.height != fh as u32 || self.dib_bits.is_null() {
                if !self.hbitmap.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(self.hbitmap.0 as _));
                    self.hbitmap = HBITMAP::default();
                }
                let bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: fw,
                        biHeight: -fh,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
                let Ok(hbm) = CreateDIBSection(self.hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
                    return false;
                };
                SelectObject(self.hdc_mem, HGDIOBJ(hbm.0 as _));
                self.hbitmap = hbm;
                self.dib_bits = bits as *mut u32;
                self.surface = ArgbSurface::new(fw as u32, fh as u32);
            }
            self.origin = (ox, oy);

            // Keep directly beneath the widget so particles never paint over the card
            let _ = SetWindowPos(self.hwnd, widget_hwnd, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        true
    }

    fn render(&mut self, widget_hwnd: HWND) {
        if !self.ensure_window(widget_hwnd) {
            return;
        }
        let (ox, oy) = (self.origin.0 as f32, self.origin.1 as f32);
        let accent = self.accent;
        let lt = self.last_target;
        self.surface.clear();

        // Screen-edge seam glow (straight screen edge, spans the widget's facing side)
        let corner = lt.corner.max(6.0);
        for (i, edge) in [SNAP_LEFT, SNAP_RIGHT, SNAP_TOP, SNAP_BOTTOM].into_iter().enumerate() {
            let g = self.seam[i];
            if g <= 0.01 {
                continue;
            }
            let layers: [(f32, f32); 4] = [(10.0, 0.10), (6.0, 0.20), (3.0, 0.45), (1.5, 0.95)];
            for (thick, a) in layers {
                let alpha = (255.0 * a * g).clamp(0.0, 255.0) as u8;
                let col = premul(accent, alpha);
                match edge {
                    SNAP_LEFT | SNAP_RIGHT => {
                        let y0 = lt.widget.top as f32 + corner - oy;
                        let len = (lt.widget.bottom - lt.widget.top) as f32 - corner * 2.0;
                        let x = if edge == SNAP_LEFT { lt.work.left as f32 - ox } else { lt.work.right as f32 - ox - thick };
                        self.surface.draw_rounded_rect(x, y0, thick, len.max(4.0), thick * 0.5, col);
                    }
                    _ => {
                        let x0 = lt.widget.left as f32 + corner - ox;
                        let len = (lt.widget.right - lt.widget.left) as f32 - corner * 2.0;
                        let y = if edge == SNAP_TOP { lt.work.top as f32 - oy } else { lt.work.bottom as f32 - oy - thick };
                        self.surface.draw_rounded_rect(x0, y, len.max(4.0), thick, thick * 0.5, col);
                    }
                }
            }
        }

        // Particles: glowing head + motion-stretched tail
        for p in self.particles.iter().filter(|p| p.alive) {
            let px = p.x - ox;
            let py = p.y - oy;
            let speed = (p.vx * p.vx + p.vy * p.vy).sqrt().max(1.0);
            let tail = (speed * 0.018).clamp(2.0, 22.0);
            let (dx, dy) = (p.vx / speed, p.vy / speed);
            let fade = p.age;
            let a_head = (235.0 * fade) as u8;
            let a_tail = (90.0 * fade) as u8;
            // Tail: few segments fading out
            let steps = 4;
            for s in 1..=steps {
                let k = s as f32 / steps as f32;
                let tx = px - dx * tail * k;
                let ty = py - dy * tail * k;
                let a = (a_tail as f32 * (1.0 - k)) as u8;
                self.surface.draw_circle(tx, ty, (p.size * (1.0 - 0.5 * k)).max(0.6), premul(accent, a));
            }
            self.surface.draw_circle(px, py, p.size * 2.2, premul(accent, a_head / 5));
            self.surface.draw_circle(px, py, p.size, premul(lighten(accent), a_head));
        }

        self.commit();
    }

    fn commit(&mut self) {
        let w = self.surface.width;
        let h = self.surface.height;
        if self.dib_bits.is_null() || self.surface.pixels.len() != (w * h) as usize {
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(self.surface.pixels.as_ptr(), self.dib_bits, self.surface.pixels.len());
            let pt_dst = POINT { x: self.origin.0, y: self.origin.1 };
            let size = SIZE { cx: w as i32, cy: h as i32 };
            let pt_src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                self.hdc_screen,
                Some(&pt_dst),
                Some(&size),
                self.hdc_mem,
                Some(&pt_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            if !self.shown {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.shown = true;
            }
        }
    }
}

impl Default for SnapFx {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SnapFx {
    fn drop(&mut self) {
        unsafe {
            if !self.hwnd.0.is_null() {
                let _ = DestroyWindow(self.hwnd);
            }
            if !self.hbitmap.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(self.hbitmap.0 as _));
            }
            if !self.hdc_mem.is_invalid() {
                let _ = DeleteDC(self.hdc_mem);
            }
            if !self.hdc_screen.is_invalid() {
                let _ = ReleaseDC(HWND::default(), self.hdc_screen);
            }
        }
    }
}

fn premul(argb: u32, alpha: u8) -> u32 {
    ArgbSurface::premul_argb(alpha, ((argb >> 16) & 0xFF) as u8, ((argb >> 8) & 0xFF) as u8, (argb & 0xFF) as u8)
}

/// Brighter core color for particle heads.
fn lighten(argb: u32) -> u32 {
    let l = |c: u32| (c + (255 - c) / 2).min(255);
    0xFF00_0000 | (l((argb >> 16) & 0xFF) << 16) | (l((argb >> 8) & 0xFF) << 8) | l(argb & 0xFF)
}

unsafe extern "system" fn fx_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn register_class() -> bool {
    if FX_CLASS_REGISTERED.load(Ordering::SeqCst) {
        return true;
    }
    unsafe {
        let Ok(hinstance) = GetModuleHandleW(PCWSTR::null()) else { return false };
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            hInstance: hinstance.into(),
            lpszClassName: FX_CLASS_NAME,
            lpfnWndProc: Some(fx_wnd_proc),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
    }
    FX_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    true
}
