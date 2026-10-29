import React, { useEffect, useState } from 'react';
import {
  UploadCloud,
  Film,
  Sparkles,
  Image as ImageIcon,
  CheckCircle2,
  Trash2,
  Monitor,
  Globe,
  Cpu,
  Shuffle,
  Repeat,
  Layers,
  Plus,
  Check,
} from 'lucide-react';
import { WallpaperItem, WallpaperKind } from '../types';
import { Language, translations } from '../i18n';
import { requestDaemon } from '../api/ipc';

interface WallpapersViewProps {
  wallpapers: WallpaperItem[];
  activeWallpaperId?: string;
  wallpaperChangeMode?: 'random' | 'sequential' | 'off';
  respectBindings?: boolean;
  onSetChangeMode?: (mode: 'random' | 'sequential' | 'off', respectBindings?: boolean) => Promise<void>;
  onSetActiveWallpaper: (id: string) => Promise<void>;
  onImportWallpaper: (path: string) => Promise<void>;
  onDeleteWallpaper: (id: string) => Promise<void>;
  lang: Language;
}

export const WallpapersView: React.FC<WallpapersViewProps> = ({
  wallpapers,
  activeWallpaperId,
  wallpaperChangeMode = 'random',
  respectBindings = false,
  onSetChangeMode,
  onSetActiveWallpaper,
  onImportWallpaper,
  onDeleteWallpaper,
  lang,
}) => {
  const t = translations[lang];

  const [filterType, setFilterType] = useState<string>('all');
  const [selectedWallpaper, setSelectedWallpaper] = useState<WallpaperItem | null>(
    wallpapers[0] || null
  );
  const [isDragging, setIsDragging] = useState(false);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  const showToast = (msg: string) => {
    setToastMessage(msg);
    setTimeout(() => setToastMessage(null), 3000);
  };

  useEffect(() => {
    if (!selectedWallpaper && wallpapers.length > 0) {
      const active = wallpapers.find((w) => w.id === activeWallpaperId) || wallpapers[0];
      setSelectedWallpaper(active);
    } else if (selectedWallpaper) {
      const updated = wallpapers.find((w) => w.id === selectedWallpaper.id);
      if (updated) {
        setSelectedWallpaper(updated);
      } else {
        setSelectedWallpaper(wallpapers[0] || null);
      }
    }
  }, [wallpapers, activeWallpaperId]);

  const filteredWallpapers = wallpapers.filter((w) => {
    if (filterType === 'all') return true;
    return w.kind === filterType;
  });

  const getKindBadge = (kind: WallpaperKind) => {
    switch (kind) {
      case 'video':
        return { label: t.filter_video, icon: <Film size={12} />, color: '#06b6d4' };
      case 'animated':
        return { label: t.filter_animated, icon: <Sparkles size={12} />, color: '#ec4899' };
      case 'html':
        return { label: t.filter_html, icon: <Globe size={12} />, color: '#f59e0b' };
      case 'shader':
        return { label: t.filter_shader, icon: <Cpu size={12} />, color: '#8b5cf6' };
      default:
        return { label: t.filter_image, icon: <ImageIcon size={12} />, color: '#10b981' };
    }
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    // In browser or Tauri, files can be passed
    const files = Array.from(e.dataTransfer.files);
    for (const f of files) {
      // In Tauri, webkitRelativePath or path property exists
      const path = (f as any).path || f.name;
      if (path) {
        await onImportWallpaper(path);
      }
    }
  };

  const handlePickFile = async () => {
    try {
      const res = await requestDaemon<{ path: string | null }>('app.pickWallpaperFile');
      if (res?.path) {
        showToast(lang === 'ru' ? 'Импортируем обои...' : 'Importing wallpaper...');
        await onImportWallpaper(res.path);
        showToast(lang === 'ru' ? 'Обои успешно добавлены!' : 'Wallpaper added successfully!');
      }
    } catch (e: any) {
      console.error('Failed to pick wallpaper file:', e);
      showToast(lang === 'ru' ? `Ошибка: ${e?.message || e}` : `Error: ${e?.message || e}`);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '16px' }}>
      {toastMessage && (
        <div
          style={{
            position: 'fixed',
            top: '20px',
            right: '24px',
            zIndex: 9999,
            background: 'rgba(15, 23, 42, 0.92)',
            backdropFilter: 'blur(12px)',
            border: '1px solid rgba(0, 245, 212, 0.4)',
            color: 'var(--accent-primary)',
            borderRadius: '10px',
            padding: '10px 18px',
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            fontSize: '0.88rem',
            boxShadow: '0 8px 32px rgba(0, 0, 0, 0.4)',
          }}
        >
          <Check size={16} />
          <span>{toastMessage}</span>
        </div>
      )}

      {/* Header */}
      <div className="content-header" style={{ padding: '0 0 16px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.wallpapers_title}</h1>
          <p>{t.wallpapers_subtitle}</p>
        </div>

        <div style={{ display: 'flex', gap: '12px', flexWrap: 'wrap', alignItems: 'center' }}>
          <button
            className="btn btn-primary btn-sm"
            style={{ display: 'flex', alignItems: 'center', gap: '6px', fontWeight: 600, padding: '7px 16px' }}
            onClick={handlePickFile}
          >
            <Plus size={15} />
            <span>{lang === 'ru' ? 'Добавить обои' : 'Add Wallpaper'}</span>
          </button>

          <div className="btn-group">
            {['all', 'video', 'animated', 'image', 'html', 'shader'].map((type) => (
              <button
                key={type}
                className={`btn btn-sm ${filterType === type ? 'btn-primary' : 'btn-ghost'}`}
                style={{ padding: '6px 12px' }}
                onClick={() => setFilterType(type)}
              >
                {type === 'all'
                  ? t.filter_all_types
                  : type === 'video'
                  ? t.filter_video
                  : type === 'animated'
                  ? t.filter_animated
                  : type === 'image'
                  ? t.filter_image
                  : type === 'html'
                  ? t.filter_html
                  : t.filter_shader}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Auto-Change on Track Switch Control Bar */}
      <div
        className="glass-card"
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '12px 18px',
          borderRadius: 'var(--radius-lg)',
          background: 'rgba(255, 255, 255, 0.03)',
          border: '1px solid var(--border-card)',
          gap: '16px',
          flexWrap: 'wrap',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <div
            style={{
              width: '36px',
              height: '36px',
              borderRadius: '10px',
              background: wallpaperChangeMode !== 'off' ? 'rgba(0, 245, 212, 0.15)' : 'rgba(255, 255, 255, 0.05)',
              color: wallpaperChangeMode !== 'off' ? 'var(--accent-primary, #00F5D4)' : 'var(--text-dim)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            {wallpaperChangeMode === 'random' ? (
              <Shuffle size={18} />
            ) : wallpaperChangeMode === 'sequential' ? (
              <Repeat size={18} />
            ) : (
              <Layers size={18} />
            )}
          </div>
          <div>
            <div style={{ fontWeight: 600, fontSize: '0.92rem', color: 'var(--text-main)' }}>
              {t.auto_change_title}
            </div>
            <div style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>
              {t.auto_change_subtitle}
            </div>
          </div>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '14px', flexWrap: 'wrap' }}>
          {/* Mode Selector Buttons */}
          <div className="btn-group">
            <button
              className={`btn btn-sm ${wallpaperChangeMode === 'random' ? 'btn-primary' : 'btn-ghost'}`}
              style={{
                fontSize: '0.82rem',
                padding: '6px 14px',
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
              }}
              onClick={() => onSetChangeMode?.('random', respectBindings)}
            >
              <Shuffle size={14} />
              <span>{t.auto_change_random}</span>
            </button>

            <button
              className={`btn btn-sm ${wallpaperChangeMode === 'sequential' ? 'btn-primary' : 'btn-ghost'}`}
              style={{
                fontSize: '0.82rem',
                padding: '6px 14px',
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
              }}
              onClick={() => onSetChangeMode?.('sequential', respectBindings)}
            >
              <Repeat size={14} />
              <span>{t.auto_change_sequential}</span>
            </button>

            <button
              className={`btn btn-sm ${wallpaperChangeMode === 'off' ? 'btn-primary' : 'btn-ghost'}`}
              style={{
                fontSize: '0.82rem',
                padding: '6px 14px',
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
              }}
              onClick={() => onSetChangeMode?.('off', respectBindings)}
            >
              <span>{t.auto_change_off}</span>
            </button>
          </div>

          {/* Respect Bindings Checkbox */}
          {wallpaperChangeMode !== 'off' && (
            <label
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                fontSize: '0.8rem',
                color: 'var(--text-muted)',
                cursor: 'pointer',
                userSelect: 'none',
              }}
              title={t.respect_bindings_hint}
            >
              <input
                type="checkbox"
                checked={respectBindings}
                onChange={(e) => onSetChangeMode?.(wallpaperChangeMode, e.target.checked)}
                style={{ cursor: 'pointer', accentColor: 'var(--accent-primary, #00F5D4)' }}
              />
              <span>{t.respect_bindings_label}</span>
            </label>
          )}
        </div>
      </div>

      <div style={{ display: 'flex', flex: 1, gap: '20px', minHeight: 0 }}>
        {/* Left Side: Drag-and-drop Import + Grid of Cards */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: '16px', overflowY: 'auto' }}>
          {/* Dropzone */}
          <div
            onClick={handlePickFile}
            onDragOver={(e) => {
              e.preventDefault();
              setIsDragging(true);
            }}
            onDragLeave={() => setIsDragging(false)}
            onDrop={handleDrop}
            style={{
              padding: '24px',
              border: `2px dashed ${isDragging ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
              borderRadius: 'var(--radius-lg)',
              background: isDragging ? 'rgba(99, 102, 241, 0.1)' : 'rgba(255, 255, 255, 0.02)',
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              gap: '8px',
              cursor: 'pointer',
              transition: 'all 0.2s ease',
            }}
          >
            <UploadCloud size={32} style={{ color: isDragging ? 'var(--accent-primary)' : 'var(--text-dim)' }} />
            <span style={{ fontSize: '0.88rem', fontWeight: 500, color: 'var(--text-main)' }}>
              {t.drag_drop_area}
            </span>
          </div>

          {/* Cards Grid */}
          <div className="grid-cards">
            {filteredWallpapers.map((wp) => {
              const badge = getKindBadge(wp.kind);
              const isActive = activeWallpaperId === wp.id;
              const isSelected = selectedWallpaper?.id === wp.id;

              return (
                <div
                  key={wp.id}
                  className="glass-card"
                  onClick={() => setSelectedWallpaper(wp)}
                  style={{
                    padding: '10px',
                    cursor: 'pointer',
                    border: isSelected
                      ? '1px solid var(--accent-primary)'
                      : '1px solid var(--border-card)',
                    position: 'relative',
                  }}
                >
                  <div
                    style={{
                      position: 'relative',
                      aspectRatio: '16/9',
                      borderRadius: 'var(--radius-md)',
                      overflow: 'hidden',
                      marginBottom: '10px',
                      background: '#141824',
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
                        top: 8,
                        left: 8,
                        padding: '3px 8px',
                        borderRadius: 'var(--radius-sm)',
                        background: 'rgba(0, 0, 0, 0.65)',
                        backdropFilter: 'blur(8px)',
                        fontSize: '0.7rem',
                        fontWeight: 600,
                        color: badge.color,
                        display: 'flex',
                        alignItems: 'center',
                        gap: '4px',
                      }}
                    >
                      {badge.icon}
                      <span>{badge.label}</span>
                    </div>

                    {isActive && (
                      <div
                        style={{
                          position: 'absolute',
                          top: 8,
                          right: 8,
                          padding: '3px 8px',
                          borderRadius: 'var(--radius-sm)',
                          background: 'var(--accent-emerald)',
                          color: 'white',
                          fontSize: '0.7rem',
                          fontWeight: 600,
                          display: 'flex',
                          alignItems: 'center',
                          gap: '4px',
                        }}
                      >
                        <CheckCircle2 size={12} />
                        <span>{t.active_badge}</span>
                      </div>
                    )}
                  </div>

                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                    <div style={{ fontWeight: 600, fontSize: '0.9rem', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                      {wp.name}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Right Side: Properties & Controls Panel */}
        {selectedWallpaper && (
          <div
            className="glass-card"
            style={{ width: '320px', display: 'flex', flexDirection: 'column', gap: '16px', padding: '20px' }}
          >
            <div style={{ fontSize: '1.05rem', fontWeight: 700 }}>{selectedWallpaper.name}</div>

            <div
              style={{
                aspectRatio: '16/9',
                borderRadius: 'var(--radius-md)',
                overflow: 'hidden',
                background: '#151926',
              }}
            >
              <img
                src={selectedWallpaper.thumb_path}
                alt="Preview"
                style={{ width: '100%', height: '100%', objectFit: 'cover' }}
              />
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', fontSize: '0.85rem' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', color: 'var(--text-muted)' }}>
                <span>{t.property_resolution}:</span>
                <span style={{ color: 'var(--text-main)', fontWeight: 500 }}>
                  {selectedWallpaper.width || 1920} × {selectedWallpaper.height || 1080}
                </span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between', color: 'var(--text-muted)' }}>
                <span>{t.property_fit}:</span>
                <span style={{ color: 'var(--text-main)', fontWeight: 500, textTransform: 'capitalize' }}>
                  {selectedWallpaper.fit}
                </span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between', color: 'var(--text-muted)' }}>
                <span>{t.property_fps}:</span>
                <span style={{ color: 'var(--text-main)', fontWeight: 500 }}>
                  {selectedWallpaper.fps_cap || 30} FPS
                </span>
              </div>
            </div>

            <div style={{ marginTop: 'auto', display: 'flex', flexDirection: 'column', gap: '10px' }}>
              <button
                className="btn btn-primary"
                onClick={() => onSetActiveWallpaper(selectedWallpaper.id)}
                disabled={activeWallpaperId === selectedWallpaper.id}
                style={{ padding: '9px 16px', gap: '8px', fontWeight: 600 }}
              >
                <Monitor size={16} />
                <span>{t.set_as_active}</span>
              </button>

              <button
                className="btn btn-danger btn-sm"
                onClick={() => onDeleteWallpaper(selectedWallpaper.id)}
                style={{ padding: '8px 14px', gap: '8px' }}
              >
                <Trash2 size={14} />
                <span>{t.delete_wallpaper}</span>
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
