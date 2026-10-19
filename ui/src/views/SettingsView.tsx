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
} from 'lucide-react';
import { Language, translations } from '../i18n';
import { HotkeysConfig } from '../types';
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
  }, [fetchHotkeys]);

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
