import React from 'react';
import { Monitor, LayoutGrid, Split, StretchHorizontal } from 'lucide-react';
import { MonitorInfo } from '../types';
import { Language, translations } from '../i18n';

interface MonitorsViewProps {
  monitors: MonitorInfo[];
  lang: Language;
}

export const MonitorsView: React.FC<MonitorsViewProps> = ({ monitors, lang }) => {
  const t = translations[lang];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '18px' }}>
      <div className="content-header" style={{ padding: '0 0 16px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.monitors_title}</h1>
          <p>{t.monitors_subtitle}</p>
        </div>
      </div>

      {/* Visual Displays Canvas */}
      <div
        className="glass-card"
        style={{
          height: '240px',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          gap: '24px',
          background: 'rgba(0, 0, 0, 0.3)',
          border: '1px solid rgba(255, 255, 255, 0.08)',
        }}
      >
        {monitors.map((m, idx) => (
          <div
            key={m.id || idx}
            style={{
              width: `${Math.max(160, Math.min(260, m.width / 12))}px`,
              height: `${Math.max(100, Math.min(160, m.height / 12))}px`,
              background: 'linear-gradient(135deg, rgba(99, 102, 241, 0.2), rgba(6, 182, 212, 0.1))',
              border: m.is_primary ? '2px solid var(--accent-primary)' : '1px solid var(--border-subtle)',
              borderRadius: 'var(--radius-md)',
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              gap: '6px',
              boxShadow: m.is_primary ? '0 0 16px var(--accent-glow)' : 'none',
              position: 'relative',
            }}
          >
            <Monitor size={28} color={m.is_primary ? '#6366f1' : '#94a3b8'} />
            <span style={{ fontSize: '0.85rem', fontWeight: 600 }}>{m.name}</span>
            <span style={{ fontSize: '0.72rem', color: 'var(--text-dim)' }}>
              {m.width} × {m.height} ({Math.round(m.scale_factor * 100)}%)
            </span>
            {m.is_primary && (
              <span
                style={{
                  position: 'absolute',
                  top: 6,
                  right: 6,
                  fontSize: '0.65rem',
                  fontWeight: 600,
                  background: 'var(--accent-primary)',
                  color: 'white',
                  padding: '2px 6px',
                  borderRadius: 'var(--radius-sm)',
                }}
              >
                {t.primary_monitor}
              </span>
            )}
          </div>
        ))}
      </div>

      {/* Multi-Monitor Modes */}
      <div className="glass-card" style={{ display: 'flex', flexDirection: 'column', gap: '14px', padding: '20px' }}>
        <h3 style={{ fontSize: '1rem', fontWeight: 600 }}>{t.monitors_subtitle}</h3>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: '14px' }}>
          <div
            style={{
              padding: '16px',
              borderRadius: 'var(--radius-md)',
              background: 'rgba(99, 102, 241, 0.1)',
              border: '1px solid var(--accent-primary)',
              cursor: 'pointer',
              display: 'flex',
              flexDirection: 'column',
              gap: '8px',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
              <LayoutGrid size={20} color="#6366f1" />
              <span style={{ fontWeight: 600, fontSize: '0.9rem' }}>{t.mode_same}</span>
            </div>
            <p style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>
              Одни и те же обои проигрываются синхронно на всех подключенных дисплеях.
            </p>
          </div>

          <div
            style={{
              padding: '16px',
              borderRadius: 'var(--radius-md)',
              background: 'rgba(255, 255, 255, 0.03)',
              border: '1px solid var(--border-subtle)',
              cursor: 'pointer',
              display: 'flex',
              flexDirection: 'column',
              gap: '8px',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
              <Split size={20} color="#94a3b8" />
              <span style={{ fontWeight: 600, fontSize: '0.9rem' }}>{t.mode_separate}</span>
            </div>
            <p style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>
              Каждый монитор имеет собственный независимый рендерер и выбор обоев.
            </p>
          </div>

          <div
            style={{
              padding: '16px',
              borderRadius: 'var(--radius-md)',
              background: 'rgba(255, 255, 255, 0.03)',
              border: '1px solid var(--border-subtle)',
              cursor: 'pointer',
              display: 'flex',
              flexDirection: 'column',
              gap: '8px',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
              <StretchHorizontal size={20} color="#94a3b8" />
              <span style={{ fontWeight: 600, fontSize: '0.9rem' }}>{t.mode_span}</span>
            </div>
            <p style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>
              Широкие обои растягиваются на всю виртуальную ширину нескольких мониторов.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
};
