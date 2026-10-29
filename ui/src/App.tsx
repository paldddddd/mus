import { useEffect, useState } from 'react';
import { Sidebar, NavTab } from './components/Sidebar';
import { PlayerBar } from './components/PlayerBar';
import { BindingsView } from './views/BindingsView';
import { WallpapersView } from './views/WallpapersView';
import { MusicView } from './views/MusicView';
import { CustomizationView } from './views/CustomizationView';
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
  const [shuffle, setShuffle] = useState<boolean>(false);
  const [repeatMode, setRepeatMode] = useState<'off' | 'all' | 'one'>('off');

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
      const [appState, wpList, bList, trList, diagReport, playerState] = await Promise.all([
        requestDaemon<any>('app.getState').catch(() => null),
        requestDaemon<WallpaperItem[]>('wallpapers.list').catch(() => []),
        requestDaemon<BindingItem[]>('bindings.list').catch(() => []),
        requestDaemon<TrackItem[]>('tracks.list').catch(() => []),
        requestDaemon<DiagReport>('diag.dump').catch(() => undefined),
        requestDaemon<any>('player.getState').catch(() => null),
      ]);

      if (appState?.config) {
        const mergedCfg: AppConfig = {
          ...appState.config,
          active_wallpaper_id:
            appState.config.active_wallpaper_id || appState.active_wallpaper?.id,
        };
        setConfig(mergedCfg);
        setLang((mergedCfg.language as Language) || 'ru');
        setTheme((mergedCfg.theme as 'dark' | 'light') || 'dark');
      }
      if (playerState) {
        if (typeof playerState.volume === 'number') {
          setVolume(playerState.volume);
        }
        if (typeof playerState.shuffle === 'boolean') {
          setShuffle(playerState.shuffle);
        }
        if (playerState.repeat_mode === 'all' || playerState.repeat_mode === 'one' || playerState.repeat_mode === 'off') {
          setRepeatMode(playerState.repeat_mode);
        }
      }
      setWallpapers(wpList || []);
      setBindings(bList || []);
      setTracks(trList || []);
      if (diagReport) setDiag(diagReport);
      setIsDaemonOnline(true);
    } catch (err) {
      console.warn('Daemon offline or unreachable:', err);
      setIsDaemonOnline(false);
    }
  };

  useEffect(() => {
    loadInitialData();
    const onDropped = () => {
      loadInitialData();
    };
    window.addEventListener('wallmus-files-dropped', onDropped);
    return () => window.removeEventListener('wallmus-files-dropped', onDropped);
  }, []);

  // Periodic Playback & Status Sync
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const cur = await requestDaemon<(CurrentTrackInfo & { active_wallpaper_id?: string }) | null>(
          'audio.getTrack'
        );
        if (cur) {
          setCurrentTrack(cur);
          if (cur.active_wallpaper_id) {
            setConfig((prev) =>
              prev && prev.active_wallpaper_id !== cur.active_wallpaper_id
                ? { ...prev, active_wallpaper_id: cur.active_wallpaper_id }
                : prev
            );
          }
        } else {
          setCurrentTrack(undefined);
        }
        setIsDaemonOnline(true);
      } catch (err) {
        setIsDaemonOnline(false);
      }
    }, 400);

    return () => clearInterval(interval);
  }, []);

  // Actions
  const handleSelectLanguage = (nextLang: Language) => {
    setLang(nextLang);
    requestDaemon('settings.setGeneral', { language: nextLang }).catch(() => {});
  };

  const handleToggleLang = () => {
    const nextLang: Language = lang === 'ru' ? 'en' : 'ru';
    handleSelectLanguage(nextLang);
  };

  const handleSelectTheme = (nextTheme: 'dark' | 'light') => {
    setTheme(nextTheme);
    requestDaemon('settings.setGeneral', { theme: nextTheme }).catch(() => {});
  };

  const handleToggleTheme = () => {
    const nextTheme = theme === 'dark' ? 'light' : 'dark';
    handleSelectTheme(nextTheme);
  };

  const handleSetBinding = async (scope: string, key: string, wallpaperId: string) => {
    await requestDaemon('bindings.set', {
      scope,
      key,
      wallpaper_id: wallpaperId,
    });
    const updated = await requestDaemon<BindingItem[]>('bindings.list').catch(() => []);
    setBindings(updated || []);
  };

  const handleRemoveBinding = async (scope: string, key: string) => {
    await requestDaemon('bindings.remove', { scope, key });
    const updated = await requestDaemon<BindingItem[]>('bindings.list').catch(() => []);
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

  const handleImportWallpaper = async (path: string): Promise<WallpaperItem | null> => {
    try {
      const imported = await requestDaemon<WallpaperItem>('wallpapers.import', {
        path,
        copy: true,
      });
      const updated = await requestDaemon<WallpaperItem[]>('wallpapers.list').catch(() => []);
      setWallpapers(updated || []);
      return imported || null;
    } catch (err: any) {
      console.error('Failed to import wallpaper:', err);
      alert(
        lang === 'ru'
          ? `Ошибка импорта обоев: ${err?.message || err}`
          : `Failed to import wallpaper: ${err?.message || err}`
      );
      return null;
    }
  };

  const handlePickWallpaper = async (): Promise<WallpaperItem | null> => {
    const res = await requestDaemon<{ path: string | null }>('app.pickWallpaperFile');
    if (res?.path) {
      return await handleImportWallpaper(res.path);
    }
    return null;
  };

  const handlePickAudioFile = async (): Promise<TrackItem | null> => {
    const res = await requestDaemon<{ path: string | null }>('app.pickAudioFile');
    if (res?.path) {
      const imported = await requestDaemon<TrackItem>('library.importFile', {
        path: res.path,
      });
      const updated = await requestDaemon<TrackItem[]>('tracks.list').catch(() => []);
      setTracks(updated || []);
      return imported || null;
    }
    return null;
  };

  const handlePickMusicFolder = async (): Promise<void> => {
    const res = await requestDaemon<{ path: string | null }>('app.pickMusicFolder');
    if (res?.path) {
      await handleAddFolder(res.path);
      await handleScanNow();
    }
  };

  const handleDeleteWallpaper = async (id: string) => {
    await requestDaemon('wallpapers.delete', { id }).catch(() => {});
    const updated = await requestDaemon<WallpaperItem[]>('wallpapers.list').catch(() => []);
    setWallpapers(updated || []);
  };

  const handleDeleteTrack = async (id: string) => {
    await requestDaemon('tracks.delete', { id }).catch(() => {});
    const [updatedTracks, updatedBindings] = await Promise.all([
      requestDaemon<TrackItem[]>('tracks.list').catch(() => []),
      requestDaemon<BindingItem[]>('bindings.list').catch(() => []),
    ]);
    setTracks(updatedTracks || []);
    setBindings(updatedBindings || []);
    const cur = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack').catch(() => null);
    setCurrentTrack(cur || undefined);
  };

  const handleSetSource = async (source: MusicSource) => {
    await requestDaemon('settings.setMusic', { source });
    if (config) {
      setConfig({ ...config, music: { ...config.music, source } });
    }
  };

  const handleAddFolder = async (folderPath: string) => {
    const curFolders = config?.music?.folders || [];
    if (curFolders.includes(folderPath)) return;
    const folders = [...curFolders, folderPath];
    await requestDaemon('settings.setMusic', { folders });
    if (config) {
      setConfig({ ...config, music: { ...config.music, folders } });
    }
  };

  const handleScanNow = async () => {
    await requestDaemon('library.scan', {}).catch(() => {});
    const updated = await requestDaemon<TrackItem[]>('tracks.list').catch(() => []);
    setTracks(updated || []);
  };

  const handlePlayTrack = async (track: TrackItem) => {
    await requestDaemon('player.playFile', {
      id: track.id,
      path: track.path,
      title: track.title || 'Untitled Track',
      artist: track.artist || 'Unknown Artist',
    });
    if (config && config.music.source !== 'local') {
      setConfig({ ...config, music: { ...config.music, source: 'local' } });
    }
    const cur = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack').catch(() => null);
    if (cur) setCurrentTrack(cur);
  };

  const handleTogglePlay = async () => {
    await requestDaemon('audio.control', { action: 'toggle' }).catch(() => {});
    const cur = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack').catch(() => null);
    setCurrentTrack(cur || undefined);
  };

  const handleStopTrack = async () => {
    await requestDaemon('audio.control', { action: 'stop' }).catch(() => {});
    await requestDaemon('player.stop', {}).catch(() => {});
    const cur = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack').catch(() => null);
    setCurrentTrack(cur || undefined);
  };

  const handleNextTrack = async () => {
    await requestDaemon('audio.control', { action: 'next' }).catch(() => {});
    const cur = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack').catch(() => null);
    if (cur) setCurrentTrack(cur);
  };

  const handlePrevTrack = async () => {
    await requestDaemon('audio.control', { action: 'previous' }).catch(() => {});
    const cur = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack').catch(() => null);
    if (cur) setCurrentTrack(cur);
  };

  const handleSeek = async (posMs: number) => {
    await requestDaemon('player.seek', { position_ms: posMs }).catch(() => {});
    const cur = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack').catch(() => null);
    if (cur) setCurrentTrack(cur);
  };

  const handleVolumeChange = async (vol: number) => {
    setVolume(vol);
    await requestDaemon('player.setVolume', { volume: vol }).catch(() => {});
  };

  const handleToggleShuffle = async () => {
    const next = !shuffle;
    setShuffle(next);
    await requestDaemon('player.setShuffle', { shuffle: next }).catch(() => {});
  };

  const handleCycleRepeat = async () => {
    const next: 'off' | 'all' | 'one' =
      repeatMode === 'off' ? 'all' : repeatMode === 'all' ? 'one' : 'off';
    setRepeatMode(next);
    await requestDaemon('player.setRepeat', { mode: next }).catch(() => {});
  };

  const handleToggleAutostart = async (enabled: boolean) => {
    await requestDaemon('settings.setAutostart', { enabled });
    if (config) {
      setConfig({ ...config, autostart: enabled });
    }
  };

  const handleUpdateGovernor = async (newGov: GovernorConfig) => {
    await requestDaemon('governor.setConfig', { governor: newGov }).catch(() => {});
    if (config) {
      setConfig({ ...config, governor: newGov });
    }
  };

  const handleUpdateFpsLimit = async (fps: number) => {
    await requestDaemon('settings.setFpsLimit', { fps_limit: fps }).catch(() => {});
    if (config) {
      setConfig({ ...config, fps_limit: fps });
    }
  };

  const handleSetWallpaperChangeMode = async (
    mode: 'random' | 'sequential' | 'off',
    respectBindings?: boolean
  ) => {
    const respect = respectBindings ?? config?.respect_bindings_in_auto_change ?? false;
    await requestDaemon('wallpapers.setChangeMode', { mode, respect_bindings: respect }).catch(() => {});
    if (config) {
      setConfig({
        ...config,
        wallpaper_change_mode: mode,
        respect_bindings_in_auto_change: respect,
      });
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
              currentTrack={currentTrack}
              activeWallpaperId={config?.active_wallpaper_id}
              onSetBinding={handleSetBinding}
              onRemoveBinding={handleRemoveBinding}
              onSetActiveWallpaper={handleSetActiveWallpaper}
              onPickWallpaper={handlePickWallpaper}
              onPickAudioFile={handlePickAudioFile}
              onPickMusicFolder={handlePickMusicFolder}
              onDeleteTrack={handleDeleteTrack}
              onPlayTrack={handlePlayTrack}
              onTogglePlay={handleTogglePlay}
              onStopTrack={handleStopTrack}
              onNextTrack={handleNextTrack}
              onPrevTrack={handlePrevTrack}
              onSeek={handleSeek}
              volume={volume}
              onVolumeChange={handleVolumeChange}
              followMusic={config?.follow_music ?? true}
              onToggleFollowMusic={handleToggleFollowMusic}
              lang={lang}
            />
          )}

          {currentTab === 'wallpapers' && (
            <WallpapersView
              wallpapers={wallpapers}
              activeWallpaperId={config?.active_wallpaper_id}
              wallpaperChangeMode={config?.wallpaper_change_mode || 'random'}
              respectBindings={config?.respect_bindings_in_auto_change ?? false}
              onSetChangeMode={handleSetWallpaperChangeMode}
              onSetActiveWallpaper={handleSetActiveWallpaper}
              onImportWallpaper={async (p) => {
                await handleImportWallpaper(p);
              }}
              onDeleteWallpaper={handleDeleteWallpaper}
              lang={lang}
            />
          )}

          {currentTab === 'music' && (
            <MusicView
              tracks={tracks}
              folders={config?.music?.folders || []}
              activeSource={config?.music?.source || 'local'}
              currentTrack={currentTrack}
              onSetSource={handleSetSource}
              onAddFolder={handleAddFolder}
              onPickAudioFile={handlePickAudioFile}
              onPlayTrack={handlePlayTrack}
              onTogglePlay={handleTogglePlay}
              onStopTrack={handleStopTrack}
              onScanNow={handleScanNow}
              lang={lang}
            />
          )}

          {currentTab === 'customization' && (
            <CustomizationView
              monitors={diag?.monitors || []}
              lang={lang}
              wallpaperChangeMode={config?.wallpaper_change_mode || 'random'}
              respectBindings={config?.respect_bindings_in_auto_change ?? false}
              onSetChangeMode={handleSetWallpaperChangeMode}
            />
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
              fpsLimit={config?.fps_limit ?? 30}
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
              onSelectLanguage={handleSelectLanguage}
              theme={theme}
              onSelectTheme={handleSelectTheme}
              onOpenLogs={handleOpenLogs}
            />
          )}
        </div>

        {/* Bottom Persistent Playback Bar */}
        <PlayerBar
          currentTrack={currentTrack}
          isPlaying={currentTrack?.playback_status === 'Playing'}
          onTogglePlay={handleTogglePlay}
          onStop={handleStopTrack}
          onNext={handleNextTrack}
          onPrevious={handlePrevTrack}
          onSeek={handleSeek}
          onVolumeChange={handleVolumeChange}
          volume={volume}
          shuffle={shuffle}
          onToggleShuffle={handleToggleShuffle}
          repeatMode={repeatMode}
          onCycleRepeat={handleCycleRepeat}
          lang={lang}
        />
      </main>
    </div>
  );
}

export default App;
