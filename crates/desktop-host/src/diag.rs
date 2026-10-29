use crate::monitors::enumerate_monitors;
use crate::topology::{collect_shell_windows, detect_topology, get_os_build};
use anyhow::Result;
use common::DiagReport;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

/// Collect full system diagnostics for desktop shell, monitors, and graphics.
pub fn collect_diag() -> Result<DiagReport> {
    let (major, minor, build) = get_os_build();
    let topo_info = detect_topology()?;
    let monitors = enumerate_monitors().unwrap_or_default();
    let shell_windows = collect_shell_windows();
    let gpu_adapter = query_primary_gpu();

    let os_product_name = if major == 10 && build >= 22000 {
        let release = if build >= 26100 {
            " 24H2"
        } else if build >= 22631 {
            " 23H2"
        } else if build >= 22621 {
            " 22H2"
        } else {
            " 21H2"
        };
        format!("Windows 11{} (Build {})", release, build)
    } else if major == 10 {
        format!("Windows 10 (Build {})", build)
    } else {
        format!("Windows {}.{} (Build {})", major, minor, build)
    };

    Ok(DiagReport {
        os_product_name,
        os_build: build.to_string(),
        os_ubr: 0,
        topology: topo_info.topology,
        monitors,
        shell_windows,
        gpu_adapter,
        libmpv_version: None,
    })
}

fn query_primary_gpu() -> Option<String> {
    unsafe {
        // SAFETY: DXGI factory query for adapter description
        let factory: Result<IDXGIFactory1, _> = CreateDXGIFactory1();
        if let Ok(factory) = factory {
            if let Ok(adapter) = factory.EnumAdapters1(0) {
                if let Ok(desc) = adapter.GetDesc1() {
                    let len = desc
                        .Description
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(desc.Description.len());
                    return Some(String::from_utf16_lossy(&desc.Description[..len]));
                }
            }
        }
    }
    None
}
