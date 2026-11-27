#[cfg(unix)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2,
    ViewportBuilder, ViewportId,
};

use crate::battery::{format_remaining, read_battery};
use crate::calendar_types::{
    CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events,
};
use crate::clock_light::{format_clock, is_valid_timezone};
use crate::config::{
    BackgroundMaterial, COMMON_TIMEZONES, Config, IdleDisplayMode, KaraokeLyricMode,
    TRANSLATE_LANGUAGES, TaskbarFormat, TaskbarPosition, TextAlign,
};
use crate::platform::light::{self as platform_light, InstalledFont};
use crate::todo::TodoStore;
pub use crate::ui::icons::SETTINGS_WINDOW_TITLE;
use crate::ui::icons::{IconKind, SettingsTab, icon_button, paint_icon, paint_mascot};

pub struct SettingsWindowState<'a> {
    pub open: &'a mut bool,
    pub focus_requested: &'a mut bool,
    pub tab: &'a mut SettingsTab,
    pub config: &'a mut Config,
    pub todos: &'a mut TodoStore,
    pub new_todo_input: &'a mut String,
    pub calendar_url_input: &'a mut String,
    pub calendar_events: &'a [CalendarEvent],
    pub calendar_status: &'a str,
    pub calendar_cmd_tx: &'a mpsc::Sender<CalendarCommand>,
    pub config_dirty: &'a mut bool,
    pub test_notification_requested: &'a mut bool,
    pub installed_fonts: &'a mut Option<Vec<InstalledFont>>,
    pub font_search: &'a mut String,
}

struct DeferredSettingsState {
    open: bool,
    focus_requested: bool,
    tab: SettingsTab,
    config: Config,
    todos: TodoStore,
    new_todo_input: String,
    calendar_url_input: String,
    calendar_events: Vec<CalendarEvent>,
    calendar_status: String,
    calendar_cmd_tx: mpsc::Sender<CalendarCommand>,
    installed_fonts: Option<Vec<InstalledFont>>,
    font_search: String,
    dirty_from_ui: bool,
    config_dirty_out: bool,
    test_notif_out: bool,
}

fn shared_settings_state() -> &'static Arc<Mutex<Option<DeferredSettingsState>>> {
    static STATE: OnceLock<Arc<Mutex<Option<DeferredSettingsState>>>> = OnceLock::new();
    STATE.get_or_init(|| Arc::new(Mutex::new(None)))
}

pub(crate) fn settings_panel_fill(config: &Config) -> Color32 {
    let wayland_alpha = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
    match config.background_material {
        BackgroundMaterial::Solid => Color32::from_rgb(14, 15, 19),
        BackgroundMaterial::Acrylic => {
            if wayland_alpha {
                Color32::from_rgba_unmultiplied(15, 17, 23, 224)
            } else {
                Color32::from_rgb(16, 18, 24)
            }
        }
        BackgroundMaterial::FrostedGlass => {
            if wayland_alpha {
                Color32::from_rgba_unmultiplied(20, 24, 33, 214)
            } else {
                Color32::from_rgb(20, 24, 33)
            }
        }
        BackgroundMaterial::LiquidGlass => {
            if wayland_alpha {
                Color32::from_rgba_unmultiplied(13, 16, 24, 206)
            } else {
                Color32::from_rgb(14, 17, 26)
            }
        }
        BackgroundMaterial::Glassmorphism => {
            if wayland_alpha {
                Color32::from_rgba_unmultiplied(16, 18, 28, 212)
            } else {
                Color32::from_rgb(16, 18, 28)
            }
        }
    }
}

fn paint_settings_material_backdrop(ui: &egui::Ui, config: &Config) {
    let rect = ui.max_rect().expand(10.0);
    let painter = ui.painter();
    let [ar, ag, ab] = config.accent_color;
    match config.background_material {
        BackgroundMaterial::Solid => {}
        BackgroundMaterial::Acrylic => {
            painter.circle_filled(
                Pos2::new(
                    rect.left() + rect.width() * 0.20,
                    rect.top() + rect.height() * 0.18,
                ),
                rect.width() * 0.34,
                Color32::from_rgba_unmultiplied(ar, ag, ab, 16),
            );
            // Subtle micro-stipple acrylic grain pattern.
            let step = 16.0;
            let mut y = rect.top() + 8.0;
            let mut row = 0u32;
            while y < rect.bottom() {
                let mut x = rect.left() + if row.is_multiple_of(2) { 6.0 } else { 14.0 };
                while x < rect.right() {
                    painter.rect_filled(
                        Rect::from_min_size(Pos2::new(x, y), Vec2::splat(1.0)),
                        CornerRadius::ZERO,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 7),
                    );
                    x += step;
                }
                row += 1;
                y += step;
            }
        }
        BackgroundMaterial::FrostedGlass => {
            let top_band = Rect::from_min_max(
                rect.min,
                Pos2::new(rect.right(), rect.top() + rect.height() * 0.45),
            );
            painter.rect_filled(
                top_band,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(210, 224, 248, 12),
            );
            painter.circle_filled(
                Pos2::new(
                    rect.right() - rect.width() * 0.18,
                    rect.top() + rect.height() * 0.24,
                ),
                rect.width() * 0.30,
                Color32::from_rgba_unmultiplied(ar, ag, ab, 18),
            );
            painter.rect_stroke(
                rect.shrink(1.0),
                CornerRadius::same(12),
                Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 34)),
                egui::StrokeKind::Inside,
            );
        }
        BackgroundMaterial::LiquidGlass => {
            let top_sheen = Rect::from_min_max(
                rect.min,
                Pos2::new(rect.right(), rect.top() + rect.height() * 0.34),
            );
            painter.rect_filled(
                top_sheen,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(255, 255, 255, 16),
            );
            painter.circle_filled(
                Pos2::new(rect.left() + rect.width() * 0.25, rect.top() + 18.0),
                rect.width() * 0.28,
                Color32::from_rgba_unmultiplied(ar, ag, ab, 24),
            );
            painter.circle_filled(
                Pos2::new(rect.right() - rect.width() * 0.22, rect.bottom() - 24.0),
                rect.width() * 0.24,
                Color32::from_rgba_unmultiplied(150, 200, 255, 16),
            );
            painter.rect_stroke(
                rect.shrink(1.0),
                CornerRadius::same(12),
                Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 255, 255, 52)),
                egui::StrokeKind::Inside,
            );
        }
        BackgroundMaterial::Glassmorphism => {
            painter.circle_filled(
                Pos2::new(
                    rect.left() + rect.width() * 0.22,
                    rect.top() + rect.height() * 0.20,
                ),
                rect.width() * 0.34,
                Color32::from_rgba_unmultiplied(ar, ag, ab, 28),
            );
            painter.circle_filled(
                Pos2::new(
                    rect.right() - rect.width() * 0.20,
                    rect.bottom() - rect.height() * 0.22,
                ),
                rect.width() * 0.32,
                Color32::from_rgba_unmultiplied(110, 175, 255, 22),
            );
            painter.rect_filled(
                rect,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(10, 12, 18, 56),
            );
            painter.rect_stroke(
                rect.shrink(1.0),
                CornerRadius::same(12),
                Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 255, 255, 64)),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn bento_card(
    ui: &mut egui::Ui,
    width: f32,
    min_height: f32,
    accent: [u8; 3],
    title: &str,
    badge: Option<(&str, Color32)>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let w = width.max(160.0);
    let frame = egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(19, 21, 28, 236))
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 26),
        ))
        .corner_radius(CornerRadius::same(13))
        .inner_margin(egui::Margin::same(11));
    ui.allocate_ui_with_layout(
        Vec2::new(w, min_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            frame.show(ui, |ui| {
                ui.set_min_width((w - 24.0).max(120.0));
                if min_height > 24.0 {
                    ui.set_min_height((min_height - 24.0).max(20.0));
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(title)
                            .strong()
                            .size(12.5)
                            .color(Color32::from_rgb(240, 242, 248)),
                    );
                    if let Some((badge_text, badge_col)) = badge {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let bg = Color32::from_rgba_unmultiplied(
                                badge_col.r(),
                                badge_col.g(),
                                badge_col.b(),
                                28,
                            );
                            egui::Frame::new()
                                .fill(bg)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(egui::Margin::symmetric(7, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(badge_text)
                                            .size(10.0)
                                            .color(badge_col)
                                            .strong(),
                                    );
                                });
                        });
                    } else {
                        let _ = accent;
                    }
                });
                ui.add_space(5.0);
                add_contents(ui);
            });
        },
    );
}

fn bento_2col(ui: &mut egui::Ui, gap: f32, mut col_fn: impl FnMut(&mut egui::Ui, f32, usize)) {
    let avail = ui.available_width();
    if avail >= 430.0 {
        let col_w = ((avail - gap) * 0.5).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            col_fn(ui, col_w, 0);
            col_fn(ui, col_w, 1);
        });
    } else {
        col_fn(ui, avail, 0);
        ui.add_space(gap);
        col_fn(ui, avail, 1);
    }
}

fn paint_settings_pill_bar(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    frac: f32,
    fill_col: Color32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width.max(24.0), height), Sense::hover());
    let r = (height * 0.5).ceil().clamp(1.0, 16.0) as u8;
    let cr = CornerRadius::same(r);
    ui.painter()
        .rect_filled(rect, cr, Color32::from_rgba_unmultiplied(255, 255, 255, 22));
    let f = frac.clamp(0.0, 1.0);
    if f > 0.01 {
        let fw = (rect.width() * f).max(height.min(rect.width()));
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(fw, height)),
            cr,
            fill_col,
        );
    }
}

fn pill_toggle_chip(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    accent: [u8; 3],
    min_size: Vec2,
) -> egui::Response {
    let [ar, ag, ab] = accent;
    let fill = if selected {
        Color32::from_rgba_unmultiplied(ar, ag, ab, 64)
    } else {
        Color32::from_rgb(31, 33, 41)
    };
    let stroke = if selected {
        Stroke::new(1.3, Color32::from_rgb(ar, ag, ab))
    } else {
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20))
    };
    let text_col = if selected {
        Color32::WHITE
    } else {
        Color32::from_rgb(180, 184, 196)
    };
    ui.add(
        egui::Button::new(RichText::new(label).color(text_col).size(11.5))
            .min_size(min_size)
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(9)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

pub(crate) struct SettingsBodyState<'a> {
    pub tab: &'a mut SettingsTab,
    pub config: &'a mut Config,
    pub todos: &'a mut TodoStore,
    pub new_todo_input: &'a mut String,
    pub calendar_url_input: &'a mut String,
    pub calendar_events: &'a [CalendarEvent],
    pub calendar_status: &'a str,
    pub calendar_cmd_tx: Option<&'a mpsc::Sender<CalendarCommand>>,
    pub config_dirty: &'a mut bool,
    pub todos_dirty: &'a mut bool,
    pub test_notification_requested: &'a mut bool,
    pub calendar_set_url_requested: &'a mut Option<String>,
    pub calendar_refresh_requested: &'a mut bool,
    pub installed_fonts: &'a mut Option<Vec<InstalledFont>>,
    pub font_search: &'a mut String,
}

pub(crate) fn render_settings_tabs_body(ui: &mut egui::Ui, st: &mut SettingsBodyState<'_>) {
    paint_settings_material_backdrop(ui, st.config);
    let left = ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));
    let right = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
    if !ui.ctx().memory(|m| m.focused().is_some()) && (left || right) {
        let tabs = SettingsTab::all();
        let pos = tabs.iter().position(|t| *t == *st.tab).unwrap_or(0);
        let next = (pos as i32 + if left { -1 } else { 1 }).rem_euclid(tabs.len() as i32) as usize;
        *st.tab = tabs[next];
    }

    let [ar, ag, ab] = st.config.accent_color;
    let accent_col = Color32::from_rgb(ar, ag, ab);
    let total_h = ui.available_height().max(320.0);
    let sidebar_w = 134.0;
    let gap = 10.0;

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        // Left Dashboard Sidebar Rail.
        let rail_alpha = match st.config.background_material {
            BackgroundMaterial::Solid => 245,
            BackgroundMaterial::Acrylic => 224,
            BackgroundMaterial::FrostedGlass => 216,
            BackgroundMaterial::LiquidGlass => 206,
            BackgroundMaterial::Glassmorphism => 214,
        };
        let rail_stroke_alpha = match st.config.background_material {
            BackgroundMaterial::LiquidGlass => 50,
            BackgroundMaterial::Glassmorphism => 66,
            _ => 24,
        };
        let rail_frame = egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(19, 20, 26, rail_alpha))
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, rail_stroke_alpha),
            ))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(egui::Margin::same(8));
        ui.allocate_ui_with_layout(
            Vec2::new(sidebar_w, total_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                rail_frame.show(ui, |ui| {
                    ui.set_min_width(sidebar_w - 18.0);
                    ui.set_min_height((total_h - 18.0).max(260.0));

                    // Brand pill with live mini Mascot.
                    let brand_frame = egui::Frame::new()
                        .fill(Color32::from_rgb(26, 28, 36))
                        .corner_radius(CornerRadius::same(11))
                        .inner_margin(egui::Margin::symmetric(8, 7));
                    brand_frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 7.0;
                            let (m_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                            ui.painter().rect_filled(
                                m_rect,
                                CornerRadius::same(6),
                                Color32::from_rgb(10, 10, 13),
                            );
                            let eye_dir = ui
                                .input(|i| i.pointer.hover_pos())
                                .map(|p| {
                                    let d = p - m_rect.center();
                                    Vec2::new(
                                        (d.x / 180.0).clamp(-1.0, 1.0),
                                        (d.y / 180.0).clamp(-1.0, 1.0),
                                    )
                                })
                                .unwrap_or(Vec2::ZERO);
                            paint_mascot(
                                ui.painter(),
                                Rect::from_center_size(m_rect.center(), Vec2::splat(16.0)),
                                eye_dir,
                                Color32::WHITE,
                            );
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(
                                    RichText::new("Liebert")
                                        .strong()
                                        .size(12.0)
                                        .color(Color32::WHITE),
                                );
                                ui.label(
                                    RichText::new("Dashboard")
                                        .size(9.5)
                                        .color(Color32::from_gray(145)),
                                );
                            });
                        });
                    });

                    ui.add_space(8.0);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    for tab in SettingsTab::all() {
                        let selected = *st.tab == tab;
                        let row_w = sidebar_w - 18.0;
                        let (rect, resp) =
                            ui.allocate_exact_size(Vec2::new(row_w, 29.0), Sense::click());
                        let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                        let fill = if selected {
                            Color32::from_rgba_unmultiplied(ar, ag, ab, 48)
                        } else if resp.hovered() {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                        } else {
                            Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
                        if selected {
                            let pill = Rect::from_min_size(
                                Pos2::new(rect.left() + 3.0, rect.center().y - 7.0),
                                Vec2::new(3.0, 14.0),
                            );
                            ui.painter()
                                .rect_filled(pill, CornerRadius::same(2), accent_col);
                        }
                        let icon_col = if selected {
                            accent_col
                        } else {
                            Color32::from_gray(150)
                        };
                        let icon_rect = Rect::from_center_size(
                            Pos2::new(rect.left() + 18.0, rect.center().y),
                            Vec2::splat(12.0),
                        );
                        paint_icon(ui.painter(), icon_rect, tab.icon(), icon_col);
                        let text_col = if selected {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(185)
                        };
                        ui.painter().text(
                            Pos2::new(rect.left() + 32.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            tab.label(),
                            FontId::proportional(12.0),
                            text_col,
                        );
                        if resp.clicked() {
                            *st.tab = tab;
                        }
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        let mode_str = if st.config.notch_dock_mode {
                            "Mode: Top Notch"
                        } else {
                            "Mode: Floating"
                        };
                        egui::Frame::new()
                            .fill(Color32::from_rgb(26, 28, 36))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.set_min_width(sidebar_w - 34.0);
                                ui.label(
                                    RichText::new(mode_str)
                                        .size(10.5)
                                        .color(Color32::from_gray(190)),
                                );
                                let (preview, tz_lbl) = format_clock(
                                    chrono::Utc::now(),
                                    &st.config.clock_timezone,
                                    st.config.clock_24h,
                                    false,
                                );
                                ui.label(
                                    RichText::new(format!("{preview} · {tz_lbl}"))
                                        .size(10.0)
                                        .monospace()
                                        .color(accent_col),
                                );
                            });
                    });
                });
            },
        );

        // Right Dashboard Content Area.
        let main_w = (ui.available_width()).max(240.0);
        ui.allocate_ui_with_layout(
            Vec2::new(main_w, total_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_clip_rect(ui.max_rect());

                // Top Dashboard Header Bar.
                let header_frame = egui::Frame::new()
                    .fill(Color32::from_rgb(19, 20, 26))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 16),
                    ))
                    .corner_radius(CornerRadius::same(13))
                    .inner_margin(egui::Margin::symmetric(12, 8));
                header_frame.show(ui, |ui| {
                    ui.set_min_width((main_w - 26.0).max(200.0));
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.label(
                                RichText::new(st.tab.label())
                                    .strong()
                                    .size(15.0)
                                    .color(Color32::WHITE),
                            );
                            ui.label(
                                RichText::new(st.tab.subtitle())
                                    .size(10.5)
                                    .color(Color32::from_gray(148)),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Test Popup")
                                            .size(11.0)
                                            .color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgba_unmultiplied(ar, ag, ab, 55))
                                    .stroke(Stroke::new(1.0, accent_col))
                                    .corner_radius(CornerRadius::same(9)),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .clicked()
                            {
                                *st.test_notification_requested = true;
                            }
                        });
                    });
                });

                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match *st.tab {
                        SettingsTab::Appearance => render_display_tab(
                            ui,
                            st.config,
                            st.config_dirty,
                            st.installed_fonts,
                            st.font_search,
                        ),
                        SettingsTab::Lyrics => {
                            render_lyrics_tab(ui, st.config, st.config_dirty);
                        }
                        SettingsTab::Features => render_features_tab(
                            ui,
                            st.config,
                            st.config_dirty,
                            st.test_notification_requested,
                        ),
                        SettingsTab::System => {
                            render_system_tab(ui, st.config, st.config_dirty);
                        }
                        SettingsTab::Battery => {
                            render_battery_tab(ui, st.config, st.config_dirty);
                        }
                        SettingsTab::Todo => {
                            *st.todos_dirty |= render_todo_tab(
                                ui,
                                st.config.accent_color,
                                st.todos,
                                st.new_todo_input,
                            );
                        }
                        SettingsTab::Calendar => render_calendar_tab(ui, st),
                        SettingsTab::About => render_about_tab(ui, st.config),
                    });
            },
        );
    });
}

pub fn render_settings_viewport(ctx: &egui::Context, st: SettingsWindowState<'_>) {
    let shared = Arc::clone(shared_settings_state());
    {
        let Ok(mut guard) = shared.lock() else {
            return;
        };
        if let Some(ds) = guard.as_mut() {
            if ds.dirty_from_ui {
                *st.open = ds.open;
                *st.focus_requested = ds.focus_requested;
                *st.tab = ds.tab;
                *st.config = ds.config.clone();
                *st.todos = ds.todos.clone();
                *st.new_todo_input = ds.new_todo_input.clone();
                *st.calendar_url_input = ds.calendar_url_input.clone();
                *st.installed_fonts = ds.installed_fonts.clone();
                *st.font_search = ds.font_search.clone();
                if ds.config_dirty_out {
                    *st.config_dirty = true;
                }
                if ds.test_notif_out {
                    *st.test_notification_requested = true;
                }
                ds.dirty_from_ui = false;
                ds.config_dirty_out = false;
                ds.test_notif_out = false;
            }
            if !*st.open {
                *guard = None;
                return;
            }
            ds.open = *st.open;
            ds.focus_requested = *st.focus_requested;
            ds.tab = *st.tab;
            ds.config = st.config.clone();
            ds.todos = st.todos.clone();
            ds.new_todo_input = st.new_todo_input.clone();
            ds.calendar_url_input = st.calendar_url_input.clone();
            if ds.calendar_status != st.calendar_status
                || ds.calendar_events.len() != st.calendar_events.len()
            {
                ds.calendar_events = st.calendar_events.to_vec();
                ds.calendar_status = st.calendar_status.to_owned();
            }
            ds.calendar_cmd_tx = st.calendar_cmd_tx.clone();
            if ds.installed_fonts.is_none() && st.installed_fonts.is_some() {
                ds.installed_fonts = st.installed_fonts.clone();
            }
            ds.font_search = st.font_search.clone();
        } else {
            if !*st.open {
                return;
            }
            *guard = Some(DeferredSettingsState {
                open: *st.open,
                focus_requested: *st.focus_requested,
                tab: *st.tab,
                config: st.config.clone(),
                todos: st.todos.clone(),
                new_todo_input: st.new_todo_input.clone(),
                calendar_url_input: st.calendar_url_input.clone(),
                calendar_events: st.calendar_events.to_vec(),
                calendar_status: st.calendar_status.to_owned(),
                calendar_cmd_tx: st.calendar_cmd_tx.clone(),
                installed_fonts: st.installed_fonts.clone(),
                font_search: st.font_search.clone(),
                dirty_from_ui: false,
                config_dirty_out: false,
                test_notif_out: false,
            });
        }
    }

    let root_ctx = ctx.clone();
    ctx.show_viewport_deferred(
        ViewportId::from_hash_of("liebert_settings"),
        ViewportBuilder::default()
            .with_title(SETTINGS_WINDOW_TITLE)
            .with_app_id("liebert-dynamic-island-settings")
            .with_inner_size([760.0, 520.0])
            .with_active(true)
            .with_resizable(true),
        move |ui, _class| {
            let Ok(mut guard) = shared.lock() else {
                return;
            };
            let Some(ds) = guard.as_mut() else {
                return;
            };
            if !ds.open {
                return;
            }
            if ui.input(|i| i.viewport().close_requested()) {
                ds.open = false;
                ds.focus_requested = false;
                ds.dirty_from_ui = true;
                root_ctx.request_repaint();
                return;
            }
            if ds.focus_requested {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                ds.focus_requested = false;
                ds.dirty_from_ui = true;
            }

            apply_settings_theme(ui, ds.config.accent_color);
            let panel_frame = egui::Frame::new()
                .fill(settings_panel_fill(&ds.config))
                .inner_margin(egui::Margin::same(10));

            let mut local_config_dirty = false;
            let mut local_todos_dirty = false;
            let mut local_test_notif = false;
            let prev_tab = ds.tab;
            let prev_new_todo = ds.new_todo_input.clone();
            let prev_cal_url = ds.calendar_url_input.clone();
            let prev_font_search = ds.font_search.clone();
            let prev_todo_len = ds.todos.items.len();

            egui::CentralPanel::default()
                .frame(panel_frame)
                .show(ui, |ui| {
                    let ds = &mut *ds;
                    render_settings_tabs_body(
                        ui,
                        &mut SettingsBodyState {
                            tab: &mut ds.tab,
                            config: &mut ds.config,
                            todos: &mut ds.todos,
                            new_todo_input: &mut ds.new_todo_input,
                            calendar_url_input: &mut ds.calendar_url_input,
                            calendar_events: &ds.calendar_events,
                            calendar_status: &ds.calendar_status,
                            calendar_cmd_tx: Some(&ds.calendar_cmd_tx),
                            config_dirty: &mut local_config_dirty,
                            todos_dirty: &mut local_todos_dirty,
                            test_notification_requested: &mut local_test_notif,
                            calendar_set_url_requested: &mut None,
                            calendar_refresh_requested: &mut false,
                            installed_fonts: &mut ds.installed_fonts,
                            font_search: &mut ds.font_search,
                        },
                    );
                });

            if local_config_dirty
                || local_todos_dirty
                || local_test_notif
                || ds.tab != prev_tab
                || ds.new_todo_input != prev_new_todo
                || ds.calendar_url_input != prev_cal_url
                || ds.font_search != prev_font_search
                || ds.todos.items.len() != prev_todo_len
            {
                ds.config_dirty_out |= local_config_dirty;
                ds.test_notif_out |= local_test_notif;
                ds.dirty_from_ui = true;
                root_ctx.request_repaint();
            }
        },
    );
}

pub enum SettingsIpcTarget {
    #[cfg(unix)]
    Unix(PathBuf),
    Tcp(String),
}

pub struct StandaloneSettingsApp {
    tab: SettingsTab,
    pub config: Config,
    todos: TodoStore,
    new_todo_input: String,
    calendar_url_input: String,
    installed_fonts: Option<Vec<InstalledFont>>,
    font_search: String,
    applied_font_name: String,
    ipc_target: Option<SettingsIpcTarget>,
}

impl StandaloneSettingsApp {
    pub fn new(ipc_target: Option<SettingsIpcTarget>) -> Self {
        let config = Config::load();
        let calendar_url_input = config.google_calendar_ics_url.clone();
        let applied_font_name = config.font_name.clone();
        let todos = TodoStore::load();
        Self {
            tab: SettingsTab::Appearance,
            config,
            todos,
            new_todo_input: String::new(),
            calendar_url_input,
            installed_fonts: None,
            font_search: String::new(),
            applied_font_name,
            ipc_target,
        }
    }

    fn notify(&self, line: &str) {
        use std::io::Write;
        use std::net::TcpStream;
        use std::time::Duration;

        let Some(target) = &self.ipc_target else {
            return;
        };
        match target {
            #[cfg(unix)]
            SettingsIpcTarget::Unix(path) => {
                if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(path) {
                    let _ = stream.write_all(line.as_bytes());
                    let _ = stream.write_all(b"\n");
                }
            }
            SettingsIpcTarget::Tcp(addr) => {
                if let Ok(sock) = addr.parse()
                    && let Ok(mut stream) =
                        TcpStream::connect_timeout(&sock, Duration::from_millis(200))
                {
                    let _ = stream.write_all(line.as_bytes());
                    let _ = stream.write_all(b"\n");
                }
            }
        }
    }

    pub fn render_ui(&mut self, ui: &mut egui::Ui) {
        apply_settings_theme(ui, self.config.accent_color);
        let panel_frame = egui::Frame::new()
            .fill(settings_panel_fill(&self.config))
            .inner_margin(egui::Margin::same(10));

        let mut config_dirty = false;
        let mut todos_dirty = false;
        let mut test_notif = false;
        let mut cal_set_url = None;
        let mut cal_refresh = false;
        let cal_status = if self.config.google_calendar_ics_url.is_empty() {
            "Not configured"
        } else {
            "Configured (synced in overlay)"
        };

        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ui, |ui| {
                render_settings_tabs_body(
                    ui,
                    &mut SettingsBodyState {
                        tab: &mut self.tab,
                        config: &mut self.config,
                        todos: &mut self.todos,
                        new_todo_input: &mut self.new_todo_input,
                        calendar_url_input: &mut self.calendar_url_input,
                        calendar_events: &[],
                        calendar_status: cal_status,
                        calendar_cmd_tx: None,
                        config_dirty: &mut config_dirty,
                        todos_dirty: &mut todos_dirty,
                        test_notification_requested: &mut test_notif,
                        calendar_set_url_requested: &mut cal_set_url,
                        calendar_refresh_requested: &mut cal_refresh,
                        installed_fonts: &mut self.installed_fonts,
                        font_search: &mut self.font_search,
                    },
                );
            });

        if config_dirty {
            if self.config.font_name != self.applied_font_name {
                self.applied_font_name = self.config.font_name.clone();
                crate::ui::apply_configured_fonts_light(ui.ctx(), &self.applied_font_name);
            }
            if let Err(err) = self.config.save() {
                eprintln!("Failed to save config: {err}");
            } else {
                if let Err(err) = platform_light::set_autostart(self.config.start_with_os) {
                    eprintln!("Failed to apply autostart setting: {err}");
                }
                self.notify("config");
            }
        }
        if todos_dirty {
            self.notify("todos");
        }
        if test_notif {
            self.notify("test_notif");
        }
        if let Some(url) = cal_set_url {
            self.notify(&format!("cal_url:{url}"));
        }
        if cal_refresh {
            self.notify("cal_refresh");
        }
    }
}

#[allow(dead_code)] // Used by liebert-settings; liebert-x11 calls settings_eframe::run_x11_settings.
pub fn run_standalone_settings(ipc_target: Option<SettingsIpcTarget>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return crate::platform::wayland_settings::run_wayland_settings(
            StandaloneSettingsApp::new(ipc_target),
        );
    }

    #[cfg(windows)]
    {
        crate::platform::windows_settings::run_windows_settings(StandaloneSettingsApp::new(
            ipc_target,
        ))
    }

    #[cfg(not(windows))]
    {
        crate::ui::settings_eframe::run_x11_settings(ipc_target)
    }
}

fn render_display_tab(
    ui: &mut egui::Ui,
    config: &mut Config,
    config_dirty: &mut bool,
    installed_fonts: &mut Option<Vec<InstalledFont>>,
    font_search: &mut String,
) {
    let mut changed = false;
    let accent = config.accent_color;
    let accent_col = Color32::from_rgb(accent[0], accent[1], accent[2]);
    let gap = 8.0;

    bento_2col(ui, gap, |ui, col_w, col| match col {
        0 => {
            let mode_badge = if config.notch_dock_mode {
                "Notch"
            } else {
                "Island"
            };
            bento_card(
                ui,
                col_w,
                168.0,
                accent,
                "Surface & Geometry",
                Some((mode_badge, accent_col)),
                |ui| {
                    let btn_w = ((ui.available_width() - 6.0) * 0.5).max(80.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if pill_toggle_chip(
                            ui,
                            "Top Notch",
                            config.notch_dock_mode,
                            accent,
                            Vec2::new(btn_w, 28.0),
                        )
                        .clicked()
                            && !config.notch_dock_mode
                        {
                            config.notch_dock_mode = true;
                            changed = true;
                        }
                        if pill_toggle_chip(
                            ui,
                            "Floating",
                            !config.notch_dock_mode,
                            accent,
                            Vec2::new(btn_w, 28.0),
                        )
                        .clicked()
                            && config.notch_dock_mode
                        {
                            config.notch_dock_mode = false;
                            changed = true;
                        }
                    });
                    ui.add_space(4.0);
                    changed |= ui
                        .add_enabled(
                            !config.notch_dock_mode,
                            egui::Slider::new(&mut config.top_offset_px, 0.0..=60.0)
                                .text("Top Offset"),
                        )
                        .changed();
                    changed |= ui
                        .add(egui::Slider::new(&mut config.scale, 0.75..=1.5).text("UI Scale"))
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.idle_width, 110.0..=240.0)
                                .text("Idle Width"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.expanded_width, 320.0..=520.0)
                                .text("Expanded Width"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.corner_radius, 8..=28)
                                .text("Corner Radius"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.collapse_timeout_secs, 2..=15)
                                .text("Auto-collapse (s)"),
                        )
                        .changed();
                },
            );
        }
        _ => {
            let mascot_badge = if config.show_idle_mascot {
                ("Mascot On", Color32::from_rgb(96, 186, 150))
            } else {
                ("Mascot Off", Color32::from_gray(140))
            };
            bento_card(
                ui,
                col_w,
                168.0,
                accent,
                "Idle, Mascot & Clock",
                Some(mascot_badge),
                |ui| {
                    let combo_w = (ui.available_width() - 8.0).clamp(130.0, 210.0);
                    egui::ComboBox::from_id_salt("idle_display_mode")
                        .width(combo_w)
                        .selected_text(config.idle_display.label())
                        .show_ui(ui, |ui| {
                            for mode in [
                                IdleDisplayMode::Auto,
                                IdleDisplayMode::Mascot,
                                IdleDisplayMode::Clock,
                                IdleDisplayMode::Lyrics,
                                IdleDisplayMode::NextEvent,
                                IdleDisplayMode::Pomodoro,
                                IdleDisplayMode::Battery,
                                IdleDisplayMode::Minimal,
                            ] {
                                if ui
                                    .selectable_value(&mut config.idle_display, mode, mode.label())
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        });
                    changed |= ui
                        .checkbox(
                            &mut config.show_idle_mascot,
                            "Show idle mascot (eye tracks cursor)",
                        )
                        .changed();

                    ui.add_space(4.0);
                    let selected_tz_label = COMMON_TIMEZONES
                        .iter()
                        .find(|(id, _)| id.eq_ignore_ascii_case(config.clock_timezone.trim()))
                        .map(|(_, label)| *label)
                        .unwrap_or(config.clock_timezone.as_str());

                    egui::ComboBox::from_id_salt("clock_tz_preset")
                        .width(combo_w)
                        .selected_text(selected_tz_label)
                        .show_ui(ui, |ui| {
                            for (id, label) in COMMON_TIMEZONES {
                                if ui
                                    .selectable_label(
                                        config.clock_timezone.eq_ignore_ascii_case(id),
                                        *label,
                                    )
                                    .clicked()
                                {
                                    config.clock_timezone = (*id).to_owned();
                                    changed = true;
                                }
                            }
                        });

                    ui.horizontal(|ui| {
                        ui.label("TZ:");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut config.clock_timezone)
                                    .hint_text("Local or Asia/Jakarta")
                                    .desired_width(120.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    let valid = is_valid_timezone(&config.clock_timezone);
                    let (preview, tz_label) = format_clock(
                        chrono::Utc::now(),
                        &config.clock_timezone,
                        config.clock_24h,
                        config.clock_show_seconds,
                    );
                    if valid {
                        ui.label(
                            RichText::new(format!("{preview} ({tz_label})"))
                                .small()
                                .color(Color32::from_rgb(100, 210, 150)),
                        );
                    } else {
                        ui.label(
                            RichText::new("Invalid TZ (using Local)")
                                .small()
                                .color(Color32::from_rgb(255, 130, 120)),
                        );
                    }
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut config.clock_24h, "24-hour").changed();
                        changed |= ui
                            .checkbox(&mut config.clock_show_seconds, "Seconds")
                            .changed();
                    });
                },
            );
        }
    });

    ui.add_space(gap);

    bento_2col(ui, gap, |ui, col_w, col| match col {
        0 => {
            bento_card(
                ui,
                col_w,
                198.0,
                accent,
                "Theme, Colors & Glass",
                Some((config.background_material.label(), accent_col)),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for mat in [
                            BackgroundMaterial::Solid,
                            BackgroundMaterial::Acrylic,
                            BackgroundMaterial::FrostedGlass,
                            BackgroundMaterial::LiquidGlass,
                            BackgroundMaterial::Glassmorphism,
                        ] {
                            let selected = config.background_material == mat;
                            if pill_toggle_chip(
                                ui,
                                mat.label(),
                                selected,
                                accent,
                                Vec2::new(72.0, 23.0),
                            )
                            .clicked()
                                && !selected
                            {
                                config.background_material = mat;
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Theme:");
                        let current_preset = config.matching_preset();
                        let selected_label = current_preset.map_or("Custom", |p| p.name);
                        let selected_accent =
                            current_preset.map_or(config.accent_color, |p| p.accent_color);
                        let sel_sw = Color32::from_rgb(
                            selected_accent[0],
                            selected_accent[1],
                            selected_accent[2],
                        );
                        egui::ComboBox::from_id_salt("theme_preset_select")
                            .width((ui.available_width() - 6.0).max(160.0))
                            .selected_text(
                                RichText::new(format!("●  {selected_label}")).color(sel_sw),
                            )
                            .show_ui(ui, |ui| {
                                for preset in crate::config::THEME_PRESETS {
                                    let is_sel = current_preset.is_some_and(|p| p.id == preset.id);
                                    let sw = Color32::from_rgb(
                                        preset.accent_color[0],
                                        preset.accent_color[1],
                                        preset.accent_color[2],
                                    );
                                    let txt = if is_sel {
                                        RichText::new(format!("●  {}", preset.name))
                                            .strong()
                                            .color(sw)
                                    } else {
                                        RichText::new(format!("●  {}", preset.name)).color(sw)
                                    };
                                    if ui.selectable_label(is_sel, txt).clicked() {
                                        config.apply_preset(preset.id);
                                        changed = true;
                                    }
                                }
                            });
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("BG:");
                        let mut bg_rgb =
                            [config.bg_color[0], config.bg_color[1], config.bg_color[2]];
                        if ui.color_edit_button_srgb(&mut bg_rgb).changed() {
                            config.bg_color[0] = bg_rgb[0];
                            config.bg_color[1] = bg_rgb[1];
                            config.bg_color[2] = bg_rgb[2];
                            changed = true;
                        }
                        ui.label("Accent:");
                        if ui
                            .color_edit_button_srgb(&mut config.accent_color)
                            .changed()
                        {
                            changed = true;
                        }
                        ui.label("Text:");
                        if ui.color_edit_button_srgb(&mut config.text_color).changed() {
                            changed = true;
                        }
                    });
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.bg_color[3], 60..=255).text("BG Opacity"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.border_alpha, 0..=160)
                                .text("Border Opacity"),
                        )
                        .changed();
                },
            );
        }
        _ => {
            bento_card(
                ui,
                col_w,
                192.0,
                accent,
                "Alignment & Font",
                Some((config.text_align.label(), accent_col)),
                |ui| {
                    let btn_w = ((ui.available_width() - 12.0) / 3.0).max(52.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for mode in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
                            let selected = config.text_align == mode;
                            if pill_toggle_chip(
                                ui,
                                mode.label(),
                                selected,
                                accent,
                                Vec2::new(btn_w, 26.0),
                            )
                            .clicked()
                                && !selected
                            {
                                config.text_align = mode;
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Device Font").size(11.0).color(Color32::GRAY));
                    ui.add(
                        egui::TextEdit::singleline(font_search)
                            .hint_text("Filter fonts...")
                            .desired_width(f32::INFINITY),
                    );
                    if !font_search.trim().is_empty() && installed_fonts.is_none() {
                        *installed_fonts = Some(platform_light::list_installed_fonts());
                    }
                    let selected_font = if config.font_name.is_empty() {
                        "System Default"
                    } else {
                        config.font_name.as_str()
                    };
                    let query = font_search.trim().to_ascii_lowercase();
                    let combo_w = (ui.available_width() - 8.0).clamp(130.0, 210.0);
                    egui::ComboBox::from_id_salt("device_font_selector")
                        .selected_text(selected_font)
                        .width(combo_w)
                        .show_ui(ui, |ui| {
                            // Lazy-scan OS fonts only when the selector is actually opened.
                            let fonts = installed_fonts
                                .get_or_insert_with(platform_light::list_installed_fonts);
                            if ui
                                .selectable_label(config.font_name.is_empty(), "System Default")
                                .clicked()
                                && !config.font_name.is_empty()
                            {
                                config.font_name.clear();
                                changed = true;
                            }
                            for f in fonts.iter().filter(|f| {
                                query.is_empty() || f.name.to_ascii_lowercase().contains(&query)
                            }) {
                                let is_sel = config.font_name.eq_ignore_ascii_case(&f.name);
                                if ui.selectable_label(is_sel, &f.name).clicked() && !is_sel {
                                    config.font_name = f.name.clone();
                                    changed = true;
                                }
                            }
                        });
                },
            );
        }
    });

    if changed {
        *config_dirty = true;
    }
}

fn render_lyrics_tab(ui: &mut egui::Ui, config: &mut Config, config_dirty: &mut bool) {
    let mut changed = false;
    let accent = config.accent_color;
    let status_badge = if config.show_lyrics {
        ("Active", Color32::from_rgb(96, 186, 150))
    } else {
        ("Disabled", Color32::from_gray(140))
    };

    bento_card(
        ui,
        ui.available_width(),
        44.0,
        accent,
        "Media & Synced Lyrics",
        Some(status_badge),
        |ui| {
            changed |= ui
                .checkbox(&mut config.show_lyrics, "Show Music & Synced Lyrics")
                .changed();
        },
    );

    if config.show_lyrics {
        ui.add_space(8.0);
        bento_2col(ui, 8.0, |ui, col_w, col| match col {
            0 => {
                bento_card(ui, col_w, 148.0, accent, "Visuals & Karaoke", None, |ui| {
                    changed |= ui
                        .checkbox(
                            &mut config.dynamic_album_color,
                            "Dynamic accent from album art",
                        )
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut config.compact_album_gradient,
                            "Album gradient in Compact/Idle pill",
                        )
                        .changed();
                    changed |= ui
                        .checkbox(&mut config.karaoke_lyrics, "Karaoke highlight")
                        .changed();
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Karaoke Mode")
                            .size(11.0)
                            .color(Color32::GRAY),
                    );
                    ui.add_enabled_ui(config.karaoke_lyrics, |ui| {
                        let combo_w = (ui.available_width() - 8.0).clamp(130.0, 210.0);
                        egui::ComboBox::from_id_salt("karaoke_lyric_mode")
                            .width(combo_w)
                            .selected_text(config.karaoke_lyric_mode.label())
                            .show_ui(ui, |ui| {
                                for mode in [
                                    KaraokeLyricMode::Original,
                                    KaraokeLyricMode::Romaji,
                                    KaraokeLyricMode::Translate,
                                ] {
                                    if ui
                                        .selectable_value(
                                            &mut config.karaoke_lyric_mode,
                                            mode,
                                            mode.label(),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                    });
                });
            }
            _ => {
                bento_card(ui, col_w, 148.0, accent, "Translation & Sync", None, |ui| {
                    changed |= ui
                        .checkbox(&mut config.romaji_lyrics, "Romaji for non-Latin lyrics")
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.lyric_offset_ms, -5000..=5000)
                                .text("Offset (ms)"),
                        )
                        .changed();
                    changed |= ui
                        .checkbox(&mut config.translate_lyrics, "Translate lyrics to:")
                        .changed();
                    ui.add_enabled_ui(config.translate_lyrics, |ui| {
                        let selected_lang = TRANSLATE_LANGUAGES
                            .iter()
                            .find(|(code, _)| *code == config.lyric_translate_lang)
                            .map(|(_, name)| *name)
                            .unwrap_or(config.lyric_translate_lang.as_str());
                        let combo_w = (ui.available_width() - 8.0).clamp(130.0, 210.0);
                        egui::ComboBox::from_id_salt("lyric_translate_lang")
                            .width(combo_w)
                            .selected_text(selected_lang)
                            .show_ui(ui, |ui| {
                                for (code, name) in TRANSLATE_LANGUAGES {
                                    if ui
                                        .selectable_label(
                                            config.lyric_translate_lang == *code,
                                            format!("{name} ({code})"),
                                        )
                                        .clicked()
                                    {
                                        config.lyric_translate_lang = (*code).to_owned();
                                        changed = true;
                                    }
                                }
                            });
                    });
                    changed |= ui
                        .add_enabled(
                            config.translate_lyrics,
                            egui::Checkbox::new(
                                &mut config.show_pill_translation,
                                "Show translation in pill",
                            ),
                        )
                        .changed();
                });
            }
        });
    }

    if changed {
        *config_dirty = true;
    }
}

fn render_features_tab(
    ui: &mut egui::Ui,
    config: &mut Config,
    config_dirty: &mut bool,
    test_notification_requested: &mut bool,
) {
    let mut changed = false;
    let accent = config.accent_color;
    let gap = 8.0;

    bento_2col(ui, gap, |ui, col_w, col| match col {
        0 => {
            bento_card(
                ui,
                col_w,
                148.0,
                accent,
                "Visibility & Fullscreen",
                None,
                |ui| {
                    changed |= ui
                        .checkbox(
                            &mut config.auto_hide_when_idle,
                            "Auto-hide island when idle",
                        )
                        .changed();
                    changed |= ui
                        .add_enabled(
                            config.auto_hide_when_idle,
                            egui::Slider::new(&mut config.auto_hide_delay_secs, 0..=60)
                                .text("Hide delay (s)"),
                        )
                        .changed();
                    if config.auto_hide_when_idle {
                        ui.label(
                            RichText::new("Hover top-center edge to reveal the notch.")
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                    changed |= ui
                        .checkbox(&mut config.hide_in_fullscreen, "Hide in fullscreen apps")
                        .changed();
                    if config.hide_in_fullscreen {
                        changed |= ui
                            .checkbox(
                                &mut config.hide_in_windowed_fullscreen,
                                "Also hide on maximized apps",
                            )
                            .changed();
                    }
                },
            );
        }
        _ => {
            bento_card(
                ui,
                col_w,
                148.0,
                accent,
                "Notifications & Modules",
                None,
                |ui| {
                    ui.horizontal(|ui| {
                        changed |= ui
                            .checkbox(&mut config.show_notifications, "Notification popups")
                            .changed();
                        if ui.button("Test").clicked() {
                            *test_notification_requested = true;
                        }
                    });
                    changed |= ui
                        .checkbox(&mut config.show_todo, "Todo List tab")
                        .changed();
                    changed |= ui
                        .checkbox(&mut config.show_calendar, "Google Calendar tab")
                        .changed();
                    changed |= ui
                        .checkbox(&mut config.show_pomodoro, "Pomodoro Timer tab")
                        .changed();
                },
            );
        }
    });

    ui.add_space(gap);

    bento_2col(ui, gap, |ui, col_w, col| match col {
        0 => {
            bento_card(ui, col_w, 108.0, accent, "Pomodoro Durations", None, |ui| {
                ui.add_enabled_ui(config.show_pomodoro, |ui| {
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.pomodoro_focus_mins, 1..=120)
                                .text("Focus (m)"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.pomodoro_short_break_mins, 1..=60)
                                .text("Short Break (m)"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut config.pomodoro_long_break_mins, 1..=90)
                                .text("Long Break (m)"),
                        )
                        .changed();
                });
            });
        }
        _ => {
            bento_card(
                ui,
                col_w,
                108.0,
                accent,
                "Reset Configuration",
                None,
                |ui| {
                    ui.label(
                        RichText::new("Restore default visuals and modules (keeps Calendar URL).")
                            .small()
                            .color(Color32::GRAY),
                    );
                    ui.add_space(6.0);
                    if ui.button("Reset to Defaults").clicked() {
                        let keep_url = config.google_calendar_ics_url.clone();
                        *config = Config {
                            google_calendar_ics_url: keep_url,
                            ..Config::default()
                        };
                        changed = true;
                    }
                },
            );
        }
    });

    if changed {
        *config_dirty = true;
    }
}

fn render_todo_tab(
    ui: &mut egui::Ui,
    accent: [u8; 3],
    todos: &mut TodoStore,
    new_todo_input: &mut String,
) -> bool {
    let mut dirty = false;
    let total = todos.items.len();
    let pending = todos.pending_count();
    let done = total.saturating_sub(pending);
    let badge = format!("{pending} pending · {done} done");

    bento_card(
        ui,
        ui.available_width(),
        68.0,
        accent,
        "Quick Add Task",
        Some((&badge, Color32::from_rgb(accent[0], accent[1], accent[2]))),
        |ui| {
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(new_todo_input)
                        .hint_text("Type a task and press Enter...")
                        .desired_width((ui.available_width() - 62.0).max(160.0)),
                );
                let submit = (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    || ui.button("Add").clicked();
                if submit && todos.add(new_todo_input).is_some() {
                    new_todo_input.clear();
                    dirty = true;
                    resp.request_focus();
                }
            });
        },
    );

    ui.add_space(8.0);

    bento_card(
        ui,
        ui.available_width(),
        160.0,
        accent,
        "Task List",
        None,
        |ui| {
            if total > 0 {
                paint_settings_pill_bar(
                    ui,
                    ui.available_width(),
                    5.0,
                    done as f32 / total as f32,
                    Color32::from_rgb(accent[0], accent[1], accent[2]),
                );
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                if ui.button("Clear Completed").clicked() {
                    todos.clear_done();
                    dirty = true;
                }
            });
            ui.add_space(4.0);

            let mut toggle_id = None;
            let mut remove_id = None;
            if todos.items.is_empty() {
                ui.label(RichText::new("No tasks yet.").color(Color32::GRAY));
            }
            for (idx, item) in todos.items.iter().enumerate() {
                let done_green = Color32::from_rgb(96, 186, 150);
                let stripe = if item.done {
                    Color32::from_rgba_unmultiplied(
                        done_green.r(),
                        done_green.g(),
                        done_green.b(),
                        150,
                    )
                } else {
                    Color32::from_rgb(accent[0], accent[1], accent[2])
                };
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(
                        255,
                        255,
                        255,
                        if item.done { 8 } else { 15 },
                    ))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(
                            255,
                            255,
                            255,
                            if item.done { 14 } else { 24 },
                        ),
                    ))
                    .corner_radius(CornerRadius::same(9))
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (bar_r, _) =
                                ui.allocate_exact_size(Vec2::new(3.0, 16.0), Sense::hover());
                            ui.painter()
                                .rect_filled(bar_r, CornerRadius::same(2), stripe);
                            let mut item_done = item.done;
                            let txt = if item.done {
                                RichText::new(&item.text)
                                    .color(Color32::from_gray(155))
                                    .strikethrough()
                            } else {
                                RichText::new(&item.text).strong().color(Color32::WHITE)
                            };
                            if ui
                                .push_id(("settings_todo", item.id, idx), |ui| {
                                    ui.checkbox(&mut item_done, txt)
                                })
                                .inner
                                .changed()
                            {
                                toggle_id = Some(item.id);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if icon_button(
                                        ui,
                                        IconKind::Close,
                                        Vec2::splat(22.0),
                                        11.0,
                                        Color32::from_gray(200),
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 14),
                                        CornerRadius::same(6),
                                    )
                                    .on_hover_text("Remove task")
                                    .clicked()
                                    {
                                        remove_id = Some(item.id);
                                    }
                                    let badge_txt = if item.done { "DONE" } else { "ACTIVE" };
                                    let badge_col = if item.done {
                                        done_green
                                    } else {
                                        Color32::from_rgb(accent[0], accent[1], accent[2])
                                    };
                                    egui::Frame::new()
                                        .fill(Color32::from_rgba_unmultiplied(
                                            badge_col.r(),
                                            badge_col.g(),
                                            badge_col.b(),
                                            26,
                                        ))
                                        .corner_radius(CornerRadius::same(6))
                                        .inner_margin(egui::Margin::symmetric(6, 1))
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(badge_txt)
                                                    .size(9.0)
                                                    .strong()
                                                    .color(badge_col),
                                            );
                                        });
                                },
                            );
                        });
                    });
                ui.add_space(3.0);
            }
            if let Some(id) = toggle_id {
                todos.toggle(id);
                dirty = true;
            }
            if let Some(id) = remove_id {
                todos.remove(id);
                dirty = true;
            }
        },
    );

    if dirty && let Err(err) = todos.save() {
        eprintln!("Failed to save todos: {err}");
    }
    dirty
}

pub(crate) fn apply_settings_theme(ui: &mut egui::Ui, accent: [u8; 3]) {
    let style = ui.style_mut();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(14, 15, 19);
    style.visuals.window_fill = Color32::from_rgb(22, 24, 30);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(31, 33, 41);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(31, 33, 41);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(44, 47, 58);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(44, 47, 58);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(8);
    style.visuals.selection.bg_fill =
        Color32::from_rgba_unmultiplied(accent[0], accent[1], accent[2], 175);
    style.visuals.selection.stroke =
        Stroke::new(1.0, Color32::from_rgb(accent[0], accent[1], accent[2]));
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
}

fn render_calendar_tab(ui: &mut egui::Ui, st: &mut SettingsBodyState<'_>) {
    let accent = st.config.accent_color;
    let accent_col = Color32::from_rgb(accent[0], accent[1], accent[2]);

    bento_card(
        ui,
        ui.available_width(),
        104.0,
        accent,
        "Google Calendar Secret iCal URL (.ics)",
        Some((st.calendar_status, accent_col)),
        |ui| {
            ui.label(
                RichText::new(
                    "Google Calendar > Settings > [Your Calendar] > Secret address in iCal format",
                )
                .small()
                .color(Color32::GRAY),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(st.calendar_url_input)
                    .hint_text("https://calendar.google.com/calendar/ical/.../basic.ics")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Save & Sync").clicked() {
                    st.config.google_calendar_ics_url = st.calendar_url_input.trim().to_owned();
                    *st.config_dirty = true;
                    if let Some(tx) = st.calendar_cmd_tx {
                        let _ = tx.send(CalendarCommand::SetUrl(
                            st.config.google_calendar_ics_url.clone(),
                        ));
                    } else {
                        *st.calendar_set_url_requested =
                            Some(st.config.google_calendar_ics_url.clone());
                    }
                }
                if ui.button("Refresh Now").clicked() {
                    if let Some(tx) = st.calendar_cmd_tx {
                        let _ = tx.send(CalendarCommand::Refresh);
                    } else {
                        *st.calendar_refresh_requested = true;
                    }
                }
                if ui
                    .add_enabled(
                        !st.calendar_url_input.trim().is_empty(),
                        egui::Button::new("Copy"),
                    )
                    .clicked()
                {
                    ui.ctx().copy_text(st.calendar_url_input.trim().to_owned());
                }
            });
        },
    );

    ui.add_space(8.0);
    bento_card(
        ui,
        ui.available_width(),
        140.0,
        accent,
        "Upcoming Events",
        None,
        |ui| {
            let now_utc = chrono::Utc::now().timestamp();
            let upcoming = upcoming_events(st.calendar_events, now_utc, 8);
            if upcoming.is_empty() {
                ui.label(RichText::new("No upcoming events loaded.").color(Color32::GRAY));
            } else {
                for ev in upcoming {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 10))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format_local_time(ev.start_utc))
                                        .monospace()
                                        .strong()
                                        .color(accent_col),
                                );
                                ui.label(&ev.summary);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(format_countdown(
                                                ev.start_utc,
                                                ev.end_utc,
                                                now_utc,
                                            ))
                                            .small()
                                            .color(Color32::GRAY),
                                        );
                                    },
                                );
                            });
                        });
                    ui.add_space(3.0);
                }
            }
        },
    );
}

fn render_system_tab(ui: &mut egui::Ui, config: &mut Config, config_dirty: &mut bool) {
    let battery = read_battery();
    let accent = config.accent_color;
    let gap = 8.0;

    bento_2col(ui, gap, |ui, col_w, col| match col {
        0 => {
            let startup_badge = if config.start_with_os {
                ("Enabled", Color32::from_rgb(96, 186, 150))
            } else {
                ("Off", Color32::from_gray(140))
            };
            bento_card(
                ui,
                col_w,
                92.0,
                accent,
                "Startup",
                Some(startup_badge),
                |ui| {
                    if ui
                        .checkbox(&mut config.start_with_os, "Start with OS (autostart)")
                        .changed()
                    {
                        *config_dirty = true;
                    }
                    ui.label(
                        RichText::new("User-level login item (no admin/root required).")
                            .small()
                            .color(Color32::GRAY),
                    );
                },
            );
        }
        _ => {
            let mon_badge = if config.taskbar_monitor_enabled {
                ("Running", Color32::from_rgb(96, 186, 150))
            } else {
                ("Off", Color32::from_gray(140))
            };
            bento_card(
                ui,
                col_w,
                92.0,
                accent,
                "Taskbar Monitor",
                Some(mon_badge),
                |ui| {
                    if ui
                        .checkbox(
                            &mut config.taskbar_monitor_enabled,
                            "Enable taskbar system monitor",
                        )
                        .changed()
                    {
                        *config_dirty = true;
                    }
                    ui.add_enabled_ui(config.taskbar_monitor_enabled, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Interval:");
                            for (ms, label) in [(500_u64, "500 ms"), (1000, "1 s"), (2000, "2 s")] {
                                if ui
                                    .selectable_value(&mut config.taskbar_interval_ms, ms, label)
                                    .changed()
                                {
                                    *config_dirty = true;
                                }
                            }
                        });
                    });
                },
            );
        }
    });

    ui.add_space(gap);

    bento_card(
        ui,
        ui.available_width(),
        142.0,
        accent,
        "Taskbar Metrics & Placement",
        None,
        |ui| {
            ui.add_enabled_ui(config.taskbar_monitor_enabled, |ui| {
                ui.label(RichText::new("Metrics").strong().size(11.5));
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .checkbox(&mut config.taskbar_show_cpu, "CPU (%)")
                        .changed()
                    {
                        *config_dirty = true;
                    }
                    if ui.checkbox(&mut config.taskbar_show_ram, "RAM").changed() {
                        *config_dirty = true;
                    }
                    if ui.checkbox(&mut config.taskbar_show_disk, "Disk").changed() {
                        *config_dirty = true;
                    }
                    if ui
                        .checkbox(&mut config.taskbar_show_network, "Network (Up/Down)")
                        .changed()
                    {
                        *config_dirty = true;
                    }
                    if battery.is_some()
                        && ui
                            .checkbox(&mut config.taskbar_show_battery, "Battery")
                            .changed()
                    {
                        *config_dirty = true;
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    for fmt in [
                        TaskbarFormat::CompactText,
                        TaskbarFormat::IconValue,
                        TaskbarFormat::MiniBar,
                    ] {
                        if ui
                            .selectable_value(&mut config.taskbar_format, fmt, fmt.label())
                            .changed()
                        {
                            *config_dirty = true;
                        }
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Position:");
                    for pos in [
                        TaskbarPosition::LeftOfTray,
                        TaskbarPosition::RightOfTray,
                        TaskbarPosition::TopOfTaskbar,
                    ] {
                        if ui
                            .selectable_value(&mut config.taskbar_position, pos, pos.label())
                            .changed()
                        {
                            *config_dirty = true;
                        }
                    }
                });
            });
        },
    );
}

fn render_battery_tab(ui: &mut egui::Ui, config: &mut Config, config_dirty: &mut bool) {
    let battery = read_battery();
    let accent = config.accent_color;
    let accent_col = Color32::from_rgb(accent[0], accent[1], accent[2]);
    let gap = 8.0;

    bento_2col(ui, gap, |ui, col_w, col| match col {
        0 => {
            let status_badge = battery.map(|b| (b.status.label(), Color32::from_rgb(96, 186, 150)));
            bento_card(
                ui,
                col_w,
                108.0,
                accent,
                "Device Battery",
                status_badge,
                |ui| {
                    if let Some(bat) = battery {
                        ui.label(
                            RichText::new(format!("{}%", bat.percent))
                                .size(22.0)
                                .strong()
                                .monospace()
                                .color(Color32::WHITE),
                        );
                        paint_settings_pill_bar(
                            ui,
                            ui.available_width(),
                            7.0,
                            bat.percent as f32 / 100.0,
                            accent_col,
                        );
                        ui.add_space(4.0);
                        if let Some(rem) = bat.remaining_secs {
                            ui.label(
                                RichText::new(format!("{} remaining", format_remaining(rem)))
                                    .small()
                                    .color(Color32::GRAY),
                            );
                        }
                    } else {
                        ui.label(
                            RichText::new("No battery detected")
                                .size(13.0)
                                .color(Color32::GRAY),
                        );
                        ui.label(
                            RichText::new("Desktop PC or unsupported power_supply device.")
                                .small()
                                .color(Color32::from_gray(130)),
                        );
                    }
                },
            );
        }
        _ => {
            bento_card(ui, col_w, 108.0, accent, "Battery Tab", None, |ui| {
                if ui
                    .checkbox(&mut config.show_battery, "Show Battery tab")
                    .changed()
                {
                    *config_dirty = true;
                }
                ui.label(
                    RichText::new(
                        "Charging animates only while plugged in; idle while on battery.",
                    )
                    .small()
                    .color(Color32::GRAY),
                );
            });
        }
    });

    ui.add_space(gap);

    bento_card(
        ui,
        ui.available_width(),
        104.0,
        accent,
        "Low Battery Alerts",
        None,
        |ui| {
            if ui
                .checkbox(&mut config.battery_low_alerts, "Low-battery notifications")
                .changed()
            {
                *config_dirty = true;
            }
            ui.add_enabled_ui(config.battery_low_alerts, |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut config.battery_warn_pct, 5..=50)
                            .text("Warning threshold (%)"),
                    )
                    .changed()
                {
                    if config.battery_critical_pct > config.battery_warn_pct {
                        config.battery_critical_pct = config.battery_warn_pct;
                    }
                    *config_dirty = true;
                }
                if ui
                    .add(
                        egui::Slider::new(&mut config.battery_critical_pct, 3..=30)
                            .text("Critical threshold (%)"),
                    )
                    .changed()
                {
                    if config.battery_critical_pct > config.battery_warn_pct {
                        config.battery_warn_pct = config.battery_critical_pct;
                    }
                    *config_dirty = true;
                }
            });
        },
    );
}

fn render_about_tab(ui: &mut egui::Ui, config: &Config) {
    let accent = config.accent_color;
    let accent_col = Color32::from_rgb(accent[0], accent[1], accent[2]);
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    let repo_url = "https://github.com/Zahwan316/liebert-dynamic-island";
    let gap = 8.0;

    bento_card(
        ui,
        ui.available_width(),
        96.0,
        accent,
        "Liebert Dynamic Island",
        Some((version, Color32::from_rgb(96, 186, 150))),
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (m_rect, _) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
                ui.painter()
                    .rect_filled(m_rect, CornerRadius::same(12), Color32::from_rgb(10, 10, 14));
                let eye_dir = ui
                    .input(|i| i.pointer.hover_pos())
                    .map(|p| {
                        let d = p - m_rect.center();
                        Vec2::new(
                            (d.x / 180.0).clamp(-1.0, 1.0),
                            (d.y / 180.0).clamp(-1.0, 1.0),
                        )
                    })
                    .unwrap_or(Vec2::ZERO);
                paint_mascot(
                    ui.painter(),
                    Rect::from_center_size(m_rect.center(), Vec2::splat(32.0)),
                    eye_dir,
                    Color32::WHITE,
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(
                        RichText::new("Lightweight Native Rust Overlay (Windows & Linux)")
                            .strong()
                            .size(13.0)
                            .color(Color32::WHITE),
                    );
                    ui.label(
                        RichText::new(format!(
                            "Version {version}  •  Created by Zahwan316  •  License GPL-3.0-or-later"
                        ))
                        .size(11.5)
                        .color(Color32::from_gray(195)),
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Repository:")
                                .size(11.0)
                                .color(Color32::GRAY),
                        );
                        ui.label(
                            RichText::new(repo_url)
                                .size(11.0)
                                .color(accent_col),
                        );
                        if ui.small_button("Copy URL").clicked() {
                            ui.ctx().copy_text(repo_url.to_owned());
                        }
                    });
                });
            });
        },
    );

    ui.add_space(gap);

    bento_2col(ui, gap, |ui, col_w, col| match col {
        0 => {
            bento_card(
                ui,
                col_w,
                148.0,
                accent,
                "Contributors & Engine",
                Some(("2 Contributors", Color32::from_rgb(96, 186, 150))),
                |ui| {
                    ui.label(
                        RichText::new("Creator: Zahwan316 (GitHub · 34 commits)")
                            .strong()
                            .size(11.8)
                            .color(Color32::WHITE),
                    );
                    ui.label(
                        RichText::new("Contributor: Gjenius20 / Zjen (GitHub · 8 commits)")
                            .strong()
                            .size(11.5)
                            .color(accent_col),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("Engine: Rust 2024 · SoftCanvas CPU · egui 0.37")
                            .size(10.8)
                            .color(Color32::from_gray(185)),
                    );
                    ui.label(
                        RichText::new("Backends: Wayland wl_shm/layer-shell · Win32 DIB/WinRT")
                            .size(10.8)
                            .color(Color32::from_gray(185)),
                    );
                },
            );
        }
        _ => {
            bento_card(
                ui,
                col_w,
                148.0,
                accent,
                "Active Session Profile",
                None,
                |ui| {
                    let os_lbl = if cfg!(windows) {
                        "Windows (Win32 + WinRT)"
                    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                        "Linux (Wayland CPU)"
                    } else {
                        "Linux (X11 / Fallback)"
                    };
                    ui.label(
                        RichText::new(format!("Platform: {os_lbl}"))
                            .size(11.5)
                            .color(Color32::WHITE),
                    );
                    ui.label(
                        RichText::new(format!(
                            "Background: {}",
                            config.background_material.label()
                        ))
                        .size(11.5)
                        .color(accent_col),
                    );
                    ui.label(
                        RichText::new(format!(
                            "Surface: {}  •  Mascot: {}",
                            if config.notch_dock_mode {
                                "Top Notch"
                            } else {
                                "Floating Island"
                            },
                            if config.show_idle_mascot {
                                "On (Blink + Glow)"
                            } else {
                                "Off"
                            }
                        ))
                        .size(11.0)
                        .color(Color32::from_gray(180)),
                    );
                },
            );
        }
    });
}
