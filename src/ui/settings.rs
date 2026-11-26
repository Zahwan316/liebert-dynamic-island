#[cfg(unix)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2, ViewportBuilder, ViewportId};

use crate::battery::{format_remaining, read_battery};
use crate::calendar::{CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events};
use crate::clock::{COMMON_TIMEZONES, format_clock, is_valid_timezone};
use crate::config::{Config, IdleDisplayMode, KaraokeLyricMode, TaskbarFormat, TaskbarPosition, TextAlign};
use crate::lyrics::TRANSLATE_LANGUAGES;
use crate::platform::InstalledFont;
use crate::todo::TodoStore;
use crate::ui::icons::{IconKind, icon_button};

pub const SETTINGS_WINDOW_TITLE: &str = "Liebert Dynamic Island — Settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Lyrics,
    Features,
    System,
    Todo,
    Calendar,
}

impl SettingsTab {
    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Display",
            Self::Lyrics => "Lyrics",
            Self::Features => "Features",
            Self::System => "System",
            Self::Todo => "Tasks",
            Self::Calendar => "Calendar",
        }
    }

    fn all() -> [Self; 6] {
        [
            Self::Appearance,
            Self::Lyrics,
            Self::Features,
            Self::System,
            Self::Todo,
            Self::Calendar,
        ]
    }
}

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
            .with_inner_size([620.0, 560.0])
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
                .fill(Color32::from_rgb(19, 19, 23))
                .inner_margin(egui::Margin::same(14));

            let mut local_config_dirty = false;
            let mut local_todos_dirty = false;
            let mut local_test_notif = false;
            let prev_tab = ds.tab;
            let prev_new_todo = ds.new_todo_input.clone();
            let prev_cal_url = ds.calendar_url_input.clone();
            let prev_font_search = ds.font_search.clone();
            let prev_todo_len = ds.todos.items.len();

            egui::CentralPanel::default().frame(panel_frame).show(ui, |ui| {
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

struct SettingsBodyState<'a> {
    tab: &'a mut SettingsTab,
    config: &'a mut Config,
    todos: &'a mut TodoStore,
    new_todo_input: &'a mut String,
    calendar_url_input: &'a mut String,
    calendar_events: &'a [CalendarEvent],
    calendar_status: &'a str,
    calendar_cmd_tx: Option<&'a mpsc::Sender<CalendarCommand>>,
    config_dirty: &'a mut bool,
    todos_dirty: &'a mut bool,
    test_notification_requested: &'a mut bool,
    calendar_set_url_requested: &'a mut Option<String>,
    calendar_refresh_requested: &'a mut bool,
    installed_fonts: &'a mut Option<Vec<InstalledFont>>,
    font_search: &'a mut String,
}

fn render_settings_tabs_body(ui: &mut egui::Ui, st: &mut SettingsBodyState<'_>) {
    let left = ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));
    let right = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
    if !ui.ctx().memory(|m| m.focused().is_some()) && (left || right) {
        let tabs = SettingsTab::all();
        let pos = tabs.iter().position(|t| *t == *st.tab).unwrap_or(0);
        let next = (pos as i32 + if left { -1 } else { 1 }).rem_euclid(tabs.len() as i32) as usize;
        *st.tab = tabs[next];
    }

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(120.0, ui.available_height().max(220.0)),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                for tab in SettingsTab::all() {
                    let selected = *st.tab == tab;
                    let [ar, ag, ab] = st.config.accent_color;
                    let fill = if selected {
                        Color32::from_rgba_unmultiplied(ar, ag, ab, 60)
                    } else {
                        Color32::from_rgb(32, 32, 38)
                    };
                    let btn = egui::Button::new(
                        RichText::new(tab.label())
                            .color(if selected { Color32::WHITE } else { Color32::GRAY }),
                    )
                    .min_size(Vec2::new(112.0, 28.0))
                    .fill(fill)
                    .corner_radius(CornerRadius::same(8));
                    if ui.add(btn).clicked() {
                        *st.tab = tab;
                    }
                }
            },
        );
        ui.add_space(10.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width().max(240.0), ui.available_height().max(220.0)),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_clip_rect(ui.max_rect());
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
                        SettingsTab::System => render_system_tab(ui, st.config, st.config_dirty),
                        SettingsTab::Todo => {
                            *st.todos_dirty |= render_todo_tab(ui, st.todos, st.new_todo_input);
                        }
                        SettingsTab::Calendar => render_calendar_tab(ui, st),
                    });
            },
        );
    });
}

pub enum SettingsIpcTarget {
    #[cfg(unix)]
    Unix(PathBuf),
    Tcp(String),
}

pub fn run_standalone_settings(ipc_target: Option<SettingsIpcTarget>) -> Result<(), String> {
    use std::io::Write;
    use std::net::TcpStream;
    use std::process::Command;
    use std::time::Duration;

    // Try the software GL path first: on hosts with a software EGL config it avoids loading
    // the multi-hundred-MB vendor GL/LLVM driver stack. If that config is unavailable eframe
    // aborts before showing a window, so retry once with hardware acceleration preferred.
    const GL_ATTEMPT_ENV: &str = "LIEBERT_SETTINGS_GL_ATTEMPT";
    if std::env::var_os(GL_ATTEMPT_ENV).is_none() {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let software_ok = Command::new(&exe)
            .args(&args)
            .env(GL_ATTEMPT_ENV, "software")
            .env("LIEBERT_SETTINGS_SOFTWARE_GL", "1")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if software_ok {
            return Ok(());
        }
        let status = Command::new(&exe)
            .args(&args)
            .env(GL_ATTEMPT_ENV, "hardware")
            .env_remove("LIEBERT_SETTINGS_SOFTWARE_GL")
            .status()
            .map_err(|e| e.to_string())?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("settings window exited with {status}"))
        };
    }

    let config = Config::load();
    let calendar_url_input = config.google_calendar_ics_url.clone();
    let applied_font_name = config.font_name.clone();
    let todos = TodoStore::load();

    struct StandaloneSettingsApp {
        tab: SettingsTab,
        config: Config,
        todos: TodoStore,
        new_todo_input: String,
        calendar_url_input: String,
        installed_fonts: Option<Vec<InstalledFont>>,
        font_search: String,
        applied_font_name: String,
        ipc_target: Option<SettingsIpcTarget>,
    }

    impl StandaloneSettingsApp {
        fn notify(&self, line: &str) {
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
    }

    impl eframe::App for StandaloneSettingsApp {
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            apply_settings_theme(ui, self.config.accent_color);
            let panel_frame = egui::Frame::new()
                .fill(Color32::from_rgb(19, 19, 23))
                .inner_margin(egui::Margin::same(14));

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

            egui::CentralPanel::default().frame(panel_frame).show(ui, |ui| {
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
                    crate::ui::apply_configured_fonts(ui.ctx(), &self.applied_font_name);
                }
                if let Err(err) = self.config.save() {
                    eprintln!("Failed to save config: {err}");
                } else {
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

    let software_gl = std::env::var_os("LIEBERT_SETTINGS_SOFTWARE_GL").is_some();
    let glow_options = egui_glow::GlowConfiguration {
        hardware_acceleration: if software_gl {
            egui_glow::HardwareAcceleration::Off
        } else {
            egui_glow::HardwareAcceleration::Preferred
        },
        ..Default::default()
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        glow_options,
        viewport: ViewportBuilder::default()
            .with_title(SETTINGS_WINDOW_TITLE)
            .with_app_id("liebert-dynamic-island-settings")
            .with_inner_size([620.0, 560.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        SETTINGS_WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            crate::ui::apply_configured_fonts(&cc.egui_ctx, &config.font_name);
            Ok(Box::new(StandaloneSettingsApp {
                tab: SettingsTab::Appearance,
                config,
                todos,
                new_todo_input: String::new(),
                calendar_url_input,
                installed_fonts: None,
                font_search: String::new(),
                applied_font_name,
                ipc_target,
            }))
        }),
    )
    .map_err(|e| e.to_string())
}

fn render_display_tab(
    ui: &mut egui::Ui,
    config: &mut Config,
    config_dirty: &mut bool,
    installed_fonts: &mut Option<Vec<InstalledFont>>,
    font_search: &mut String,
) {
    // `None` means "not scanned yet"; an empty scanned list must not be rescanned every frame.
    let fonts = installed_fonts.get_or_insert_with(crate::platform::list_installed_fonts);
    let mut changed = false;

    ui.label(RichText::new("Display Mode (Notch / Dynamic Island)").strong());
    ui.horizontal(|ui| {
        let btn_w = ((ui.available_width() - ui.spacing().item_spacing.x) * 0.5).max(140.0);
        for (is_notch, label) in [
            (true, "Top Notch (Seamless Top)"),
            (false, "Floating Dynamic Island"),
        ] {
            let selected = config.notch_dock_mode == is_notch;
            let [ar, ag, ab] = config.accent_color;
            let fill = if selected {
                Color32::from_rgba_unmultiplied(ar, ag, ab, 65)
            } else {
                Color32::from_rgb(32, 32, 38)
            };
            let stroke = if selected {
                Stroke::new(1.5, Color32::from_rgb(ar, ag, ab))
            } else {
                Stroke::new(1.0, Color32::from_rgb(65, 65, 76))
            };
            let text_col = if selected {
                Color32::WHITE
            } else {
                Color32::from_rgb(185, 185, 195)
            };
            let btn = egui::Button::new(RichText::new(label).color(text_col).strong())
                .min_size(egui::vec2(btn_w, 30.0))
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(8));
            if ui.add(btn).clicked() && !selected {
                config.notch_dock_mode = is_notch;
                changed = true;
            }
        }
    });
    ui.label(
        RichText::new(if config.notch_dock_mode {
            "Notch menempel di tepi atas monitor"
        } else {
            "Floating pill dengan jarak Top Offset"
        })
        .small()
        .color(Color32::GRAY),
    );
    changed |= ui
        .add_enabled(
            !config.notch_dock_mode,
            egui::Slider::new(&mut config.top_offset_px, 0.0..=60.0).text("Top Offset (px)"),
        )
        .changed();

    ui.add_space(8.0);
    ui.label(RichText::new("Text Alignment").strong());
    ui.horizontal(|ui| {
        let btn_w = ((ui.available_width() - ui.spacing().item_spacing.x * 2.0) / 3.0).max(96.0);
        for mode in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
            let selected = config.text_align == mode;
            let [ar, ag, ab] = config.accent_color;
            let fill = if selected {
                Color32::from_rgba_unmultiplied(ar, ag, ab, 65)
            } else {
                Color32::from_rgb(32, 32, 38)
            };
            let stroke = if selected {
                Stroke::new(1.5, Color32::from_rgb(ar, ag, ab))
            } else {
                Stroke::new(1.0, Color32::from_rgb(65, 65, 76))
            };
            let text_col = if selected {
                Color32::WHITE
            } else {
                Color32::from_rgb(185, 185, 195)
            };
            let btn = egui::Button::new(RichText::new(mode.label()).color(text_col).strong())
                .min_size(egui::vec2(btn_w, 28.0))
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(8));
            if ui.add(btn).clicked() && !selected {
                config.text_align = mode;
                changed = true;
            }
        }
    });

    ui.add_space(8.0);
    ui.label(RichText::new("Theme Presets").strong());
    ui.horizontal(|ui| {
        for (id, label) in [
            ("crimson", "Crimson"),
            ("obsidian", "Obsidian"),
            ("midnight", "Midnight"),
            ("amoled", "AMOLED"),
            ("slate", "Slate"),
        ] {
            if ui.button(label).clicked() {
                config.apply_preset(id);
                changed = true;
            }
        }
    });

    ui.add_space(8.0);
    ui.label(RichText::new("Size & Position").strong());
    changed |= ui
        .add(egui::Slider::new(&mut config.scale, 0.75..=1.5).text("UI Scale"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.idle_width, 110.0..=240.0).text("Idle Width (px)"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.expanded_width, 320.0..=520.0).text("Expanded Width (px)"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.corner_radius, 8..=28).text("Corner Radius"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.collapse_timeout_secs, 2..=15).text("Auto-collapse (s)"))
        .changed();

    ui.add_space(8.0);
    ui.label(RichText::new("Colors").strong());
    ui.horizontal(|ui| {
        ui.label("Background:");
        let mut bg_rgb = [config.bg_color[0], config.bg_color[1], config.bg_color[2]];
        if ui.color_edit_button_srgb(&mut bg_rgb).changed() {
            config.bg_color[0] = bg_rgb[0];
            config.bg_color[1] = bg_rgb[1];
            config.bg_color[2] = bg_rgb[2];
            changed = true;
        }
        ui.label("Accent:");
        if ui.color_edit_button_srgb(&mut config.accent_color).changed() {
            changed = true;
        }
        ui.label("Text:");
        if ui.color_edit_button_srgb(&mut config.text_color).changed() {
            changed = true;
        }
    });
    changed |= ui
        .add(egui::Slider::new(&mut config.bg_color[3], 60..=255).text("Background Opacity"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.border_alpha, 0..=160).text("Border Opacity"))
        .changed();
    if config.show_lyrics && config.dynamic_album_color {
        ui.label(
            RichText::new("Note: Album art dynamic accent is active while music plays.")
                .small()
                .color(Color32::GRAY),
        );
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Font (Installed on Device)").strong());
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(font_search)
                .hint_text("Filter fonts...")
                .desired_width(130.0),
        );
        let selected_font = if config.font_name.is_empty() {
            "System Default"
        } else {
            config.font_name.as_str()
        };
        let query = font_search.trim().to_ascii_lowercase();
        egui::ComboBox::from_id_salt("device_font_selector")
            .selected_text(selected_font)
            .width(235.0)
            .show_ui(ui, |ui| {
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
    });

    ui.add_space(8.0);
    ui.label(RichText::new("Idle Display & Clock").strong());
    egui::ComboBox::from_id_salt("idle_display_mode")
        .selected_text(config.idle_display.label())
        .show_ui(ui, |ui| {
            for mode in [
                IdleDisplayMode::Auto,
                IdleDisplayMode::Clock,
                IdleDisplayMode::Lyrics,
                IdleDisplayMode::NextEvent,
                IdleDisplayMode::Pomodoro,
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

    let selected_tz_label = COMMON_TIMEZONES
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(config.clock_timezone.trim()))
        .map(|(_, label)| *label)
        .unwrap_or(config.clock_timezone.as_str());

    egui::ComboBox::from_id_salt("clock_tz_preset")
        .selected_text(selected_tz_label)
        .show_ui(ui, |ui| {
            for (id, label) in COMMON_TIMEZONES {
                if ui
                    .selectable_label(config.clock_timezone.eq_ignore_ascii_case(id), *label)
                    .clicked()
                {
                    config.clock_timezone = (*id).to_owned();
                    changed = true;
                }
            }
        });

    ui.horizontal(|ui| {
        ui.label("Custom IANA TZ:");
        if ui
            .add(
                egui::TextEdit::singleline(&mut config.clock_timezone)
                    .hint_text("Local or Asia/Jakarta")
                    .desired_width(170.0),
            )
            .changed()
        {
            changed = true;
        }
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
    });
    ui.horizontal(|ui| {
        changed |= ui.checkbox(&mut config.clock_24h, "24-hour format").changed();
        changed |= ui
            .checkbox(&mut config.clock_show_seconds, "Show seconds")
            .changed();
    });

    if changed {
        *config_dirty = true;
    }
}
fn render_lyrics_tab(ui: &mut egui::Ui, config: &mut Config, config_dirty: &mut bool) {
    let mut changed = false;
    changed |= ui
        .checkbox(&mut config.show_lyrics, "Show Music & Synced Lyrics")
        .changed();
    if config.show_lyrics {
        ui.indent("lyrics_sub_opts", |ui| {
            changed |= ui
                .checkbox(
                    &mut config.dynamic_album_color,
                    "Dynamic accent & glow from album art",
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut config.compact_album_gradient,
                    "Album color gradient in Compact/Idle pill (#000 -> album)",
                )
                .changed();
            changed |= ui
                .checkbox(&mut config.karaoke_lyrics, "Karaoke highlight")
                .changed();
            ui.horizontal(|ui| {
                ui.label("Karaoke lyric:");
                ui.add_enabled_ui(config.karaoke_lyrics, |ui| {
                    egui::ComboBox::from_id_salt("karaoke_lyric_mode")
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
            changed |= ui
                .checkbox(&mut config.romaji_lyrics, "Romaji for non-Latin lyrics")
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut config.lyric_offset_ms, -5000..=5000)
                        .text("Lyric sync offset (ms)"),
                )
                .changed();
            ui.horizontal(|ui| {
                changed |= ui
                    .checkbox(&mut config.translate_lyrics, "Translate lyrics to:")
                    .changed();
                ui.add_enabled_ui(config.translate_lyrics, |ui| {
                    let selected_lang = TRANSLATE_LANGUAGES
                        .iter()
                        .find(|(code, _)| *code == config.lyric_translate_lang)
                        .map(|(_, name)| *name)
                        .unwrap_or(config.lyric_translate_lang.as_str());
                    egui::ComboBox::from_id_salt("lyric_translate_lang")
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
            });
            changed |= ui
                .add_enabled(
                    config.translate_lyrics,
                    egui::Checkbox::new(
                        &mut config.show_pill_translation,
                        "Show translation below lyrics in Idle/Compact pill",
                    ),
                )
                .changed();
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
    ui.add_space(8.0);
    ui.label(RichText::new("Modules & Auto-Hide").strong());
    changed |= ui
        .checkbox(&mut config.auto_hide_when_idle, "Auto-hide island when idle")
        .changed();
    changed |= ui
        .add_enabled(
            config.auto_hide_when_idle,
            egui::Slider::new(&mut config.auto_hide_delay_secs, 0..=60)
                .text("Auto-hide delay (s)"),
        )
        .changed();
    if config.auto_hide_when_idle {
        ui.label(
            RichText::new("Tip: arahkan kursor ke tepi atas tengah layar untuk memunculkan notch kembali.")
                .small()
                .color(Color32::GRAY),
        );
    }
    changed |= ui
        .checkbox(
            &mut config.hide_in_fullscreen,
            "Hide when a fullscreen app/game is active",
        )
        .changed();
    if config.hide_in_fullscreen {
        ui.indent("fullscreen_sub_opts", |ui| {
            changed |= ui
                .checkbox(
                    &mut config.hide_in_windowed_fullscreen,
                    "Also hide on windowed fullscreen / maximized apps",
                )
                .changed();
        });
    }
    ui.horizontal(|ui| {
        changed |= ui
            .checkbox(&mut config.show_notifications, "Show notification popups")
            .changed();
        if ui.button("Send Test Notification").clicked() {
            *test_notification_requested = true;
        }
    });
    ui.label(
        RichText::new("Supports WhatsApp, Discord, Browser, Gmail, and Windows toasts")
            .small()
            .color(Color32::GRAY),
    );
    changed |= ui
        .checkbox(&mut config.show_todo, "Show Todo List tab")
        .changed();
    changed |= ui
        .checkbox(&mut config.show_calendar, "Show Google Calendar tab")
        .changed();
    changed |= ui
        .checkbox(&mut config.show_pomodoro, "Show Pomodoro Timer tab")
        .changed();
    if config.show_pomodoro {
        ui.indent("pomodoro_sub_opts", |ui| {
            changed |= ui
                .add(egui::Slider::new(&mut config.pomodoro_focus_mins, 1..=120).text("Focus (min)"))
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut config.pomodoro_short_break_mins, 1..=60)
                        .text("Short Break (min)"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut config.pomodoro_long_break_mins, 1..=90)
                        .text("Long Break (min)"),
                )
                .changed();
        });
    }

    if ui.button("Reset to Defaults").clicked() {
        let keep_url = config.google_calendar_ics_url.clone();
        *config = Config {
            google_calendar_ics_url: keep_url,
            ..Config::default()
        };
        changed = true;
    }

    if changed {
        *config_dirty = true;
    }
}

fn render_todo_tab(ui: &mut egui::Ui, todos: &mut TodoStore, new_todo_input: &mut String) -> bool {
    let mut dirty = false;
    ui.label(RichText::new("Add Task").strong());
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(new_todo_input)
                .hint_text("Type a task and press Enter...")
                .desired_width(310.0),
        );
        let submit = (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            || ui.button("Add").clicked();
        if submit && todos.add(new_todo_input).is_some() {
            new_todo_input.clear();
            dirty = true;
            resp.request_focus();
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Tasks ({} pending)", todos.pending_count())).strong());
        if ui.button("Clear Completed").clicked() {
            todos.clear_done();
            dirty = true;
        }
    });
    ui.separator();

    let mut toggle_id = None;
    let mut remove_id = None;
    for item in &todos.items {
        ui.horizontal(|ui| {
            let mut done = item.done;
            if ui.checkbox(&mut done, &item.text).changed() {
                toggle_id = Some(item.id);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
            });
        });
    }
    if let Some(id) = toggle_id {
        todos.toggle(id);
        dirty = true;
    }
    if let Some(id) = remove_id {
        todos.remove(id);
        dirty = true;
    }

    if dirty && let Err(err) = todos.save() {
        eprintln!("Failed to save todos: {err}");
    }
    dirty
}

fn apply_settings_theme(ui: &mut egui::Ui, accent: [u8; 3]) {
    let style = ui.style_mut();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(19, 19, 23);
    style.visuals.window_fill = Color32::from_rgb(19, 19, 23);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(32, 32, 38);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(32, 32, 38);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(7);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(46, 46, 56);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(46, 46, 56);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(7);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(7);
    style.visuals.selection.bg_fill =
        Color32::from_rgba_unmultiplied(accent[0], accent[1], accent[2], 175);
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(accent[0], accent[1], accent[2]));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
}

fn render_calendar_tab(ui: &mut egui::Ui, st: &mut SettingsBodyState<'_>) {
    ui.label(RichText::new("Google Calendar Secret iCal URL (.ics)").strong());
    ui.label(
        RichText::new("Google Calendar > Settings > [Your Calendar] > Secret address in iCal format")
            .small()
            .color(Color32::GRAY),
    );
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(st.calendar_url_input)
            .hint_text("https://calendar.google.com/calendar/ical/.../basic.ics")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Save & Sync").clicked() {
            st.config.google_calendar_ics_url = st.calendar_url_input.trim().to_owned();
            *st.config_dirty = true;
            if let Some(tx) = st.calendar_cmd_tx {
                let _ = tx.send(CalendarCommand::SetUrl(
                    st.config.google_calendar_ics_url.clone(),
                ));
            } else {
                *st.calendar_set_url_requested = Some(st.config.google_calendar_ics_url.clone());
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
            .add_enabled(!st.calendar_url_input.trim().is_empty(), egui::Button::new("Copy"))
            .clicked()
        {
            ui.ctx().copy_text(st.calendar_url_input.trim().to_owned());
        }
        ui.label(RichText::new(st.calendar_status).small());
    });

    ui.add_space(10.0);
    ui.label(RichText::new("Upcoming Events").strong());
    ui.separator();

    let now_utc = chrono::Utc::now().timestamp();
    let upcoming = upcoming_events(st.calendar_events, now_utc, 8);
    if upcoming.is_empty() {
        ui.label(RichText::new("No upcoming events loaded.").color(Color32::GRAY));
    } else {
        for ev in upcoming {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format_local_time(ev.start_utc))
                        .monospace()
                        .strong(),
                );
                ui.label(&ev.summary);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format_countdown(ev.start_utc, ev.end_utc, now_utc))
                            .small()
                            .color(Color32::GRAY),
                    );
                });
            });
        }
    }
}

fn render_system_tab(ui: &mut egui::Ui, config: &mut Config, config_dirty: &mut bool) {
    let battery = read_battery();

    ui.heading("Taskbar System Monitor");
    ui.add_space(4.0);
    if ui
        .checkbox(&mut config.taskbar_monitor_enabled, "Enable taskbar system monitor")
        .changed()
    {
        *config_dirty = true;
    }

    ui.add_enabled_ui(config.taskbar_monitor_enabled, |ui| {
        ui.add_space(6.0);
        ui.label(RichText::new("Metrics").strong());
        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut config.taskbar_show_cpu, "CPU (%)").changed() {
                *config_dirty = true;
            }
            if ui.checkbox(&mut config.taskbar_show_ram, "RAM").changed() {
                *config_dirty = true;
            }
            if ui.checkbox(&mut config.taskbar_show_disk, "Disk (C:)").changed() {
                *config_dirty = true;
            }
            if ui.checkbox(&mut config.taskbar_show_network, "Network (Up/Down)").changed() {
                *config_dirty = true;
            }
            if battery.is_some()
                && ui.checkbox(&mut config.taskbar_show_battery, "Battery").changed()
            {
                *config_dirty = true;
            }
        });

        ui.add_space(8.0);
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

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Update Interval:");
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

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(6.0);
    ui.heading("Laptop Battery");
    if let Some(bat) = battery {
        let rem_str = bat
            .remaining_secs
            .map(|s| format!(" — {} remaining", format_remaining(s)))
            .unwrap_or_default();
        ui.label(format!(
            "Status: {} {}% ({}){}",
            bat.status.short_symbol(),
            bat.percent,
            bat.status.label(),
            rem_str
        ));
        ui.add_space(4.0);
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
    } else {
        ui.label("No battery detected (desktop or unsupported device)");
    }
}
