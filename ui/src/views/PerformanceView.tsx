import React from 'react';
import {
  Layers,
  Battery,
  Lock,
  AppWindow,
  MonitorOff,
} from 'lucide-react';
import { DiagReport, GovernorConfig } from '../types';
import { Language, translations } from '../i18n';

interface PerformanceViewProps {
  governorConfig: GovernorConfig;
  onUpdateGovernor: (newConfig: GovernorConfig) => Promise<void>;
  fpsLimit: number;
  onUpdateFpsLimit: (fps: number) => Promise<void>;
  diag?: DiagReport;
  lang: Language;
}

export const PerformanceView: React.FC<PerformanceViewProps> = ({
  governorConfig,
  onUpdateGovernor,
  fpsLimit,
  onUpdateFpsLimit,
  diag,
  lang,
}) => {
  const t = translations[lang];

  const toggleTrigger = async (key: keyof GovernorConfig) => {
    const updated = {
      ...governorConfig,
      [key]: !governorConfig[key],
    };
    await onUpdateGovernor(updated);
  };

  const triggers = [
    {
      key: 'pause_on_fullscreen' as const,
      label: t.gov_fullscreen,
      icon: <AppWindow size={18} color="#6366f1" />,
      active: governorConfig.pause_on_fullscreen,
    },
    {
      key: 'pause_on_desktop_covered' as const,
      label: t.gov_covered,
      icon: <Layers size={18} color="#06b6d4" />,
      active: governorConfig.pause_on_desktop_covered,
    },
    {
      key: 'pause_on_lock' as const,
      label: t.gov_lock,
      icon: <Lock size={18} color="#ec4899" />,
      active: governorConfig.pause_on_lock,
    },
    {
      key: 'pause_on_display_off' as const,
      label: t.gov_display_off,
      icon: <MonitorOff size={18} color="#f59e0b" />,
      active: governorConfig.pause_on_display_off,
    },
    {
      key: 'pause_on_battery' as const,
      label: t.gov_battery,
      icon: <Battery size={18} color="#10b981" />,
      active: governorConfig.pause_on_battery,
    },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '18px' }}>
      <div className="content-header" style={{ padding: '0 0 16px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.performance_title}</h1>
          <p>{t.performance_subtitle}</p>
        </div>
      </div>

      {/* Governor Auto-Pause Policy */}
      <div className="glass-card" style={{ display: 'flex', flexDirection: 'column', gap: '14px', padding: '20px' }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <h3 style={{ fontSize: '1rem', fontWeight: 600 }}>Governor Policy (Автопауза)</h3>
          <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
            <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>Лимит FPS:</span>
            <select
              value={fpsLimit}
              onChange={(e) => onUpdateFpsLimit(parseInt(e.target.value))}
              style={{
                background: 'rgba(255, 255, 255, 0.08)',
                color: 'white',
                border: '1px solid var(--border-subtle)',
                borderRadius: 'var(--radius-sm)',
                padding: '4px 8px',
                fontSize: '0.85rem',
              }}
            >
              <option value="20" style={{ background: '#121622' }}>20 FPS</option>
              <option value="30" style={{ background: '#121622' }}>30 FPS (Рекомендуется)</option>
              <option value="60" style={{ background: '#121622' }}>60 FPS</option>
              <option value="0" style={{ background: '#121622' }}>Без ограничений</option>
            </select>
          </div>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
          {triggers.map((trig) => (
            <div
              key={trig.key}
              onClick={() => toggleTrigger(trig.key)}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '12px 16px',
                borderRadius: 'var(--radius-md)',
                background: 'rgba(255, 255, 255, 0.03)',
                border: '1px solid var(--border-subtle)',
                cursor: 'pointer',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                {trig.icon}
                <span style={{ fontSize: '0.88rem', fontWeight: 500 }}>{trig.label}</span>
              </div>
              <input
                type="checkbox"
                checked={trig.active}
                onChange={() => {}}
                style={{ width: 18, height: 18, accentColor: '#6366f1', cursor: 'pointer' }}
              />
            </div>
          ))}
        </div>
      </div>

      {/* Live System Diagnostics */}
      <div className="glass-card" style={{ display: 'flex', flexDirection: 'column', gap: '12px', padding: '20px' }}>
        <h3 style={{ fontSize: '1rem', fontWeight: 600 }}>{t.live_diag_title}</h3>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '14px' }}>
          <div style={{ padding: '12px', background: 'rgba(255, 255, 255, 0.03)', borderRadius: 'var(--radius-md)' }}>
            <span style={{ fontSize: '0.75rem', color: 'var(--text-dim)' }}>{t.diag_os}</span>
            <div style={{ fontSize: '0.9rem', fontWeight: 600, marginTop: '4px' }}>
              {diag?.os_product_name || 'Windows 11'} ({diag?.os_build || '24H2'})
            </div>
          </div>

          <div style={{ padding: '12px', background: 'rgba(255, 255, 255, 0.03)', borderRadius: 'var(--radius-md)' }}>
            <span style={{ fontSize: '0.75rem', color: 'var(--text-dim)' }}>{t.diag_topology}</span>
            <div style={{ fontSize: '0.9rem', fontWeight: 600, marginTop: '4px', color: '#10b981' }}>
              {typeof diag?.topology === 'string' ? diag.topology : 'Raised (24H2+)'}
            </div>
          </div>

          <div style={{ padding: '12px', background: 'rgba(255, 255, 255, 0.03)', borderRadius: 'var(--radius-md)' }}>
            <span style={{ fontSize: '0.75rem', color: 'var(--text-dim)' }}>{t.diag_gpu}</span>
            <div style={{ fontSize: '0.9rem', fontWeight: 600, marginTop: '4px' }}>
              {diag?.gpu_adapter || 'DirectX 11 Hardware'}
            </div>
          </div>

          <div style={{ padding: '12px', background: 'rgba(255, 255, 255, 0.03)', borderRadius: 'var(--radius-md)' }}>
            <span style={{ fontSize: '0.75rem', color: 'var(--text-dim)' }}>{t.diag_mpv}</span>
            <div style={{ fontSize: '0.9rem', fontWeight: 600, marginTop: '4px', color: '#06b6d4' }}>
              {diag?.libmpv_version || 'libmpv d3d11va'}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
