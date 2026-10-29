use crate::{MpvRenderer, Renderer, StaticImageRenderer};
use anyhow::{Context, Result};
use common::{MonitorInfo, WallpaperItem};
use desktop_host::DesktopHostWindow;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Wrapper around thread-safe StaticImageRenderer to implement Renderer trait.
struct ImageRendererWrapper(Arc<Mutex<StaticImageRenderer>>);

impl Renderer for ImageRendererWrapper {
    fn load(&mut self, source: &str) -> Result<()> {
        self.0.lock().unwrap().load(source)
    }

    fn pause(&mut self) -> Result<()> {
        self.0.lock().unwrap().pause()
    }

    fn resume(&mut self) -> Result<()> {
        self.0.lock().unwrap().resume()
    }

    fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        self.0.lock().unwrap().resize(width, height)
    }

    fn unload(&mut self) -> Result<()> {
        self.0.lock().unwrap().unload()
    }

    fn update_mouse(&mut self, x: f32, y: f32) {
        let hwnd_val = if let Ok(mut g) = self.0.lock() {
            g.update_mouse(x, y);
            g.hwnd
        } else {
            0
        };
        if hwnd_val != 0 {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(
                    windows::Win32::Foundation::HWND(hwnd_val as _),
                );
            }
        }
    }
}

/// Active visual layer on the desktop host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveLayer {
    A,
    B,
}

/// Active crossfade transition state.
#[derive(Debug, Clone)]
pub struct FadeAnimation {
    pub start: Instant,
    pub duration: Duration,
}

/// Dual-layer host managing smooth crossfade transitions between live wallpapers.
///
/// Implements Option A (Dual-Layer Window Crossfade):
/// Two borderless layered windows (Layer A and Layer B) attached behind desktop icons.
/// When switching wallpapers, the incoming layer prepares the new video/image,
/// smoothly fades its opacity (0 -> 255) over the active layer using DWM hardware compositing,
/// and unloads the previous layer once the transition completes.
pub struct DualLayerHost {
    pub monitor: MonitorInfo,
    pub layer_a: Box<DesktopHostWindow>,
    pub layer_b: Box<DesktopHostWindow>,
    renderer_a: Option<Box<dyn Renderer>>,
    renderer_b: Option<Box<dyn Renderer>>,
    pub active_layer: ActiveLayer,
    fade_animation: Option<FadeAnimation>,
    pub current_wallpaper_id: Option<String>,
}

impl DualLayerHost {
    /// Create dual-layer host for a specific monitor.
    pub fn new(monitor: MonitorInfo) -> Result<Self> {
        let layer_a = DesktopHostWindow::create(monitor.clone(), desktop_host::RenderMode::StaticImage)
            .context("Failed to create DesktopHostWindow Layer A")?;
        let layer_b = DesktopHostWindow::create(monitor.clone(), desktop_host::RenderMode::StaticImage)
            .context("Failed to create DesktopHostWindow Layer B")?;

        let _ = layer_a.bring_above(layer_b.hwnd);
        let _ = layer_a.set_alpha(255);
        let _ = layer_b.set_alpha(0);

        Ok(Self {
            monitor,
            layer_a,
            layer_b,
            renderer_a: None,
            renderer_b: None,
            active_layer: ActiveLayer::A,
            fade_animation: None,
            current_wallpaper_id: None,
        })
    }

    /// Load initial wallpaper on the active layer without crossfade animation.
    pub fn load_initial(&mut self, wallpaper: &WallpaperItem) -> Result<()> {
        self.active_layer = ActiveLayer::A;
        let rend = setup_layer_renderer(&mut self.layer_a, &self.monitor, wallpaper)?;
        self.renderer_a = Some(rend);
        let _ = self.layer_a.bring_above(self.layer_b.hwnd);
        let _ = self.layer_a.set_alpha(255);
        let _ = self.layer_b.set_alpha(0);

        if let Some(ref mut b) = self.renderer_b {
            let _ = b.unload();
        }
        self.renderer_b = None;
        self.current_wallpaper_id = Some(wallpaper.id.clone());
        self.fade_animation = None;

        tracing::info!(
            "Loaded initial wallpaper '{}' ({}) on monitor {}",
            wallpaper.name,
            wallpaper.id,
            self.monitor.name
        );
        Ok(())
    }

    /// Initiate a smooth crossfade transition to a new wallpaper.
    pub fn start_transition(&mut self, wallpaper: &WallpaperItem, duration_ms: u64) -> Result<()> {
        if self.current_wallpaper_id.as_deref() == Some(&wallpaper.id) && self.fade_animation.is_none() {
            tracing::debug!("Wallpaper '{}' is already active; skipping transition", wallpaper.id);
            return Ok(());
        }

        // Instant transition if duration is 0
        if duration_ms == 0 {
            match self.active_layer {
                ActiveLayer::A => {
                    let rend = setup_layer_renderer(&mut self.layer_b, &self.monitor, wallpaper)?;
                    let _ = self.layer_b.bring_above(self.layer_a.hwnd);
                    let _ = self.layer_b.set_alpha(255);
                    if let Some(ref mut out) = self.renderer_a {
                        let _ = out.unload();
                    }
                    let _ = self.layer_a.set_alpha(0);
                    self.renderer_b = Some(rend);
                    self.active_layer = ActiveLayer::B;
                }
                ActiveLayer::B => {
                    let rend = setup_layer_renderer(&mut self.layer_a, &self.monitor, wallpaper)?;
                    let _ = self.layer_a.bring_above(self.layer_b.hwnd);
                    let _ = self.layer_a.set_alpha(255);
                    if let Some(ref mut out) = self.renderer_b {
                        let _ = out.unload();
                    }
                    let _ = self.layer_b.set_alpha(0);
                    self.renderer_a = Some(rend);
                    self.active_layer = ActiveLayer::A;
                }
            }
            self.current_wallpaper_id = Some(wallpaper.id.clone());
            self.fade_animation = None;
            return Ok(());
        }

        // Setup incoming layer with 0 alpha
        match self.active_layer {
            ActiveLayer::A => {
                let rend = setup_layer_renderer(&mut self.layer_b, &self.monitor, wallpaper)?;
                let _ = self.layer_b.bring_above(self.layer_a.hwnd);
                let _ = self.layer_b.set_alpha(0);
                self.renderer_b = Some(rend);
            }
            ActiveLayer::B => {
                let rend = setup_layer_renderer(&mut self.layer_a, &self.monitor, wallpaper)?;
                let _ = self.layer_a.bring_above(self.layer_b.hwnd);
                let _ = self.layer_a.set_alpha(0);
                self.renderer_a = Some(rend);
            }
        }

        self.current_wallpaper_id = Some(wallpaper.id.clone());
        self.fade_animation = Some(FadeAnimation {
            start: Instant::now(),
            duration: Duration::from_millis(duration_ms),
        });

        tracing::info!(
            "Started crossfade transition to '{}' ({} ms) on monitor {}",
            wallpaper.name,
            duration_ms,
            self.monitor.name
        );
        Ok(())
    }

    /// Update crossfade animation step. Returns true if animation is still progressing.
    pub fn update(&mut self) -> bool {
        let (start, duration) = match self.fade_animation {
            Some(ref fade) => (fade.start, fade.duration),
            None => return false,
        };

        let elapsed_ms = start.elapsed().as_millis() as f32;
        let total_ms = duration.as_millis().max(1) as f32;
        let t = (elapsed_ms / total_ms).clamp(0.0, 1.0);

        // Smoothstep interpolation for silky smooth easing: 3t^2 - 2t^3
        let eased_t = t * t * (3.0 - 2.0 * t);
        let alpha = (eased_t * 255.0).round() as u8;

        match self.active_layer {
            ActiveLayer::A => {
                let _ = self.layer_b.set_alpha(alpha);
            }
            ActiveLayer::B => {
                let _ = self.layer_a.set_alpha(alpha);
            }
        }

        if t >= 1.0 {
            // Transition complete
            match self.active_layer {
                ActiveLayer::A => {
                    let _ = self.layer_b.set_alpha(255);
                    if let Some(ref mut out) = self.renderer_a {
                        let _ = out.unload();
                    }
                    let _ = self.layer_a.set_alpha(0);
                    self.active_layer = ActiveLayer::B;
                }
                ActiveLayer::B => {
                    let _ = self.layer_a.set_alpha(255);
                    if let Some(ref mut out) = self.renderer_b {
                        let _ = out.unload();
                    }
                    let _ = self.layer_b.set_alpha(0);
                    self.active_layer = ActiveLayer::A;
                }
            }
            self.fade_animation = None;
            tracing::debug!("Crossfade transition completed on monitor {}", self.monitor.name);
            false
        } else {
            true
        }
    }

    /// Pause playback on active renderers.
    pub fn pause(&mut self) -> Result<()> {
        if let Some(ref mut a) = self.renderer_a {
            let _ = a.pause();
        }
        if let Some(ref mut b) = self.renderer_b {
            let _ = b.pause();
        }
        Ok(())
    }

    /// Resume playback on active renderers.
    pub fn resume(&mut self) -> Result<()> {
        match self.active_layer {
            ActiveLayer::A => {
                if let Some(ref mut a) = self.renderer_a {
                    let _ = a.resume();
                }
            }
            ActiveLayer::B => {
                if let Some(ref mut b) = self.renderer_b {
                    let _ = b.resume();
                }
            }
        }
        Ok(())
    }

    /// Unload all renderers and release memory.
    pub fn unload(&mut self) -> Result<()> {
        if let Some(ref mut a) = self.renderer_a {
            let _ = a.unload();
        }
        if let Some(ref mut b) = self.renderer_b {
            let _ = b.unload();
        }
        self.current_wallpaper_id = None;
        self.fade_animation = None;
        Ok(())
    }

    /// Revalidate attachment when desktop shell or monitors reconfigure.
    pub fn revalidate(&mut self) -> Result<()> {
        self.layer_a.revalidate()?;
        self.layer_b.revalidate()?;
        match self.active_layer {
            ActiveLayer::A => {
                let _ = self.layer_a.bring_above(self.layer_b.hwnd);
                if self.fade_animation.is_none() {
                    let _ = self.layer_a.set_alpha(255);
                    let _ = self.layer_b.set_alpha(0);
                }
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(self.layer_a.hwnd, None, false);
                    let _ = windows::Win32::Graphics::Gdi::UpdateWindow(self.layer_a.hwnd);
                }
            }
            ActiveLayer::B => {
                let _ = self.layer_b.bring_above(self.layer_a.hwnd);
                if self.fade_animation.is_none() {
                    let _ = self.layer_b.set_alpha(255);
                    let _ = self.layer_a.set_alpha(0);
                }
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(self.layer_b.hwnd, None, false);
                    let _ = windows::Win32::Graphics::Gdi::UpdateWindow(self.layer_b.hwnd);
                }
            }
        }
        Ok(())
    }

    /// Forward normalized mouse coordinates (0.0 .. 1.0) to active wallpaper renderer.
    pub fn update_mouse(&mut self, norm_x: f32, norm_y: f32) {
        if self.fade_animation.is_some() {
            if let Some(ref mut a) = self.renderer_a {
                a.update_mouse(norm_x, norm_y);
            }
            if let Some(ref mut b) = self.renderer_b {
                b.update_mouse(norm_x, norm_y);
            }
        } else {
            match self.active_layer {
                ActiveLayer::A => {
                    if let Some(ref mut a) = self.renderer_a {
                        a.update_mouse(norm_x, norm_y);
                    }
                }
                ActiveLayer::B => {
                    if let Some(ref mut b) = self.renderer_b {
                        b.update_mouse(norm_x, norm_y);
                    }
                }
            }
        }
    }

    /// Forward audio-reactive bass pulse (0.0 .. 1.0) to active wallpaper renderer.
    pub fn update_bass(&mut self, bass: f32) {
        if self.fade_animation.is_some() {
            if let Some(ref mut a) = self.renderer_a {
                a.update_bass(bass);
            }
            if let Some(ref mut b) = self.renderer_b {
                b.update_bass(bass);
            }
        } else {
            match self.active_layer {
                ActiveLayer::A => {
                    if let Some(ref mut a) = self.renderer_a {
                        a.update_bass(bass);
                    }
                }
                ActiveLayer::B => {
                    if let Some(ref mut b) = self.renderer_b {
                        b.update_bass(bass);
                    }
                }
            }
        }
    }
}

struct DummyRenderer;

impl Renderer for DummyRenderer {
    fn load(&mut self, _source: &str) -> Result<()> {
        Ok(())
    }
    fn pause(&mut self) -> Result<()> {
        Ok(())
    }
    fn resume(&mut self) -> Result<()> {
        Ok(())
    }
    fn resize(&mut self, _width: i32, _height: i32) -> Result<()> {
        Ok(())
    }
    fn unload(&mut self) -> Result<()> {
        Ok(())
    }
}

fn setup_layer_renderer(
    layer: &mut DesktopHostWindow,
    monitor: &MonitorInfo,
    wallpaper: &WallpaperItem,
) -> Result<Box<dyn Renderer>> {
    layer.clear_paint_callback();

    let path = std::path::Path::new(&wallpaper.processed_path);
    let is_web_url = wallpaper.processed_path.starts_with("http://") || wallpaper.processed_path.starts_with("https://");
    let is_builtin_shader = wallpaper.kind == "shader" && (wallpaper.processed_path.starts_with("builtin:") || wallpaper.processed_path.is_empty());

    if !is_web_url && !is_builtin_shader && !path.exists() {
        tracing::warn!("Wallpaper media file not found: {}", wallpaper.processed_path);
        return Ok(Box::new(DummyRenderer));
    }

    match wallpaper.kind.as_str() {
        "video" => {
            match MpvRenderer::new(layer.hwnd.0 as usize, monitor.width, monitor.height)
                .and_then(|mut mpv| {
                    mpv.load(&wallpaper.processed_path)?;
                    Ok(mpv)
                }) {
                Ok(mpv) => Ok(Box::new(mpv)),
                Err(e) => {
                    tracing::warn!(
                        "MpvRenderer failed for '{}' ({e:#}); falling back to static frame renderer",
                        wallpaper.name
                    );
                    let fallback_img = if std::path::Path::new(&wallpaper.thumb_path).exists() {
                        wallpaper.thumb_path.as_str()
                    } else {
                        wallpaper.processed_path.as_str()
                    };
                    let mut img = StaticImageRenderer::new(layer.hwnd, monitor.width, monitor.height);
                    let _ = img.load(fallback_img);
                    let img_arc = Arc::new(Mutex::new(img));
                    let img_cb = img_arc.clone();
                    layer.set_paint_callback(move |hdc, _w, _h| {
                        if let Ok(g) = img_cb.try_lock() {
                            g.paint(hdc);
                        }
                    });
                    unsafe {
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(layer.hwnd, None, false);
                        let _ = windows::Win32::Graphics::Gdi::UpdateWindow(layer.hwnd);
                    }
                    Ok(Box::new(ImageRendererWrapper(img_arc)))
                }
            }
        }
        "html" => {
            let mut html = crate::html::HtmlRenderer::new(layer.hwnd, monitor.width, monitor.height);
            html.load(&wallpaper.processed_path)?;
            Ok(Box::new(html))
        }
        "shader" => {
            let mut shader = crate::shader::ShaderRenderer::new(layer.hwnd, monitor.width, monitor.height);
            shader.load(&wallpaper.processed_path)?;
            Ok(Box::new(shader))
        }
        _ => {
            let mut img = StaticImageRenderer::new(layer.hwnd, monitor.width, monitor.height);
            img.load(&wallpaper.processed_path)?;
            let img_arc = Arc::new(Mutex::new(img));
            let img_cb = img_arc.clone();
            layer.set_paint_callback(move |hdc, _w, _h| {
                if let Ok(g) = img_cb.try_lock() {
                    g.paint(hdc);
                }
            });
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(layer.hwnd, None, false);
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(layer.hwnd);
            }
            Ok(Box::new(ImageRendererWrapper(img_arc)))
        }
    }
}
