import React, { useState, useEffect, useCallback } from 'react';
import {
  Rocket,
  Languages,
  Palette,
  FolderOpen,
  Keyboard,
  RotateCcw,
  Check,
  RefreshCw,
  Download,
} from 'lucide-react';
import { Language, translations } from '../i18n';
import { HotkeysConfig, UpdateInfo } from '../types';
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

  const [isPortable, setIsPortable] = useState<boolean>(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState<boolean>(false);

  const fetchPortableStatus = useCallback(async () => {
    try {
      const resp = await requestDaemon<{ portable: boolean }>('app.isPortable');
      if (resp && typeof resp.portable === 'boolean') {
        setIsPortable(resp.portable);
      }
    } catch (err) {
      console.warn('Failed to fetch portable status:', err);
    }
  }, []);

  const handleCheckUpdate = async () => {
    setIsCheckingUpdate(true);
    try {
      const info = await requestDaemon<UpdateInfo>('update.check');
      if (info) {
        setUpdateInfo(info);
      }
    } catch (err) {
      console.error('Failed to check for updates:', err);
    } finally {
      setIsCheckingUpdate(false);
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
    fetchPortableStatus();
  }, [fetchHotkeys, fetchPortableStatus]);

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
                {lang === 'ru'
                  ? 'Запускать фоновый демон wallmus автоматически при входе пользователя.'
                  : 'Start background wallmus daemon automatically on Windows login.'}
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

        {/* Language & App Theme */}
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))',
            gap: '16px',
          }}
        >
          {/* Language Selector */}
          <div className="glass-card" style={{ padding: '18px 24px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
              <Languages size={18} color="#06b6d4" />
              <div>
                <div style={{ fontWeight: 600, fontSize: '0.92rem' }}>{t.setting_language}</div>
                <p style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>Русский / English</p>
              </div>
            </div>
            <div className="btn-group" style={{ display: 'flex', width: '100%' }}>
              <button
                className={`btn btn-sm ${lang === 'ru' ? 'btn-primary' : 'btn-ghost'}`}
                style={{ flex: 1, padding: '7px 12px' }}
                onClick={() => onSelectLanguage('ru')}
              >
                Русский
              </button>
              <button
                className={`btn btn-sm ${lang === 'en' ? 'btn-primary' : 'btn-ghost'}`}
                style={{ flex: 1, padding: '7px 12px' }}
                onClick={() => onSelectLanguage('en')}
              >
                English
              </button>
            </div>
          </div>

          {/* Theme Selector */}
          <div className="glass-card" style={{ padding: '18px 24px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
              <Palette size={18} color="#a855f7" />
              <div>
                <div style={{ fontWeight: 600, fontSize: '0.92rem' }}>{t.setting_theme}</div>
                <p style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>
                  {lang === 'ru' ? 'Светлая или темная тема оформления' : 'Dark or light UI appearance'}
                </p>
              </div>
            </div>
            <div className="btn-group" style={{ display: 'flex', width: '100%' }}>
              <button
                className={`btn btn-sm ${theme === 'dark' ? 'btn-primary' : 'btn-ghost'}`}
                style={{ flex: 1, padding: '7px 12px' }}
                onClick={() => onSelectTheme('dark')}
              >
                {t.theme_dark}
              </button>
              <button
                className={`btn btn-sm ${theme === 'light' ? 'btn-primary' : 'btn-ghost'}`}
                style={{ flex: 1, padding: '7px 12px' }}
                onClick={() => onSelectTheme('light')}
              >
                {t.theme_light}
              </button>
            </div>
          </div>
        </div>

        {/* Global Hotkeys */}
        <div className="glass-card" style={{ padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
              <Keyboard size={20} color="#00F5D4" />
              <div>
                <div style={{ fontWeight: 600, fontSize: '0.94rem' }}>{t.hk_title}</div>
                <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>{t.hk_subtitle}</p>
              </div>
            </div>
            <button className="btn btn-sm btn-ghost" onClick={handleResetHotkeys}>
              <RotateCcw size={14} />
              <span>{t.hk_reset_btn}</span>
            </button>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '12px' }}>
            {hotkeyItems.map((item) => {
              const isRegistered = hotkeyStatus[item.key] ?? true;
              const isRecording = recordingAction === item.key;

              return (
                <div
                  key={item.key}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    padding: '12px 16px',
                    background: 'rgba(0, 0, 0, 0.25)',
                    borderRadius: 'var(--radius-md)',
                    border: '1px solid var(--border-subtle)',
                  }}
                >
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
                    <span style={{ fontSize: '0.86rem', fontWeight: 500 }}>{item.title}</span>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                      <span
                        style={{
                          width: '6px',
                          height: '6px',
                          borderRadius: '50%',
                          background: isRegistered ? '#10b981' : '#ef4444',
                        }}
                      />
                      <span style={{ fontSize: '0.72rem', color: isRegistered ? '#10b981' : '#ef4444' }}>
                        {isRegistered ? t.hk_status_registered : t.hk_status_conflict}
                      </span>
                    </div>
                  </div>

                  <button
                    className={`btn btn-sm ${isRecording ? 'btn-primary' : 'btn-ghost'}`}
                    style={{
                      fontFamily: 'monospace',
                      fontSize: '0.84rem',
                      padding: '4px 12px',
                      border: isRecording ? '1px solid var(--accent-primary)' : '1px solid var(--border-subtle)',
                      background: isRecording ? 'rgba(0, 245, 212, 0.2)' : 'rgba(255, 255, 255, 0.05)',
                      color: isRecording ? 'var(--accent-primary)' : 'var(--text-primary)',
                      animation: isRecording ? 'pulse 1s infinite' : 'none',
                    }}
                    onClick={() => setRecordingAction(isRecording ? null : item.key)}
                  >
                    {isRecording ? t.hk_press_keys_prompt : item.value}
                  </button>
                </div>
              );
            })}
          </div>
        </div>

        {/* Updates & Installation Status */}
        <div className="glass-card" style={{ padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '14px' }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
              <RefreshCw size={20} color="#6366f1" />
              <div>
                <div style={{ fontWeight: 600, fontSize: '0.94rem' }}>{t.update_title}</div>
                <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>{t.update_subtitle}</p>
              </div>
            </div>
            <button className="btn btn-sm btn-ghost" onClick={handleCheckUpdate} disabled={isCheckingUpdate}>
              <RefreshCw size={14} className={isCheckingUpdate ? 'spin' : ''} />
              <span>{isCheckingUpdate ? t.update_checking : t.update_check_btn}</span>
            </button>
          </div>

          <div
            style={{
              padding: '12px 16px',
              borderRadius: 'var(--radius-md)',
              background: 'rgba(0, 0, 0, 0.25)',
              border: '1px solid var(--border-subtle)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
            }}
          >
            <div>
              <span style={{ fontSize: '0.86rem', fontWeight: 500 }}>
                {isPortable ? t.update_portable_active : t.update_installed_mode}
              </span>
              <p style={{ fontSize: '0.74rem', color: 'var(--text-dim)', marginTop: '2px' }}>
                {isPortable
                  ? t.update_portable_desc
                  : (lang === 'ru' ? 'Конфигурация и кэш хранятся в %APPDATA%/wallmus' : 'Settings and cache stored in %APPDATA%/wallmus')}
              </p>
            </div>
            <span className="badge">{isPortable ? 'Portable' : 'Installed'}</span>
          </div>

          {updateInfo && (
            <div
              style={{
                padding: '14px 16px',
                borderRadius: 'var(--radius-md)',
                background: updateInfo.update_available ? 'rgba(0, 245, 212, 0.1)' : 'rgba(255, 255, 255, 0.03)',
                border: updateInfo.update_available ? '1px solid rgba(0, 245, 212, 0.3)' : '1px solid var(--border-subtle)',
                display: 'flex',
                flexDirection: 'column',
                gap: '8px',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <span style={{ fontSize: '0.88rem', fontWeight: 600 }}>
                  {updateInfo.update_available ? t.update_available : t.update_latest}
                </span>
                <span className="badge">v{updateInfo.latest_version || '0.1.0'}</span>
              </div>
              {updateInfo.update_available && updateInfo.download_url && (
                <a
                  href={updateInfo.download_url}
                  target="_blank"
                  rel="noreferrer"
                  className="btn btn-sm btn-primary"
                  style={{ alignSelf: 'flex-start', display: 'flex', alignItems: 'center', gap: '6px' }}
                >
                  <Download size={14} />
                  <span>{t.update_download_btn}</span>
                </a>
              )}
            </div>
          )}
        </div>

        {/* System Logs */}
        <div
          className="glass-card"
          style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '18px 24px' }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
            <FolderOpen size={20} color="#10b981" />
            <div>
              <div style={{ fontWeight: 600, fontSize: '0.92rem' }}>{t.setting_logs}</div>
              <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                {lang === 'ru'
                  ? 'Открыть директорию журналов событий и отладочных дампов'
                  : 'Open the directory containing application event logs and debug dumps'}
              </p>
            </div>
          </div>
          <button className="btn btn-sm btn-ghost" onClick={onOpenLogs}>
            <FolderOpen size={14} />
            <span>{t.btn_open_logs}</span>
          </button>
        </div>
      </div>
    </div>
  );
};
