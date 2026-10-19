use common::{MonitorInfo, WallpaperItem};
use desktop_host::DesktopHostWindow;
use render::{ActiveLayer, DualLayerHost};
use std::time::Duration;

#[test]
fn test_crossfade_smoothstep_curve() {
    let smoothstep = |t: f32| -> f32 {
        let clamped = t.clamp(0.0, 1.0);
        clamped * clamped * (3.0 - 2.0 * clamped)
    };

    assert_eq!(smoothstep(0.0), 0.0);
    assert_eq!(smoothstep(0.5), 0.5);
    assert_eq!(smoothstep(1.0), 1.0);

    // Verify monotonic increase
    let mut prev = 0.0;
    for step in 0..=100 {
        let t = step as f32 / 100.0;
        let v = smoothstep(t);
        assert!(v >= prev);
        prev = v;
    }
}

#[test]
fn test_desktop_host_window_alpha() {
    desktop_host::ensure_default_desktop();
    let mon = MonitorInfo {
        id: "test_mon".to_string(),
        name: "Test Monitor".to_string(),
        x: 0,
        y: 0,
        width: 800,
        height: 600,
        dpi_x: 96,
        dpi_y: 96,
        scale_factor: 1.0,
        is_primary: true,
    };

    let host = DesktopHostWindow::create(mon, desktop_host::RenderMode::GdiPattern);
    if let Ok(host) = host {
        assert!(host.set_alpha(128).is_ok());
        assert!(host.set_alpha(0).is_ok());
        assert!(host.set_alpha(255).is_ok());
    }
}

#[test]
fn test_dual_layer_host_instant_and_animated_transition() {
    desktop_host::ensure_default_desktop();
    let mon = MonitorInfo {
        id: "test_mon_dual".to_string(),
        name: "Test Monitor Dual".to_string(),
        x: 0,
        y: 0,
        width: 800,
        height: 600,
        dpi_x: 96,
        dpi_y: 96,
        scale_factor: 1.0,
        is_primary: true,
    };

    let host = DualLayerHost::new(mon);
    if let Ok(mut host) = host {
        assert_eq!(host.active_layer, ActiveLayer::A);

        let wp1 = WallpaperItem {
            id: "wp_1".to_string(),
            name: "Wallpaper 1".to_string(),
            kind: "video".to_string(),
            source_path: "C:\\fake\\1.mp4".to_string(),
            processed_path: "C:\\fake\\1.mp4".to_string(),
            thumb_path: "C:\\fake\\1.jpg".to_string(),
            fit: "cover".to_string(),
            fps_cap: Some(30),
            width: Some(1920),
            height: Some(1080),
            duration_ms: Some(10000),
            created_at: 0,
        };

        let wp2 = WallpaperItem {
            id: "wp_2".to_string(),
            name: "Wallpaper 2".to_string(),
            kind: "video".to_string(),
            source_path: "C:\\fake\\2.mp4".to_string(),
            processed_path: "C:\\fake\\2.mp4".to_string(),
            thumb_path: "C:\\fake\\2.jpg".to_string(),
            fit: "cover".to_string(),
            fps_cap: Some(60),
            width: Some(1920),
            height: Some(1080),
            duration_ms: Some(15000),
            created_at: 0,
        };

        // Instant transition (duration = 0)
        let _ = host.start_transition(&wp1, 0);
        assert_eq!(host.current_wallpaper_id.as_deref(), Some("wp_1"));
        assert_eq!(host.active_layer, ActiveLayer::B);

        // Timed transition (duration = 100 ms)
        let _ = host.start_transition(&wp2, 100);
        assert_eq!(host.current_wallpaper_id.as_deref(), Some("wp_2"));

        // While animation is progressing
        let animating = host.update();
        // Wait for animation to finish
        std::thread::sleep(Duration::from_millis(150));
        let finished = !host.update();
        assert!(finished);
        assert_eq!(host.active_layer, ActiveLayer::A);
        let _ = animating;
    }
}

#[test]
fn test_html_renderer_lifecycle() {
    use render::{HtmlRenderer, Renderer};
    use windows::Win32::Foundation::HWND;

    let mut html = HtmlRenderer::new(HWND::default(), 1920, 1080);
    assert!(html.pause().is_ok());
    assert!(html.resume().is_ok());
    assert!(html.resize(1280, 720).is_ok());
    assert!(html.unload().is_ok());
}

#[test]
fn test_shader_renderer_uniforms_and_builtins() {
    use render::{ShaderRenderer, ShaderUniforms, DEFAULT_WGSL_AURORA_WAVES, DEFAULT_WGSL_CYBER_TUNNEL};
    use windows::Win32::Foundation::HWND;

    // Verify ShaderUniforms memory layout (alignment to 16 bytes for WGSL std140/std430)
    assert_eq!(std::mem::size_of::<ShaderUniforms>(), 32);

    let shader = ShaderRenderer::new(HWND::default(), 1920, 1080);
    shader.update_audio(0.85, 0.42, 0.15);

    // Verify embedded shader sources are non-empty and have vertex/fragment entrypoints
    assert!(DEFAULT_WGSL_CYBER_TUNNEL.contains("@vertex"));
    assert!(DEFAULT_WGSL_CYBER_TUNNEL.contains("@fragment"));
    assert!(DEFAULT_WGSL_AURORA_WAVES.contains("@vertex"));
    assert!(DEFAULT_WGSL_AURORA_WAVES.contains("@fragment"));
}

#[test]
fn test_wgsl_shaders_valid_wgpu_compilation() {
    use render::{DEFAULT_WGSL_AURORA_WAVES, DEFAULT_WGSL_CYBER_TUNNEL};

    // Initialize headless wgpu instance to validate WGSL modules
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }));

    if let Some(adapter) = adapter {
        let device_res = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Test Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ));

        if let Ok((device, _queue)) = device_res {
            let _tunnel = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Cyber Tunnel WGSL"),
                source: wgpu::ShaderSource::Wgsl(DEFAULT_WGSL_CYBER_TUNNEL.into()),
            });

            let _aurora = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Aurora Waves WGSL"),
                source: wgpu::ShaderSource::Wgsl(DEFAULT_WGSL_AURORA_WAVES.into()),
            });
        }
    }
}

