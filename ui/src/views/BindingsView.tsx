import React, { useEffect, useState } from 'react';
import {
  Link2,
  Search,
  Check,
  Trash2,
  Music2,
  Disc,
  User,
  ToggleLeft,
  ToggleRight,
  Plus,
  FolderPlus,
  ImagePlus,
  Play,
  Pause,
  Square,
  SkipBack,
  SkipForward,
  Volume2,
  VolumeX,
  Radio,
  Monitor,
} from 'lucide-react';
import { BindingItem, CurrentTrackInfo, TrackItem, WallpaperItem } from '../types';
import { Language, translations } from '../i18n';

interface BindingsViewProps {
  tracks: TrackItem[];
  wallpapers: WallpaperItem[];
  bindings: BindingItem[];
  currentTrack?: CurrentTrackInfo;
  activeWallpaperId?: string;
  onSetBinding: (scope: string, key: string, wallpaperId: string) => Promise<void>;
  onRemoveBinding: (scope: string, key: string) => Promise<void>;
  onSetActiveWallpaper: (id: string) => Promise<void>;
  onPickWallpaper: () => Promise<WallpaperItem | null>;
  onPickAudioFile: () => Promise<TrackItem | null>;
  onPickMusicFolder: () => Promise<void>;
  onDeleteTrack?: (id: string) => Promise<void>;
  onPlayTrack: (track: TrackItem) => Promise<void>;
  onTogglePlay: () => Promise<void>;
  onStopTrack: () => Promise<void>;
  onNextTrack: () => Promise<void>;
  onPrevTrack: () => Promise<void>;
  onSeek: (posMs: number) => Promise<void>;
  volume: number;
  onVolumeChange: (vol: number) => Promise<void>;
  followMusic: boolean;
  onToggleFollowMusic: () => Promise<void>;
  lang: Language;
}

export const BindingsView: React.FC<BindingsViewProps> = ({
  tracks,
  wallpapers,
  bindings,
  currentTrack,
  activeWallpaperId,
  onSetBinding,
  onRemoveBinding,
  onSetActiveWallpaper,
  onPickWallpaper,
  onPickAudioFile,
  onPickMusicFolder,
  onDeleteTrack,
  onPlayTrack,
  onTogglePlay,
  onStopTrack,
  onNextTrack,
  onPrevTrack,
  onSeek,
  volume,
  onVolumeChange,
  followMusic,
  onToggleFollowMusic,
  lang,
}) => {
  const t = translations[lang];

  const [searchQuery, setSearchQuery] = useState('');
  const [filterUnboundOnly, setFilterUnboundOnly] = useState(false);
  const [selectedTrack, setSelectedTrack] = useState<TrackItem | null>(tracks[0] || null);
  const [useExternalCurrent, setUseExternalCurrent] = useState<boolean>(false);
  const [selectedWallpaperId, setSelectedWallpaperId] = useState<string>(
    wallpapers[0]?.id || ''
  );
  const [statusToast, setStatusToast] = useState<string | null>(null);

  useEffect(() => {
    if (!selectedWallpaperId && wallpapers.length > 0) {
      setSelectedWallpaperId(activeWallpaperId || wallpapers[0].id);
    }
  }, [wallpapers, activeWallpaperId]);

  useEffect(() => {
    if (!selectedTrack && tracks.length > 0 && !useExternalCurrent) {
      setSelectedTrack(tracks[0]);
    } else if (!selectedTrack && tracks.length === 0 && currentTrack) {
      setUseExternalCurrent(true);
    }
  }, [tracks, currentTrack]);

  const showToast = (msg: string) => {
    setStatusToast(msg);
    setTimeout(() => setStatusToast(null), 2600);
  };

  // Helper to find existing binding for a local track
  const getTrackBinding = (track: TrackItem) => {
    const trackB = bindings.find((b) => b.scope === 'track' && b.key === track.id);
    if (trackB) return { binding: trackB, origin: t.origin_track, icon: <Music2 size={13} /> };

    if (track.artist && track.title) {
      const extKey = `${track.artist.trim().toLowerCase()}::${track.title.trim().toLowerCase()}`;
      const extB = bindings.find((b) => b.scope === 'external' && b.key.toLowerCase() === extKey);
      if (extB) return { binding: extB, origin: t.origin_track, icon: <Music2 size={13} /> };
    }

    if (track.artist && track.album) {
      const albumB = bindings.find(
        (b) => b.scope === 'album' && b.key.toLowerCase().includes(track.album!.toLowerCase())
      );
      if (albumB) return { binding: albumB, origin: t.origin_album, icon: <Disc size={13} /> };
    }

    if (track.artist) {
      const artistB = bindings.find(
        (b) => b.scope === 'artist' && b.key.toLowerCase() === track.artist!.toLowerCase()
      );
      if (artistB) return { binding: artistB, origin: t.origin_artist, icon: <User size={13} /> };
    }

    return null;
  };

  const getCurrentExternalBinding = () => {
    if (!currentTrack) return null;
    const direct = bindings.find(
      (b) =>
        (b.scope === 'external' || b.scope === 'track') &&
        b.key.toLowerCase() === currentTrack.key.toLowerCase()
    );
    if (direct) return { binding: direct, origin: t.origin_track, icon: <Music2 size={13} /> };
    if (currentTrack.artist) {
      const curArtist = currentTrack.artist.toLowerCase();
      const artistB = bindings.find(
        (b) => b.scope === 'artist' && b.key.toLowerCase() === curArtist
      );
      if (artistB) return { binding: artistB, origin: t.origin_artist, icon: <User size={13} /> };
    }
    return null;
  };

  const filteredTracks = tracks.filter((tr) => {
    const q = searchQuery.toLowerCase();
    const matchesSearch =
      !q ||
      tr.title?.toLowerCase().includes(q) ||
      tr.artist?.toLowerCase().includes(q) ||
      tr.album?.toLowerCase().includes(q);

    if (!matchesSearch) return false;
    if (filterUnboundOnly) {
      return getTrackBinding(tr) === null;
    }
    return true;
  });

  const handleAddAudioClick = async () => {
    const imported = await onPickAudioFile();
    if (imported) {
      setUseExternalCurrent(false);
      setSelectedTrack(imported);
      showToast(
        lang === 'ru'
          ? `Трек добавлен: ${imported.title || 'Без названия'}`
          : `Track added: ${imported.title || 'Untitled'}`
      );
    }
  };

  const handleAddWallpaperClick = async () => {
    const imported = await onPickWallpaper();
    if (imported) {
      setSelectedWallpaperId(imported.id);
      showToast(
        lang === 'ru'
          ? `Обои добавлены: ${imported.name}`
          : `Wallpaper added: ${imported.name}`
      );
    }
  };

  const handleBindSelectedTrack = async () => {
    if (!selectedWallpaperId) return;

    if (useExternalCurrent && currentTrack) {
      const scope = currentTrack.source === 'external' ? 'external' : 'track';
      await onSetBinding(scope, currentTrack.key, selectedWallpaperId);
      await onSetActiveWallpaper(selectedWallpaperId);
      showToast(
        lang === 'ru'
          ? `Обои привязаны к «${currentTrack.title}» и активированы!`
          : `Wallpaper bound to "${currentTrack.title}" & activated!`
      );
      return;
    }

    if (!selectedTrack) return;
    await onSetBinding('track', selectedTrack.id, selectedWallpaperId);
    if (
      currentTrack &&
      (currentTrack.key === selectedTrack.id ||
        (selectedTrack.title &&
          currentTrack.title &&
          currentTrack.title.toLowerCase() === selectedTrack.title.toLowerCase()))
    ) {
      await onSetActiveWallpaper(selectedWallpaperId);
    }
    showToast(
      lang === 'ru'
        ? `Обои привязаны к «${selectedTrack.title || 'треку'}»!`
        : `Wallpaper bound to "${selectedTrack.title || 'track'}"!`
    );
  };

  const handleBindAlbum = async () => {
    if (!selectedWallpaperId) return;
    const album = useExternalCurrent ? currentTrack?.album_title : selectedTrack?.album;
    const artist = useExternalCurrent ? currentTrack?.artist : selectedTrack?.artist;
    if (!album) return;
    const key = `${artist || 'Unknown'}|${album}`;
    await onSetBinding('album', key.toLowerCase(), selectedWallpaperId);
    showToast(lang === 'ru' ? `Обои привязаны к альбому «${album}»` : `Bound to album "${album}"`);
  };

  const handleBindArtist = async () => {
    if (!selectedWallpaperId) return;
    const artist = useExternalCurrent ? currentTrack?.artist : selectedTrack?.artist;
    if (!artist) return;
    await onSetBinding('artist', artist.toLowerCase(), selectedWallpaperId);
    showToast(
      lang === 'ru' ? `Обои привязаны к исполнителю «${artist}»` : `Bound to artist "${artist}"`
    );
  };

  const handleDropOnTrack = async (track: TrackItem, e: React.DragEvent) => {
    e.preventDefault();
    const wpId = e.dataTransfer.getData('text/plain');
    if (wpId) {
      await onSetBinding('track', track.id, wpId);
      showToast(
        lang === 'ru'
          ? `Обои привязаны к «${track.title || 'треку'}»!`
          : `Wallpaper bound to "${track.title || 'track'}"!`
      );
    }
  };

  const activeTargetTitle = useExternalCurrent
    ? currentTrack?.title
    : selectedTrack?.title || (lang === 'ru' ? 'Выберите трек' : 'Select a track');

  const activeTargetAlbum = useExternalCurrent ? currentTrack?.album_title : selectedTrack?.album;
  const activeTargetArtist = useExternalCurrent ? currentTrack?.artist : selectedTrack?.artist;
  const activeMatch = useExternalCurrent
    ? getCurrentExternalBinding()
    : selectedTrack
    ? getTrackBinding(selectedTrack)
    : null;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '14px' }}>
      {/* View Header with Mode Switch */}
      <div className="content-header" style={{ padding: '0 0 12px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.bindings_title}</h1>
          <p>
            {lang === 'ru'
              ? 'Выберите или добавьте трек слева и изображение/обои справа в одном окне'
              : 'Add or select a track on the left and an image/wallpaper on the right in one place'}
          </p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
          {statusToast && (
            <div
              style={{
                padding: '8px 14px',
                borderRadius: 'var(--radius-md)',
                background: 'rgba(16, 185, 129, 0.18)',
                border: '1px solid rgba(16, 185, 129, 0.45)',
                color: '#34d399',
                fontSize: '0.82rem',
                fontWeight: 600,
              }}
            >
              {statusToast}
            </div>
          )}

          <button
            className={`btn ${followMusic ? 'btn-primary' : 'btn-secondary'}`}
            onClick={onToggleFollowMusic}
            title={followMusic ? t.mode_follow : t.mode_fixed}
            style={{ minWidth: 230 }}
          >
            {followMusic ? <ToggleRight size={18} /> : <ToggleLeft size={18} />}
            <span>{followMusic ? t.mode_follow : t.mode_fixed}</span>
          </button>
        </div>
      </div>

      {/* Main Two-Column All-in-One Work Area */}
      <div style={{ display: 'flex', flex: 1, gap: '18px', minHeight: 0 }}>
        {/* Left Column: Step 1 — Add or Select Track */}
        <div
          className="glass-card"
          style={{
            flex: 1.15,
            display: 'flex',
            flexDirection: 'column',
            gap: '12px',
            padding: '16px',
            minWidth: 0,
          }}
        >
          {/* Step 1 Header + Quick Add Track / Folder Buttons */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: '8px',
              flexWrap: 'wrap',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <span
                style={{
                  width: 24,
                  height: 24,
                  borderRadius: '50%',
                  background: 'var(--accent-primary)',
                  color: 'white',
                  fontSize: '0.78rem',
                  fontWeight: 700,
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                1
              </span>
              <h3 style={{ fontSize: '0.96rem', fontWeight: 600 }}>
                {lang === 'ru' ? 'Выберите или добавьте трек' : 'Select or Add Track'}
              </h3>
            </div>

            <div style={{ display: 'flex', gap: '8px' }}>
              <button
                className="btn btn-primary btn-sm"
                onClick={handleAddAudioClick}
                title={
                  lang === 'ru'
                    ? 'Выбрать аудиофайл (MP3, FLAC, WAV, OGG, M4A)'
                    : 'Pick audio file'
                }
              >
                <Plus size={15} />
                <span>{lang === 'ru' ? 'Добавить трек' : 'Add Track'}</span>
              </button>
              <button
                className="btn btn-secondary btn-sm"
                onClick={onPickMusicFolder}
                title={lang === 'ru' ? 'Добавить папку с музыкой' : 'Add music folder'}
              >
                <FolderPlus size={15} />
                <span>{lang === 'ru' ? 'Папка' : 'Folder'}</span>
              </button>
            </div>
          </div>

          {/* Search & Filter Row */}
          <div style={{ display: 'flex', gap: '8px' }}>
            <div className="search-input-wrapper" style={{ flex: 1 }}>
              <Search size={16} />
              <input
                type="text"
                className="input-field"
                placeholder={t.search_tracks}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
            </div>
            <button
              className={`btn btn-sm ${filterUnboundOnly ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => setFilterUnboundOnly(!filterUnboundOnly)}
            >
              {filterUnboundOnly ? t.filter_unbound : t.filter_all}
            </button>
          </div>

          {/* Embedded Full Audio Player Card (SMTC / External or Active Local) */}
          {currentTrack && (
            <div
              onClick={() => {
                setUseExternalCurrent(true);
              }}
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: '10px',
                padding: '12px 14px',
                borderRadius: 'var(--radius-md)',
                background: useExternalCurrent
                  ? 'linear-gradient(135deg, rgba(99, 102, 241, 0.28), rgba(6, 182, 212, 0.18))'
                  : 'linear-gradient(135deg, rgba(16, 22, 36, 0.95), rgba(6, 182, 212, 0.12))',
                border: useExternalCurrent
                  ? '1.5px solid var(--accent-primary)'
                  : '1px solid rgba(6, 182, 212, 0.35)',
                cursor: 'pointer',
                transition: 'all 0.15s ease',
              }}
            >
              {/* Top Row: Cover, Meta, Binding Badge */}
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                <div
                  style={{
                    width: 46,
                    height: 46,
                    borderRadius: 'var(--radius-sm)',
                    background: '#1a1f2c',
                    overflow: 'hidden',
                    flexShrink: 0,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    boxShadow: '0 2px 8px rgba(0,0,0,0.4)',
                  }}
                >
                  {currentTrack.cover_path ? (
                    <img
                      src={currentTrack.cover_path}
                      alt="Now Playing"
                      style={{ width: '100%', height: '100%', objectFit: 'cover' }}
                    />
                  ) : (
                    <Radio size={20} color="#06b6d4" />
                  )}
                </div>

                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: '0.68rem',
                      fontWeight: 700,
                      textTransform: 'uppercase',
                      letterSpacing: '0.04em',
                      color: currentTrack.playback_status === 'Playing' ? '#22d3ee' : '#94a3b8',
                      marginBottom: 2,
                    }}
                  >
                    {currentTrack.playback_status === 'Playing'
                      ? lang === 'ru'
                        ? `● Сейчас играет (${(currentTrack as any).source_app || 'wallmus'})`
                        : `● Now Playing (${(currentTrack as any).source_app || 'wallmus'})`
                      : lang === 'ru'
                      ? `❚❚ На паузе (${(currentTrack as any).source_app || 'wallmus'})`
                      : `❚❚ Paused (${(currentTrack as any).source_app || 'wallmus'})`}
                  </div>
                  <div
                    style={{
                      fontSize: '0.88rem',
                      fontWeight: 600,
                      whiteSpace: 'nowrap',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                    }}
                  >
                    {currentTrack.title}
                  </div>
                  <div
                    style={{
                      fontSize: '0.76rem',
                      color: 'var(--text-muted)',
                      whiteSpace: 'nowrap',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                    }}
                  >
                    {currentTrack.artist}
                  </div>
                </div>

                {(() => {
                  const extMatch = getCurrentExternalBinding();
                  const boundWp = extMatch
                    ? wallpapers.find((w) => w.id === extMatch.binding.wallpaper_id)
                    : null;
                  return extMatch ? (
                    <div
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: '6px',
                        padding: '4px 8px',
                        borderRadius: 'var(--radius-sm)',
                        background: 'rgba(6, 182, 212, 0.18)',
                        color: '#22d3ee',
                        fontSize: '0.74rem',
                        fontWeight: 600,
                        flexShrink: 0,
                      }}
                    >
                      {extMatch.icon}
                      <span>{boundWp?.name || extMatch.binding.wallpaper_id}</span>
                    </div>
                  ) : (
                    <span
                      style={{
                        fontSize: '0.72rem',
                        color: 'var(--text-muted)',
                        padding: '4px 8px',
                        borderRadius: 'var(--radius-sm)',
                        background: 'rgba(255,255,255,0.06)',
                      }}
                    >
                      {lang === 'ru' ? 'Выбрать' : 'Select'}
                    </span>
                  );
                })()}
              </div>

              {/* Interactive Controls & Seek Bar Row inside Bindings */}
              <div
                onClick={(e) => e.stopPropagation()}
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '8px',
                  paddingTop: '8px',
                  borderTop: '1px solid rgba(255,255,255,0.08)',
                }}
              >
                {/* Seek Bar */}
                {(() => {
                  const durMs =
                    currentTrack.duration_ms && currentTrack.duration_ms > 0
                      ? currentTrack.duration_ms
                      : 180000;
                  const posMs = Math.min(currentTrack.position_ms || 0, durMs);
                  const fmt = (ms: number) => {
                    const s = Math.max(0, Math.floor(ms / 1000));
                    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, '0')}`;
                  };
                  return (
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <span
                        style={{
                          fontSize: '0.7rem',
                          color: 'var(--text-muted)',
                          minWidth: 32,
                          fontVariantNumeric: 'tabular-nums',
                        }}
                      >
                        {fmt(posMs)}
                      </span>
                      <input
                        type="range"
                        min={0}
                        max={durMs}
                        step={250}
                        value={posMs}
                        onChange={(e) => onSeek(Number(e.target.value))}
                        style={{
                          flex: 1,
                          height: 5,
                          accentColor: '#06b6d4',
                          cursor: 'pointer',
                        }}
                      />
                      <span
                        style={{
                          fontSize: '0.7rem',
                          color: 'var(--text-muted)',
                          minWidth: 32,
                          textAlign: 'right',
                          fontVariantNumeric: 'tabular-nums',
                        }}
                      >
                        {currentTrack.duration_ms ? fmt(durMs) : '--:--'}
                      </span>
                    </div>
                  );
                })()}

                {/* Transport Buttons + Volume */}
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    gap: '10px',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => onPrevTrack()}
                      title={lang === 'ru' ? 'Предыдущий трек' : 'Previous'}
                      style={{ padding: '6px 8px' }}
                    >
                      <SkipBack size={14} />
                    </button>

                    <button
                      className="btn btn-primary btn-sm"
                      onClick={() => onTogglePlay()}
                      title={
                        currentTrack.playback_status === 'Playing'
                          ? lang === 'ru'
                            ? 'Пауза'
                            : 'Pause'
                          : lang === 'ru'
                          ? 'Воспроизвести'
                          : 'Play'
                      }
                      style={{ padding: '6px 14px', gap: 6 }}
                    >
                      {currentTrack.playback_status === 'Playing' ? (
                        <>
                          <Pause size={14} fill="currentColor" />
                          <span>{lang === 'ru' ? 'Пауза' : 'Pause'}</span>
                        </>
                      ) : (
                        <>
                          <Play size={14} fill="currentColor" />
                          <span>{lang === 'ru' ? 'Играть' : 'Play'}</span>
                        </>
                      )}
                    </button>

                    <button
                      className="btn btn-danger btn-sm"
                      onClick={() => onStopTrack()}
                      title={lang === 'ru' ? 'Остановить трек (Стоп)' : 'Stop'}
                      style={{ padding: '6px 10px', gap: 5 }}
                    >
                      <Square size={13} fill="currentColor" />
                      <span>{lang === 'ru' ? 'Стоп' : 'Stop'}</span>
                    </button>

                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => onNextTrack()}
                      title={lang === 'ru' ? 'Следующий трек' : 'Next'}
                      style={{ padding: '6px 8px' }}
                    >
                      <SkipForward size={14} />
                    </button>
                  </div>

                  {/* Volume Slider */}
                  <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                    <button
                      onClick={() => onVolumeChange(volume > 0 ? 0 : 0.8)}
                      style={{
                        background: 'none',
                        border: 'none',
                        color: 'var(--text-muted)',
                        cursor: 'pointer',
                        display: 'flex',
                        alignItems: 'center',
                      }}
                      title={lang === 'ru' ? 'Звук Вкл/Выкл' : 'Mute/Unmute'}
                    >
                      {volume === 0 ? <VolumeX size={14} color="#fb7185" /> : <Volume2 size={14} />}
                    </button>
                    <input
                      type="range"
                      min={0}
                      max={1}
                      step={0.01}
                      value={volume}
                      onChange={(e) => onVolumeChange(parseFloat(e.target.value))}
                      style={{ width: 68, accentColor: '#6366f1', cursor: 'pointer' }}
                    />
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Scrollable Local Tracks List */}
          <div
            style={{
              flex: 1,
              overflowY: 'auto',
              display: 'flex',
              flexDirection: 'column',
              gap: '4px',
            }}
          >
            {filteredTracks.length === 0 ? (
              <div
                style={{
                  textAlign: 'center',
                  padding: '28px 16px',
                  color: 'var(--text-dim)',
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  gap: '10px',
                }}
              >
                <Music2 size={28} style={{ opacity: 0.4 }} />
                <div>
                  {lang === 'ru'
                    ? 'В локальной библиотеке пока нет треков. Нажмите «+ Добавить трек» выше или выберите карточку «Сейчас играет».'
                    : 'No local tracks yet. Click "+ Add Track" above or select the "Now Playing" card.'}
                </div>
              </div>
            ) : (
              filteredTracks.map((tr) => {
                const isSelected = !useExternalCurrent && selectedTrack?.id === tr.id;
                const isCurrentTrack =
                  currentTrack?.key === tr.id ||
                  (Boolean(currentTrack?.title) &&
                    Boolean(tr.title) &&
                    currentTrack?.source === 'local' &&
                    currentTrack?.title?.toLowerCase() === tr.title?.toLowerCase());
                const isTrackPlaying =
                  isCurrentTrack && currentTrack?.playback_status === 'Playing';
                const match = getTrackBinding(tr);
                const boundWp = match
                  ? wallpapers.find((w) => w.id === match.binding.wallpaper_id)
                  : null;
                const durMs =
                  isCurrentTrack && currentTrack?.duration_ms && currentTrack.duration_ms > 0
                    ? currentTrack.duration_ms
                    : tr.duration_ms && tr.duration_ms > 0
                    ? tr.duration_ms
                    : 180000;
                const posMs = isCurrentTrack ? Math.min(currentTrack?.position_ms || 0, durMs) : 0;
                const fmtMs = (ms: number) => {
                  const s = Math.max(0, Math.floor(ms / 1000));
                  return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, '0')}`;
                };

                return (
                  <div
                    key={tr.id}
                    onClick={() => {
                      setUseExternalCurrent(false);
                      setSelectedTrack(tr);
                    }}
                    onDragOver={(e) => e.preventDefault()}
                    onDrop={(e) => handleDropOnTrack(tr, e)}
                    style={{
                      display: 'flex',
                      flexDirection: 'column',
                      gap: '8px',
                      padding: '10px 12px',
                      borderRadius: 'var(--radius-md)',
                      background: isTrackPlaying
                        ? 'rgba(6, 182, 212, 0.14)'
                        : isSelected
                        ? 'rgba(99, 102, 241, 0.18)'
                        : 'rgba(255, 255, 255, 0.03)',
                      border: isTrackPlaying
                        ? '1px solid rgba(6, 182, 212, 0.5)'
                        : isSelected
                        ? '1px solid rgba(99, 102, 241, 0.45)'
                        : '1px solid rgba(255, 255, 255, 0.05)',
                      cursor: 'pointer',
                      transition: 'all 0.15s ease',
                    }}
                  >
                    {/* Main Track Row */}
                    <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: '5px', flexShrink: 0 }}>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setUseExternalCurrent(false);
                            setSelectedTrack(tr);
                            if (isCurrentTrack) {
                              onTogglePlay();
                            } else {
                              onPlayTrack(tr);
                            }
                          }}
                          title={
                            isTrackPlaying
                              ? lang === 'ru'
                                ? 'Поставить на паузу'
                                : 'Pause track'
                              : lang === 'ru'
                              ? 'Воспроизвести этот трек'
                              : 'Play this track'
                          }
                          style={{
                            width: 34,
                            height: 34,
                            borderRadius: '50%',
                            border: 'none',
                            background: isTrackPlaying
                              ? 'linear-gradient(135deg, #06b6d4, #6366f1)'
                              : 'rgba(99, 102, 241, 0.22)',
                            color: 'white',
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            cursor: 'pointer',
                            flexShrink: 0,
                            boxShadow: isTrackPlaying ? '0 0 10px rgba(6, 182, 212, 0.4)' : 'none',
                          }}
                        >
                          {isTrackPlaying ? (
                            <Pause size={14} fill="currentColor" />
                          ) : (
                            <Play size={14} fill="currentColor" style={{ marginLeft: 1 }} />
                          )}
                        </button>

                        {isCurrentTrack && (
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              onStopTrack();
                            }}
                            title={lang === 'ru' ? 'Остановить (Стоп)' : 'Stop'}
                            style={{
                              width: 28,
                              height: 28,
                              borderRadius: '50%',
                              border: 'none',
                              background: 'rgba(244, 63, 94, 0.22)',
                              color: '#fb7185',
                              display: 'flex',
                              alignItems: 'center',
                              justifyContent: 'center',
                              cursor: 'pointer',
                              flexShrink: 0,
                            }}
                          >
                            <Square size={11} fill="currentColor" />
                          </button>
                        )}
                      </div>

                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div
                          style={{
                            fontSize: '0.87rem',
                            fontWeight: 600,
                            whiteSpace: 'nowrap',
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                          }}
                        >
                          {tr.title || 'Untitled Track'}
                        </div>
                        <div
                          style={{
                            fontSize: '0.76rem',
                            color: 'var(--text-muted)',
                            whiteSpace: 'nowrap',
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                          }}
                        >
                          {tr.artist || 'Unknown Artist'}
                          {tr.album ? ` • ${tr.album}` : ''}
                        </div>
                      </div>

                      {/* Quick Action Buttons Right on Track Item */}
                      <div
                        style={{ display: 'flex', alignItems: 'center', gap: '6px', flexShrink: 0 }}
                        onClick={(e) => e.stopPropagation()}
                      >
                        {match ? (
                          <div
                            style={{
                              display: 'flex',
                              alignItems: 'center',
                              gap: '6px',
                              padding: '4px 8px',
                              borderRadius: 'var(--radius-sm)',
                              background: 'rgba(6, 182, 212, 0.14)',
                              color: '#06b6d4',
                              fontSize: '0.73rem',
                              fontWeight: 600,
                            }}
                            title={
                              lang === 'ru'
                                ? `Привязано к обоям: ${boundWp?.name || match.binding.wallpaper_id}`
                                : `Bound to: ${boundWp?.name || match.binding.wallpaper_id}`
                            }
                          >
                            {match.icon}
                            <span style={{ maxWidth: 95, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                              {boundWp?.name || match.binding.wallpaper_id}
                            </span>
                          </div>
                        ) : selectedWallpaperId ? (
                          <button
                            className="btn btn-secondary btn-sm"
                            onClick={async () => {
                              setUseExternalCurrent(false);
                              setSelectedTrack(tr);
                              await onSetBinding('track', tr.id, selectedWallpaperId);
                              if (isCurrentTrack) {
                                await onSetActiveWallpaper(selectedWallpaperId);
                              }
                              showToast(
                                lang === 'ru'
                                  ? `Обои привязаны к «${tr.title || 'треку'}»!`
                                  : `Bound to "${tr.title || 'track'}"!`
                              );
                            }}
                            title={
                              lang === 'ru'
                                ? 'Привязать выбранные справа обои к этому треку'
                                : 'Bind selected wallpaper to this track'
                            }
                            style={{ padding: '4px 8px', fontSize: '0.72rem', gap: 4 }}
                          >
                            <Link2 size={12} />
                            <span>{lang === 'ru' ? 'Привязать' : 'Bind'}</span>
                          </button>
                        ) : null}

                        {onDeleteTrack && (
                          <button
                            onClick={async () => {
                              await onDeleteTrack(tr.id);
                              if (selectedTrack?.id === tr.id) {
                                setSelectedTrack(null);
                              }
                              showToast(
                                lang === 'ru'
                                  ? `Трек «${tr.title || 'Без названия'}» удален из списка`
                                  : `Track removed from list`
                              );
                            }}
                            title={lang === 'ru' ? 'Удалить трек из списка' : 'Remove track from list'}
                            style={{
                              width: 26,
                              height: 26,
                              borderRadius: '6px',
                              border: '1px solid rgba(244, 63, 94, 0.25)',
                              background: 'rgba(244, 63, 94, 0.1)',
                              color: '#fb7185',
                              display: 'flex',
                              alignItems: 'center',
                              justifyContent: 'center',
                              cursor: 'pointer',
                            }}
                          >
                            <Trash2 size={12} />
                          </button>
                        )}
                      </div>
                    </div>

                    {/* Inline Track Controls when Track is Active or Selected in List */}
                    {(isCurrentTrack || isSelected) && (
                      <div
                        onClick={(e) => e.stopPropagation()}
                        style={{
                          display: 'flex',
                          flexDirection: 'column',
                          gap: '6px',
                          paddingTop: '8px',
                          borderTop: '1px solid rgba(255, 255, 255, 0.08)',
                        }}
                      >
                        {/* Seek Bar for Active Track */}
                        {isCurrentTrack && (
                          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                            <span
                              style={{
                                fontSize: '0.69rem',
                                color: 'var(--text-muted)',
                                minWidth: 32,
                                fontVariantNumeric: 'tabular-nums',
                              }}
                            >
                              {fmtMs(posMs)}
                            </span>
                            <input
                              type="range"
                              min={0}
                              max={durMs}
                              step={250}
                              value={posMs}
                              onChange={(e) => onSeek(Number(e.target.value))}
                              style={{
                                flex: 1,
                                height: 4,
                                accentColor: '#06b6d4',
                                cursor: 'pointer',
                              }}
                            />
                            <span
                              style={{
                                fontSize: '0.69rem',
                                color: 'var(--text-muted)',
                                minWidth: 32,
                                textAlign: 'right',
                                fontVariantNumeric: 'tabular-nums',
                              }}
                            >
                              {fmtMs(durMs)}
                            </span>
                          </div>
                        )}

                        {/* Inline Transport & Quick Bind Controls */}
                        <div
                          style={{
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'space-between',
                            gap: '8px',
                            flexWrap: 'wrap',
                          }}
                        >
                          <div style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
                            <button
                              className="btn btn-secondary btn-sm"
                              onClick={() => onPrevTrack()}
                              title={lang === 'ru' ? 'Предыдущий трек' : 'Previous'}
                              style={{ padding: '4px 7px' }}
                            >
                              <SkipBack size={12} />
                            </button>

                            <button
                              className="btn btn-primary btn-sm"
                              onClick={() => {
                                if (isCurrentTrack) {
                                  onTogglePlay();
                                } else {
                                  onPlayTrack(tr);
                                }
                              }}
                              style={{ padding: '4px 10px', fontSize: '0.74rem', gap: 5 }}
                            >
                              {isTrackPlaying ? (
                                <>
                                  <Pause size={12} fill="currentColor" />
                                  <span>{lang === 'ru' ? 'Пауза' : 'Pause'}</span>
                                </>
                              ) : (
                                <>
                                  <Play size={12} fill="currentColor" />
                                  <span>{lang === 'ru' ? 'Играть' : 'Play'}</span>
                                </>
                              )}
                            </button>

                            {isCurrentTrack && (
                              <button
                                className="btn btn-danger btn-sm"
                                onClick={() => onStopTrack()}
                                style={{ padding: '4px 9px', fontSize: '0.74rem', gap: 4 }}
                              >
                                <Square size={11} fill="currentColor" />
                                <span>{lang === 'ru' ? 'Стоп' : 'Stop'}</span>
                              </button>
                            )}

                            <button
                              className="btn btn-secondary btn-sm"
                              onClick={() => onNextTrack()}
                              title={lang === 'ru' ? 'Следующий трек' : 'Next'}
                              style={{ padding: '4px 7px' }}
                            >
                              <SkipForward size={12} />
                            </button>
                          </div>

                          {/* Inline Volume + Bind/Unbind */}
                          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                            <Volume2 size={13} color="var(--text-muted)" />
                            <input
                              type="range"
                              min={0}
                              max={1}
                              step={0.01}
                              value={volume}
                              onChange={(e) => onVolumeChange(parseFloat(e.target.value))}
                              style={{ width: 60, accentColor: '#6366f1', cursor: 'pointer' }}
                              title={lang === 'ru' ? `Громкость: ${Math.round(volume * 100)}%` : 'Volume'}
                            />
                            {selectedWallpaperId && (
                              <button
                                className="btn btn-secondary btn-sm"
                                onClick={async () => {
                                  await onSetBinding('track', tr.id, selectedWallpaperId);
                                  if (isCurrentTrack) {
                                    await onSetActiveWallpaper(selectedWallpaperId);
                                  }
                                  showToast(
                                    lang === 'ru'
                                      ? `Обои привязаны к «${tr.title || 'треку'}»!`
                                      : `Wallpaper bound to "${tr.title || 'track'}"!`
                                  );
                                }}
                                style={{ padding: '4px 8px', fontSize: '0.72rem', gap: 4 }}
                              >
                                <Link2 size={12} />
                                <span>
                                  {lang === 'ru' ? 'Привязать к обоям' : 'Bind wallpaper'}
                                </span>
                              </button>
                            )}
                          </div>
                        </div>
                      </div>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </div>

        {/* Right Column: Step 2 — Add or Select Wallpaper & Bind */}
        <div
          className="glass-card"
          style={{
            flex: 1.05,
            display: 'flex',
            flexDirection: 'column',
            gap: '12px',
            padding: '16px',
            minWidth: 0,
          }}
        >
          {/* Step 2 Header + Quick Add Image/Video Button */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: '8px',
              flexWrap: 'wrap',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <span
                style={{
                  width: 24,
                  height: 24,
                  borderRadius: '50%',
                  background: 'var(--accent-primary)',
                  color: 'white',
                  fontSize: '0.78rem',
                  fontWeight: 700,
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                2
              </span>
              <h3 style={{ fontSize: '0.96rem', fontWeight: 600 }}>
                {lang === 'ru' ? 'Выберите или добавьте изображение' : 'Select or Add Image'}
              </h3>
            </div>

            <button className="btn btn-primary btn-sm" onClick={handleAddWallpaperClick}>
              <ImagePlus size={15} />
              <span>
                {lang === 'ru' ? '+ Добавить изображение / видео' : '+ Add Image / Video'}
              </span>
            </button>
          </div>

          {/* Wallpaper Selection Grid */}
          <div
            style={{
              flex: 1,
              overflowY: 'auto',
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(135px, 1fr))',
              gridAutoRows: 'minmax(92px, auto)',
              gap: '10px',
              padding: '2px 4px 12px 2px',
            }}
          >
            {/* Quick Upload Tile inside Grid */}
            <div
              onClick={handleAddWallpaperClick}
              style={{
                width: '100%',
                minHeight: '92px',
                aspectRatio: '16/10',
                borderRadius: 'var(--radius-md)',
                border: '1.5px dashed rgba(99, 102, 241, 0.45)',
                background: 'rgba(99, 102, 241, 0.06)',
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                justifyContent: 'center',
                gap: '6px',
                cursor: 'pointer',
                color: 'var(--accent-primary)',
                padding: '8px',
                textAlign: 'center',
                boxSizing: 'border-box',
              }}
            >
              <ImagePlus size={22} />
              <span style={{ fontSize: '0.75rem', fontWeight: 600 }}>
                {lang === 'ru' ? 'Загрузить файл...' : 'Upload file...'}
              </span>
            </div>

            {wallpapers.map((wp) => {
              const isSelected = selectedWallpaperId === wp.id;
              const isDesktopActive = activeWallpaperId === wp.id;
              return (
                <div
                  key={wp.id}
                  draggable
                  onDragStart={(e) => e.dataTransfer.setData('text/plain', wp.id)}
                  onClick={() => setSelectedWallpaperId(wp.id)}
                  style={{
                    position: 'relative',
                    width: '100%',
                    minHeight: '92px',
                    aspectRatio: '16/10',
                    borderRadius: 'var(--radius-md)',
                    overflow: 'hidden',
                    border: isSelected
                      ? '2px solid var(--accent-primary)'
                      : '1px solid var(--border-subtle)',
                    cursor: 'pointer',
                    boxShadow: isSelected ? '0 0 12px var(--accent-glow)' : 'none',
                    transition: 'all 0.15s ease',
                    background: '#141824',
                    boxSizing: 'border-box',
                  }}
                >
                  <img
                    src={wp.thumb_path}
                    alt={wp.name}
                    style={{ width: '100%', height: '100%', objectFit: 'cover' }}
                  />
                  <div
                    style={{
                      position: 'absolute',
                      inset: 0,
                      background: 'linear-gradient(to top, rgba(0,0,0,0.85) 0%, transparent 60%)',
                      display: 'flex',
                      alignItems: 'flex-end',
                      justifyContent: 'space-between',
                      padding: '7px 8px',
                    }}
                  >
                    <span
                      style={{
                        fontSize: '0.74rem',
                        fontWeight: 600,
                        color: 'white',
                        whiteSpace: 'nowrap',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                      }}
                    >
                      {wp.name}
                    </span>
                  </div>
                  {isDesktopActive && (
                    <div
                      style={{
                        position: 'absolute',
                        top: 6,
                        left: 6,
                        padding: '2px 6px',
                        borderRadius: 4,
                        background: 'rgba(16, 185, 129, 0.9)',
                        color: 'white',
                        fontSize: '0.64rem',
                        fontWeight: 700,
                      }}
                    >
                      {lang === 'ru' ? 'На экране' : 'Active'}
                    </div>
                  )}
                  {isSelected && (
                    <div
                      style={{
                        position: 'absolute',
                        top: 6,
                        right: 6,
                        width: 20,
                        height: 20,
                        borderRadius: '50%',
                        background: 'var(--accent-primary)',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        color: 'white',
                      }}
                    >
                      <Check size={12} strokeWidth={3} />
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {/* Step 3: Pairing & Instant Apply Action Buttons */}
          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              gap: '8px',
              borderTop: '1px solid var(--border-subtle)',
              paddingTop: '12px',
            }}
          >
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                fontSize: '0.78rem',
                color: 'var(--text-muted)',
              }}
            >
              <span>{lang === 'ru' ? 'Выбранный трек:' : 'Selected track:'}</span>
              <strong
                style={{
                  color: 'var(--text-main)',
                  maxWidth: '65%',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {activeTargetTitle}
              </strong>
            </div>

            <button
              className="btn btn-primary"
              onClick={handleBindSelectedTrack}
              disabled={(!selectedTrack && !useExternalCurrent) || !selectedWallpaperId}
              style={{ width: '100%' }}
            >
              <Link2 size={16} />
              <span>{t.bind_track_btn}</span>
            </button>

            <div style={{ display: 'flex', gap: '8px' }}>
              <button
                className="btn btn-secondary"
                onClick={handleBindAlbum}
                disabled={!activeTargetAlbum || !selectedWallpaperId}
                style={{ flex: 1 }}
                title="Привязать эти обои ко всему альбому"
              >
                <Disc size={15} />
                <span style={{ fontSize: '0.78rem' }}>{t.bind_album_btn}</span>
              </button>

              <button
                className="btn btn-secondary"
                onClick={handleBindArtist}
                disabled={!activeTargetArtist || !selectedWallpaperId}
                style={{ flex: 1 }}
                title="Привязать эти обои ко всем трекам исполнителя"
              >
                <User size={15} />
                <span style={{ fontSize: '0.78rem' }}>{t.bind_artist_btn}</span>
              </button>

              <button
                className="btn btn-secondary"
                onClick={() => {
                  if (selectedWallpaperId) {
                    onSetActiveWallpaper(selectedWallpaperId);
                    showToast(
                      lang === 'ru'
                        ? 'Обои установлены на рабочий стол!'
                        : 'Wallpaper applied to desktop!'
                    );
                  }
                }}
                disabled={!selectedWallpaperId}
                style={{ flex: 1 }}
                title={
                  lang === 'ru'
                    ? 'Сразу установить выбранные обои на рабочий стол'
                    : 'Set as active desktop wallpaper now'
                }
              >
                <Monitor size={15} />
                <span style={{ fontSize: '0.78rem' }}>
                  {lang === 'ru' ? 'На экран' : 'Apply Now'}
                </span>
              </button>
            </div>

            {activeMatch && (
              <button
                className="btn btn-danger btn-sm"
                onClick={() => {
                  onRemoveBinding(activeMatch.binding.scope, activeMatch.binding.key);
                }}
                style={{ marginTop: '2px' }}
              >
                <Trash2 size={14} />
                <span>{t.remove_binding}</span>
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
