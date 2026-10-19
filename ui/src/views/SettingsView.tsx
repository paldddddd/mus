import React from 'react';
import {
  Rocket,
  Languages,
  Palette,
  FileText,
  FolderOpen,
} from 'lucide-react';
import { Language, translations } from '../i18n';

interface SettingsViewProps {
  autostart: boolean;
  onToggleAutostart: (enabled: boolean) => Promise<void>;
  lang: Language;
  onSelectLanguage: (lang: Language) => void;
  theme: 'dark' | 'light';
  onSelectTheme: (theme: 'dark' | 'light') => void;
  onOpenLogs: () => Promise<void>;
}

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

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '18px' }}>
      <div className="content-header" style={{ padding: '0 0 16px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.settings_title}</h1>
          <p>{t.settings_subtitle}</p>
        </div>
      </div>

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
