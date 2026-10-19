import React, { useState, useEffect, useCallback } from 'react';
import {
  Rocket,
  Languages,
  Palette,
  FileText,
  FolderOpen,
  Keyboard,
  RotateCcw,
  Check,
  AlertCircle,
  Activity,
  BarChart2,
  Waves,
  Sparkles,
} from 'lucide-react';
import { Language, translations } from '../i18n';
import { HotkeysConfig, WallpaperVisualizerConfig } from '../types';
import { requestDaemon } from '../api/ipc';

interface SettingsViewProps {
  autostart: boolean;
  onToggleAutostart: (enabled: boolean) => Promise<void>;
  lang: Language;
  onSelectLanguage: (lang: Language) => void;
  theme: 'dark' | 'light';
  onSelectTheme: (theme: 'dark' | 'light') => void;
  onOpenLogs: () => Promise<void>;
}

const DEFAULT_HOTKEYS: HotkeysConfig = {
  toggle_playback: 'Ctrl+Alt+Space',
  next_track: 'Ctrl+Alt+Right',
  prev_track: 'Ctrl+Alt+Left',
  toggle_wallpaper_pause: 'Ctrl+Alt+P',
  next_wallpaper: 'Ctrl+Alt+W',
  toggle_widget: 'Ctrl+Alt+M',
};

export const SettingsView: React.FC<SettingsViewProps> = ({
  autostart,
  onToggleAutostart,
  lang,
  onSelectLanguage,
  theme,
  onSelectTheme,
  onOpenLogs,
}) => {
  const t = translations[lang];

  const [hotkeys, setHotkeys] = useState<HotkeysConfig>(DEFAULT_HOTKEYS);
  const [hotkeyStatus, setHotkeyStatus] = useState<Record<string, boolean>>({});
  const [recordingAction, setRecordingAction] = useState<keyof HotkeysConfig | null>(null);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  const [visualizer, setVisualizer] = useState<WallpaperVisualizerConfig>({
    enabled: true,
    style: 'bars',
    bars_count: 48,
    height_px: 160,
    opacity: 0.85,
    accent_color: 'spotify',
  });

  const fetchVisualizer = useCallback(async () => {
    try {
      const resp = await requestDaemon<WallpaperVisualizerConfig>('visualizer.getConfig');
      if (resp) {
        setVisualizer(resp);
      }
    } catch (err) {
      console.warn('Failed to fetch visualizer config:', err);
    }
  }, []);

  const handleUpdateVisualizer = async (partial: Partial<WallpaperVisualizerConfig>) => {
    const updated = { ...visualizer, ...partial };
    setVisualizer(updated);
    try {
      await requestDaemon('visualizer.setConfig', { visualizer: updated });
      setToastMessage(t.visualizer_saved_toast);
      setTimeout(() => setToastMessage(null), 2500);
    } catch (err) {
      console.error('Failed to save visualizer config:', err);
    }
  };

  const fetchHotkeys = useCallback(async () => {
    try {
      const resp = await requestDaemon<{ hotkeys: HotkeysConfig; status: Record<string, boolean> }>('hotkeys.get');
      if (resp?.hotkeys) {
        setHotkeys(resp.hotkeys);
      }
      if (resp?.status) {
        setHotkeyStatus(resp.status);
      }
    } catch (err) {
      console.warn('Failed to fetch hotkeys:', err);
    }
  }, []);

  useEffect(() => {
    fetchHotkeys();
    fetchVisualizer();
  }, [fetchHotkeys, fetchVisualizer]);

  // Handle global key recording
  useEffect(() => {
    if (!recordingAction) return;

    const handleKeyDown = async (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === 'Escape') {
        setRecordingAction(null);
        return;
      }

      // Ignore lone modifier keys
      if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) {
        return;
      }

      const parts: string[] = [];
      if (e.ctrlKey) parts.push('Ctrl');
      if (e.altKey) parts.push('Alt');
      if (e.shiftKey) parts.push('Shift');
      if (e.metaKey) parts.push('Win');

      let keyName = e.key.toUpperCase();
      if (e.code === 'Space') keyName = 'Space';
      else if (e.code === 'ArrowRight') keyName = 'Right';
      else if (e.code === 'ArrowLeft') keyName = 'Left';
      else if (e.code === 'ArrowUp') keyName = 'Up';
      else if (e.code === 'ArrowDown') keyName = 'Down';
      else if (e.code.startsWith('Key')) keyName = e.code.replace('Key', '');
      else if (e.code.startsWith('Digit')) keyName = e.code.replace('Digit', '');

      parts.push(keyName);
      const newHotkeyStr = parts.join('+');

      const updatedHotkeys = {
        ...hotkeys,
        [recordingAction]: newHotkeyStr,
      };

      setHotkeys(updatedHotkeys);
      setRecordingAction(null);

      try {
        await requestDaemon('hotkeys.set', { hotkeys: updatedHotkeys });
        setToastMessage(t.hk_saved_toast);
        setTimeout(() => setToastMessage(null), 3000);
        await fetchHotkeys();
      } catch (err) {
        console.error('Failed to set hotkeys:', err);
      }
    };

    window.addEventListener('keydown', handleKeyDown, { capture: true });
    return () => {
      window.removeEventListener('keydown', handleKeyDown, { capture: true });
    };
  }, [recordingAction, hotkeys, t.hk_saved_toast, fetchHotkeys]);

  const handleResetHotkeys = async () => {
    try {
      const resp = await requestDaemon<{ hotkeys: HotkeysConfig }>('hotkeys.reset');
      if (resp?.hotkeys) {
        setHotkeys(resp.hotkeys);
      }
      setToastMessage(t.hk_saved_toast);
      setTimeout(() => setToastMessage(null), 3000);
      await fetchHotkeys();
    } catch (err) {
      console.error('Failed to reset hotkeys:', err);
    }
  };

  const hotkeyItems: Array<{
    key: keyof HotkeysConfig;
    title: string;
    value: string;
  }> = [
    { key: 'toggle_playback', title: t.hk_toggle_playback, value: hotkeys.toggle_playback },
    { key: 'next_track', title: t.hk_next_track, value: hotkeys.next_track },
    { key: 'prev_track', title: t.hk_prev_track, value: hotkeys.prev_track },
    { key: 'toggle_wallpaper_pause', title: t.hk_toggle_wallpaper_pause, value: hotkeys.toggle_wallpaper_pause },
    { key: 'next_wallpaper', title: t.hk_next_wallpaper, value: hotkeys.next_wallpaper },
    { key: 'toggle_widget', title: t.hk_toggle_widget, value: hotkeys.toggle_widget },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '18px', overflowY: 'auto' }}>
      <div className="content-header" style={{ padding: '0 0 16px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.settings_title}</h1>
          <p>{t.settings_subtitle}</p>
        </div>
      </div>

      {toastMessage && (
        <div
          style={{
            background: 'rgba(16, 185, 129, 0.2)',
            border: '1px solid rgba(16, 185, 129, 0.4)',
            color: '#10b981',
            borderRadius: '10px',
            padding: '10px 16px',
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            fontSize: '0.88rem',
            animation: 'fadeIn 0.2s ease',
          }}
        >
          <Check size={16} />
          <span>{toastMessage}</span>
        </div>
      )}

      <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
        {/* Autostart */}
        <div
          className="glass-card"
          style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '18px 24px' }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
            <Rocket size={20} color="#6366f1" />
            <div>
              <div style={{ fontWeight: 600, fontSize: '0.92rem' }}>{t.setting_autostart}</div>
              <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                Запускать фоновый демон wallmus автоматически при входе пользователя.
              </p>
            </div>
          </div>
          <input
            type="checkbox"
            checked={autostart}
            onChange={(e) => onToggleAutostart(e.target.checked)}
            style={{ width: 20, height: 20, accentColor: '#6366f1', cursor: 'pointer' }}
          />
        </div>

        {/* Global Hotkeys Configuration */}
        <div className="glass-card" style={{ padding: '20px 24px' }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '16px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
              <Keyboard size={22} color="#f59e0b" />
              <div>
                <div style={{ fontWeight: 600, fontSize: '0.96rem' }}>{t.hotkeys_title}</div>
                <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                  {t.hotkeys_subtitle}
                </p>
              </div>
            </div>
            <button className="btn btn-secondary btn-sm" onClick={handleResetHotkeys} title={t.hk_reset_btn}>
              <RotateCcw size={14} />
              <span>{t.hk_reset_btn}</span>
            </button>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(320px, 1fr))', gap: '12px' }}>
            {hotkeyItems.map((item) => {
              const isRecording = recordingAction === item.key;
              const isConflict = hotkeyStatus[item.key] === false;

              return (
                <div
                  key={item.key}
                  style={{
                    background: isRecording ? 'rgba(99, 102, 241, 0.15)' : 'var(--bg-glass-card)',
                    border: isRecording
                      ? '1px solid #6366f1'
                      : isConflict
                      ? '1px solid rgba(239, 68, 68, 0.4)'
                      : '1px solid var(--border-subtle)',
                    borderRadius: '10px',
                    padding: '12px 16px',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    transition: 'all 0.2s ease',
                  }}
                >
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '3px' }}>
                    <span style={{ fontSize: '0.86rem', fontWeight: 500 }}>{item.title}</span>
                    <span style={{ fontSize: '0.72rem', color: isConflict ? '#ef4444' : 'var(--text-muted)' }}>
                      {isConflict ? (
                        <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                          <AlertCircle size={12} /> {t.hk_status_conflict}
                        </span>
                      ) : (
                        <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                          <Check size={12} color="#10b981" /> {t.hk_status_registered}
                        </span>
                      )}
                    </span>
                  </div>

                  <button
                    onClick={() => setRecordingAction(isRecording ? null : item.key)}
                    style={{
                      background: isRecording ? '#6366f1' : 'rgba(255, 255, 255, 0.08)',
                      color: isRecording ? '#ffffff' : 'var(--text-main)',
                      border: '1px solid rgba(255, 255, 255, 0.12)',
                      borderRadius: '6px',
                      padding: '6px 12px',
                      fontSize: '0.8rem',
                      fontFamily: 'monospace',
                      fontWeight: 600,
                      cursor: 'pointer',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '6px',
                      boxShadow: '0 2px 5px rgba(0,0,0,0.2)',
                    }}
                    title={isRecording ? 'Нажмите комбинацию клавиш' : t.hk_press_to_change}
                  >
                    <Keyboard size={13} opacity={0.7} />
                    <span>{isRecording ? t.hk_recording_hint : item.value}</span>
                  </button>
                </div>
              );
            })}
          </div>
        </div>

        {/* Audio Visualizer Layer Section */}
        <div className="glass-card" style={{ padding: '24px' }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '16px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
              <Activity size={22} color="#06b6d4" />
              <div>
                <div style={{ fontWeight: 600, fontSize: '0.98rem' }}>{t.visualizer_title}</div>
                <p style={{ fontSize: '0.82rem', color: 'var(--text-muted)' }}>
                  {t.visualizer_subtitle}
                </p>
              </div>
            </div>
            <button
              className={`btn btn-sm ${visualizer.enabled ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => handleUpdateVisualizer({ enabled: !visualizer.enabled })}
            >
              {visualizer.enabled ? (lang === 'ru' ? 'Включен' : 'Enabled') : (lang === 'ru' ? 'Выключен' : 'Disabled')}
            </button>
          </div>

          {visualizer.enabled && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: '20px', marginTop: '16px', borderTop: '1px solid rgba(255,255,255,0.08)', paddingTop: '18px' }}>
              {/* Style selector */}
              <div>
                <label style={{ fontSize: '0.85rem', fontWeight: 600, display: 'block', marginBottom: '8px', color: 'var(--text-secondary)' }}>
                  {t.visualizer_style}
                </label>
                <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '10px' }}>
                  {[
                    { key: 'bars', label: t.visualizer_style_bars, icon: <BarChart2 size={16} /> },
                    { key: 'wave', label: t.visualizer_style_wave, icon: <Waves size={16} /> },
                    { key: 'glow', label: t.visualizer_style_glow, icon: <Sparkles size={16} /> },
                  ].map((s) => (
                    <button
                      key={s.key}
                      className={`btn btn-sm ${visualizer.style === s.key ? 'btn-primary' : 'btn-secondary'}`}
                      style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '8px', padding: '10px 14px' }}
                      onClick={() => handleUpdateVisualizer({ style: s.key as 'bars' | 'wave' | 'glow' })}
                    >
                      {s.icon}
                      <span>{s.label}</span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Color Palette */}
              <div>
                <label style={{ fontSize: '0.85rem', fontWeight: 600, display: 'block', marginBottom: '8px', color: 'var(--text-secondary)' }}>
                  {t.visualizer_palette}
                </label>
                <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '10px' }}>
                  {[
                    { key: 'spotify', label: t.visualizer_palette_spotify, gradient: 'linear-gradient(135deg, #1DB954, #1ED760)' },
                    { key: 'cyan_purple', label: t.visualizer_palette_cyan_purple, gradient: 'linear-gradient(135deg, #06B6D4, #8B5CF6)' },
                    { key: 'neon_blue', label: t.visualizer_palette_neon_blue, gradient: 'linear-gradient(135deg, #2563EB, #38BDF8)' },
                    { key: 'fire', label: t.visualizer_palette_fire, gradient: 'linear-gradient(135deg, #F59E0B, #EF4444)' },
                  ].map((p) => {
                    const isSelected = visualizer.accent_color === p.key;
                    return (
                      <button
                        key={p.key}
                        onClick={() => handleUpdateVisualizer({ accent_color: p.key as 'spotify' | 'cyan_purple' | 'neon_blue' | 'fire' })}
                        style={{
                          background: isSelected ? 'rgba(255, 255, 255, 0.12)' : 'rgba(255, 255, 255, 0.04)',
                          border: isSelected ? '2px solid var(--accent-color, #06b6d4)' : '1px solid rgba(255, 255, 255, 0.1)',
                          borderRadius: '8px',
                          padding: '10px',
                          display: 'flex',
                          flexDirection: 'column',
                          alignItems: 'center',
                          gap: '6px',
                          cursor: 'pointer',
                          color: '#fff',
                        }}
                      >
                        <div style={{ width: '28px', height: '12px', borderRadius: '4px', background: p.gradient }} />
                        <span style={{ fontSize: '0.75rem', fontWeight: 500 }}>{p.label}</span>
                      </button>
                    );
                  })}
                </div>
              </div>

              {/* Sliders: Bars Count, Height, Opacity */}
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '16px' }}>
                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '6px' }}>
                    <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>{t.visualizer_bars_count}</span>
                    <span style={{ fontSize: '0.8rem', fontWeight: 600 }}>{visualizer.bars_count}</span>
                  </div>
                  <input
                    type="range"
                    min="16"
                    max="128"
                    step="8"
                    value={visualizer.bars_count}
                    onChange={(e) => handleUpdateVisualizer({ bars_count: parseInt(e.target.value, 10) })}
                    style={{ width: '100%', accentColor: 'var(--accent-color, #06b6d4)' }}
                  />
                </div>

                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '6px' }}>
                    <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>{t.visualizer_height}</span>
                    <span style={{ fontSize: '0.8rem', fontWeight: 600 }}>{visualizer.height_px} px</span>
                  </div>
                  <input
                    type="range"
                    min="40"
                    max="400"
                    step="10"
                    value={visualizer.height_px}
                    onChange={(e) => handleUpdateVisualizer({ height_px: parseInt(e.target.value, 10) })}
                    style={{ width: '100%', accentColor: 'var(--accent-color, #06b6d4)' }}
                  />
                </div>

                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '6px' }}>
                    <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>{t.visualizer_opacity}</span>
                    <span style={{ fontSize: '0.8rem', fontWeight: 600 }}>{Math.round(visualizer.opacity * 100)}%</span>
                  </div>
                  <input
                    type="range"
                    min="0.1"
                    max="1.0"
                    step="0.05"
                    value={visualizer.opacity}
                    onChange={(e) => handleUpdateVisualizer({ opacity: parseFloat(e.target.value) })}
                    style={{ width: '100%', accentColor: 'var(--accent-color, #06b6d4)' }}
                  />
                </div>
              </div>

              {/* Mini Visualizer Live Preview */}
              <div
                style={{
                  background: 'rgba(0, 0, 0, 0.35)',
                  borderRadius: '10px',
                  padding: '16px 20px',
                  border: '1px solid rgba(255, 255, 255, 0.08)',
                  display: 'flex',
                  alignItems: 'flex-end',
                  justifyContent: 'center',
                  gap: '4px',
                  height: '70px',
                  overflow: 'hidden',
                  position: 'relative',
                }}
              >
                <div style={{ position: 'absolute', top: '8px', left: '12px', fontSize: '0.7rem', color: 'rgba(255,255,255,0.4)', textTransform: 'uppercase', letterSpacing: '0.5px' }}>
                  Live Preview
                </div>
                {Array.from({ length: 32 }).map((_, i) => {
                  const factor = Math.sin((i / 31) * Math.PI) * 0.8 + 0.2;
                  const h = Math.max(4, factor * 48);
                  const gradient =
                    visualizer.accent_color === 'cyan_purple'
                      ? 'linear-gradient(to top, #06b6d4, #8b5cf6)'
                      : visualizer.accent_color === 'neon_blue'
                      ? 'linear-gradient(to top, #2563eb, #38bdf8)'
                      : visualizer.accent_color === 'fire'
                      ? 'linear-gradient(to top, #f59e0b, #ef4444)'
                      : 'linear-gradient(to top, #1db954, #1ed760)';
                  return (
                    <div
                      key={i}
                      style={{
                        width: '6px',
                        height: visualizer.style === 'glow' ? `${factor * 20 + 4}px` : `${h}px`,
                        borderRadius: visualizer.style === 'glow' ? '2px' : '3px 3px 0 0',
                        background: gradient,
                        opacity: visualizer.opacity,
                        transition: 'height 0.2s ease',
                      }}
                    />
                  );
                })}
              </div>
            </div>
          )}
        </div>

        {/* Language Selection */}
        <div
          className="glass-card"
          style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '18px 24px' }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
            <Languages size={20} color="#06b6d4" />
            <div>
              <div style={{ fontWeight: 600, fontSize: '0.92rem' }}>{t.setting_language}</div>
              <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                Язык отображения интерфейса настроек и виджета.
              </p>
            </div>
          </div>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              className={`btn btn-sm ${lang === 'ru' ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => onSelectLanguage('ru')}
            >
              Русский (RU)
            </button>
            <button
              className={`btn btn-sm ${lang === 'en' ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => onSelectLanguage('en')}
            >
              English (EN)
            </button>
          </div>
        </div>

        {/* Color Theme */}
        <div
          className="glass-card"
          style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '18px 24px' }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
            <Palette size={20} color="#ec4899" />
            <div>
              <div style={{ fontWeight: 600, fontSize: '0.92rem' }}>{t.setting_theme}</div>
              <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                Тёмная тема со стеклянным размытием Mica/Acrylic или светлая.
              </p>
            </div>
          </div>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              className={`btn btn-sm ${theme === 'dark' ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => onSelectTheme('dark')}
            >
              {t.theme_dark}
            </button>
            <button
              className={`btn btn-sm ${theme === 'light' ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => onSelectTheme('light')}
            >
              {t.theme_light}
            </button>
          </div>
        </div>

        {/* Application Logs */}
        <div
          className="glass-card"
          style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '18px 24px' }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
            <FileText size={20} color="#10b981" />
            <div>
              <div style={{ fontWeight: 600, fontSize: '0.92rem' }}>{t.setting_logs}</div>
              <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                Логи ведутся в %LOCALAPPDATA%\wallmus\logs\ с суточной ротацией.
              </p>
            </div>
          </div>
          <button className="btn btn-secondary btn-sm" onClick={onOpenLogs}>
            <FolderOpen size={15} />
            <span>{t.open_logs_btn}</span>
          </button>
        </div>
      </div>
    </div>
  );
};
