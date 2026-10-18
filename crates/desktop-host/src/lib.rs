//! Desktop host library for attaching live wallpaper windows behind desktop icons.

pub mod attach;
pub mod diag;
pub mod monitors;
pub mod process;
pub mod topology;
pub mod window;

pub use attach::{attach, detach, revalidate, AttachHandle};
pub use common::{MonitorInfo, Topology};
pub use diag::collect_diag;
pub use monitors::enumerate_monitors;
pub use process::apply_eco_profile;
pub use topology::{detect_topology, ensure_default_desktop, get_os_build, TopologyInfo};
pub use window::{DesktopHostWindow, RenderMode};
