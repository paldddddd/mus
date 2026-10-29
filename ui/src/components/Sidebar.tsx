import React from 'react';
import {
  Link2,
  Image,
  Music,
  Sliders,
  Gauge,
  Settings,
  Languages,
  Sun,
  Moon,
  Sparkles,
} from 'lucide-react';
import { Language, translations } from '../i18n';

export type NavTab = 'bindings' | 'wallpapers' | 'music' | 'customization' | 'performance' | 'settings';

interface SidebarProps {
  currentTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  lang: Language;
  onToggleLang: () => void;
  theme: 'dark' | 'light';
  onToggleTheme: () => void;
  isDaemonOnline: boolean;
}

export const Sidebar: React.FC<SidebarProps> = ({
  currentTab,
  onSelectTab,
  lang,
  onToggleLang,
  theme,
  onToggleTheme,
  isDaemonOnline,
}) => {
  const t = translations[lang];

  const navItems: { id: NavTab; label: string; icon: React.ReactNode }[] = [
    { id: 'bindings', label: t.nav_bindings, icon: <Link2 size={18} /> },
    { id: 'wallpapers', label: t.nav_wallpapers, icon: <Image size={18} /> },
    { id: 'music', label: t.nav_music, icon: <Music size={18} /> },
    { id: 'customization', label: t.nav_customization, icon: <Sliders size={18} /> },
    { id: 'performance', label: t.nav_performance, icon: <Gauge size={18} /> },
    { id: 'settings', label: t.nav_settings, icon: <Settings size={18} /> },
  ];

  return (
    <aside className="sidebar">
      {/* Brand Header */}
      <div className="app-brand">
        <div className="brand-icon">
          <Sparkles size={18} />
        </div>
        <div className="brand-title">wallmus</div>
        <div className="brand-badge">Engine</div>
      </div>

      {/* Navigation Links */}
      <ul className="nav-list">
        {navItems.map((item) => (
          <li
            key={item.id}
            className={`nav-item ${currentTab === item.id ? 'active' : ''}`}
            onClick={() => onSelectTab(item.id)}
          >
            {item.icon}
            <span>{item.label}</span>
          </li>
        ))}
      </ul>

      {/* Footer Status & Switches */}
      <div className="sidebar-footer">
        <div className="status-badge">
          <div className={`status-dot ${isDaemonOnline ? '' : 'offline'}`} />
          <span>{isDaemonOnline ? t.daemon_online : t.daemon_offline}</span>
        </div>

        <div style={{ display: 'flex', gap: '8px' }}>
          <button
            className="btn btn-secondary btn-sm"
            style={{ flex: 1 }}
            onClick={onToggleLang}
            title="Toggle Language (RU / EN)"
          >
            <Languages size={14} />
            <span style={{ textTransform: 'uppercase', fontWeight: 600 }}>{lang}</span>
          </button>
          <button
            className="btn btn-secondary btn-sm"
            onClick={onToggleTheme}
            title="Toggle Theme"
          >
            {theme === 'dark' ? <Sun size={14} /> : <Moon size={14} />}
          </button>
        </div>
      </div>
    </aside>
  );
};
