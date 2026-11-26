use eframe::egui::{self, Color32, CornerRadius, FontId, Rect, RichText, Stroke, StrokeKind, Vec2};

use crate::battery::format_remaining;
use crate::config::{Config, TaskbarFormat, TaskbarPosition};
use crate::platform::TaskbarGeometry;
use crate::sysmon::{SystemMetrics, format_speed_bps};

/// Computes physical `(x, y, width, height)` for the taskbar widget from taskbar & tray bounds.
pub fn compute_taskbar_widget_rect(
    geom: &TaskbarGeometry,
    pos: TaskbarPosition,
    logical_size: Vec2,
) -> (i32, i32, i32, i32) {
    let ppp = geom.dpi_scale.max(1.0);
    let w = (logical_size.x * ppp).ceil().max(40.0) as i32;
    let h = (logical_size.y * ppp).ceil().max(24.0) as i32;
    let tb_h = (geom.bottom - geom.top).max(1);
    let pad_x = (8.0 * ppp).round() as i32;
    let pad_y = (4.0 * ppp).round() as i32;

    let mut x = match pos {
        TaskbarPosition::LeftOfTray => {
            if let Some(tl) = geom.tray_left {
                tl - w - pad_x
            } else {
                geom.right - w - (180.0 * ppp) as i32
            }
        }
        TaskbarPosition::RightOfTray => geom.right - w - pad_x,
        TaskbarPosition::TopOfTaskbar => {
            if let Some(tl) = geom.tray_left {
                tl - w - pad_x
            } else {
                geom.right - w - pad_x
            }
        }
    };

    let y = match pos {
        TaskbarPosition::TopOfTaskbar => (geom.top - h - pad_y).max(0),
        TaskbarPosition::LeftOfTray | TaskbarPosition::RightOfTray => geom.top + ((tb_h - h) / 2),
    };

    x = x.clamp(geom.left + pad_x, (geom.right - w - pad_x).max(geom.left + pad_x));
    (x, y, w, h)
}

pub fn estimate_taskbar_size(config: &Config, metrics: &SystemMetrics) -> Vec2 {
    let mut items = 0usize;
    if config.taskbar_show_cpu {
        items += 1;
    }
    if config.taskbar_show_ram {
        items += 1;
    }
    if config.taskbar_show_disk {
        items += 1;
    }
    if config.taskbar_show_network {
        items += 1;
    }
    if config.taskbar_show_battery && metrics.battery.is_some() {
        items += 1;
    }
    let items = items.max(1);
    let per_item = match config.taskbar_format {
        TaskbarFormat::CompactText => 68.0,
        TaskbarFormat::IconValue => 76.0,
        TaskbarFormat::MiniBar => 72.0,
    };
    let w = (items as f32 * per_item + 18.0).clamp(90.0, 460.0);
    Vec2::new(w, 30.0)
}

pub fn render_taskbar_ui(ui: &mut egui::Ui, config: &Config, metrics: &SystemMetrics) {
    let rect = ui.max_rect().shrink2(Vec2::new(2.0, 2.0));
    let bg = Color32::from_rgba_unmultiplied(
        config.bg_color[0],
        config.bg_color[1],
        config.bg_color[2],
        config.bg_color[3].max(215),
    );
    let accent = Color32::from_rgb(
        config.accent_color[0],
        config.accent_color[1],
        config.accent_color[2],
    );
    let text_col = Color32::from_rgb(
        config.text_color[0],
        config.text_color[1],
        config.text_color[2],
    );
    let muted = Color32::from_rgba_unmultiplied(text_col.r(), text_col.g(), text_col.b(), 165);
    let border = Color32::from_rgba_unmultiplied(255, 255, 255, config.border_alpha.max(24));

    let corner = CornerRadius::same(10);
    ui.painter().rect_filled(rect, corner, bg);
    ui.painter()
        .rect_stroke(rect, corner, Stroke::new(1.0, border), StrokeKind::Inside);

    let inner = rect.shrink2(Vec2::new(9.0, 3.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let mut any_shown = false;

            if config.taskbar_show_cpu {
                any_shown = true;
                draw_metric(
                    ui,
                    config.taskbar_format,
                    "CPU",
                    "⚙",
                    &format!("{:.0}%", metrics.cpu_pct),
                    metrics.cpu_pct / 100.0,
                    accent,
                    text_col,
                    muted,
                );
            }

            if config.taskbar_show_ram {
                any_shown = true;
                let val = match config.taskbar_format {
                    TaskbarFormat::IconValue => {
                        format!("{:.1}/{:.0}G", metrics.ram_used_gb, metrics.ram_total_gb)
                    }
                    _ => format!("{:.0}%", metrics.ram_pct),
                };
                draw_metric(
                    ui,
                    config.taskbar_format,
                    "RAM",
                    "▤",
                    &val,
                    metrics.ram_pct / 100.0,
                    accent,
                    text_col,
                    muted,
                );
            }

            if config.taskbar_show_disk {
                any_shown = true;
                let pct = metrics.disk_activity_pct.or(metrics.disk_used_pct).unwrap_or(0.0);
                draw_metric(
                    ui,
                    config.taskbar_format,
                    "DSK",
                    "⛁",
                    &format!("{pct:.0}%"),
                    pct / 100.0,
                    accent,
                    text_col,
                    muted,
                );
            }

            if config.taskbar_show_network {
                any_shown = true;
                let down = format_speed_bps(metrics.net_down_bps);
                let up = format_speed_bps(metrics.net_up_bps);
                ui.label(
                    RichText::new(format!("↓{down} ↑{up}"))
                        .font(FontId::proportional(10.5))
                        .color(text_col),
                );
            }

            if config.taskbar_show_battery
                && let Some(bat) = metrics.battery
            {
                any_shown = true;
                let bat_col = if bat.percent <= config.battery_critical_pct {
                    Color32::from_rgb(245, 95, 95)
                } else if bat.percent <= config.battery_warn_pct {
                    Color32::from_rgb(245, 190, 75)
                } else {
                    Color32::from_rgb(110, 220, 145)
                };
                let val = if let Some(rem) = bat.remaining_secs {
                    format!("{}% ({})", bat.percent, format_remaining(rem))
                } else {
                    format!("{}%", bat.percent)
                };
                draw_metric(
                    ui,
                    config.taskbar_format,
                    "BAT",
                    bat.status.short_symbol(),
                    &val,
                    bat.percent as f32 / 100.0,
                    bat_col,
                    text_col,
                    muted,
                );
            }

            if !any_shown {
                ui.label(
                    RichText::new("SysMon")
                        .font(FontId::proportional(10.5))
                        .color(muted),
                );
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_metric(
    ui: &mut egui::Ui,
    fmt: TaskbarFormat,
    label: &str,
    icon: &str,
    value: &str,
    frac: f32,
    bar_col: Color32,
    text_col: Color32,
    muted: Color32,
) {
    match fmt {
        TaskbarFormat::CompactText => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(
                    RichText::new(label)
                        .font(FontId::proportional(10.0))
                        .color(muted),
                );
                ui.label(
                    RichText::new(value)
                        .font(FontId::proportional(10.5))
                        .color(text_col),
                );
            });
        }
        TaskbarFormat::IconValue => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(
                    RichText::new(icon)
                        .font(FontId::proportional(10.5))
                        .color(bar_col),
                );
                ui.label(
                    RichText::new(value)
                        .font(FontId::proportional(10.5))
                        .color(text_col),
                );
            });
        }
        TaskbarFormat::MiniBar => {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(
                    RichText::new(format!("{label} {value}"))
                        .font(FontId::proportional(9.5))
                        .color(text_col),
                );
                let (bar_rect, _) =
                    ui.allocate_exact_size(Vec2::new(44.0, 3.5), egui::Sense::hover());
                let cr = CornerRadius::same(2);
                ui.painter().rect_filled(
                    bar_rect,
                    cr,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 28),
                );
                let fill_w = (bar_rect.width() * frac.clamp(0.0, 1.0)).max(2.0);
                let fill_rect =
                    Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, bar_rect.height()));
                ui.painter().rect_filled(fill_rect, cr, bar_col);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taskbar_rect_positions_stay_within_taskbar_bounds() {
        let geom = TaskbarGeometry {
            left: 0,
            top: 1032,
            right: 1920,
            bottom: 1080,
            tray_left: Some(1650),
            dpi_scale: 1.0,
        };
        let (lx, ly, lw, lh) =
            compute_taskbar_widget_rect(&geom, TaskbarPosition::LeftOfTray, Vec2::new(220.0, 30.0));
        assert!(lx + lw <= 1650);
        assert!(ly >= 1032 && ly + lh <= 1080);

        let (_tx, ty, _tw, th) = compute_taskbar_widget_rect(
            &geom,
            TaskbarPosition::TopOfTaskbar,
            Vec2::new(220.0, 30.0),
        );
        assert!(ty + th <= 1032);
    }
}
