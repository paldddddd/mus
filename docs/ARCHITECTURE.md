# Architecture: wallmus

## Overview
`wallmus` is a lightweight, high-performance Windows desktop live wallpaper engine with an integrated music player and dynamic wallpaper-to-track binding.

### Core Components
1. `wallmus.exe` (Rust daemon, runs permanently in background, no WebView/Chromium)
   - `desktop-host`: Manages host windows behind desktop icons (one per active monitor)
   - `render`: StaticImageRenderer (WIC/GDI/Direct2D), MpvRenderer (libmpv hwdec d3d11va), and future HtmlRenderer
   - `player`: rodio + symphonia audio playback and queue
   - `sources`: Local music library scanner + Windows SMTC session listener
   - `library`: SQLite database for tracks, playlists, wallpapers, and bindings
   - `governor`: Event-driven state manager (pauses/unloads rendering on full-screen apps, lock screen, battery save)
   - `tray`: muda + tray-icon system tray menu
   - `ipc-server`: Named Pipe server (`wallmus-ipc-v1-<SID>`) with JSON Lines protocol

2. `wallmus-ui.exe` (Tauri 2 + React + TypeScript + Vite + Tailwind CSS)
   - On-demand configuration UI (settings, wallpapers library, track table, bindings editor, monitor layout, performance stats)
   - Optional borderless floating music widget (IPC-driven)
