import React, { useState } from 'react';
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
} from 'lucide-react';
import { BindingItem, TrackItem, WallpaperItem } from '../types';
import { Language, translations } from '../i18n';

interface BindingsViewProps {
  tracks: TrackItem[];
  wallpapers: WallpaperItem[];
  bindings: BindingItem[];
  onSetBinding: (scope: string, key: string, wallpaperId: string) => Promise<void>;
  onRemoveBinding: (scope: string, key: string) => Promise<void>;
  followMusic: boolean;
  onToggleFollowMusic: () => Promise<void>;
  lang: Language;
}

export const BindingsView: React.FC<BindingsViewProps> = ({
  tracks,
  wallpapers,
  bindings,
  onSetBinding,
  onRemoveBinding,
  followMusic,
  onToggleFollowMusic,
  lang,
}) => {
  const t = translations[lang];

  const [searchQuery, setSearchQuery] = useState('');
  const [filterUnboundOnly, setFilterUnboundOnly] = useState(false);
  const [selectedTrack, setSelectedTrack] = useState<TrackItem | null>(tracks[0] || null);
  const [selectedWallpaperId, setSelectedWallpaperId] = useState<string>(
    wallpapers[0]?.id || ''
  );

  // Helper to find existing binding for a track
  const getTrackBinding = (track: TrackItem) => {
    // 1. Direct Track
    const trackB = bindings.find((b) => b.scope === 'track' && b.key === track.id);
    if (trackB) return { binding: trackB, origin: t.origin_track, icon: <Music2 size={13} /> };

    // 2. Album
    if (track.artist && track.album) {
      const albumB = bindings.find(
        (b) => b.scope === 'album' && b.key.toLowerCase().includes(track.album!.toLowerCase())
      );
      if (albumB) return { binding: albumB, origin: t.origin_album, icon: <Disc size={13} /> };
    }

    // 3. Artist
    if (track.artist) {
      const artistB = bindings.find(
        (b) => b.scope === 'artist' && b.key.toLowerCase() === track.artist!.toLowerCase()
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

  const handleBindSelectedTrack = async () => {
    if (!selectedTrack || !selectedWallpaperId) return;
    await onSetBinding('track', selectedTrack.id, selectedWallpaperId);
  };

  const handleBindAlbum = async () => {
    if (!selectedTrack || !selectedWallpaperId || !selectedTrack.album) return;
    const key = `${selectedTrack.artist || 'Unknown'}|${selectedTrack.album}`;
    await onSetBinding('album', key.toLowerCase(), selectedWallpaperId);
  };

  const handleBindArtist = async () => {
    if (!selectedTrack || !selectedWallpaperId || !selectedTrack.artist) return;
    await onSetBinding('artist', selectedTrack.artist.toLowerCase(), selectedWallpaperId);
  };

  const handleDropOnTrack = async (track: TrackItem, e: React.DragEvent) => {
    e.preventDefault();
    const wpId = e.dataTransfer.getData('text/plain');
    if (wpId) {
      await onSetBinding('track', track.id, wpId);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '16px' }}>
      {/* View Header with Mode Switch */}
      <div className="content-header" style={{ padding: '0 0 16px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.bindings_title}</h1>
          <p>{t.bindings_subtitle}</p>
        </div>

        <button
          className={`btn ${followMusic ? 'btn-primary' : 'btn-secondary'}`}
          onClick={onToggleFollowMusic}
          title={followMusic ? t.mode_follow : t.mode_fixed}
          style={{ minWidth: 260 }}
        >
          {followMusic ? <ToggleRight size={18} /> : <ToggleLeft size={18} />}
          <span>{followMusic ? t.mode_follow : t.mode_fixed}</span>
        </button>
      </div>

      {/* Main Two-Column Work Area */}
      <div style={{ display: 'flex', flex: 1, gap: '20px', minHeight: 0 }}>
        {/* Left Column: Tracks List */}
        <div
          className="glass-card"
          style={{ flex: 1.2, display: 'flex', flexDirection: 'column', gap: '14px', padding: '16px' }}
        >
          <div style={{ display: 'flex', gap: '10px' }}>
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

          {/* Scrollable Tracks Table */}
          <div style={{ flex: 1, overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: '4px' }}>
            {filteredTracks.length === 0 ? (
              <div style={{ textAlign: 'center', padding: '40px', color: 'var(--text-dim)' }}>
                {t.no_tracks_found}
              </div>
            ) : (
              filteredTracks.map((tr) => {
                const isSelected = selectedTrack?.id === tr.id;
                const match = getTrackBinding(tr);
                const boundWp = match ? wallpapers.find((w) => w.id === match.binding.wallpaper_id) : null;

                return (
                  <div
                    key={tr.id}
                    onClick={() => setSelectedTrack(tr)}
                    onDragOver={(e) => e.preventDefault()}
                    onDrop={(e) => handleDropOnTrack(tr, e)}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '12px',
                      padding: '10px 14px',
                      borderRadius: 'var(--radius-md)',
                      background: isSelected ? 'rgba(99, 102, 241, 0.16)' : 'rgba(255, 255, 255, 0.03)',
                      border: isSelected
                        ? '1px solid rgba(99, 102, 241, 0.4)'
                        : '1px solid transparent',
                      cursor: 'pointer',
                      transition: 'all 0.15s ease',
                    }}
                  >
                    <div
                      style={{
                        width: 38,
                        height: 38,
                        borderRadius: 'var(--radius-sm)',
                        background: '#1a1f2c',
                        overflow: 'hidden',
                        flexShrink: 0,
                      }}
                    >
                      {tr.cover_path ? (
                        <img
                          src={tr.cover_path}
                          alt="Cover"
                          style={{ width: '100%', height: '100%', objectFit: 'cover' }}
                        />
                      ) : (
                        <div style={{ width: '100%', height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                          <Music2 size={18} style={{ opacity: 0.5 }} />
                        </div>
                      )}
                    </div>

                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontSize: '0.88rem', fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                        {tr.title || 'Untitled Track'}
                      </div>
                      <div style={{ fontSize: '0.78rem', color: 'var(--text-muted)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                        {tr.artist || 'Unknown Artist'} • {tr.album || 'Unknown Album'}
                      </div>
                    </div>

                    {/* Matched Binding Badge */}
                    {match ? (
                      <div
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          gap: '6px',
                          padding: '4px 8px',
                          borderRadius: 'var(--radius-sm)',
                          background: 'rgba(6, 182, 212, 0.12)',
                          color: '#06b6d4',
                          fontSize: '0.74rem',
                          fontWeight: 500,
                          flexShrink: 0,
                        }}
                      >
                        {match.icon}
                        <span>{match.origin}: {boundWp?.name || match.binding.wallpaper_id}</span>
                      </div>
                    ) : (
                      <span style={{ fontSize: '0.75rem', color: 'var(--text-dim)', flexShrink: 0 }}>
                        —
                      </span>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </div>

        {/* Right Column: Wallpaper Selector & Pairing Actions */}
        <div
          className="glass-card"
          style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: '16px', padding: '16px' }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <h3 style={{ fontSize: '1rem', fontWeight: 600 }}>{t.select_wallpaper_prompt}</h3>
            {selectedTrack && (
              <span style={{ fontSize: '0.8rem', color: 'var(--accent-primary)', fontWeight: 500 }}>
                {selectedTrack.title}
              </span>
            )}
          </div>

          {/* Wallpaper Selection Grid */}
          <div
            style={{
              flex: 1,
              overflowY: 'auto',
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(135px, 1fr))',
              gap: '12px',
            }}
          >
            {wallpapers.map((wp) => {
              const isSelected = selectedWallpaperId === wp.id;
              return (
                <div
                  key={wp.id}
                  draggable
                  onDragStart={(e) => e.dataTransfer.setData('text/plain', wp.id)}
                  onClick={() => setSelectedWallpaperId(wp.id)}
                  style={{
                    position: 'relative',
                    aspectRatio: '16/10',
                    borderRadius: 'var(--radius-md)',
                    overflow: 'hidden',
                    border: isSelected ? '2px solid var(--accent-primary)' : '1px solid var(--border-subtle)',
                    cursor: 'pointer',
                    boxShadow: isSelected ? '0 0 12px var(--accent-glow)' : 'none',
                    transition: 'all 0.15s ease',
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
                      padding: '8px',
                    }}
                  >
                    <span style={{ fontSize: '0.75rem', fontWeight: 600, color: 'white', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                      {wp.name}
                    </span>
                  </div>
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

          {/* Pairing Action Buttons */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', borderTop: '1px solid var(--border-subtle)', paddingTop: '14px' }}>
            <button
              className="btn btn-primary"
              onClick={handleBindSelectedTrack}
              disabled={!selectedTrack}
              style={{ width: '100%' }}
            >
              <Link2 size={16} />
              <span>{t.bind_track_btn}</span>
            </button>

            <div style={{ display: 'flex', gap: '8px' }}>
              <button
                className="btn btn-secondary"
                onClick={handleBindAlbum}
                disabled={!selectedTrack || !selectedTrack.album}
                style={{ flex: 1 }}
                title="Привязать эти обои ко всему альбому"
              >
                <Disc size={15} />
                <span style={{ fontSize: '0.8rem' }}>{t.bind_album_btn}</span>
              </button>

              <button
                className="btn btn-secondary"
                onClick={handleBindArtist}
                disabled={!selectedTrack || !selectedTrack.artist}
                style={{ flex: 1 }}
                title="Привязать эти обои ко всем трекам исполнителя"
              >
                <User size={15} />
                <span style={{ fontSize: '0.8rem' }}>{t.bind_artist_btn}</span>
              </button>
            </div>

            {selectedTrack && getTrackBinding(selectedTrack) && (
              <button
                className="btn btn-danger btn-sm"
                onClick={() => {
                  const m = getTrackBinding(selectedTrack);
                  if (m) onRemoveBinding(m.binding.scope, m.binding.key);
                }}
                style={{ marginTop: '4px' }}
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
