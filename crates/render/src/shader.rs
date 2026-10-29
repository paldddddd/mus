use crate::Renderer;
use anyhow::{Context, Result};
use raw_window_handle::{
    HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    Win32WindowHandle, WindowsDisplayHandle,
};
use std::fs;
use std::num::NonZeroIsize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

pub struct HwndWrapper(pub HWND);

unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, HandleError> {
        let non_zero = NonZeroIsize::new(self.0.0 as isize).ok_or(HandleError::Unavailable)?;
        let handle = Win32WindowHandle::new(non_zero);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(RawWindowHandle::Win32(handle))) }
    }
}

impl HasDisplayHandle for HwndWrapper {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, HandleError> {
        unsafe { Ok(raw_window_handle::DisplayHandle::borrow_raw(RawDisplayHandle::Windows(WindowsDisplayHandle::new()))) }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShaderUniforms {
    pub time: f32,
    pub width: f32,
    pub height: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub mouse_x: f32,
    pub mouse_y: f32,
}

pub const DEFAULT_WGSL_CYBER_TUNNEL: &str = r#"
struct Uniforms {
    time: f32,
    width: f32,
    height: f32,
    bass: f32,
    mid: f32,
    treble: f32,
    mouse_x: f32,
    mouse_y: f32,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(in_vertex_index) / 2) * 4.0 - 1.0;
    let y = f32(i32(in_vertex_index) % 2) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 1.0 - (y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let resolution = vec2<f32>(u.width, u.height);
    // Mouse subtly steers perspective towards cursor
    let mouse_offset = (vec2<f32>(u.mouse_x, u.mouse_y) * 2.0 - 1.0) * 0.35;
    let p = (in.uv * 2.0 - 1.0) * vec2<f32>(resolution.x / resolution.y, 1.0) - mouse_offset;

    let r = length(p) + 0.001;
    let theta = atan2(p.y, p.x);

    // Audio-reactive tunnel coordinates
    let speed = u.time * (0.8 + u.bass * 1.5);
    let u_coord = 0.5 / r + speed;
    let v_coord = (theta / 3.14159265) * 4.0;

    let grid_x = abs(fract(u_coord * 4.0) - 0.5);
    let grid_y = abs(fract(v_coord) - 0.5);
    let line = smoothstep(0.42, 0.49, max(grid_x, grid_y));

    // Cyberpunk neon colors
    let cyan = vec3<f32>(0.02, 0.71, 0.83);
    let violet = vec3<f32>(0.54, 0.17, 0.89);
    let pink = vec3<f32>(0.92, 0.28, 0.60);

    let color_mix = mix(cyan, violet, sin(speed + theta * 2.0) * 0.5 + 0.5);
    let pulse_color = mix(color_mix, pink, u.bass);

    let depth_fade = smoothstep(2.5, 0.1, r);
    let final_rgb = pulse_color * (line * (1.2 + u.mid * 2.0)) * depth_fade;

    return vec4<f32>(final_rgb, 1.0);
}
"#;

pub const DEFAULT_WGSL_AURORA_WAVES: &str = r#"
struct Uniforms {
    time: f32,
    width: f32,
    height: f32,
    bass: f32,
    mid: f32,
    treble: f32,
    mouse_x: f32,
    mouse_y: f32,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(in_vertex_index) / 2) * 4.0 - 1.0;
    let y = f32(i32(in_vertex_index) % 2) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 1.0 - (y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time * 0.4;
    let mouse = vec2<f32>(u.mouse_x, u.mouse_y);
    let mouse_dist = length(uv - mouse);

    var color = vec3<f32>(0.02, 0.04, 0.08); // Deep midnight blue base

    // Interactive ripple wave propagating from cursor
    let mouse_ripple = sin(mouse_dist * 20.0 - t * 5.0) * exp(-mouse_dist * 5.0) * 0.06;

    // Multi-layer aurora ribbons
    for (var i: i32 = 1; i <= 4; i = i + 1) {
        let fi = f32(i);
        let wave = sin(uv.x * (2.0 + fi * 1.5) + t * (0.8 + fi * 0.2)) * 0.15
                 + cos(uv.x * (4.0 + fi) - t * 0.5) * 0.08
                 + mouse_ripple;
        let baseline = 0.35 + fi * 0.12 + wave + (u.bass * 0.1);
        let dist = abs(uv.y - baseline);
        let glow = exp(-dist * (18.0 - fi * 2.0));

        let aurora_green = vec3<f32>(0.11, 0.85, 0.45);
        let aurora_cyan = vec3<f32>(0.05, 0.75, 0.95);
        let layer_color = mix(aurora_green, aurora_cyan, sin(t + fi) * 0.5 + 0.5);

        color = color + layer_color * glow * (0.7 + u.mid * 0.8);
    }

    // Soft cursor illumination aura
    let aura = exp(-mouse_dist * 12.0) * vec3<f32>(0.15, 0.85, 0.75) * 0.5;
    color = color + aura;

    return vec4<f32>(color, 1.0);
}
"#;

/// Native WGSL shader wallpaper renderer powered by WebGPU (Direct3D 12 / Vulkan).
pub struct ShaderRenderer {
    hwnd: HWND,
    width: u32,
    height: u32,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    audio_bass: Arc<AtomicU32>,
    audio_mid: Arc<AtomicU32>,
    audio_treble: Arc<AtomicU32>,
    mouse_x: Arc<AtomicU32>,
    mouse_y: Arc<AtomicU32>,
    render_thread: Option<std::thread::JoinHandle<()>>,
    current_source: Option<String>,
}

unsafe impl Send for ShaderRenderer {}
unsafe impl Sync for ShaderRenderer {}

impl ShaderRenderer {
    /// Create a new WGSL shader wallpaper renderer.
    pub fn new(hwnd: HWND, width: i32, height: i32) -> Self {
        Self {
            hwnd,
            width: width.max(1) as u32,
            height: height.max(1) as u32,
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            audio_bass: Arc::new(AtomicU32::new(0)),
            audio_mid: Arc::new(AtomicU32::new(0)),
            audio_treble: Arc::new(AtomicU32::new(0)),
            mouse_x: Arc::new(AtomicU32::new(0.5f32.to_bits())),
            mouse_y: Arc::new(AtomicU32::new(0.5f32.to_bits())),
            render_thread: None,
            current_source: None,
        }
    }

    /// Update audio spectrum uniform values (0.0 .. 1.0).
    pub fn update_audio(&self, bass: f32, mid: f32, treble: f32) {
        self.audio_bass.store(bass.to_bits(), Ordering::Relaxed);
        self.audio_mid.store(mid.to_bits(), Ordering::Relaxed);
        self.audio_treble.store(treble.to_bits(), Ordering::Relaxed);
    }

    fn start_render_loop(&mut self, wgsl_source: String) -> Result<()> {
        let params = ShaderRenderParams {
            hwnd_val: self.hwnd.0 as usize,
            width: self.width,
            height: self.height,
            wgsl_source,
            running: self.running.clone(),
            paused: self.paused.clone(),
            audio_bass: self.audio_bass.clone(),
            audio_mid: self.audio_mid.clone(),
            audio_treble: self.audio_treble.clone(),
            mouse_x: self.mouse_x.clone(),
            mouse_y: self.mouse_y.clone(),
        };

        self.running.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);

        let handle = std::thread::Builder::new()
            .name("wallmus-shader-render".to_string())
            .spawn(move || {
                if let Err(e) = run_wgpu_loop(params) {
                    tracing::error!("WGSL shader render thread error: {e}");
                }
            })
            .context("Failed to spawn shader render thread")?;

        self.render_thread = Some(handle);
        Ok(())
    }
}

struct ShaderRenderParams {
    hwnd_val: usize,
    width: u32,
    height: u32,
    wgsl_source: String,
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    audio_bass: Arc<AtomicU32>,
    audio_mid: Arc<AtomicU32>,
    audio_treble: Arc<AtomicU32>,
    mouse_x: Arc<AtomicU32>,
    mouse_y: Arc<AtomicU32>,
}

impl Renderer for ShaderRenderer {
    fn load(&mut self, source: &str) -> Result<()> {
        self.unload()?;

        let wgsl_code = if Path::new(source).exists() {
            fs::read_to_string(source).context("Failed to read WGSL shader file")?
        } else if source.contains("aurora") {
            DEFAULT_WGSL_AURORA_WAVES.to_string()
        } else if source.contains("@fragment") || source.contains("fn fs_main") {
            source.to_string()
        } else {
            DEFAULT_WGSL_CYBER_TUNNEL.to_string()
        };

        self.start_render_loop(wgsl_code)?;
        self.current_source = Some(source.to_string());
        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        self.paused.store(true, Ordering::SeqCst);
        tracing::debug!("Paused WGSL shader on HWND 0x{:X}", self.hwnd.0 as usize);
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        self.paused.store(false, Ordering::SeqCst);
        tracing::debug!("Resumed WGSL shader on HWND 0x{:X}", self.hwnd.0 as usize);
        Ok(())
    }

    fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        self.width = width.max(1) as u32;
        self.height = height.max(1) as u32;
        // Reload shader with updated dimensions
        if let Some(src) = self.current_source.clone() {
            let _ = self.load(&src);
        }
        Ok(())
    }

    fn unload(&mut self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.render_thread.take() {
            let _ = handle.join();
        }
        self.current_source = None;
        Ok(())
    }

    fn update_mouse(&mut self, x: f32, y: f32) {
        self.mouse_x.store(x.to_bits(), Ordering::Relaxed);
        self.mouse_y.store(y.to_bits(), Ordering::Relaxed);
    }

    fn update_bass(&mut self, bass: f32) {
        self.audio_bass.store(bass.to_bits(), Ordering::Relaxed);
    }
}

impl Drop for ShaderRenderer {
    fn drop(&mut self) {
        let _ = self.unload();
    }
}

fn run_wgpu_loop(params: ShaderRenderParams) -> Result<()> {
    let instance = wgpu::Instance::default();

    let non_zero = NonZeroIsize::new(params.hwnd_val as isize)
        .context("Invalid HWND for WGPU surface")?;
    let handle = Win32WindowHandle::new(non_zero);

    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_window_handle: RawWindowHandle::Win32(handle),
            raw_display_handle: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
        })
    }
    .context("Failed to create WGPU surface from HWND")?;

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .context("Failed to find suitable WGPU adapter")?;

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("wallmus Shader Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .context("Failed to request WGPU device")?;

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps
        .formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(surface_caps.formats[0]);

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: params.width,
        height: params.height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Wallpaper WGSL Shader"),
        source: wgpu::ShaderSource::Wgsl(params.wgsl_source.into()),
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Shader Uniform Buffer"),
        size: std::mem::size_of::<ShaderUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Uniform Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Uniform Bind Group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shader Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Shader Render Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let start_time = Instant::now();

    while params.running.load(Ordering::Relaxed) {
        if params.paused.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        let time = start_time.elapsed().as_secs_f32();
        let bass = f32::from_bits(params.audio_bass.load(Ordering::Relaxed));
        let mid = f32::from_bits(params.audio_mid.load(Ordering::Relaxed));
        let treble = f32::from_bits(params.audio_treble.load(Ordering::Relaxed));
        let mouse_x = f32::from_bits(params.mouse_x.load(Ordering::Relaxed));
        let mouse_y = f32::from_bits(params.mouse_y.load(Ordering::Relaxed));

        let uniforms = ShaderUniforms {
            time,
            width: params.width as f32,
            height: params.height as f32,
            bass,
            mid,
            treble,
            mouse_x,
            mouse_y,
        };

        queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        match surface.get_current_texture() {
            Ok(output) => {
                let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Shader Command Encoder"),
                });

                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Shader Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    rpass.set_pipeline(&render_pipeline);
                    rpass.set_bind_group(0, &bind_group, &[]);
                    rpass.draw(0..3, 0..1);
                }

                queue.submit(std::iter::once(encoder.finish()));
                output.present();
            }
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                surface.configure(&device, &config);
            }
            Err(wgpu::SurfaceError::Timeout) => {}
            Err(e) => {
                tracing::warn!("WGPU surface render error: {e}");
            }
        }

        // Smooth ~60 FPS frame pacing alongside PresentMode::Fifo
        std::thread::sleep(Duration::from_millis(14));
    }

    Ok(())
}
