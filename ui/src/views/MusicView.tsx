import React, { useState } from 'react';
import {
  FolderPlus,
  RefreshCw,
  Folder,
  Radio,
  Play,
  HardDrive,
  Clock,
} from 'lucide-react';
import { MusicSource, TrackItem } from '../types';
import { Language, translations } from '../i18n';

interface MusicViewProps {
  tracks: TrackItem[];
  folders: string[];
  activeSource: MusicSource;
  onSetSource: (src: MusicSource) => Promise<void>;
  onAddFolder: (folderPath: string) => Promise<void>;
  onPlayTrack: (track: TrackItem) => Promise<void>;
  onScanNow: () => Promise<void>;
  lang: Language;
}

export const MusicView: React.FC<MusicViewProps> = ({
  tracks,
  folders,
  activeSource,
  onSetSource,
  onAddFolder,
  onPlayTrack,
  onScanNow,
  lang,
}) => {
  const t = translations[lang];

  const [newFolderPath, setNewFolderPath] = useState('');
  const [search, setSearch] = useState('');

  const formatDuration = (ms?: number) => {
    if (!ms) return '0:00';
    const totalSecs = Math.floor(ms / 1000);
    const mins = Math.floor(totalSecs / 60);
    const secs = totalSecs % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  const filteredTracks = tracks.filter((t) => {
    const q = search.toLowerCase();
    return (
      !q ||
      t.title?.toLowerCase().includes(q) ||
      t.artist?.toLowerCase().includes(q) ||
      t.album?.toLowerCase().includes(q)
    );
  });

  const handleAddFolderSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newFolderPath.trim()) return;
    await onAddFolder(newFolderPath.trim());
    setNewFolderPath('');
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '18px' }}>
      {/* Header */}
      <div className="content-header" style={{ padding: '0 0 16px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.music_title}</h1>
          <p>{t.music_subtitle}</p>
        </div>

        {/* Source Switcher */}
        <div style={{ display: 'flex', gap: '8px', background: 'rgba(255, 255, 255, 0.05)', padding: '4px', borderRadius: 'var(--radius-md)' }}>
          <button
            className={`btn btn-sm ${activeSource === 'local' ? 'btn-primary' : 'btn-secondary'}`}
            onClick={() => onSetSource('local')}
            style={{ border: 'none' }}
          >
            <HardDrive size={14} />
            <span>{t.source_local}</span>
          </button>
          <button
            className={`btn btn-sm ${activeSource === 'external' ? 'btn-primary' : 'btn-secondary'}`}
            onClick={() => onSetSource('external')}
            style={{ border: 'none' }}
          >
            <Radio size={14} />
            <span>{t.source_external}</span>
          </button>
        </div>
      </div>

      {/* Monitored Folders Section */}
      <div className="glass-card" style={{ padding: '16px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <h3 style={{ fontSize: '0.95rem', fontWeight: 600 }}>{t.monitored_folders}</h3>
          <button className="btn btn-secondary btn-sm" onClick={onScanNow}>
            <RefreshCw size={13} />
            <span>{t.scan_now_btn}</span>
          </button>
        </div>

        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
          {folders.map((f, i) => (
            <div
              key={i}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                padding: '6px 12px',
                borderRadius: 'var(--radius-sm)',
                background: 'rgba(255, 255, 255, 0.06)',
                fontSize: '0.8rem',
                color: 'var(--text-muted)',
              }}
            >
              <Folder size={14} color="#6366f1" />
              <span>{f}</span>
            </div>
          ))}
        </div>

        {/* Add Folder Input */}
        <form onSubmit={handleAddFolderSubmit} style={{ display: 'flex', gap: '8px', marginTop: '4px' }}>
          <input
            type="text"
            className="input-field"
            placeholder="C:\Music\..."
            value={newFolderPath}
            onChange={(e) => setNewFolderPath(e.target.value)}
            style={{ paddingLeft: '14px', flex: 1 }}
          />
          <button type="submit" className="btn btn-secondary btn-sm">
            <FolderPlus size={15} />
            <span>{t.add_folder_btn}</span>
          </button>
        </form>
      </div>

      {/* Tracks Table */}
      <div className="glass-card" style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: '10px', minHeight: 0, padding: '16px' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <h3 style={{ fontSize: '0.95rem', fontWeight: 600 }}>
            {t.nav_music} ({filteredTracks.length})
          </h3>
          <input
            type="text"
            className="input-field"
            placeholder={t.search_tracks}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            style={{ maxWidth: '280px', padding: '6px 12px 6px 12px' }}
          />
        </div>

        {/* Table Content */}
        <div style={{ flex: 1, overflowY: 'auto' }}>
          <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: '0.86rem' }}>
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)', color: 'var(--text-dim)', fontSize: '0.78rem' }}>
                <th style={{ padding: '8px 12px' }}>#</th>
                <th style={{ padding: '8px 12px' }}>{t.track_col_title}</th>
                <th style={{ padding: '8px 12px' }}>{t.track_col_artist}</th>
                <th style={{ padding: '8px 12px' }}>{t.track_col_album}</th>
                <th style={{ padding: '8px 12px', textAlign: 'right' }}>
                  <Clock size={13} />
                </th>
              </tr>
            </thead>
            <tbody>
              {filteredTracks.map((tr, idx) => (
                <tr
                  key={tr.id}
                  onClick={() => onPlayTrack(tr)}
                  style={{
                    borderBottom: '1px solid rgba(255, 255, 255, 0.03)',
                    cursor: 'pointer',
                    transition: 'background 0.15s ease',
                  }}
                  onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(255, 255, 255, 0.04)')}
                  onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
                >
                  <td style={{ padding: '10px 12px', color: 'var(--text-dim)', width: '40px' }}>
                    {idx + 1}
                  </td>
                  <td style={{ padding: '10px 12px', fontWeight: 600, color: 'var(--text-main)' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <Play size={12} fill="currentColor" style={{ opacity: 0.5 }} />
                      <span>{tr.title || 'Untitled Track'}</span>
                    </div>
                  </td>
                  <td style={{ padding: '10px 12px', color: 'var(--text-muted)' }}>{tr.artist || '—'}</td>
                  <td style={{ padding: '10px 12px', color: 'var(--text-muted)' }}>{tr.album || '—'}</td>
                  <td style={{ padding: '10px 12px', color: 'var(--text-dim)', textAlign: 'right' }}>
                    {formatDuration(tr.duration_ms)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
