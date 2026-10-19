import React, { useState } from 'react';
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Volume2,
  VolumeX,
  Shuffle,
  Repeat,
  Radio,
  Disc3,
} from 'lucide-react';
import { CurrentTrackInfo } from '../types';

interface PlayerBarProps {
  currentTrack?: CurrentTrackInfo;
  isPlaying: boolean;
  onTogglePlay: () => void;
  onNext: () => void;
  onPrevious: () => void;
  onSeek: (posMs: number) => void;
  onVolumeChange: (vol: number) => void;
  volume: number;
}

export const PlayerBar: React.FC<PlayerBarProps> = ({
  currentTrack,
  isPlaying,
  onTogglePlay,
  onNext,
  onPrevious,
  onSeek,
  onVolumeChange,
  volume,
}) => {
  const [isMuted, setIsMuted] = useState(false);
  const [prevVolume, setPrevVolume] = useState(volume);

  const durationMs = currentTrack?.duration_ms || 240000;
  const positionMs = currentTrack?.position_ms || 0;
  const progressRatio = Math.min(1, Math.max(0, positionMs / durationMs));

  const formatTime = (ms: number) => {
    const totalSecs = Math.floor(ms / 1000);
    const mins = Math.floor(totalSecs / 60);
    const secs = totalSecs % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  const handleProgressClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const clickX = e.clientX - rect.left;
    const ratio = Math.max(0, Math.min(1, clickX / rect.width));
    onSeek(Math.floor(ratio * durationMs));
  };

  const toggleMute = () => {
    if (isMuted) {
      setIsMuted(false);
      onVolumeChange(prevVolume);
    } else {
      setPrevVolume(volume);
      setIsMuted(true);
      onVolumeChange(0);
    }
  };

  return (
    <div className="player-bar">
      {/* Track Info */}
      <div className="player-track-info">
        {currentTrack?.cover_path ? (
          <img
            src={currentTrack.cover_path}
            alt="Track Cover"
            className="player-cover"
          />
        ) : (
          <div className="player-cover" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Disc3 size={24} style={{ opacity: 0.6 }} />
          </div>
        )}
        <div className="player-meta">
          <h4>{currentTrack?.title || 'No media playing'}</h4>
          <p>{currentTrack?.artist || 'Idle'}</p>
        </div>
      </div>

      {/* Main Controls & Progress */}
      <div className="player-controls">
        <div className="control-buttons">
          <button className="btn-control" title="Shuffle">
            <Shuffle size={15} />
          </button>
          <button className="btn-control" onClick={onPrevious} title="Previous Track">
            <SkipBack size={18} />
          </button>
          <button className="btn-control play-btn" onClick={onTogglePlay} title={isPlaying ? 'Pause' : 'Play'}>
            {isPlaying ? <Pause size={18} fill="currentColor" /> : <Play size={18} fill="currentColor" style={{ marginLeft: 2 }} />}
          </button>
          <button className="btn-control" onClick={onNext} title="Next Track">
            <SkipForward size={18} />
          </button>
          <button className="btn-control" title="Repeat Mode">
            <Repeat size={15} />
          </button>
        </div>

        <div className="timeline-bar">
          <span className="time-label">{formatTime(positionMs)}</span>
          <div className="progress-track" onClick={handleProgressClick}>
            <div className="progress-fill" style={{ width: `${progressRatio * 100}%` }} />
          </div>
          <span className="time-label">{formatTime(durationMs)}</span>
        </div>
      </div>

      {/* Volume & Source Badge */}
      <div className="player-actions">
        {currentTrack?.source === 'external' ? (
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: '0.78rem', color: '#06b6d4', background: 'rgba(6, 182, 212, 0.12)', padding: '4px 8px', borderRadius: 6 }}>
            <Radio size={13} />
            <span>SMTC</span>
          </div>
        ) : null}

        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <button className="btn-control" onClick={toggleMute} title={isMuted ? 'Unmute' : 'Mute'}>
            {isMuted || volume === 0 ? <VolumeX size={16} /> : <Volume2 size={16} />}
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
            style={{ width: 80, accentColor: '#6366f1', cursor: 'pointer' }}
          />
        </div>
      </div>
    </div>
  );
};
