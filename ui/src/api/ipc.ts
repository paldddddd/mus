import { invoke } from '@tauri-apps/api/core';
import {
  AppConfig,
  BindingItem,
  TrackItem,
  WallpaperItem,
} from '../types';

// Check if running inside Tauri environment
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function requestDaemon<T>(method: string, params: unknown = null): Promise<T> {
  if (isTauri) {
    try {
      const res = await invoke<T>('daemon_request', { method, params });
      return res;
    } catch (err) {
      console.error(`[IPC Error] ${method}:`, err);
      throw new Error(String(err));
    }
  }

  // Fallback dev mock when running purely in vite dev browser
  return mockDaemonResponse<T>(method, params);
}

export async function setWidgetWindowVisible(visible: boolean): Promise<void> {
  if (isTauri) {
    await invoke('set_widget_visible', { visible });
  }
}

export async function setWidgetAlwaysOnTop(alwaysOnTop: boolean): Promise<void> {
  if (isTauri) {
    await invoke('set_widget_always_on_top', { alwaysOnTop });
  }
}

// In-memory mock store for preview testing in standard web browsers
const mockConfig: AppConfig = {
  autostart: true,
  language: 'ru',
  theme: 'dark',
  follow_music: true,
  active_wallpaper_id: 'wp_1',
  fps_limit: 30,
  governor: {
    enabled: true,
    pause_on_fullscreen: true,
    pause_on_desktop_covered: true,
    pause_on_lock: true,
    pause_on_display_off: true,
    pause_on_battery: false,
    unload_after_minutes: 5,
  },
  widget: {
    enabled: true,
    x: 100,
    y: 100,
    width: 380,
    height: 140,
    scale: 1,
    theme: 'spotify',
    pinned: false,
    interactive: true,
    visualizer_bars: 24,
    opacity: 0.95,
  },
  music: {
    folders: ['C:\\Music\\Favorites', 'D:\\Audio\\Hi-Res'],
    source: 'local',
  },
};

const mockWallpapers: WallpaperItem[] = [
  {
    id: 'wp_1',
    name: 'Cyberpunk Neon Alley',
    kind: 'video',
    source_path: 'C:\\Wallpapers\\cyberpunk.mp4',
    processed_path: 'C:\\Wallpapers\\cyberpunk.mp4',
    thumb_path: 'https://images.unsplash.com/photo-1518709268805-4e9042af9f23?w=400&q=80',
    fit: 'fill',
    width: 3840,
    height: 2160,
    fps_cap: 30,
    duration_ms: 18000,
    created_at: Date.now() - 100000,
  },
  {
    id: 'wp_2',
    name: 'Cosmic Nebula Horizon',
    kind: 'animated',
    source_path: 'C:\\Wallpapers\\nebula.mp4',
    processed_path: 'C:\\Wallpapers\\nebula.mp4',
    thumb_path: 'https://images.unsplash.com/photo-1451187580459-43490279c0fa?w=400&q=80',
    fit: 'fill',
    width: 2560,
    height: 1440,
    fps_cap: 30,
    duration_ms: 12000,
    created_at: Date.now() - 200000,
  },
  {
    id: 'wp_3',
    name: 'Quiet Japanese Rain',
    kind: 'image',
    source_path: 'C:\\Wallpapers\\japan_rain.jpg',
    processed_path: 'C:\\Wallpapers\\japan_rain.jpg',
    thumb_path: 'https://images.unsplash.com/photo-1503899036084-c55cdd92da26?w=400&q=80',
    fit: 'fill',
    width: 1920,
    height: 1080,
    created_at: Date.now() - 300000,
  },
];

let mockBindings: BindingItem[] = [
  {
    id: 1,
    scope: 'artist',
    key: 'queen',
    wallpaper_id: 'wp_1',
  },
  {
    id: 2,
    scope: 'album',
    key: 'pink floyd|the dark side of the moon',
    wallpaper_id: 'wp_2',
  },
];

const mockTracks: TrackItem[] = [
  {
    id: 'tr_1',
    path: 'C:\\Music\\Queen - Bohemian Rhapsody.flac',
    title: 'Bohemian Rhapsody',
    artist: 'Queen',
    album: 'A Night at the Opera',
    duration_ms: 354000,
    cover_path: 'https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=200&q=80',
    added_at: Date.now() - 50000,
  },
  {
    id: 'tr_2',
    path: 'C:\\Music\\Pink Floyd - Time.flac',
    title: 'Time',
    artist: 'Pink Floyd',
    album: 'The Dark Side of the Moon',
    duration_ms: 425000,
    cover_path: 'https://images.unsplash.com/photo-1511671782779-c97d3d27a1d4?w=200&q=80',
    added_at: Date.now() - 60000,
  },
  {
    id: 'tr_3',
    path: 'C:\\Music\\Daft Punk - Get Lucky.mp3',
    title: 'Get Lucky',
    artist: 'Daft Punk',
    album: 'Random Access Memories',
    duration_ms: 248000,
    cover_path: 'https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=200&q=80',
    added_at: Date.now() - 70000,
  },
];

function mockDaemonResponse<T>(method: string, params: any): T {
  switch (method) {
    case 'app.getState':
      return {
        running: true,
        is_paused: false,
        active_wallpaper: mockWallpapers[0],
        config: mockConfig,
      } as unknown as T;
    case 'wallpapers.list':
      return mockWallpapers as unknown as T;
    case 'bindings.list':
      return mockBindings as unknown as T;
    case 'bindings.set':
      mockBindings = mockBindings.filter(
        (b) => !(b.scope === params.scope && b.key === params.key)
      );
      mockBindings.push({
        id: Date.now(),
        scope: params.scope,
        key: params.key,
        wallpaper_id: params.wallpaper_id,
      });
      return { status: 'saved' } as unknown as T;
    case 'bindings.remove':
      if (params?.id) {
        mockBindings = mockBindings.filter((b) => b.id !== params.id);
      } else if (params?.scope && params?.key) {
        mockBindings = mockBindings.filter(
          (b) => !(b.scope === params.scope && b.key === params.key)
        );
      }
      return { deleted: true } as unknown as T;
    case 'bindings.getMode':
      return {
        follow_music: mockConfig.follow_music,
        mode: mockConfig.follow_music ? 'follow_music' : 'fixed',
      } as unknown as T;
    case 'bindings.setMode':
      mockConfig.follow_music = params.mode === 'follow_music';
      return {
        follow_music: mockConfig.follow_music,
        mode: params.mode,
      } as unknown as T;
    case 'tracks.list':
      return mockTracks as unknown as T;
    case 'playlists.list':
      return [] as unknown as T;
    case 'audio.getTrack':
      return {
        source: 'local',
        key: mockTracks[0].id,
        title: mockTracks[0].title,
        artist: mockTracks[0].artist,
        album_title: mockTracks[0].album,
        playback_status: 'Playing',
        position_ms: 120000,
        duration_ms: mockTracks[0].duration_ms,
        cover_path: mockTracks[0].cover_path,
      } as unknown as T;
    case 'audio.getSpectrum': {
      const bars = params?.bars || 24;
      const spectrum = Array.from({ length: bars }, (_, i) =>
        Math.max(0.05, Math.sin(Date.now() / 200 + i * 0.4) * 0.4 + 0.45)
      );
      return { spectrum } as unknown as T;
    }
    case 'diag.dump':
      return {
        os_product_name: 'Windows 11 Pro 24H2',
        os_build: '26100',
        os_ubr: 1742,
        topology: 'Raised',
        monitors: [
          {
            id: 'DISPLAY1',
            name: 'Generic PnP Monitor',
            is_primary: true,
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            dpi_x: 120,
            dpi_y: 120,
            scale_factor: 1.25,
          },
        ],
        gpu_adapter: 'NVIDIA GeForce RTX 4070 Laptop GPU',
        libmpv_version: 'libmpv 0.38.0',
      } as unknown as T;
    case 'hotkeys.get':
      return {
        hotkeys: mockConfig.hotkeys || {
          toggle_playback: 'Ctrl+Alt+Space',
          next_track: 'Ctrl+Alt+Right',
          prev_track: 'Ctrl+Alt+Left',
          toggle_wallpaper_pause: 'Ctrl+Alt+P',
          next_wallpaper: 'Ctrl+Alt+W',
          toggle_widget: 'Ctrl+Alt+M',
        },
        status: {
          toggle_playback: true,
          next_track: true,
          prev_track: true,
          toggle_wallpaper_pause: true,
          next_wallpaper: true,
          toggle_widget: true,
        },
      } as unknown as T;
    case 'hotkeys.set': {
      const p = params as { hotkeys: any };
      mockConfig.hotkeys = p.hotkeys;
      return {
        status: 'pending_reload',
        hotkeys: p.hotkeys,
      } as unknown as T;
    }
    case 'hotkeys.reset': {
      mockConfig.hotkeys = {
        toggle_playback: 'Ctrl+Alt+Space',
        next_track: 'Ctrl+Alt+Right',
        prev_track: 'Ctrl+Alt+Left',
        toggle_wallpaper_pause: 'Ctrl+Alt+P',
        next_wallpaper: 'Ctrl+Alt+W',
        toggle_widget: 'Ctrl+Alt+M',
      };
      return {
        status: 'reset',
        hotkeys: mockConfig.hotkeys,
      } as unknown as T;
    }
    default:
      return {} as unknown as T;
  }
}
