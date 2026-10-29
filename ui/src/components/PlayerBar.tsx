import React, { useState, useEffect } from 'react';
import {
  Play,
  Pause,
  Square,
  SkipBack,
  SkipForward,
  Volume2,
  VolumeX,
  Shuffle,
  Repeat,
  Repeat1,
  Radio,
  Disc3,
} from 'lucide-react';
import { CurrentTrackInfo } from '../types';
import { Language } from '../i18n';

interface PlayerBarProps {
  currentTrack?: CurrentTrackInfo;
  isPlaying: boolean;
  onTogglePlay: () => void;
  onStop: () => void;
  onNext: () => void;
  onPrevious: () => void;
  onSeek: (posMs: number) => void;
  onVolumeChange: (vol: number) => void;
  volume: number;
  shuffle?: boolean;
  onToggleShuffle?: () => void;
  repeatMode?: 'off' | 'all' | 'one';
  onCycleRepeat?: () => void;
  lang?: Language;
}

export const PlayerBar: React.FC<PlayerBarProps> = ({
  currentTrack,
  isPlaying,
  onTogglePlay,
  onStop,
  onNext,
  onPrevious,
  onSeek,
  onVolumeChange,
  volume,
  shuffle = false,
  onToggleShuffle,
  repeatMode = 'off',
  onCycleRepeat,
  lang = 'ru',
}) => {
  const [isMuted, setIsMuted] = useState(false);
  const [prevVolume, setPrevVolume] = useState(volume);
  const [isDraggingSeek, setIsDraggingSeek] = useState(false);
  const [dragSeekMs, setDragSeekMs] = useState(0);

  const durationMs =
    currentTrack?.duration_ms && currentTrack.duration_ms > 0
      ? currentTrack.duration_ms
      : 180000;
  const rawPositionMs = currentTrack?.position_ms || 0;
  const positionMs = isDraggingSeek ? dragSeekMs : Math.min(rawPositionMs, durationMs);
  const progressRatio = durationMs > 0 ? Math.min(1, Math.max(0, positionMs / durationMs)) : 0;

  useEffect(() => {
    if (!isDraggingSeek) {
      setDragSeekMs(rawPositionMs);
    }
  }, [rawPositionMs, isDraggingSeek]);

  const formatTime = (ms: number) => {
    const totalSecs = Math.max(0, Math.floor(ms / 1000));
    const mins = Math.floor(totalSecs / 60);
    const secs = totalSecs % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  const toggleMute = () => {
    if (isMuted || volume === 0) {
      setIsMuted(false);
      onVolumeChange(prevVolume > 0.05 ? prevVolume : 0.8);
    } else {
      setPrevVolume(volume);
      setIsMuted(true);
      onVolumeChange(0);
    }
  };

  const hasTrack = Boolean(currentTrack && currentTrack.title);

  return (
    <div
      className="player-bar"
      style={{
        borderTop: isPlaying
          ? '1px solid rgba(99, 102, 241, 0.45)'
          : '1px solid var(--border-subtle)',
        boxShadow: isPlaying ? '0 -4px 24px rgba(99, 102, 241, 0.14)' : 'none',
      }}
    >
      {/* Track Info */}
      <div className="player-track-info">
        {currentTrack?.cover_path ? (
          <img
            src={currentTrack.cover_path}
            alt="Track Cover"
            className="player-cover"
          />
        ) : (
          <div
            className="player-cover"
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: isPlaying
                ? 'linear-gradient(135deg, rgba(99, 102, 241, 0.35), rgba(6, 182, 212, 0.35))'
                : '#1e2433',
            }}
          >
            <Disc3 size={24} style={{ opacity: isPlaying ? 0.95 : 0.6, color: isPlaying ? '#22d3ee' : undefined }} />
          </div>
        )}
        <div className="player-meta">
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <h4 style={{ color: hasTrack ? 'var(--text-main)' : 'var(--text-muted)' }}>
              {currentTrack?.title ||
                (lang === 'ru' ? 'Ничего не воспроизводится' : 'No media playing')}
            </h4>
          </div>
          <p>
            {currentTrack?.artist ||
              (lang === 'ru'
                ? 'Запустите трек в разделе «Привязки» или «Музыка»'
                : 'Start a track in Bindings or Music')}
          </p>
        </div>
      </div>

      {/* Main Controls & Interactive Seek Bar */}
      <div className="player-controls">
        <div className="control-buttons">
          <button
            className="btn-control"
            onClick={onToggleShuffle}
            title={lang === 'ru' ? 'Перемешать (Shuffle)' : 'Shuffle'}
            style={{
              color: shuffle ? '#22d3ee' : undefined,
              background: shuffle ? 'rgba(6, 182, 212, 0.15)' : undefined,
              padding: 6,
              borderRadius: 6,
            }}
          >
            <Shuffle size={15} />
          </button>

          <button
            className="btn-control"
            onClick={onPrevious}
            title={lang === 'ru' ? 'Предыдущий трек' : 'Previous Track'}
          >
            <SkipBack size={18} />
          </button>

          <button
            className="btn-control play-btn"
            onClick={onTogglePlay}
            title={
              isPlaying
                ? lang === 'ru'
                  ? 'Пауза'
                  : 'Pause'
                : lang === 'ru'
                ? 'Воспроизвести'
                : 'Play'
            }
            style={{
              background: isPlaying
                ? 'linear-gradient(135deg, #06b6d4, #6366f1)'
                : 'white',
              color: isPlaying ? 'white' : 'black',
            }}
          >
            {isPlaying ? (
              <Pause size={18} fill="currentColor" />
            ) : (
              <Play size={18} fill="currentColor" style={{ marginLeft: 2 }} />
            )}
          </button>

          <button
            className="btn-control"
            onClick={onStop}
            disabled={!hasTrack}
            title={lang === 'ru' ? 'Остановить воспроизведение (Стоп)' : 'Stop Playback'}
            style={{
              opacity: hasTrack ? 1 : 0.35,
              color: hasTrack ? '#fb7185' : undefined,
              padding: 6,
              borderRadius: 6,
              background: hasTrack ? 'rgba(244, 63, 94, 0.12)' : undefined,
            }}
          >
            <Square size={15} fill="currentColor" />
          </button>

          <button
            className="btn-control"
            onClick={onNext}
            title={lang === 'ru' ? 'Следующий трек' : 'Next Track'}
          >
            <SkipForward size={18} />
          </button>

          <button
            className="btn-control"
            onClick={onCycleRepeat}
            title={
              lang === 'ru'
                ? `Повтор: ${repeatMode === 'one' ? 'Один трек' : repeatMode === 'all' ? 'Все' : 'Выкл'}`
                : `Repeat: ${repeatMode}`
            }
            style={{
              color: repeatMode !== 'off' ? '#22d3ee' : undefined,
              background: repeatMode !== 'off' ? 'rgba(6, 182, 212, 0.15)' : undefined,
              padding: 6,
              borderRadius: 6,
            }}
          >
            {repeatMode === 'one' ? <Repeat1 size={15} /> : <Repeat size={15} />}
          </button>
        </div>

        <div className="timeline-bar">
          <span className="time-label">{formatTime(positionMs)}</span>
          <div style={{ position: 'relative', flex: 1, display: 'flex', alignItems: 'center' }}>
            <input
              type="range"
              min={0}
              max={durationMs}
              step={250}
              value={positionMs}
              disabled={!hasTrack}
              onMouseDown={() => setIsDraggingSeek(true)}
              onTouchStart={() => setIsDraggingSeek(true)}
              onChange={(e) => {
                const nextPos = Number(e.target.value);
                setDragSeekMs(nextPos);
                if (!isDraggingSeek) {
                  onSeek(nextPos);
                }
              }}
              onMouseUp={(e) => {
                setIsDraggingSeek(false);
                onSeek(Number((e.target as HTMLInputElement).value));
              }}
              onTouchEnd={(e) => {
                setIsDraggingSeek(false);
                onSeek(Number((e.target as HTMLInputElement).value));
              }}
              style={{
                width: '100%',
                height: 6,
                accentColor: '#06b6d4',
                cursor: hasTrack ? 'pointer' : 'default',
                background: `linear-gradient(90deg, #6366f1 0%, #06b6d4 ${progressRatio * 100}%, rgba(255,255,255,0.14) ${progressRatio * 100}%)`,
                borderRadius: 999,
              }}
            />
          </div>
          <span className="time-label">
            {currentTrack?.duration_ms ? formatTime(durationMs) : '--:--'}
          </span>
        </div>
      </div>

      {/* Volume & Source Badge */}
      <div className="player-actions">
        {currentTrack?.source === 'external' ? (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              fontSize: '0.74rem',
              color: '#06b6d4',
              background: 'rgba(6, 182, 212, 0.12)',
              padding: '4px 8px',
              borderRadius: 6,
              flexShrink: 0,
            }}
          >
            <Radio size={13} />
            <span>SMTC</span>
          </div>
        ) : hasTrack ? (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 5,
              fontSize: '0.72rem',
              color: '#818cf8',
              background: 'rgba(99, 102, 241, 0.14)',
              padding: '3px 8px',
              borderRadius: 6,
              flexShrink: 0,
            }}
          >
            <span>{lang === 'ru' ? 'Плеер' : 'Local'}</span>
          </div>
        ) : null}

        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <button
            className="btn-control"
            onClick={toggleMute}
            title={
              isMuted || volume === 0
                ? lang === 'ru'
                  ? 'Включить звук'
                  : 'Unmute'
                : lang === 'ru'
                ? 'Без звука'
                : 'Mute'
            }
          >
            {isMuted || volume === 0 ? <VolumeX size={16} color="#fb7185" /> : <Volume2 size={16} />}
          </button>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={isMuted ? 0 : volume}
            onChange={(e) => {
              setIsMuted(false);
              onVolumeChange(parseFloat(e.target.value));
            }}
            style={{ width: 84, accentColor: '#6366f1', cursor: 'pointer' }}
            title={`${Math.round((isMuted ? 0 : volume) * 100)}%`}
          />
          <span
            style={{
              fontSize: '0.74rem',
              color: 'var(--text-muted)',
              minWidth: 32,
              textAlign: 'right',
              fontVariantNumeric: 'tabular-nums',
            }}
          >
            {Math.round((isMuted ? 0 : volume) * 100)}%
          </span>
        </div>
      </div>
    </div>
  );
};
