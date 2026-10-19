export type WallpaperKind = 'image' | 'animated' | 'video' | 'html' | 'shader';

export interface WallpaperItem {
  id: string;
  name: string;
  kind: WallpaperKind;
  source_path: string;
  processed_path: string;
  thumb_path: string;
  fit: string;
  fps_cap?: number;
  width?: number;
  height?: number;
  duration_ms?: number;
  created_at: number;
}

export interface TrackItem {
  id: string;
  path: string;
  title?: string;
  artist?: string;
  album?: string;
  duration_ms?: number;
  cover_path?: string;
  added_at: number;
}

export type BindingScope = 'track' | 'album' | 'artist' | 'playlist' | 'external';

export interface BindingItem {
  id?: number;
  scope: BindingScope;
  key: string;
  wallpaper_id: string;
}

export interface PlaylistItem {
  id: number;
  name: string;
  track_ids: string[];
}

export type MusicSource = 'local' | 'external';

export interface GovernorConfig {
  enabled: boolean;
  pause_on_fullscreen: boolean;
  pause_on_desktop_covered: boolean;
  pause_on_lock: boolean;
  pause_on_display_off: boolean;
  pause_on_battery: boolean;
  unload_after_minutes: number;
}

export interface WidgetConfig {
  enabled: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
  theme: string;
  pinned: boolean;
  interactive: boolean;
  visualizer_bars: number;
  opacity: number;
}

export interface MusicConfig {
  folders: string[];
  source: MusicSource;
}

export interface HotkeysConfig {
  toggle_playback: string;
  next_track: string;
  prev_track: string;
  toggle_wallpaper_pause: string;
  next_wallpaper: string;
  toggle_widget: string;
}

export interface WallpaperVisualizerConfig {
  enabled: boolean;
  style: 'bars' | 'wave' | 'glow';
  bars_count: number;
  height_px: number;
  opacity: number;
  accent_color: 'spotify' | 'cyan_purple' | 'neon_blue' | 'fire';
}

export interface AppConfig {
  autostart: boolean;
  language: string;
  theme: string;
  follow_music: boolean;
  active_wallpaper_id?: string;
  fps_limit: number;
  crossfade_duration_ms?: number;
  governor: GovernorConfig;
  widget: WidgetConfig;
  music: MusicConfig;
  hotkeys?: HotkeysConfig;
  visualizer?: WallpaperVisualizerConfig;
}

export interface MonitorInfo {
  id: string;
  name: string;
  is_primary: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  dpi_x: number;
  dpi_y: number;
  scale_factor: number;
}

export interface DiagReport {
  os_product_name: string;
  os_build: string;
  os_ubr: number;
  topology: string | { Unknown: string };
  monitors: MonitorInfo[];
  gpu_adapter?: string;
  libmpv_version?: string;
}

export interface CurrentTrackInfo {
  source: 'local' | 'external';
  key: string;
  title?: string;
  artist?: string;
  album_title?: string;
  album_artist?: string;
  playback_status?: string;
  position_ms?: number;
  duration_ms?: number;
  cover_path?: string;
}

export interface UpdateInfo {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  release_notes: string;
  download_url?: string;
  published_at?: string;
  sha256?: string;
}
