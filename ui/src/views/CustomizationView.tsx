import React, { useState, useEffect, useCallback } from 'react';
import {
  Sliders,
  Sparkles,
  Music,
  Waves,
  BarChart2,
  Activity,
  MousePointer,
  Shuffle,
  Repeat,
  Layers,
  Magnet,
  Plus,
  Trash2,
  Copy,
  Download,
  Upload,
  Check,
  LayoutGrid,
  Monitor,
} from 'lucide-react';
import { Language, translations } from '../i18n';
import {
  CustomWidgetPreset,
  MonitorInfo,
  WallpaperVisualizerConfig,
} from '../types';
import { requestDaemon } from '../api/ipc';
import { MonitorsView } from './MonitorsView';

const BUILTIN_WIDGET_PRESETS: CustomWidgetPreset[] = [
  {
    id: 'builtin_obsidian_vert',
    name: 'Obsidian Vertical',
    theme: 'glass',
    visualizer_style: 'bars',
    dynamic_accent: true,
    custom_accent: '#00F5D4',
    corner_radius: 18,
    bg_opacity: 0.92,
    show_lyrics: true,
    show_visualizer: true,
    width: 290,
    height: 395,
  },
  {
    id: 'builtin_cyberpunk_wave',
    name: 'Cyberpunk Neon Wave',
    theme: 'glass',
    visualizer_style: 'wave',
    dynamic_accent: false,
    custom_accent: '#F43F5E',
    corner_radius: 16,
    bg_opacity: 0.90,
    show_lyrics: true,
    show_visualizer: true,
    width: 420,
    height: 165,
  },
  {
    id: 'builtin_studio_mirror',
    name: 'Studio Mirror EQ',
    theme: 'spotify',
    visualizer_style: 'mirror',
    dynamic_accent: false,
    custom_accent: '#1DB954',
    corner_radius: 20,
    bg_opacity: 0.95,
    show_lyrics: true,
    show_visualizer: true,
    width: 440,
    height: 170,
  },
  {
    id: 'builtin_retro_matrix',
    name: 'Retro LED Matrix',
    theme: 'vinyl',
    visualizer_style: 'dots',
    dynamic_accent: false,
    custom_accent: '#F59E0B',
    corner_radius: 14,
    bg_opacity: 0.96,
    show_lyrics: false,
    show_visualizer: true,
    width: 380,
    height: 155,
  },
  {
    id: 'builtin_minimal_clean',
    name: 'Minimal Clean (No EQ)',
    theme: 'glass',
    visualizer_style: 'bars',
    dynamic_accent: true,
    custom_accent: '#00F5D4',
    corner_radius: 16,
    bg_opacity: 0.88,
    show_lyrics: true,
    show_visualizer: false,
    width: 360,
    height: 130,
  },
];

interface CustomizationViewProps {
  monitors: MonitorInfo[];
  lang: Language;
  wallpaperChangeMode?: 'random' | 'sequential' | 'off';
  respectBindings?: boolean;
  onSetChangeMode?: (mode: 'random' | 'sequential' | 'off', respectBindings?: boolean) => Promise<void>;
}

export const CustomizationView: React.FC<CustomizationViewProps> = ({
  monitors,
  lang,
  wallpaperChangeMode = 'random',
  respectBindings = false,
  onSetChangeMode,
}) => {
  const t = translations[lang];
  const [subtab, setSubtab] = useState<'widget' | 'effects' | 'monitors'>('widget');
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  // Widget Settings
  const [widgetConfig, setWidgetConfig] = useState<{
    enabled: boolean;
    theme: string;
    width: number;
    height: number;
    opacity: number;
    pinned: boolean;
    visualizer_style: string;
    dynamic_accent: boolean;
    custom_accent: string;
    corner_radius: number;
    show_lyrics: boolean;
    show_visualizer: boolean;
    magnetic_snap: boolean;
    custom_presets: CustomWidgetPreset[];
  }>({
    enabled: true,
    theme: 'spotify',
    width: 380,
    height: 150,
    opacity: 0.95,
    pinned: false,
    visualizer_style: 'bars',
    dynamic_accent: true,
    custom_accent: '#00F5D4',
    corner_radius: 16,
    show_lyrics: true,
    show_visualizer: true,
    magnetic_snap: true,
    custom_presets: [],
  });

  const [newPresetName, setNewPresetName] = useState<string>('');
  const [importJsonText, setImportJsonText] = useState<string>('');
  const [showImportBox, setShowImportBox] = useState<boolean>(false);

  // Wallpaper Visualizer Settings
  const [visualizer, setVisualizer] = useState<WallpaperVisualizerConfig>({
    enabled: true,
    style: 'bars',
    bars_count: 48,
    height_px: 160,
    opacity: 0.85,
    accent_color: 'spotify',
  });

  // Wallpaper Bass & Mouse Interaction
  const [mouseEnabled, setMouseEnabled] = useState<boolean>(true);
  const [bassIntensity, setBassIntensity] = useState<number>(0.5);
  const [previewCoord, setPreviewCoord] = useState<{ x: number; y: number }>({ x: 50, y: 50 });

  const showToast = (msg: string) => {
    setToastMessage(msg);
    setTimeout(() => setToastMessage(null), 3000);
  };

  const fetchWidgetConfig = useCallback(async () => {
    try {
      const resp = await requestDaemon<any>('widget.getState');
      if (resp) {
        setWidgetConfig((prev) => ({
          ...prev,
          ...resp,
          visualizer_style: resp.visualizer_style || 'bars',
          dynamic_accent: resp.dynamic_accent ?? true,
          custom_accent: resp.custom_accent || '#00F5D4',
          corner_radius: resp.corner_radius ?? 16,
          show_lyrics: resp.show_lyrics ?? true,
          show_visualizer: resp.show_visualizer ?? true,
          magnetic_snap: resp.magnetic_snap ?? true,
          custom_presets: Array.isArray(resp.custom_presets) ? resp.custom_presets : [],
        }));
      }
    } catch (err) {
      console.warn('Failed to fetch widget config:', err);
    }
  }, []);

  const handleUpdateWidget = async (partial: Partial<typeof widgetConfig>) => {
    try {
      const updated = await requestDaemon<any>('widget.updateConfig', partial);
      if (updated) {
        setWidgetConfig((prev) => ({
          ...prev,
          ...updated,
          visualizer_style: updated.visualizer_style || 'bars',
          dynamic_accent: updated.dynamic_accent ?? true,
          custom_accent: updated.custom_accent || '#00F5D4',
          corner_radius: updated.corner_radius ?? 16,
          show_lyrics: updated.show_lyrics ?? true,
          show_visualizer: updated.show_visualizer ?? true,
          magnetic_snap: updated.magnetic_snap ?? true,
        }));
      }
    } catch (err) {
      console.error('Failed to update widget config:', err);
    }
  };

  const handleApplyPreset = async (preset: CustomWidgetPreset) => {
    try {
      const updated = await requestDaemon<any>('widget.applyPreset', { preset });
      if (updated) {
        setWidgetConfig((prev) => ({
          ...prev,
          ...updated,
          visualizer_style: updated.visualizer_style || preset.visualizer_style,
          dynamic_accent: updated.dynamic_accent ?? preset.dynamic_accent,
          custom_accent: updated.custom_accent || preset.custom_accent,
          corner_radius: updated.corner_radius ?? preset.corner_radius,
          show_lyrics: updated.show_lyrics ?? preset.show_lyrics,
          show_visualizer: updated.show_visualizer ?? preset.show_visualizer ?? true,
        }));
        showToast(lang === 'ru' ? `Пресет "${preset.name}" применен!` : `Preset "${preset.name}" applied!`);
      }
    } catch (err) {
      console.error('Failed to apply preset:', err);
    }
  };

  const handleSaveCurrentAsPreset = async () => {
    const name = newPresetName.trim() || (lang === 'ru' ? 'Мой стиль' : 'My Preset');
    const newPreset: CustomWidgetPreset = {
      id: `custom_${Date.now()}`,
      name,
      theme: widgetConfig.theme,
      visualizer_style: widgetConfig.visualizer_style,
      dynamic_accent: widgetConfig.dynamic_accent,
      custom_accent: widgetConfig.custom_accent,
      corner_radius: widgetConfig.corner_radius,
      bg_opacity: widgetConfig.opacity,
      show_lyrics: widgetConfig.show_lyrics,
      show_visualizer: widgetConfig.show_visualizer,
      width: widgetConfig.width,
      height: widgetConfig.height,
    };

    try {
      const updatedPresets = await requestDaemon<CustomWidgetPreset[]>('widget.savePreset', { preset: newPreset });
      if (updatedPresets) {
        setWidgetConfig((prev) => ({ ...prev, custom_presets: updatedPresets }));
      }
      setNewPresetName('');
      showToast(lang === 'ru' ? `Пресет "${name}" сохранен!` : `Preset "${name}" saved!`);
    } catch (err) {
      console.error('Failed to save preset:', err);
    }
  };

  const handleDeletePreset = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const updatedPresets = await requestDaemon<CustomWidgetPreset[]>('widget.deletePreset', { id });
      if (updatedPresets) {
        setWidgetConfig((prev) => ({ ...prev, custom_presets: updatedPresets }));
      }
    } catch (err) {
      console.error('Failed to delete preset:', err);
    }
  };

  const handleExportPresetsJson = () => {
    const data = JSON.stringify(widgetConfig.custom_presets, null, 2);
    navigator.clipboard.writeText(data).then(() => {
      showToast(lang === 'ru' ? 'JSON скопирован в буфер обмена!' : 'JSON copied to clipboard!');
    });
  };

  const handleImportPresetsJson = async () => {
    try {
      const parsed = JSON.parse(importJsonText);
      const list: CustomWidgetPreset[] = Array.isArray(parsed) ? parsed : [parsed];
      for (const p of list) {
        if (p.name && p.theme) {
          const validPreset: CustomWidgetPreset = {
            id: p.id || `imported_${Date.now()}_${Math.random().toString(36).substring(2, 6)}`,
            name: p.name,
            theme: p.theme,
            visualizer_style: p.visualizer_style || 'bars',
            dynamic_accent: p.dynamic_accent ?? true,
            custom_accent: p.custom_accent || '#00F5D4',
            corner_radius: p.corner_radius ?? 16,
            bg_opacity: p.bg_opacity ?? 0.95,
            show_lyrics: p.show_lyrics ?? true,
            show_visualizer: p.show_visualizer ?? true,
            width: p.width || 380,
            height: p.height || 150,
          };
          await requestDaemon('widget.savePreset', { preset: validPreset });
        }
      }
      await fetchWidgetConfig();
      setShowImportBox(false);
      setImportJsonText('');
      showToast(lang === 'ru' ? 'Пресеты успешно импортированы!' : 'Presets successfully imported!');
    } catch (err) {
      alert(lang === 'ru' ? 'Неверный формат JSON!' : 'Invalid JSON format!');
    }
  };

  // Visualizer fetch & update
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
      await requestDaemon('visualizer.setConfig', updated);
    } catch (err) {
      console.error('Failed to update visualizer config:', err);
    }
  };

  // Mouse & Bass config
  const fetchMouseConfig = useCallback(async () => {
    try {
      const resp = await requestDaemon<{ enabled: boolean }>('mouse.getConfig');
      if (resp && typeof resp.enabled === 'boolean') {
        setMouseEnabled(resp.enabled);
      }
    } catch (err) {
      console.warn('Failed to fetch mouse interaction config:', err);
    }
  }, []);

  const handleUpdateMouse = async (enabled: boolean) => {
    setMouseEnabled(enabled);
    try {
      await requestDaemon('mouse.setConfig', { enabled });
    } catch (err) {
      console.error('Failed to update mouse interaction:', err);
    }
  };

  const fetchBassConfig = useCallback(async () => {
    try {
      const resp = await requestDaemon<{ intensity: number }>('bass.getConfig');
      if (resp && typeof resp.intensity === 'number') {
        setBassIntensity(resp.intensity);
      }
    } catch (err) {
      console.warn('Failed to fetch bass pulse config:', err);
    }
  }, []);

  const handleUpdateBass = async (intensity: number) => {
    setBassIntensity(intensity);
    try {
      await requestDaemon('bass.setConfig', { intensity });
    } catch (err) {
      console.error('Failed to update bass pulse intensity:', err);
    }
  };

  useEffect(() => {
    fetchWidgetConfig();
    fetchVisualizer();
    fetchMouseConfig();
    fetchBassConfig();
  }, [fetchWidgetConfig, fetchVisualizer, fetchMouseConfig, fetchBassConfig]);

  const handleMouseMovePreview = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = Math.round(((e.clientX - rect.left) / rect.width) * 100);
    const y = Math.round(((e.clientY - rect.top) / rect.height) * 100);
    setPreviewCoord({ x, y });
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '18px', overflowY: 'auto' }}>
      {/* View Header */}
      <div className="content-header" style={{ padding: '0 0 14px 0', background: 'transparent' }}>
        <div className="header-title-group">
          <h1>{t.customization_title}</h1>
          <p>{t.customization_subtitle}</p>
        </div>

        {/* Subtabs Navigation */}
        <div className="btn-group">
          <button
            className={`btn btn-sm ${subtab === 'widget' ? 'btn-primary' : 'btn-ghost'}`}
            style={{ fontSize: '0.84rem', padding: '7px 18px', display: 'flex', alignItems: 'center', gap: '6px' }}
            onClick={() => setSubtab('widget')}
          >
            <Sliders size={15} />
            <span>{t.subtab_widget}</span>
          </button>
          <button
            className={`btn btn-sm ${subtab === 'effects' ? 'btn-primary' : 'btn-ghost'}`}
            style={{ fontSize: '0.84rem', padding: '7px 18px', display: 'flex', alignItems: 'center', gap: '6px' }}
            onClick={() => setSubtab('effects')}
          >
            <Sparkles size={15} />
            <span>{t.subtab_effects}</span>
          </button>
          <button
            className={`btn btn-sm ${subtab === 'monitors' ? 'btn-primary' : 'btn-ghost'}`}
            style={{ fontSize: '0.84rem', padding: '7px 18px', display: 'flex', alignItems: 'center', gap: '6px' }}
            onClick={() => setSubtab('monitors')}
          >
            <Monitor size={15} />
            <span>{t.subtab_monitors}</span>
          </button>
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

      {/* SUBTAB 1: WIDGET CUSTOMIZATION */}
      {subtab === 'widget' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          {/* Main Widget Settings Card */}
          <div className="glass-card" style={{ padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '18px' }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                <div
                  style={{
                    width: '38px',
                    height: '38px',
                    borderRadius: '10px',
                    background: widgetConfig.enabled ? 'rgba(0, 245, 212, 0.15)' : 'rgba(255, 255, 255, 0.05)',
                    color: widgetConfig.enabled ? 'var(--accent-primary, #00F5D4)' : 'var(--text-dim)',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                >
                  <Music size={20} />
                </div>
                <div>
                  <div style={{ fontWeight: 600, fontSize: '0.94rem' }}>
                    {lang === 'ru' ? 'Плавающий музыкальный виджет' : 'Floating Music Widget'}
                  </div>
                  <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                    {lang === 'ru'
                      ? 'Отображает текущий трек, обложку, синхронизированный текст песни и спектр звука на рабочем столе'
                      : 'Renders active track info, artwork, synced lyrics and audio equalizer over the desktop'}
                  </p>
                </div>
              </div>
              <input
                type="checkbox"
                checked={widgetConfig.enabled}
                onChange={(e) => handleUpdateWidget({ enabled: e.target.checked })}
                style={{ width: 20, height: 20, accentColor: 'var(--accent-primary, #00F5D4)', cursor: 'pointer' }}
              />
            </div>

            {/* Quick Feature Toggles: Lyrics, Equalizer, Magnetic Snap */}
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))',
                gap: '12px',
                paddingTop: '10px',
                borderTop: '1px solid var(--border-subtle)',
              }}
            >
              {/* Equalizer Toggle */}
              <label
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  padding: '12px 14px',
                  background: 'rgba(0, 0, 0, 0.25)',
                  borderRadius: 'var(--radius-sm)',
                  border: '1px solid var(--border-subtle)',
                  cursor: 'pointer',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                  <BarChart2 size={16} color={widgetConfig.show_visualizer ? 'var(--accent-primary)' : 'var(--text-dim)'} />
                  <div>
                    <div style={{ fontSize: '0.86rem', fontWeight: 600 }}>{t.setting_show_visualizer}</div>
                    <div style={{ fontSize: '0.72rem', color: 'var(--text-dim)' }}>
                      {lang === 'ru' ? 'Анимированные полосы частот' : 'Animated frequency bars'}
                    </div>
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={widgetConfig.show_visualizer}
                  onChange={(e) => handleUpdateWidget({ show_visualizer: e.target.checked })}
                  style={{ width: 18, height: 18, accentColor: 'var(--accent-primary)', cursor: 'pointer' }}
                />
              </label>

              {/* Lyrics Toggle */}
              <label
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  padding: '12px 14px',
                  background: 'rgba(0, 0, 0, 0.25)',
                  borderRadius: 'var(--radius-sm)',
                  border: '1px solid var(--border-subtle)',
                  cursor: 'pointer',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                  <span style={{ fontSize: '14px', fontWeight: 700, color: widgetConfig.show_lyrics ? 'var(--accent-primary)' : 'var(--text-dim)' }}>
                    TXT
                  </span>
                  <div>
                    <div style={{ fontSize: '0.86rem', fontWeight: 600 }}>
                      {lang === 'ru' ? 'Синхронизированный текст' : 'Synced Lyrics'}
                    </div>
                    <div style={{ fontSize: '0.72rem', color: 'var(--text-dim)' }}>
                      {lang === 'ru' ? 'Караоке-строчки с анимацией' : 'Live multi-line lyrics'}
                    </div>
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={widgetConfig.show_lyrics}
                  onChange={(e) => handleUpdateWidget({ show_lyrics: e.target.checked })}
                  style={{ width: 18, height: 18, accentColor: 'var(--accent-primary)', cursor: 'pointer' }}
                />
              </label>

              {/* Magnetic Snap with Particles Toggle */}
              <label
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  padding: '12px 14px',
                  background: 'rgba(0, 0, 0, 0.25)',
                  borderRadius: 'var(--radius-sm)',
                  border: '1px solid var(--border-subtle)',
                  cursor: 'pointer',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                  <Magnet size={16} color={widgetConfig.magnetic_snap ? 'var(--accent-primary)' : 'var(--text-dim)'} />
                  <div>
                    <div style={{ fontSize: '0.86rem', fontWeight: 600 }}>
                      {lang === 'ru' ? 'Магнит к краям экрана' : 'Screen Edge Magnet'}
                    </div>
                    <div style={{ fontSize: '0.72rem', color: 'var(--text-dim)' }}>
                      {lang === 'ru' ? 'Поток светящихся частиц' : 'Glowing particle stream'}
                    </div>
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={widgetConfig.magnetic_snap}
                  onChange={(e) => handleUpdateWidget({ magnetic_snap: e.target.checked })}
                  style={{ width: 18, height: 18, accentColor: 'var(--accent-primary)', cursor: 'pointer' }}
                />
              </label>
            </div>

            {/* Themes, EQ Styles, and Dynamic Accents */}
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))',
                gap: '16px',
                paddingTop: '10px',
                borderTop: '1px solid var(--border-subtle)',
              }}
            >
              {/* Theme Selector */}
              <div>
                <label style={{ fontSize: '0.84rem', fontWeight: 600, marginBottom: '8px', display: 'block' }}>
                  {lang === 'ru' ? 'Тема оформления виджета' : 'Widget Theme'}
                </label>
                <div className="btn-group" style={{ display: 'flex', width: '100%' }}>
                  {[
                    { id: 'spotify', label: 'Spotify Green' },
                    { id: 'glass', label: 'Frosted Glass' },
                    { id: 'vinyl', label: 'Retro Vinyl' },
                  ].map((tItem) => (
                    <button
                      key={tItem.id}
                      className={`btn btn-sm ${widgetConfig.theme === tItem.id ? 'btn-primary' : 'btn-ghost'}`}
                      style={{ flex: 1, fontSize: '0.82rem', padding: '6px 10px' }}
                      onClick={() => handleUpdateWidget({ theme: tItem.id })}
                    >
                      {tItem.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* EQ Style Selector (if enabled) */}
              <div>
                <label style={{ fontSize: '0.84rem', fontWeight: 600, marginBottom: '8px', display: 'block' }}>
                  {lang === 'ru' ? 'Стиль эквалайзера' : 'Equalizer Style'}
                </label>
                <div className="btn-group" style={{ display: 'flex', width: '100%' }}>
                  {[
                    { id: 'bars', label: 'Bars' },
                    { id: 'wave', label: 'Wave' },
                    { id: 'mirror', label: 'Mirror' },
                    { id: 'dots', label: 'Dots' },
                  ].map((sItem) => (
                    <button
                      key={sItem.id}
                      disabled={!widgetConfig.show_visualizer}
                      className={`btn btn-sm ${widgetConfig.visualizer_style === sItem.id ? 'btn-primary' : 'btn-ghost'}`}
                      style={{ flex: 1, fontSize: '0.8rem', padding: '6px 8px', opacity: widgetConfig.show_visualizer ? 1 : 0.4 }}
                      onClick={() => handleUpdateWidget({ visualizer_style: sItem.id })}
                    >
                      {sItem.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Accent Color Mode */}
              <div>
                <label style={{ fontSize: '0.84rem', fontWeight: 600, marginBottom: '8px', display: 'block' }}>
                  {lang === 'ru' ? 'Цветовой акцент' : 'Accent Color'}
                </label>
                <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                  <div className="btn-group">
                    <button
                      className={`btn btn-sm ${widgetConfig.dynamic_accent ? 'btn-primary' : 'btn-ghost'}`}
                      style={{ fontSize: '0.82rem', padding: '6px 14px' }}
                      onClick={() => handleUpdateWidget({ dynamic_accent: true })}
                    >
                      {lang === 'ru' ? 'Из обложки трека' : 'From Album Art'}
                    </button>
                    <button
                      className={`btn btn-sm ${!widgetConfig.dynamic_accent ? 'btn-primary' : 'btn-ghost'}`}
                      style={{ fontSize: '0.82rem', padding: '6px 14px' }}
                      onClick={() => handleUpdateWidget({ dynamic_accent: false })}
                    >
                      {lang === 'ru' ? 'Свой цвет' : 'Custom'}
                    </button>
                  </div>
                  {!widgetConfig.dynamic_accent && (
                    <input
                      type="color"
                      value={widgetConfig.custom_accent}
                      onChange={(e) => handleUpdateWidget({ custom_accent: e.target.value })}
                      style={{
                        width: '32px',
                        height: '32px',
                        borderRadius: '6px',
                        border: '1px solid var(--border-subtle)',
                        cursor: 'pointer',
                        background: 'transparent',
                        padding: 0,
                      }}
                    />
                  )}
                </div>
              </div>
            </div>

            {/* Sliders: Width, Height, Radius, Opacity */}
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
                gap: '16px',
                paddingTop: '10px',
                borderTop: '1px solid var(--border-subtle)',
              }}
            >
              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                  <span>{lang === 'ru' ? 'Ширина' : 'Width'}</span>
                  <span style={{ color: 'var(--accent-primary)', fontWeight: 600 }}>{widgetConfig.width}px</span>
                </div>
                <input
                  type="range"
                  min="240"
                  max="680"
                  step="10"
                  value={widgetConfig.width}
                  onChange={(e) => handleUpdateWidget({ width: Number(e.target.value) })}
                  style={{ width: '100%', accentColor: 'var(--accent-primary)' }}
                />
              </div>

              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                  <span>{lang === 'ru' ? 'Высота' : 'Height'}</span>
                  <span style={{ color: 'var(--accent-primary)', fontWeight: 600 }}>{widgetConfig.height}px</span>
                </div>
                <input
                  type="range"
                  min="110"
                  max="480"
                  step="5"
                  value={widgetConfig.height}
                  onChange={(e) => handleUpdateWidget({ height: Number(e.target.value) })}
                  style={{ width: '100%', accentColor: 'var(--accent-primary)' }}
                />
              </div>

              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                  <span>{lang === 'ru' ? 'Скругление углов' : 'Corner Radius'}</span>
                  <span style={{ color: 'var(--accent-primary)', fontWeight: 600 }}>{widgetConfig.corner_radius}px</span>
                </div>
                <input
                  type="range"
                  min="0"
                  max="36"
                  value={widgetConfig.corner_radius}
                  onChange={(e) => handleUpdateWidget({ corner_radius: Number(e.target.value) })}
                  style={{ width: '100%', accentColor: 'var(--accent-primary)' }}
                />
              </div>

              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                  <span>{lang === 'ru' ? 'Прозрачность' : 'Opacity'}</span>
                  <span style={{ color: 'var(--accent-primary)', fontWeight: 600 }}>{Math.round(widgetConfig.opacity * 100)}%</span>
                </div>
                <input
                  type="range"
                  min="0.3"
                  max="1.0"
                  step="0.05"
                  value={widgetConfig.opacity}
                  onChange={(e) => handleUpdateWidget({ opacity: Number(e.target.value) })}
                  style={{ width: '100%', accentColor: 'var(--accent-primary)' }}
                />
              </div>
            </div>
          </div>

          {/* Presets Gallery Card */}
          <div className="glass-card" style={{ padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '16px' }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: '10px' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                <LayoutGrid size={20} color="var(--accent-primary)" />
                <div>
                  <div style={{ fontWeight: 600, fontSize: '0.94rem' }}>
                    {lang === 'ru' ? 'Коллекция пресетов виджета' : 'Widget Presets Gallery'}
                  </div>
                  <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                    {lang === 'ru'
                      ? 'Сохраняйте свои стили, переключайтесь в 1 клик или делитесь конфигурацией JSON'
                      : 'Save custom styles, switch in 1 click, or share JSON presets'}
                  </p>
                </div>
              </div>

              <div style={{ display: 'flex', gap: '8px' }}>
                <button className="btn btn-sm btn-ghost" onClick={handleExportPresetsJson}>
                  <Copy size={14} />
                  <span>{lang === 'ru' ? 'Экспорт JSON' : 'Export JSON'}</span>
                </button>
                <button className="btn btn-sm btn-ghost" onClick={() => setShowImportBox(!showImportBox)}>
                  <Download size={14} />
                  <span>{lang === 'ru' ? 'Импорт JSON' : 'Import JSON'}</span>
                </button>
              </div>
            </div>

            {showImportBox && (
              <div
                style={{
                  background: 'rgba(0, 0, 0, 0.4)',
                  padding: '14px',
                  borderRadius: 'var(--radius-md)',
                  border: '1px solid var(--border-subtle)',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '10px',
                }}
              >
                <textarea
                  rows={3}
                  placeholder={lang === 'ru' ? 'Вставьте JSON пресета сюда...' : 'Paste preset JSON here...'}
                  value={importJsonText}
                  onChange={(e) => setImportJsonText(e.target.value)}
                  style={{
                    width: '100%',
                    background: 'rgba(0, 0, 0, 0.5)',
                    border: '1px solid var(--border-subtle)',
                    borderRadius: 'var(--radius-sm)',
                    color: 'var(--text-primary)',
                    fontFamily: 'monospace',
                    fontSize: '0.8rem',
                    padding: '8px',
                  }}
                />
                <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px' }}>
                  <button className="btn btn-sm btn-ghost" onClick={() => setShowImportBox(false)}>
                    {lang === 'ru' ? 'Отмена' : 'Cancel'}
                  </button>
                  <button className="btn btn-sm btn-primary" onClick={handleImportPresetsJson}>
                    <Upload size={14} />
                    <span>{lang === 'ru' ? 'Импортировать' : 'Import'}</span>
                  </button>
                </div>
              </div>
            )}

            {/* Presets List */}
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))', gap: '12px' }}>
              {[...BUILTIN_WIDGET_PRESETS, ...widgetConfig.custom_presets].map((preset) => {
                const isCustom = !preset.id.startsWith('builtin_');
                return (
                  <div
                    key={preset.id}
                    onClick={() => handleApplyPreset(preset)}
                    style={{
                      padding: '12px 14px',
                      background: 'rgba(0, 0, 0, 0.3)',
                      border: '1px solid var(--border-subtle)',
                      borderRadius: 'var(--radius-md)',
                      cursor: 'pointer',
                      display: 'flex',
                      flexDirection: 'column',
                      gap: '6px',
                      transition: 'all 0.18s ease',
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.borderColor = 'var(--accent-primary)')}
                    onMouseLeave={(e) => (e.currentTarget.style.borderColor = 'var(--border-subtle)')}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                      <span style={{ fontWeight: 600, fontSize: '0.88rem' }}>{preset.name}</span>
                      {isCustom && (
                        <button
                          className="btn-icon"
                          style={{ padding: '4px', color: '#ef4444' }}
                          onClick={(e) => handleDeletePreset(preset.id, e)}
                        >
                          <Trash2 size={13} />
                        </button>
                      )}
                    </div>
                    <div style={{ display: 'flex', gap: '6px', flexWrap: 'wrap', fontSize: '0.72rem', color: 'var(--text-dim)' }}>
                      <span className="badge">{preset.theme}</span>
                      <span className="badge">{preset.visualizer_style}</span>
                      <span className="badge">{preset.width}×{preset.height}</span>
                      {!preset.show_visualizer && <span className="badge">No EQ</span>}
                    </div>
                  </div>
                );
              })}
            </div>

            {/* Save Current as New Preset */}
            <div style={{ display: 'flex', gap: '10px', paddingTop: '10px', borderTop: '1px solid var(--border-subtle)' }}>
              <input
                type="text"
                placeholder={lang === 'ru' ? 'Название нового пресета...' : 'Preset name...'}
                value={newPresetName}
                onChange={(e) => setNewPresetName(e.target.value)}
                style={{
                  flex: 1,
                  background: 'rgba(0, 0, 0, 0.3)',
                  border: '1px solid var(--border-subtle)',
                  borderRadius: 'var(--radius-sm)',
                  padding: '8px 12px',
                  color: 'var(--text-primary)',
                  fontSize: '0.86rem',
                }}
              />
              <button className="btn btn-sm btn-primary" onClick={handleSaveCurrentAsPreset}>
                <Plus size={15} />
                <span>{lang === 'ru' ? 'Сохранить стиль' : 'Save Style'}</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* SUBTAB 2: WALLPAPER EFFECTS */}
      {subtab === 'effects' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          {/* Auto-change Wallpapers on Track Switch */}
          <div className="glass-card" style={{ padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '14px' }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: '12px' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                <div
                  style={{
                    width: '38px',
                    height: '38px',
                    borderRadius: '10px',
                    background: wallpaperChangeMode !== 'off' ? 'rgba(0, 245, 212, 0.15)' : 'rgba(255, 255, 255, 0.05)',
                    color: wallpaperChangeMode !== 'off' ? 'var(--accent-primary, #00F5D4)' : 'var(--text-dim)',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                >
                  {wallpaperChangeMode === 'random' ? (
                    <Shuffle size={20} />
                  ) : wallpaperChangeMode === 'sequential' ? (
                    <Repeat size={20} />
                  ) : (
                    <Layers size={20} />
                  )}
                </div>
                <div>
                  <div style={{ fontWeight: 600, fontSize: '0.94rem' }}>{t.auto_change_title}</div>
                  <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>{t.auto_change_subtitle}</p>
                </div>
              </div>

              {/* Mode Segmented Controls */}
              <div className="btn-group">
                <button
                  className={`btn btn-sm ${wallpaperChangeMode === 'random' ? 'btn-primary' : 'btn-ghost'}`}
                  style={{ fontSize: '0.82rem', padding: '6px 14px', display: 'flex', alignItems: 'center', gap: '6px' }}
                  onClick={() => onSetChangeMode?.('random', respectBindings)}
                >
                  <Shuffle size={14} />
                  <span>{t.auto_change_random}</span>
                </button>
                <button
                  className={`btn btn-sm ${wallpaperChangeMode === 'sequential' ? 'btn-primary' : 'btn-ghost'}`}
                  style={{ fontSize: '0.82rem', padding: '6px 14px', display: 'flex', alignItems: 'center', gap: '6px' }}
                  onClick={() => onSetChangeMode?.('sequential', respectBindings)}
                >
                  <Repeat size={14} />
                  <span>{t.auto_change_sequential}</span>
                </button>
                <button
                  className={`btn btn-sm ${wallpaperChangeMode === 'off' ? 'btn-primary' : 'btn-ghost'}`}
                  style={{ fontSize: '0.82rem', padding: '6px 14px' }}
                  onClick={() => onSetChangeMode?.('off', respectBindings)}
                >
                  {t.auto_change_off}
                </button>
              </div>
            </div>

            {wallpaperChangeMode !== 'off' && (
              <label
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '10px',
                  fontSize: '0.84rem',
                  color: 'var(--text-dim)',
                  cursor: 'pointer',
                  paddingTop: '6px',
                }}
              >
                <input
                  type="checkbox"
                  checked={respectBindings}
                  onChange={(e) => onSetChangeMode?.(wallpaperChangeMode, e.target.checked)}
                  style={{ accentColor: 'var(--accent-primary)' }}
                />
                <span>{t.auto_change_respect_bindings}</span>
              </label>
            )}
          </div>

          {/* Wallpaper Audio Visualizer Overlay */}
          <div className="glass-card" style={{ padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '18px' }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                <div
                  style={{
                    width: '38px',
                    height: '38px',
                    borderRadius: '10px',
                    background: visualizer.enabled ? 'rgba(0, 245, 212, 0.15)' : 'rgba(255, 255, 255, 0.05)',
                    color: visualizer.enabled ? 'var(--accent-primary, #00F5D4)' : 'var(--text-dim)',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                >
                  <Waves size={20} />
                </div>
                <div>
                  <div style={{ fontWeight: 600, fontSize: '0.94rem' }}>
                    {lang === 'ru' ? 'Аудиовизуализатор на обоях' : 'Wallpaper Audio Visualizer'}
                  </div>
                  <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                    {lang === 'ru'
                      ? 'Полупрозрачный спектр в нижней части экрана над обоями (100% прозрачность без черных рамок)'
                      : 'Live audio visualizer layered over desktop wallpapers with per-pixel alpha transparency'}
                  </p>
                </div>
              </div>
              <input
                type="checkbox"
                checked={visualizer.enabled}
                onChange={(e) => handleUpdateVisualizer({ enabled: e.target.checked })}
                style={{ width: 20, height: 20, accentColor: 'var(--accent-primary)', cursor: 'pointer' }}
              />
            </div>

            {visualizer.enabled && (
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))',
                  gap: '16px',
                  paddingTop: '10px',
                  borderTop: '1px solid var(--border-subtle)',
                }}
              >
                <div>
                  <label style={{ fontSize: '0.84rem', fontWeight: 600, marginBottom: '8px', display: 'block' }}>
                    {lang === 'ru' ? 'Стиль визуализатора' : 'Visualizer Style'}
                  </label>
                  <div className="btn-group" style={{ display: 'flex', width: '100%' }}>
                    {[
                      { id: 'bars', label: 'Bars' },
                      { id: 'wave', label: 'Wave' },
                      { id: 'glow', label: 'Horizon Glow' },
                    ].map((s) => (
                      <button
                        key={s.id}
                        className={`btn btn-sm ${visualizer.style === s.id ? 'btn-primary' : 'btn-ghost'}`}
                        style={{ flex: 1, fontSize: '0.82rem', padding: '6px 10px' }}
                        onClick={() => handleUpdateVisualizer({ style: s.id as any })}
                      >
                        {s.label}
                      </button>
                    ))}
                  </div>
                </div>

                <div>
                  <label style={{ fontSize: '0.84rem', fontWeight: 600, marginBottom: '8px', display: 'block' }}>
                    {lang === 'ru' ? 'Цветовая палитра' : 'Color Palette'}
                  </label>
                  <div className="btn-group" style={{ display: 'flex', width: '100%' }}>
                    {[
                      { id: 'spotify', label: 'Spotify' },
                      { id: 'cyan_purple', label: 'Cyber' },
                      { id: 'neon_blue', label: 'Sky' },
                      { id: 'fire', label: 'Fire' },
                    ].map((p) => (
                      <button
                        key={p.id}
                        className={`btn btn-sm ${visualizer.accent_color === p.id ? 'btn-primary' : 'btn-ghost'}`}
                        style={{ flex: 1, fontSize: '0.82rem', padding: '6px 8px' }}
                        onClick={() => handleUpdateVisualizer({ accent_color: p.id as any })}
                      >
                        {p.label}
                      </button>
                    ))}
                  </div>
                </div>

                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                    <span>{lang === 'ru' ? 'Высота полос' : 'Visualizer Height'}</span>
                    <span style={{ color: 'var(--accent-primary)', fontWeight: 600 }}>{visualizer.height_px}px</span>
                  </div>
                  <input
                    type="range"
                    min="60"
                    max="320"
                    step="10"
                    value={visualizer.height_px}
                    onChange={(e) => handleUpdateVisualizer({ height_px: Number(e.target.value) })}
                    style={{ width: '100%', accentColor: 'var(--accent-primary)' }}
                  />
                </div>

                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.82rem', marginBottom: '4px' }}>
                    <span>{lang === 'ru' ? 'Прозрачность' : 'Opacity'}</span>
                    <span style={{ color: 'var(--accent-primary)', fontWeight: 600 }}>{Math.round(visualizer.opacity * 100)}%</span>
                  </div>
                  <input
                    type="range"
                    min="0.2"
                    max="1.0"
                    step="0.05"
                    value={visualizer.opacity}
                    onChange={(e) => handleUpdateVisualizer({ opacity: Number(e.target.value) })}
                    style={{ width: '100%', accentColor: 'var(--accent-primary)' }}
                  />
                </div>
              </div>
            )}
          </div>

          {/* Audio-Reactive Bass Pulse & Mouse Parallax */}
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(320px, 1fr))',
              gap: '16px',
            }}
          >
            {/* Bass Pulse */}
            <div className="glass-card" style={{ padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '14px' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                <div
                  style={{
                    width: '38px',
                    height: '38px',
                    borderRadius: '10px',
                    background: bassIntensity > 0.05 ? 'rgba(239, 68, 68, 0.15)' : 'rgba(255, 255, 255, 0.05)',
                    color: bassIntensity > 0.05 ? '#ef4444' : 'var(--text-dim)',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                >
                  <Activity size={20} />
                </div>
                <div>
                  <div style={{ fontWeight: 600, fontSize: '0.94rem' }}>
                    {lang === 'ru' ? 'Пульсация обоев в такт басам' : 'Bass Beat Wallpaper Pulse'}
                  </div>
                  <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                    {lang === 'ru'
                      ? 'Ритмичный зум/удар обоев в такт басовым частотам для любых плееров (Яндекс Музыка, Spotify, YouTube)'
                      : 'Audio-reactive scale punch in rhythm with bass drum kicks across all audio sources'}
                  </p>
                </div>
              </div>

              <div style={{ display: 'flex', flexDirection: 'column', gap: '6px', paddingTop: '6px' }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.84rem' }}>
                  <span>{lang === 'ru' ? 'Интенсивность удара' : 'Punch Intensity'}</span>
                  <span style={{ color: '#ef4444', fontWeight: 600 }}>{Math.round(bassIntensity * 100)}%</span>
                </div>
                <input
                  type="range"
                  min="0.0"
                  max="1.0"
                  step="0.05"
                  value={bassIntensity}
                  onChange={(e) => handleUpdateBass(Number(e.target.value))}
                  style={{ width: '100%', accentColor: '#ef4444' }}
                />
                <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.72rem', color: 'var(--text-dim)' }}>
                  <span>0% ({lang === 'ru' ? 'Выкл' : 'Off'})</span>
                  <span>50% ({lang === 'ru' ? 'Оптимально' : 'Normal'})</span>
                  <span>100% ({lang === 'ru' ? 'Максимум' : 'Max Kick'})</span>
                </div>
              </div>
            </div>

            {/* Mouse Parallax */}
            <div className="glass-card" style={{ padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: '14px' }}>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                  <div
                    style={{
                      width: '38px',
                      height: '38px',
                      borderRadius: '10px',
                      background: mouseEnabled ? 'rgba(99, 102, 241, 0.15)' : 'rgba(255, 255, 255, 0.05)',
                      color: mouseEnabled ? '#6366f1' : 'var(--text-dim)',
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                    }}
                  >
                    <MousePointer size={20} />
                  </div>
                  <div>
                    <div style={{ fontWeight: 600, fontSize: '0.94rem' }}>
                      {lang === 'ru' ? 'Параллакс обоев за курсором' : 'Mouse Parallax Interaction'}
                    </div>
                    <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                      {lang === 'ru' ? 'Плавный наклон и смещение обоев при движении мыши' : 'Smooth camera shift tracking mouse cursor'}
                    </p>
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={mouseEnabled}
                  onChange={(e) => handleUpdateMouse(e.target.checked)}
                  style={{ width: 20, height: 20, accentColor: '#6366f1', cursor: 'pointer' }}
                />
              </div>

              {mouseEnabled && (
                <div
                  onMouseMove={handleMouseMovePreview}
                  style={{
                    height: '60px',
                    borderRadius: '8px',
                    background: 'radial-gradient(circle at ' + previewCoord.x + '% ' + previewCoord.y + '%, rgba(99, 102, 241, 0.35) 0%, rgba(0,0,0,0.3) 70%)',
                    border: '1px solid var(--border-subtle)',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    fontSize: '0.78rem',
                    color: 'var(--text-dim)',
                    cursor: 'crosshair',
                  }}
                >
                  {lang === 'ru' ? `Тест позиции: (${previewCoord.x}%, ${previewCoord.y}%)` : `Cursor Test: (${previewCoord.x}%, ${previewCoord.y}%)`}
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* SUBTAB 3: MONITORS VIEW */}
      {subtab === 'monitors' && (
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
          <MonitorsView monitors={monitors} lang={lang} />
        </div>
      )}
    </div>
  );
};
