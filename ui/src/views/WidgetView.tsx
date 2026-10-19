import React, { useEffect, useState } from 'react';
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Disc,
  Pin,
  PinOff,
  Maximize2,
  Minimize2,
} from 'lucide-react';
import { requestDaemon, setWidgetAlwaysOnTop } from '../api/ipc';
import { CurrentTrackInfo } from '../types';

export const WidgetView: React.FC = () => {
  const [track, setTrack] = useState<CurrentTrackInfo | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isPinned, setIsPinned] = useState(false);
  const [isCompact, setIsCompact] = useState(false);
  const [spectrumBars, setSpectrumBars] = useState<number[]>(new Array(16).fill(0.1));

  // Polling track info and spectrum from daemon via Named Pipe
  useEffect(() => {
    let mounted = true;

    const interval = setInterval(async () => {
      try {
        const curTrack = await requestDaemon<CurrentTrackInfo | null>('audio.getTrack');
        if (mounted && curTrack) {
          setTrack(curTrack);
          setIsPlaying(curTrack.playback_status === 'Playing');
        }

        const spec = await requestDaemon<{ spectrum: number[] }>('audio.getSpectrum', {
          bars: 16,
        });
        if (mounted && spec?.spectrum) {
          setSpectrumBars(spec.spectrum);
        }
      } catch (err) {
        // Daemon might be starting
      }
    }, 200);

    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, []);

  const handleTogglePlay = async () => {
    await requestDaemon('audio.control', { action: 'toggle' });
    setIsPlaying(!isPlaying);
  };

  const handleNext = async () => {
    await requestDaemon('audio.control', { action: 'next' });
  };

  const handlePrev = async () => {
    await requestDaemon('audio.control', { action: 'previous' });
  };

  const handleTogglePin = async () => {
    const nextPin = !isPinned;
    setIsPinned(nextPin);
    await setWidgetAlwaysOnTop(nextPin);
  };

  const durationMs = track?.duration_ms || 200000;
  const positionMs = track?.position_ms || 0;
  const progressRatio = Math.min(1, Math.max(0, positionMs / durationMs));

  return (
    <div className="widget-root">
      <div className="widget-card">
        {/* Draggable Header Handle for Tauri */}
        <div data-tauri-drag-region className="widget-drag-handle" />

        {/* Cover Art / Disc */}
        <div
          style={{
            width: isCompact ? 48 : 64,
            height: isCompact ? 48 : 64,
            borderRadius: '12px',
            overflow: 'hidden',
            background: '#181d2a',
            boxShadow: '0 4px 14px rgba(0,0,0,0.4)',
            flexShrink: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            position: 'relative',
          }}
        >
          {track?.cover_path ? (
            <img
              src={track.cover_path}
              alt="Cover"
              style={{ width: '100%', height: '100%', objectFit: 'cover' }}
            />
          ) : (
            <Disc
              size={32}
              style={{
                color: '#6366f1',
                animation: isPlaying ? 'spin 6s linear infinite' : 'none',
              }}
            />
          )}
        </div>

        {/* Track Metadata & Controls */}
        <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 6 }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <div style={{ minWidth: 0, flex: 1, paddingRight: 8 }}>
              <div
                style={{
                  fontSize: '0.92rem',
                  fontWeight: 700,
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  color: '#ffffff',
                }}
              >
                {track?.title || 'No audio playing'}
              </div>
              <div
                style={{
                  fontSize: '0.78rem',
                  color: 'var(--text-muted)',
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
              >
                {track?.artist || 'Ready'}
              </div>
            </div>

            {/* Widget Mode & Pin Buttons */}
            <div style={{ display: 'flex', gap: 6, zIndex: 10 }}>
              <button
                className="btn-control"
                onClick={handleTogglePin}
                title={isPinned ? 'Unpin' : 'Pin Always on Top'}
              >
                {isPinned ? <Pin size={13} color="#6366f1" /> : <PinOff size={13} />}
              </button>
              <button
                className="btn-control"
                onClick={() => setIsCompact(!isCompact)}
                title={isCompact ? 'Expand' : 'Compact'}
              >
                {isCompact ? <Maximize2 size={13} /> : <Minimize2 size={13} />}
              </button>
            </div>
          </div>

          {/* Real-time Audio Spectrum Bars */}
          {!isCompact && (
            <div
              style={{
                display: 'flex',
                alignItems: 'flex-end',
                gap: '2px',
                height: '14px',
                padding: '2px 0',
              }}
            >
              {spectrumBars.map((val, idx) => (
                <div
                  key={idx}
                  style={{
                    flex: 1,
                    height: `${Math.max(15, val * 100)}%`,
                    background: 'linear-gradient(to top, #6366f1, #06b6d4)',
                    borderRadius: '2px',
                    transition: 'height 0.1s ease',
                  }}
                />
              ))}
            </div>
          )}

          {/* Progress & Controls Row */}
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            {/* Timeline Bar */}
            <div
              style={{
                flex: 1,
                height: 3,
                background: 'rgba(255,255,255,0.12)',
                borderRadius: 99,
                overflow: 'hidden',
                marginRight: 12,
              }}
            >
              <div
                style={{
                  width: `${progressRatio * 100}%`,
                  height: '100%',
                  background: '#6366f1',
                }}
              />
            </div>

            {/* Playback Buttons */}
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <button className="btn-control" onClick={handlePrev} title="Previous">
                <SkipBack size={14} />
              </button>
              <button
                className="btn-control play-btn"
                onClick={handleTogglePlay}
                style={{ width: 28, height: 28 }}
                title={isPlaying ? 'Pause' : 'Play'}
              >
                {isPlaying ? <Pause size={14} fill="currentColor" /> : <Play size={14} fill="currentColor" style={{ marginLeft: 1 }} />}
              </button>
              <button className="btn-control" onClick={handleNext} title="Next">
                <SkipForward size={14} />
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
