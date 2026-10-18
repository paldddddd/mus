use anyhow::{Context, Result};
use desktop_host::{collect_diag, enumerate_monitors, DesktopHostWindow, RenderMode};
use std::env;
use std::fs::File;
use std::io::Write;
use std::panic;
use std::time::{Duration, Instant};
use tracing::info;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
};

fn main() -> Result<()> {
    // 1. Setup panic hook to log panics gracefully as required by section 5.6
    panic::set_hook(Box::new(|panic_info| {
        eprintln!("CRITICAL PANIC: {:?}", panic_info);
    }));

    // 2. Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = env::args().collect();

    if args.iter().any(|arg| arg == "--diag") {
        return run_diag();
    }

    if let Some(pos) = args.iter().position(|arg| arg == "--image") {
        if let Some(image_path) = args.get(pos + 1) {
            return run_image_test(image_path, 10);
        } else {
            eprintln!("Error: --image requires a file path");
            std::process::exit(1);
        }
    }

    if let Some(pos) = args.iter().position(|arg| arg == "--video") {
        if let Some(video_path) = args.get(pos + 1) {
            return run_video_test(video_path, 10);
        } else {
            eprintln!("Error: --video requires a file path");
            std::process::exit(1);
        }
    }

    if args.iter().any(|arg| arg == "--selftest") {
        return run_selftest(10);
    }

    // Default: run selftest for 10 seconds if run interactively, or help
    println!("wallmus - Desktop Live Wallpaper Engine");
    println!("Usage:");
    println!("  wallmus.exe --diag            Print diagnostic report & save diag_report.json");
    println!("  wallmus.exe --selftest        Run 10-second GDI pattern desktop attachment self-test");
    println!("  wallmus.exe --image <path>    Run 10-second WIC static image wallpaper test");
    println!("  wallmus.exe --video <path>    Run 10-second libmpv video wallpaper test");
    println!("Running --selftest by default...");
    run_selftest(10)
}

fn run_diag() -> Result<()> {
    info!("Collecting system diagnostics...");
    let mut diag = collect_diag().context("Failed to collect diagnostics")?;
    diag.libmpv_version = render::get_mpv_version();

    println!("============================================================");
    println!("                  WALLMUS DIAGNOSTIC REPORT                 ");
    println!("============================================================");
    println!("OS Product Name : {}", diag.os_product_name);
    println!("OS Build        : {}", diag.os_build);
    println!("Shell Topology  : {:?}", diag.topology);
    println!(
        "GPU Adapter     : {}",
        diag.gpu_adapter.as_deref().unwrap_or("Unknown")
    );
    println!(
        "libmpv Version  : {}",
        diag.libmpv_version.as_deref().unwrap_or("Not loaded")
    );
    println!("\nActive Monitors ({}):", diag.monitors.len());
    for (i, m) in diag.monitors.iter().enumerate() {
        println!(
            "  [{}] {} | Primary: {} | Rect: [{}, {} to {}, {}] ({}x{}) | DPI: {}x{} | Scale: {:.2}x",
            i, m.name, m.is_primary, m.x, m.y, m.x + m.width, m.y + m.height, m.width, m.height, m.dpi_x, m.dpi_y, m.scale_factor
        );
    }

    println!("\nShell Windows Tree ({}):", diag.shell_windows.len());
    for w in &diag.shell_windows {
        println!(
            "  HWND: 0x{:08X} | Parent: 0x{:08X} | Class: {:<18} | Visible: {:<5} | Rect: {:?}",
            w.hwnd, w.parent_hwnd, w.class_name, w.is_visible, w.rect
        );
    }
    println!("============================================================");

    // Save to JSON
    let json_data = serde_json::to_string_pretty(&diag).context("Failed to serialize diag to JSON")?;
    let mut file = File::create("diag_report.json").context("Failed to create diag_report.json")?;
    file.write_all(json_data.as_bytes())
        .context("Failed to write diag_report.json")?;
    info!("Diagnostics saved to diag_report.json");

    Ok(())
}

fn run_selftest(duration_secs: u64) -> Result<()> {
    info!("Starting wallmus Stage 0 desktop attachment self-test ({} seconds)...", duration_secs);

    let monitors = enumerate_monitors().context("Failed to enumerate monitors")?;
    if monitors.is_empty() {
        anyhow::bail!("No active monitors found!");
    }

    info!("Found {} active monitor(s)", monitors.len());

    let mut hosts = Vec::new();
    for monitor in monitors {
        info!(
            "Creating desktop host window for monitor: {} ({}x{} at {},{})",
            monitor.name, monitor.width, monitor.height, monitor.x, monitor.y
        );
        let host = DesktopHostWindow::create(monitor, RenderMode::GdiPattern)
            .context("Failed to create DesktopHostWindow")?;
        hosts.push(host);
    }

    info!("Host windows attached behind desktop icons! Testing Win32 message pump...");
    let start_time = Instant::now();
    let max_duration = Duration::from_secs(duration_secs);

    unsafe {
        let mut msg = MSG::default();
        while start_time.elapsed() < max_duration {
            // Non-blocking peek message to handle redraws, taskbar events, and display changes
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    info!("Received WM_QUIT during self-test");
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    info!("Self-test duration elapsed. Cleaning up and detaching host windows...");
    drop(hosts);
    info!("Cleanup complete. Desktop restored to original state.");

    Ok(())
}

fn run_image_test(image_path: &str, duration_secs: u64) -> Result<()> {
    use render::{Renderer, StaticImageRenderer};
    use std::sync::{Arc, Mutex};

    info!(
        "Starting wallmus Stage 0 WIC static image test with: {} ({} seconds)...",
        image_path, duration_secs
    );

    let monitors = enumerate_monitors().context("Failed to enumerate monitors")?;
    if monitors.is_empty() {
        anyhow::bail!("No active monitors found!");
    }

    let mut hosts = Vec::new();
    for monitor in monitors {
        let mut host = DesktopHostWindow::create(monitor.clone(), RenderMode::StaticImage)
            .context("Failed to create DesktopHostWindow")?;

        let mut renderer = StaticImageRenderer::new(host.hwnd, monitor.width, monitor.height);
        renderer
            .load(image_path)
            .with_context(|| format!("Failed to load static image: {}", image_path))?;

        let renderer_arc = Arc::new(Mutex::new(renderer));
        host.set_paint_callback(move |hdc, _w, _h| {
            if let Ok(guard) = renderer_arc.lock() {
                guard.paint(hdc);
            }
        });

        hosts.push(host);
    }

    info!("WIC static image attached behind desktop icons! Message pump active...");
    let start_time = Instant::now();
    let max_duration = Duration::from_secs(duration_secs);

    unsafe {
        let mut msg = MSG::default();
        while start_time.elapsed() < max_duration {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    info!("Received WM_QUIT during image test");
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    info!("Image test elapsed. Detaching host windows...");
    drop(hosts);
    info!("Desktop restored to original state.");

    Ok(())
}

fn run_video_test(video_path: &str, duration_secs: u64) -> Result<()> {
    use render::{MpvRenderer, Renderer};

    info!(
        "Starting wallmus Stage 0 libmpv video test with: {} ({} seconds)...",
        video_path, duration_secs
    );

    let monitors = enumerate_monitors().context("Failed to enumerate monitors")?;
    if monitors.is_empty() {
        anyhow::bail!("No active monitors found!");
    }

    let mut hosts = Vec::new();
    let mut renderers = Vec::new();

    for monitor in monitors {
        let host = DesktopHostWindow::create(monitor.clone(), RenderMode::Video)
            .context("Failed to create DesktopHostWindow")?;

        let mut renderer = MpvRenderer::new(host.hwnd.0 as usize, monitor.width, monitor.height)
            .context("Failed to initialize MpvRenderer")?;
        renderer
            .load(video_path)
            .with_context(|| format!("Failed to load video file: {}", video_path))?;

        hosts.push(host);
        renderers.push(renderer);
    }

    info!("libmpv video attached behind desktop icons! Message pump active...");
    let start_time = Instant::now();
    let max_duration = Duration::from_secs(duration_secs);

    unsafe {
        let mut msg = MSG::default();
        while start_time.elapsed() < max_duration {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    info!("Received WM_QUIT during video test");
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    info!("Video test elapsed. Cleaning up renderers and detaching host windows...");
    drop(renderers);
    drop(hosts);
    info!("Desktop restored to original state.");

    Ok(())
}
