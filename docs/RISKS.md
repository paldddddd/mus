# Project Risks and Mitigations

| Risk | Impact | Likelihood | Mitigation |
|---|---|---|---|
| Windows 11 24H2+ shell window hierarchy changes (WorkerW / SHELLDLL_DefView layered under Progman) | Critical | High | Detect topology dynamically via window structure rather than build number. In `Raised` mode, attach to Progman directly or manage Z-order beneath `SHELLDLL_DefView`. |
| Explorer crash or restart (`explorer.exe`) | High | Medium | Register for `TaskbarCreated` message, detect invalid handles, and revalidate attachment seamlessly. |
| Monitor hotplug, resolution change, or DPI change | Medium | Medium | Handle `WM_DISPLAYCHANGE` and `WM_DPICHANGED`, adjust bounds using physical/DPI-aware coordinates. |
| libmpv embedding incompatibility with HWDEC or child window | High | Medium | Support fallback rendering modes (WIC static fallback, D3D11 swapchain, software/d3d11va options). |
| Resource consumption (CPU/GPU/RAM) while running games or full-screen apps | High | High | Implement Governor module with WinEvents and foreground checks to pause/unload rendering immediately. |
