import { useEffect, useState } from 'react';
import { Sidebar, NavTab } from './components/Sidebar';
import { PlayerBar } from './components/PlayerBar';
import { BindingsView } from './views/BindingsView';
import { WallpapersView } from './views/WallpapersView';
import { MusicView } from './views/MusicView';
import { MonitorsView } from './views/MonitorsView';
import { PerformanceView } from './views/PerformanceView';
import { SettingsView } from './views/SettingsView';
import { WidgetView } from './views/WidgetView';
import { Language } from './i18n';
import {
  AppConfig,
  BindingItem,
  CurrentTrackInfo,
  DiagReport,
  GovernorConfig,
  MusicSource,
  TrackItem,
  WallpaperItem,
} from './types';
import { requestDaemon } from './api/ipc';

export function App() {
  // Check if this instance is running as the floating desktop widget window
  const isWidgetWindow =
    window.location.hash === '#widget' || window.location.search.includes('view=widget');

  if (isWidgetWindow) {
    return <WidgetView />;
  }

  // Main UI State
  const [currentTab, setCurrentTab] = useState<NavTab>('bindings');
  const [lang, setLang] = useState<Language>('ru');
  const [theme, setTheme] = useState<'dark' | 'light'>('dark');
  const [isDaemonOnline, setIsDaemonOnline] = useState(true);

  const [wallpapers, setWallpapers] = useState<WallpaperItem[]>([]);
  const [tracks, setTracks] = useState<TrackItem[]>([]);
  const [bindings, setBindings] = useState<BindingItem[]>([]);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [diag, setDiag] = useState<DiagReport | undefined>(undefined);
  const [currentTrack, setCurrentTrack] = useState<CurrentTrackInfo | undefined>(undefined);
  const [volume, setVolume] = useState<number>(0.8);

  // Sync theme class to body
  useEffect(() => {
    if (theme === 'light') {
      document.body.classList.add('theme-light');
    } else {
      document.body.classList.remove('theme-light');
    }
  }, [theme]);

  // Initial Data Fetch from Daemon
  const loadInitialData = async () => {
    try {
      const [appState, wpList, bList, trList, diagReport] = await Promise.all([
        requestDaemon<any>('app.getState'),
        requestDaemon<WallpaperItem[]>('wallpapers.list'),
        requestDaemon<BindingItem[]>('bindings.list'),
        requestDaemon<TrackItem[]>('tracks.list').catch(() => []),
        requestDaemon<DiagReport>('diag.dump').catch(() => undefined),
      ]);

      if (appState?.config) {
        setConfig(appState.config);
        setLang((appState.config.language as Language) || 'ru');
        setTheme((appState.config.theme as 'dark' | 'light') || 'dark');
      }
      setWallpapers(wpList || []);
      setBindings(bList || []);
      setTracks(trList || []);
      setDiag(diagReport);
      setIsDaemonOnline(true);
    } catch (err) {
      console.warn('Daemon offline or unreachable:', err);
      setIsDaemonOnline(false);
    }
  };

  useEffect(() => {
    loadInitialData();
  }, []);

  // Periodic Playback & Status Sync
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const cur = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack');
        if (cur) {
          setCurrentTrack(cur);
        }
        setIsDaemonOnline(true);
      } catch (err) {
        setIsDaemonOnline(false);
      }
    }, 500);

    return () => clearInterval(interval);
  }, []);

  // Actions
  const handleToggleLang = () => {
    setLang((prev) => (prev === 'ru' ? 'en' : 'ru'));
  };

  const handleToggleTheme = () => {
    setTheme((prev) => (prev === 'dark' ? 'light' : 'dark'));
  };

  const handleSetBinding = async (scope: string, key: string, wallpaperId: string) => {
    await requestDaemon('bindings.set', {
      scope,
      key,
      wallpaper_id: wallpaperId,
    });
    const updated = await requestDaemon<BindingItem[]>('bindings.list');
    setBindings(updated || []);
  };

  const handleRemoveBinding = async (scope: string, key: string) => {
    await requestDaemon('bindings.remove', { scope, key });
    const updated = await requestDaemon<BindingItem[]>('bindings.list');
    setBindings(updated || []);
  };

  const handleToggleFollowMusic = async () => {
    const nextMode = !config?.follow_music;
    await requestDaemon('bindings.setMode', { follow_music: nextMode });
    if (config) {
      setConfig({ ...config, follow_music: nextMode });
    }
  };

  const handleSetActiveWallpaper = async (id: string) => {
    await requestDaemon('wallpapers.setActive', { id });
    if (config) {
      setConfig({ ...config, active_wallpaper_id: id });
    }
  };

  const handleImportWallpaper = async (path: string) => {
    await requestDaemon('wallpapers.import', { path, copy: true });
    const updated = await requestDaemon<WallpaperItem[]>('wallpapers.list');
    setWallpapers(updated || []);
  };

  const handleDeleteWallpaper = async (id: string) => {
    await requestDaemon('wallpapers.delete', { id }).catch(() => {});
    const updated = await requestDaemon<WallpaperItem[]>('wallpapers.list');
    setWallpapers(updated || []);
  };

  const handleSetSource = async (source: MusicSource) => {
    await requestDaemon('settings.setMusic', { source });
    if (config) {
      setConfig({ ...config, music: { ...config.music, source } });
    }
  };

  const handleAddFolder = async (folderPath: string) => {
    if (!config) return;
    const folders = [...config.music.folders, folderPath];
    await requestDaemon('settings.setMusic', { folders });
    setConfig({ ...config, music: { ...config.music, folders } });
  };

  const handleScanNow = async () => {
    await requestDaemon('library.scan', {}).catch(() => {});
    const updated = await requestDaemon<TrackItem[]>('tracks.list').catch(() => []);
    setTracks(updated || []);
  };

  const handlePlayTrack = async (track: TrackItem) => {
    await requestDaemon('player.playFile', {
      path: track.path,
      title: track.title,
      artist: track.artist,
    });
  };

  const handleTogglePlay = async () => {
    await requestDaemon('audio.control', { action: 'toggle' });
  };

  const handleNextTrack = async () => {
    await requestDaemon('audio.control', { action: 'next' });
  };

  const handlePrevTrack = async () => {
    await requestDaemon('audio.control', { action: 'previous' });
  };

  const handleSeek = async (posMs: number) => {
    await requestDaemon('player.seek', { position_ms: posMs }).catch(() => {});
  };

  const handleVolumeChange = async (vol: number) => {
    setVolume(vol);
    await requestDaemon('player.setVolume', { volume: vol }).catch(() => {});
  };

  const handleToggleAutostart = async (enabled: boolean) => {
    await requestDaemon('settings.setAutostart', { enabled });
    if (config) {
      setConfig({ ...config, autostart: enabled });
    }
  };

  const handleUpdateGovernor = async (newGov: GovernorConfig) => {
    if (config) {
      setConfig({ ...config, governor: newGov });
    }
  };

  const handleUpdateFpsLimit = async (fps: number) => {
    if (config) {
      setConfig({ ...config, fps_limit: fps });
    }
  };

  const handleOpenLogs = async () => {
    await requestDaemon('app.openLogs', {}).catch(() => {});
  };

  return (
    <div className="app-container">
      {/* Sidebar Navigation */}
      <Sidebar
        currentTab={currentTab}
        onSelectTab={setCurrentTab}
        lang={lang}
        onToggleLang={handleToggleLang}
        theme={theme}
        onToggleTheme={handleToggleTheme}
        isDaemonOnline={isDaemonOnline}
      />

      {/* Main View Area */}
      <main className="main-wrapper">
        <div className="view-content">
          {currentTab === 'bindings' && (
            <BindingsView
              tracks={tracks}
              wallpapers={wallpapers}
              bindings={bindings}
              onSetBinding={handleSetBinding}
              onRemoveBinding={handleRemoveBinding}
              followMusic={config?.follow_music ?? true}
              onToggleFollowMusic={handleToggleFollowMusic}
              lang={lang}
            />
          )}

          {currentTab === 'wallpapers' && (
            <WallpapersView
              wallpapers={wallpapers}
              activeWallpaperId={config?.active_wallpaper_id}
              onSetActiveWallpaper={handleSetActiveWallpaper}
              onImportWallpaper={handleImportWallpaper}
              onDeleteWallpaper={handleDeleteWallpaper}
              lang={lang}
            />
          )}

          {currentTab === 'music' && (
            <MusicView
              tracks={tracks}
              folders={config?.music?.folders || []}
              activeSource={config?.music?.source || 'local'}
              onSetSource={handleSetSource}
              onAddFolder={handleAddFolder}
              onPlayTrack={handlePlayTrack}
              onScanNow={handleScanNow}
              lang={lang}
            />
          )}

          {currentTab === 'monitors' && (
            <MonitorsView monitors={diag?.monitors || []} lang={lang} />
          )}

          {currentTab === 'performance' && (
            <PerformanceView
              governorConfig={
                config?.governor || {
                  enabled: true,
                  pause_on_fullscreen: true,
                  pause_on_desktop_covered: true,
                  pause_on_lock: true,
                  pause_on_display_off: true,
                  pause_on_battery: false,
                  unload_after_minutes: 5,
                }
              }
              onUpdateGovernor={handleUpdateGovernor}
              fpsLimit={config?.fps_limit || 30}
              onUpdateFpsLimit={handleUpdateFpsLimit}
              diag={diag}
              lang={lang}
            />
          )}

          {currentTab === 'settings' && (
            <SettingsView
              autostart={config?.autostart || false}
              onToggleAutostart={handleToggleAutostart}
              lang={lang}
              onSelectLanguage={setLang}
              theme={theme}
              onSelectTheme={setTheme}
              onOpenLogs={handleOpenLogs}
            />
          )}
        </div>

        {/* Bottom Persistent Playback Bar */}
        <PlayerBar
          currentTrack={currentTrack}
          isPlaying={currentTrack?.playback_status === 'Playing'}
          onTogglePlay={handleTogglePlay}
          onNext={handleNextTrack}
          onPrevious={handlePrevTrack}
          onSeek={handleSeek}
          onVolumeChange={handleVolumeChange}
          volume={volume}
        />
      </main>
    </div>
  );
}

export default App;
