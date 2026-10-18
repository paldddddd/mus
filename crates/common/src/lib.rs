//! Common types, error definitions, and IPC contracts for wallmus.

use serde::{Deserialize, Serialize};

/// High-level topology classification of the Windows desktop window hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Topology {
    /// Windows 10 and Windows 11 prior to 24H2.
    /// SHELLDLL_DefView resides inside a WorkerW window spawned by 0x052C message to Progman.
    Classic,
    /// Windows 11 24H2 and newer.
    /// Progman is created with WS_EX_NOREDIRECTIONBITMAP and SHELLDLL_DefView is a layered child of Progman.
    Raised,
    /// Unknown window hierarchy configuration.
    Unknown(String),
}

/// Information about a connected display monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub dpi_x: u32,
    pub dpi_y: u32,
    pub scale_factor: f64,
}

/// OS and environment diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagReport {
    pub os_product_name: String,
    pub os_build: String,
    pub os_ubr: u32,
    pub topology: Topology,
    pub monitors: Vec<MonitorInfo>,
    pub shell_windows: Vec<WindowTreeItem>,
    pub gpu_adapter: Option<String>,
    pub libmpv_version: Option<String>,
}

/// Summary of a shell window in the desktop tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowTreeItem {
    pub hwnd: usize,
    pub parent_hwnd: usize,
    pub class_name: String,
    pub window_text: String,
    pub style: u32,
    pub ex_style: u32,
    pub is_visible: bool,
    pub rect: [i32; 4],
}
