use anyhow::{Context, Result};
use muda::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub struct SystemTray {
    pub tray: TrayIcon,
    pub open_ui_id: muda::MenuId,
    pub toggle_widget_id: muda::MenuId,
    pub pause_id: muda::MenuId,
    pub next_id: muda::MenuId,
    pub diag_id: muda::MenuId,
    pub quit_id: muda::MenuId,
    pub status_item: MenuItem,
}

impl SystemTray {
    pub fn create(is_paused: bool) -> Result<Self> {
        let menu = Menu::new();

        let status_text = if is_paused {
            "wallmus: На паузе"
        } else {
            "wallmus: Активен"
        };
        let status_item = MenuItem::new(status_text, false, None);

        let open_ui_item = MenuItem::new("Открыть настройки", true, None);
        let toggle_widget_item = MenuItem::new("Показать / Скрыть виджет", true, None);
        let pause_item = MenuItem::new("Пауза / Возобновить обои", true, None);
        let next_item = MenuItem::new("Следующие обои", true, None);
        let diag_item = MenuItem::new("Диагностика (выгрузить отчёт)", true, None);
        let sep = PredefinedMenuItem::separator();
        let quit_item = MenuItem::new("Выход", true, None);

        let open_ui_id = open_ui_item.id().clone();
        let toggle_widget_id = toggle_widget_item.id().clone();
        let pause_id = pause_item.id().clone();
        let next_id = next_item.id().clone();
        let diag_id = diag_item.id().clone();
        let quit_id = quit_item.id().clone();

        menu.append(&status_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&open_ui_item)?;
        menu.append(&toggle_widget_item)?;
        menu.append(&pause_item)?;
        menu.append(&next_item)?;
        menu.append(&diag_item)?;
        menu.append(&sep)?;
        menu.append(&quit_item)?;

        let icon = generate_tray_icon(is_paused)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("wallmus — Живые обои и музыка")
            .with_icon(icon)
            .build()
            .context("Failed to build TrayIcon")?;

        Ok(Self {
            tray,
            open_ui_id,
            toggle_widget_id,
            pause_id,
            next_id,
            diag_id,
            quit_id,
            status_item,
        })
    }

    /// Update status text and icon when pause state toggles.
    pub fn update_state(&self, is_paused: bool) {
        let status_text = if is_paused {
            "wallmus: На паузе"
        } else {
            "wallmus: Активен"
        };
        self.status_item.set_text(status_text);
        if let Ok(icon) = generate_tray_icon(is_paused) {
            let _ = self.tray.set_icon(Some(icon));
        }
    }
}

/// Generate a programmatic 32x32 RGBA icon (Spotify green or orange paused).
fn generate_tray_icon(is_paused: bool) -> Result<Icon> {
    const W: u32 = 32;
    const H: u32 = 32;
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);

    let (r_col, g_col, b_col) = if is_paused {
        (255, 165, 0) // Orange for pause
    } else {
        (29, 185, 84) // Vibrant Spotify green
    };

    let cx = W as f32 / 2.0;
    let cy = H as f32 / 2.0;
    let radius = 13.0;

    for y in 0..H {
        for x in 0..W {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                // Circle fill
                let alpha = if dist > radius - 1.0 {
                    ((radius - dist) * 255.0).round() as u8
                } else {
                    255
                };
                rgba.extend_from_slice(&[r_col, g_col, b_col, alpha]);
            } else {
                // Transparent background
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    Icon::from_rgba(rgba, W, H).context("Failed to create tray Icon from RGBA")
}
