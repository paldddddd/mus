# Testing Matrix

## Environments and Test Matrix

| Windows Version | Monitors & DPI | Desktop Shell | Stage 0 (Attach/Z-order) | Win+D Resilience | Explorer Restart | Lock/Unlock | Fullscreen App | Status |
|---|---|---|---|---|---|---|---|---|
| Windows 11 Build 26300 (26H2 / 24H2+ architecture) | 1 monitor (1920x1080 @ 120 DPI, 1.25x scale) | Progman with layered SHELLDLL_DefView (Raised topology) | Verified (GDI, WIC, libmpv) | Ready for user verification | Handled (TaskbarCreated hook) | Handled (revalidate hook) | Stage 1 (Governor) | **Passed Stage 0 Automated** |
| Windows 10 22H2 | N/A | WorkerW behind SHELLDLL_DefView (Classic) | Implemented, not verified on physical hardware | Untested | Untested | Untested | Untested | Untested |
| Windows 11 < 24H2 | N/A | WorkerW behind SHELLDLL_DefView (Classic) | Implemented, not verified on physical hardware | Untested | Untested | Untested | Untested | Untested |

## Stage 0 Performance Benchmark Results

- **Static Wallpaper (WIC)**:
  - Avg CPU: 0.13%
  - Avg RAM (RSS): 89.8 MB
  - GPU: 0% active drawing once painted
- **Video Wallpaper (libmpv 1080p, hwdec d3d11va)**:
  - Avg CPU: 0.07% (target budget: <3%)
  - Avg RAM (RSS): 38.97 MB (target budget: <150 MB)
  - Avg GPU Util: 0.7% (NVIDIA GeForce RTX 3050 Laptop GPU)
