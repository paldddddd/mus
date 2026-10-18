use desktop_host::{detect_topology, enumerate_monitors, get_os_build};

#[test]
fn test_get_os_build() {
    let (major, _minor, build) = get_os_build();
    assert!(major >= 10, "Expected Windows 10 or newer, got {major}");
    assert!(build > 0, "Expected non-zero build number");
}

#[test]
fn test_detect_topology() {
    let topo = detect_topology();
    assert!(topo.is_ok(), "Topology detection failed: {:?}", topo.err());
    let info = topo.unwrap();
    println!("Detected topology: {:?}", info);
    assert!(info.progman_hwnd != 0, "Progman HWND must not be null");
    assert!(info.target_parent_hwnd != 0, "Target parent HWND must not be null");
}

#[test]
fn test_enumerate_monitors() {
    let monitors = enumerate_monitors();
    assert!(monitors.is_ok(), "Failed to enumerate monitors: {:?}", monitors.err());
    let list = monitors.unwrap();
    assert!(!list.is_empty(), "Must have at least one active monitor");
    for m in &list {
        assert!(m.width > 0, "Monitor width must be positive");
        assert!(m.height > 0, "Monitor height must be positive");
        assert!(m.dpi_x > 0, "Monitor DPI must be positive");
    }
}
