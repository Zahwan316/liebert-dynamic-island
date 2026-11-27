mod icons;
mod settings;
pub use settings::StandaloneSettingsApp;
pub mod soft_render;
pub mod taskbar;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::f32::consts::{FRAC_PI_2, TAU};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use chrono::Datelike;
use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, CursorIcon, FontId, Mesh, Pos2, Rect,
    RichText, Sense, Shape, Stroke, StrokeKind, TextureHandle, TextureId, TextureOptions, Vec2,
    ViewportCommand,
    text::{LayoutJob, TextFormat},
};
use eframe::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use eframe::epaint::{RectShape, Vertex, WHITE_UV};

use crate::battery::BatteryAlertTracker;
use crate::calendar::{
    CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events,
};
use crate::clock::format_clock;
use crate::config::{BackgroundMaterial, Config, IdleDisplayMode, KaraokeLyricMode, TextAlign};
use crate::ipc::{DaemonSnapshot, UiToDaemon};
use crate::lyrics::{LyricsData, LyricsRequest, active_karaoke};
use crate::platform::{
    self, CjkScript, FontFamilyTarget, FontFileSpec, HoverPollInput, InstalledFont, MediaCommand,
    MediaTrack, PlatformEvent, RepeatMode,
};
use crate::pomodoro::{PomodoroPhase, PomodoroTimer};
use crate::state::{IslandState, IslandVisualState, ListenerStatus, NotificationItem};
use crate::sysmon::{SysmonCommand, SystemMetrics};
use crate::todo::TodoStore;
use crate::tray::{TrayAction, TrayController};
use crate::ui::icons::{IconKind, icon_button, paint_icon, paint_mascot_fx};
pub use crate::ui::settings::{SETTINGS_WINDOW_TITLE, SettingsIpcTarget, run_standalone_settings};
use crate::ui::settings::{SettingsTab, SettingsWindowState, render_settings_viewport};

pub const WINDOW_TITLE: &str = "Liebert Dynamic Island";
/// Keep fired calendar alert keys for one day; recurring events must not grow the set forever.
const ALERT_RETENTION_SECS: i64 = 86_400;

struct MmapFont(memmap2::Mmap);

impl AsRef<[u8]> for MmapFont {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

fn load_font_blob(path: &str) -> Option<Arc<dyn AsRef<[u8]> + Send + Sync>> {
    if let Ok(file) = std::fs::File::open(path) {
        // SAFETY: system font files are immutable during runtime; fallback to fs::read if mmap fails.
        if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
            return Some(Arc::new(MmapFont(mmap)));
        }
    }
    std::fs::read(path)
        .ok()
        .map(|bytes| Arc::new(bytes) as Arc<dyn AsRef<[u8]> + Send + Sync>)
}

fn lyrics_bold_family() -> egui::FontFamily {
    egui::FontFamily::Name(Arc::from("LyricsBold"))
}

fn lyrics_bold_font(size: f32) -> FontId {
    FontId::new(size, lyrics_bold_family())
}

fn ui_bold_font(size: f32) -> FontId {
    lyrics_bold_font(size)
}

fn insert_font_file_into_family(
    defs: &mut egui::FontDefinitions,
    family: &egui::FontFamily,
    insert_at_front: bool,
    key: String,
    blob: Arc<dyn AsRef<[u8]> + Send + Sync>,
    face_index: u32,
) {
    defs.font_data.insert(
        key.clone(),
        Arc::new(egui::FontData::from_blob(blob, face_index)),
    );
    if let Some(fam) = defs.families.get_mut(family) {
        if insert_at_front {
            fam.insert(0, key);
        } else {
            fam.push(key);
        }
    }
}

fn find_bold_sibling_font<'a>(
    installed: &'a [InstalledFont],
    selected_name: &str,
) -> Option<&'a InstalledFont> {
    let lower = selected_name.to_ascii_lowercase();
    let space_bold = format!("{lower} bold");
    let dash_bold = format!("{lower}-bold");
    installed
        .iter()
        .find(|f| {
            let n = f.name.to_ascii_lowercase();
            n == space_bold || n == dash_bold
        })
        .or_else(|| {
            installed
                .iter()
                .filter(|f| {
                    let n = f.name.to_ascii_lowercase();
                    n.starts_with(&lower) && n.contains("bold")
                })
                .min_by_key(|f| f.name.len())
        })
}

#[cfg(test)]
pub fn apply_configured_fonts(ctx: &egui::Context, font_name: &str) {
    apply_configured_fonts_with_options(ctx, font_name, true);
}

pub fn apply_configured_fonts_light(ctx: &egui::Context, font_name: &str) {
    apply_configured_fonts_with_options(ctx, font_name, false);
}

fn apply_configured_fonts_with_options(
    ctx: &egui::Context,
    font_name: &str,
    include_bold_lyrics: bool,
) {
    let mut defs = egui::FontDefinitions::empty();
    defs.families
        .insert(egui::FontFamily::Proportional, Vec::new());
    defs.families
        .insert(egui::FontFamily::Monospace, Vec::new());
    defs.families.insert(lyrics_bold_family(), Vec::new());

    let trimmed = font_name.trim();
    let mut user_selected_regular_fallback: Option<String> = None;

    if !trimmed.is_empty() {
        let installed = platform::list_installed_fonts();
        if let Some(found) = installed
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(trimmed))
            && let Some(blob) = load_font_blob(&found.path)
        {
            let selected_is_bold = trimmed.to_ascii_lowercase().contains("bold");
            insert_font_file_into_family(
                &mut defs,
                &egui::FontFamily::Proportional,
                true,
                "user_selected_font".to_owned(),
                blob.clone(),
                0u32,
            );
            if selected_is_bold {
                if let Some(fam) = defs.families.get_mut(&lyrics_bold_family()) {
                    fam.push("user_selected_font".to_owned());
                }
            } else if let Some(bold_sib) = find_bold_sibling_font(&installed, trimmed)
                && let Some(bold_blob) = load_font_blob(&bold_sib.path)
            {
                insert_font_file_into_family(
                    &mut defs,
                    &lyrics_bold_family(),
                    false,
                    "user_selected_bold_font".to_owned(),
                    bold_blob,
                    0u32,
                );
            } else {
                user_selected_regular_fallback = Some("user_selected_font".to_owned());
            }
        }
    }

    // Always register the lightweight primary system Bold face for expanded headers, bold clock,
    // and notification titles even before heavy CJK bold lyric fonts are loaded lazily.
    let bold_specs = platform::bold_fallback_fonts();
    let bold_count = if include_bold_lyrics {
        bold_specs.len()
    } else {
        1
    };
    for spec in bold_specs.into_iter().take(bold_count) {
        if let Some(blob) = load_font_blob(&spec.path) {
            for &face_index in spec.face_indices {
                let name = if spec.face_indices.len() == 1 && face_index == 0 {
                    spec.name.clone()
                } else {
                    format!("{}_{}", spec.name, face_index)
                };
                let font_data = egui::FontData::from_blob(blob.clone(), face_index);
                defs.font_data.insert(name.clone(), Arc::new(font_data));
                if let Some(fam) = defs.families.get_mut(&lyrics_bold_family()) {
                    fam.push(name);
                }
            }
        }
    }
    if let Some(key) = user_selected_regular_fallback
        && let Some(fam) = defs.families.get_mut(&lyrics_bold_family())
    {
        fam.push(key);
    }

    ctx.set_fonts(defs);
    install_base_fallback_fonts(ctx);
}

#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayBackendMode {
    Eframe,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    WaylandLayerShell,
    #[cfg_attr(not(windows), allow(dead_code))]
    WindowsLayered,
}

#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub struct OverlayStep {
    pub visible: bool,
    pub size: Vec2,
    pub top_y: f32,
    pub repaint_after: Duration,
    pub want_exit: bool,
    pub open_settings_requested: bool,
}

pub fn install_base_fallback_fonts(ctx: &egui::Context) {
    for mut spec in platform::base_fallback_fonts() {
        if spec.family == FontFamilyTarget::Proportional {
            // Also register system sans as fallback for LyricsBold so LyricsBold works even before
            // bold fonts are lazily loaded for music playback.
            spec.family = FontFamilyTarget::All;
        }
        load_font_spec(ctx, &spec, FontPriority::Lowest);
    }
}

fn load_font_spec(ctx: &egui::Context, spec: &FontFileSpec, priority: FontPriority) -> bool {
    let Some(blob) = load_font_blob(&spec.path) else {
        return false;
    };
    let families_base = match spec.family {
        FontFamilyTarget::Proportional => vec![InsertFontFamily {
            family: egui::FontFamily::Proportional,
            priority,
        }],
        FontFamilyTarget::Monospace => vec![InsertFontFamily {
            family: egui::FontFamily::Monospace,
            priority,
        }],
        FontFamilyTarget::Both => vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority,
            },
        ],
        FontFamilyTarget::Bold => vec![InsertFontFamily {
            family: lyrics_bold_family(),
            priority,
        }],
        FontFamilyTarget::All => vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority,
            },
            InsertFontFamily {
                family: lyrics_bold_family(),
                priority,
            },
        ],
    };

    for &face_index in spec.face_indices {
        let name = if spec.face_indices.len() == 1 && face_index == 0 {
            spec.name.clone()
        } else {
            format!("{}_{}", spec.name, face_index)
        };
        let font_data = egui::FontData::from_blob(blob.clone(), face_index);
        ctx.add_font(FontInsert::new(&name, font_data, families_base.clone()));
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IslandTab {
    Overview,
    Notifications,
    Music,
    Todo,
    Calendar,
    Pomodoro,
    System,
    Battery,
}

impl IslandTab {
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "HOME",
            Self::Notifications => "NOTIFICATIONS",
            Self::Music => "MUSIC",
            Self::Todo => "TASKS",
            Self::Calendar => "CALENDAR",
            Self::Pomodoro => "TIMER",
            Self::System => "SYSTEM",
            Self::Battery => "BATTERY",
        }
    }
}

/// Tabs available in the current configuration, in Left/Right arrow order.
fn visible_tabs(config: &Config) -> Vec<IslandTab> {
    let mut tabs = vec![IslandTab::Overview, IslandTab::Notifications];
    if config.show_lyrics {
        tabs.push(IslandTab::Music);
    }
    if config.show_todo {
        tabs.push(IslandTab::Todo);
    }
    if config.show_calendar {
        tabs.push(IslandTab::Calendar);
    }
    if config.show_pomodoro {
        tabs.push(IslandTab::Pomodoro);
    }
    if config.show_battery {
        tabs.push(IslandTab::Battery);
    }
    tabs.push(IslandTab::System);
    tabs
}

fn navigate_tab(current: IslandTab, direction: i32, config: &Config) -> IslandTab {
    let tabs = visible_tabs(config);
    let Some(pos) = tabs.iter().position(|t| *t == current) else {
        return tabs[0];
    };
    let len = tabs.len() as i32;
    let next = (pos as i32 + direction).rem_euclid(len) as usize;
    tabs[next]
}

/// Fixed transparent window size big enough for the largest card, so hover/notch animations
/// never resize the OS window (the cause of open/close flicker).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn permanent_canvas_size(config: &Config) -> Vec2 {
    let s = config.scale;
    let ear_pad = (12.0 * s).ceil() * 2.0;
    Vec2::new(
        (config.expanded_width.max(468.0) * s + ear_pad).ceil(),
        (292.0 * s + config.top_offset_px.max(12.0)).ceil(),
    )
}

pub enum UiControlEvent {
    ConfigUpdated(Box<Config>),
    SetPaused(bool),
    OpenSettings,
    TestNotification,
    Shutdown,
}

pub struct AppChannels {
    pub event_rx: mpsc::Receiver<PlatformEvent>,
    pub lyrics_req_tx: mpsc::Sender<LyricsRequest>,
    pub media_cmd_tx: mpsc::Sender<MediaCommand>,
    pub calendar_cmd_tx: mpsc::Sender<CalendarCommand>,
    pub ui_ctrl_rx: Option<mpsc::Receiver<UiControlEvent>>,
    pub daemon_tx: Option<mpsc::Sender<UiToDaemon>>,
    pub sysmon_cmd_tx: Option<mpsc::Sender<SysmonCommand>>,
}

#[derive(Debug, Clone, Default)]
struct PillLyricTransition {
    active_key: Option<(String, String, usize, String)>,
    prev_text: Option<String>,
    prev_progress: f32,
    prev_translation: Option<String>,
    curr_progress: f32,
    curr_translation: Option<String>,
    started_at: Option<Instant>,
    direction: f32,
}

const PILL_LYRIC_TRANSITION_MS: u64 = 240;

/// Short fade/slide applied to incoming tab content after a next/prev switch.
#[derive(Debug, Clone, Copy)]
struct TabSwitch {
    direction: f32,
    started_at: Instant,
}

const TAB_TRANSITION_MS: u64 = 180;

pub struct IslandApp {
    config: Config,
    state: IslandState,
    channels: AppChannels,
    tray: Option<TrayController>,
    media: Option<MediaTrack>,
    lyrics: LyricsData,
    last_lyrics_key: Option<(String, String)>,
    last_lyrics_had_duration: bool,
    album_art_tex: Option<TextureHandle>,
    album_art_key: Option<(String, String, Option<u64>)>,
    album_art_fingerprint: Option<(usize, usize, u64, u64)>,
    album_art_hash: Option<u64>,
    gradient_tex: Option<TextureHandle>,
    gradient_key: Option<([u8; 4], [u8; 3], bool)>,
    loaded_cjk_scripts: u8,
    todos: TodoStore,
    calendar_events: Vec<CalendarEvent>,
    calendar_status: String,
    alerted_events: HashSet<(String, i64)>,
    active_tab: IslandTab,
    tab_switch: Option<TabSwitch>,
    show_settings: bool,
    settings_focus_requested: bool,
    settings_os_focus_pending: bool,
    settings_tab: SettingsTab,
    new_todo_input: String,
    calendar_url_input: String,
    current_size: Vec2,
    size_velocity: Vec2,
    #[cfg_attr(not(windows), allow(dead_code))]
    applied_window_size: Vec2,
    #[cfg_attr(not(windows), allow(dead_code))]
    last_passthrough: Option<bool>,
    applied_size: Vec2,
    notch_morph: f32,
    visibility_anim: f32,
    notch_morph_vel: f32,
    applied_top_y: f32,
    test_notif_seq: u32,
    last_frame: Instant,
    last_fullscreen_exclusive: bool,
    last_fullscreen_windowed: bool,
    no_activate_applied: bool,
    last_visible: bool,
    last_egui_hovered: bool,
    demo_deadline: Option<Instant>,
    pomodoro: PomodoroTimer,
    installed_fonts: Option<Vec<InstalledFont>>,
    font_search: String,
    applied_font_name: String,
    hidden_since: Option<Instant>,
    allow_close: bool,
    pill_lyric_anim: PillLyricTransition,
    system_metrics: SystemMetrics,
    battery_alerts: BatteryAlertTracker,
    clock_auto_width: Option<f32>,
    clock_width_cache: Option<(String, bool, f32)>,
    music_scroll_focus_key: Option<(String, String, usize)>,
    bold_fonts_loaded: bool,
    mascot_eye_offset: Vec2,
    mascot_eye_target: Vec2,
    mascot_last_cursor: Option<(f32, f32)>,
    mascot_cursor_moved_at: Option<Instant>,
    mascot_blink_next_at: Instant,
    mascot_blink_started_at: Option<Instant>,
    mascot_eye_open: f32,
    notification_history: VecDeque<NotificationItem>,
    notif_icon_cache: Vec<(String, u64, TextureHandle)>,
    acrylic_grain_tex: Option<TextureHandle>,
    calendar_view_ym: (i32, u32),
    calendar_selected_day: Option<(i32, u32, u32)>,
    pending_user_seek_until: Option<Instant>,
}

impl IslandApp {
    pub fn new(
        config: Config,
        channels: AppChannels,
        tray: Option<TrayController>,
        demo_mode: bool,
    ) -> Self {
        let timeout = Duration::from_secs(config.collapse_timeout_secs.max(1));
        let mut state = IslandState::new(config.auto_hide_when_idle, timeout);
        state.set_auto_hide_delay(Duration::from_secs(config.auto_hide_delay_secs));
        state.set_hide_in_fullscreen(config.hide_in_fullscreen);
        let now = Instant::now();
        let calendar_url_input = config.google_calendar_ics_url.clone();
        let ear_pad = if config.notch_dock_mode {
            (12.0 * config.scale).round() * 2.0
        } else {
            0.0
        };
        let initial_idle_w = match config.idle_display {
            IdleDisplayMode::Mascot => config.idle_width.clamp(92.0, 128.0) * config.scale,
            IdleDisplayMode::Auto | IdleDisplayMode::Clock => {
                let (time_str, tz_opt) = clock_display_parts(&config);
                estimate_clock_pill_width(
                    &time_str,
                    tz_opt.as_deref(),
                    config.scale,
                    config.show_idle_mascot,
                )
            }
            _ => config.idle_width * config.scale,
        };
        let initial_w = initial_idle_w + ear_pad;
        let initial_h = match config.idle_display {
            IdleDisplayMode::Mascot => 38.0 * config.scale,
            IdleDisplayMode::Auto | IdleDisplayMode::Clock | IdleDisplayMode::Minimal
                if config.show_idle_mascot =>
            {
                34.0 * config.scale
            }
            _ => 30.0 * config.scale,
        };
        let initial_size = Vec2::new(initial_w, initial_h);
        let notch_morph = if config.notch_dock_mode { 1.0 } else { 0.0 };
        let pomodoro = PomodoroTimer::new(
            config.pomodoro_focus_mins,
            config.pomodoro_short_break_mins,
            config.pomodoro_long_break_mins,
        );
        let applied_font_name = config.font_name.clone();

        Self {
            config,
            state,
            channels,
            tray,
            media: None,
            lyrics: LyricsData::NotFound,
            last_lyrics_key: None,
            last_lyrics_had_duration: false,
            album_art_tex: None,
            album_art_key: None,
            album_art_fingerprint: None,
            album_art_hash: None,
            gradient_tex: None,
            gradient_key: None,
            loaded_cjk_scripts: 0,
            todos: TodoStore::load(),
            calendar_events: Vec::new(),
            calendar_status: if calendar_url_input.is_empty() {
                "Not configured".to_owned()
            } else {
                "Syncing...".to_owned()
            },
            alerted_events: HashSet::new(),
            system_metrics: SystemMetrics::default(),
            battery_alerts: BatteryAlertTracker::default(),
            active_tab: IslandTab::Overview,
            tab_switch: None,
            show_settings: false,
            settings_focus_requested: false,
            settings_os_focus_pending: false,
            settings_tab: SettingsTab::Appearance,
            new_todo_input: String::new(),
            calendar_url_input,
            current_size: initial_size,
            size_velocity: Vec2::ZERO,
            applied_window_size: Vec2::ZERO,
            last_passthrough: None,
            applied_size: Vec2::ZERO,
            notch_morph,
            visibility_anim: 1.0,
            notch_morph_vel: 0.0,
            applied_top_y: -1.0,
            test_notif_seq: 0,
            last_frame: now,
            last_fullscreen_exclusive: false,
            last_fullscreen_windowed: false,
            no_activate_applied: false,
            last_visible: true,
            last_egui_hovered: false,
            demo_deadline: demo_mode.then(|| now + Duration::from_secs(1)),
            pomodoro,
            installed_fonts: None,
            font_search: String::new(),
            applied_font_name,
            hidden_since: None,
            allow_close: false,
            pill_lyric_anim: PillLyricTransition::default(),
            clock_auto_width: None,
            clock_width_cache: None,
            music_scroll_focus_key: None,
            bold_fonts_loaded: false,
            mascot_eye_offset: Vec2::ZERO,
            mascot_eye_target: Vec2::ZERO,
            mascot_last_cursor: None,
            mascot_cursor_moved_at: None,
            mascot_blink_next_at: now + Duration::from_secs(4),
            mascot_blink_started_at: None,
            mascot_eye_open: 1.0,
            notification_history: VecDeque::new(),
            notif_icon_cache: Vec::new(),
            acrylic_grain_tex: None,
            calendar_view_ym: {
                let today = chrono::Local::now().date_naive();
                (today.year(), today.month())
            },
            calendar_selected_day: None,
            pending_user_seek_until: None,
        }
    }

    fn is_mascot_visible_in_idle(&self) -> bool {
        if self.state.visual != IslandVisualState::Idle
            || self.state.active.is_some()
            || matches!(self.state.listener_status, ListenerStatus::AccessDenied(_))
        {
            return false;
        }
        match self.effective_idle_mode() {
            IdleDisplayMode::Mascot => true,
            IdleDisplayMode::Clock | IdleDisplayMode::Minimal => {
                self.config.show_idle_mascot && !self.state.paused
            }
            _ => false,
        }
    }

    fn ensure_bold_lyrics_fonts_loaded(&mut self, ctx: &egui::Context) {
        if self.bold_fonts_loaded {
            return;
        }
        self.bold_fonts_loaded = true;
        if !self.config.font_name.trim().is_empty() {
            let installed = platform::list_installed_fonts();
            if let Some(bold_sib) = find_bold_sibling_font(&installed, self.config.font_name.trim())
                && let Some(blob) = load_font_blob(&bold_sib.path)
            {
                let font_data = egui::FontData::from_blob(blob, 0);
                ctx.add_font(FontInsert::new(
                    "user_selected_bold_font",
                    font_data,
                    vec![InsertFontFamily {
                        family: lyrics_bold_family(),
                        priority: FontPriority::Highest,
                    }],
                ));
            }
        }
        for spec in platform::bold_fallback_fonts() {
            load_font_spec(ctx, &spec, FontPriority::Highest);
        }
    }

    fn effective_idle_mode(&self) -> IdleDisplayMode {
        match self.config.idle_display {
            IdleDisplayMode::Auto => {
                if self.config.show_lyrics && self.media.is_some() {
                    IdleDisplayMode::Lyrics
                } else if self.config.show_pomodoro && self.pomodoro.running {
                    IdleDisplayMode::Pomodoro
                } else {
                    IdleDisplayMode::Clock
                }
            }
            other => other,
        }
    }

    fn notch_ear_width(&self) -> f32 {
        let ear_factor = ((self.notch_morph - 0.5) * 2.0).clamp(0.0, 1.0);
        12.0 * self.config.scale * ear_factor
    }

    fn effective_top_offset(&self) -> f32 {
        self.config.top_offset_px * (1.0 - self.notch_morph)
    }

    fn toggle_dock_mode(&mut self) {
        self.config.notch_dock_mode = !self.config.notch_dock_mode;
        if let Some(tray) = &self.tray {
            tray.update_dock_mode(self.config.notch_dock_mode);
        }
        if let Err(err) = self.config.save() {
            eprintln!("Failed to save config: {err}");
        }
    }

    fn push_test_notification(&mut self, now: Instant) {
        if self.state.paused {
            self.state.set_paused(false);
            self.sync_tray();
        }
        let samples = [
            (
                "WhatsApp",
                "Budi",
                "Meeting jam 14:00 jadi pindah ke Discord?",
            ),
            (
                "Discord",
                "#general — Alex",
                "Deploy staging barusan sukses, cek log!",
            ),
            ("Gmail", "GitHub", "[liebert-dynamic-island] PR #12 merged"),
            (
                "Google Chrome",
                "Download Complete",
                "release-v0.1.3-x86_64.zip siap dibuka",
            ),
        ];
        let idx = (self.test_notif_seq as usize) % samples.len();
        self.test_notif_seq = self.test_notif_seq.wrapping_add(1);
        let (app_name, title, body) = samples[idx];
        self.record_notification(
            NotificationItem {
                id: format!("test-{}-{}", self.test_notif_seq, app_name),
                app_name: app_name.to_owned(),
                title: title.to_owned(),
                body: body.to_owned(),
                icon_rgba: None,
                timestamp: 0,
            },
            now,
        );
    }

    fn record_notification(&mut self, mut item: NotificationItem, now: Instant) {
        if item.timestamp == 0 {
            item.timestamp = chrono::Utc::now().timestamp().max(0) as u64;
        }
        push_notification_history(&mut self.notification_history, item.clone());
        self.state.push_notification(item, now);
    }

    fn sync_notif_icon_textures(&mut self, ctx: &egui::Context) {
        let side = crate::platform::NOTIF_ICON_SIZE as usize;
        let expected_len = side * side * 4;
        let items: Vec<(String, Vec<u8>)> = self
            .state
            .active
            .iter()
            .chain(self.notification_history.iter())
            .take(12)
            .filter_map(|item| {
                let rgba = item.icon_rgba.as_ref()?;
                if rgba.len() == expected_len {
                    Some((item.app_name.clone(), rgba.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (app_name, rgba) in items {
            let hash = hash_notif_icon(&app_name, &rgba);
            if self
                .notif_icon_cache
                .iter()
                .any(|(cached_app, cached_hash, _)| *cached_hash == hash && cached_app == &app_name)
            {
                continue;
            }
            let img = ColorImage::from_rgba_unmultiplied([side, side], &rgba);
            let tex = ctx.load_texture(
                format!("notif_icon_{hash:016x}"),
                img,
                TextureOptions::LINEAR,
            );
            self.notif_icon_cache.push((app_name, hash, tex));
            if self.notif_icon_cache.len() > 24 {
                let _ = self.notif_icon_cache.remove(0);
            }
        }
    }

    fn lookup_notif_icon(&self, item: &NotificationItem) -> Option<TextureId> {
        let rgba = item.icon_rgba.as_ref()?;
        let hash = hash_notif_icon(&item.app_name, rgba);
        self.notif_icon_cache
            .iter()
            .find(|(cached_app, cached_hash, _)| {
                *cached_hash == hash && cached_app == &item.app_name
            })
            .map(|(_, _, tex)| tex.id())
    }

    fn ensure_acrylic_grain_texture(&mut self, ctx: &egui::Context) {
        if self.acrylic_grain_tex.is_some() {
            return;
        }
        const SIZE: usize = 32;
        let mut pixels = Vec::with_capacity(SIZE * SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let mut v =
                    (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
                v = (v ^ (v >> 13)).wrapping_mul(1_274_126_177);
                let byte = (v >> 24) as u8;
                let lum = if byte > 136 { 255 } else { 190 };
                let alpha = match byte % 5 {
                    0 => 18,
                    1 | 2 => 10,
                    _ => 4,
                };
                pixels.push(Color32::from_rgba_unmultiplied(lum, lum, lum, alpha));
            }
        }
        let img = ColorImage::new([SIZE, SIZE], pixels);
        self.acrylic_grain_tex = Some(ctx.load_texture(
            "acrylic_grain_tile",
            img,
            TextureOptions {
                wrap_mode: egui::TextureWrapMode::Repeat,
                ..TextureOptions::LINEAR
            },
        ));
    }

    fn target_size(&self) -> Vec2 {
        let s = self.config.scale;
        let ear_pad = self.notch_ear_width() * 2.0;
        let mut size = match self.state.visual {
            IslandVisualState::Idle => {
                if matches!(self.state.listener_status, ListenerStatus::AccessDenied(_)) {
                    Vec2::new(330.0 * s, 44.0 * s)
                } else {
                    match self.effective_idle_mode() {
                        IdleDisplayMode::Lyrics
                            if self.config.show_lyrics && self.media.is_some() =>
                        {
                            let has_tr = self.config.show_pill_translation
                                && self.config.translate_lyrics
                                && active_karaoke(
                                    &self.lyrics,
                                    self.current_lyric_ms(self.last_frame),
                                )
                                .and_then(|k| k.translation)
                                .is_some();
                            let h = if has_tr { 44.0 } else { 32.0 };
                            Vec2::new(self.config.idle_width.max(255.0) * s, h * s)
                        }
                        IdleDisplayMode::NextEvent if self.config.show_calendar => {
                            let now_utc = chrono::Utc::now().timestamp();
                            if upcoming_events(&self.calendar_events, now_utc, 1).is_empty() {
                                Vec2::new(self.config.idle_width * s, 30.0 * s)
                            } else {
                                Vec2::new(self.config.idle_width.max(220.0) * s, 30.0 * s)
                            }
                        }
                        IdleDisplayMode::Mascot => {
                            Vec2::new(self.config.idle_width.clamp(92.0, 128.0) * s, 38.0 * s)
                        }
                        IdleDisplayMode::Clock => {
                            let with_mascot = self.config.show_idle_mascot && !self.state.paused;
                            let h = if with_mascot { 34.0 } else { 30.0 };
                            let w = self.clock_auto_width.unwrap_or_else(|| {
                                let (time_str, tz_opt) = clock_display_parts(&self.config);
                                estimate_clock_pill_width(
                                    &time_str,
                                    tz_opt.as_deref(),
                                    s,
                                    with_mascot,
                                )
                            });
                            Vec2::new(w, h * s)
                        }
                        IdleDisplayMode::Minimal => {
                            let h = if self.config.show_idle_mascot && !self.state.paused {
                                34.0
                            } else {
                                30.0
                            };
                            Vec2::new(self.config.idle_width * s, h * s)
                        }
                        _ => Vec2::new(self.config.idle_width * s, 30.0 * s),
                    }
                }
            }
            IslandVisualState::Compact => Vec2::new(348.0 * s, 60.0 * s),
            IslandVisualState::Expanded => {
                // One width for every expanded tab: switching tabs must not shift the card
                // sideways. Overview uses 208pt for a balanced gallery grid; Music grows for lyrics.
                let h = if self.state.active.is_none()
                    && self.active_tab == IslandTab::Music
                    && self.media.is_some()
                {
                    286.0
                } else if self.state.active.is_none() && self.active_tab == IslandTab::Overview {
                    208.0
                } else if self.state.active.is_none() && self.active_tab == IslandTab::Notifications
                {
                    198.0
                } else {
                    184.0
                };
                Vec2::new(self.config.expanded_width.max(468.0) * s, h * s)
            }
        };
        size.x += ear_pad;
        size
    }

    fn ensure_cjk_font_loaded_for_text(&mut self, ctx: &egui::Context, text: &str) {
        if self.loaded_cjk_scripts == 0b11 {
            return;
        }
        let mut need_jp_han = false;
        let mut need_kr = false;
        for ch in text.chars() {
            match ch as u32 {
                0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => need_kr = true,
                0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF => need_jp_han = true,
                _ => {}
            }
            if need_jp_han && need_kr {
                break;
            }
        }
        if need_jp_han && (self.loaded_cjk_scripts & 0b01) == 0 {
            self.loaded_cjk_scripts |= 0b01;
            for spec in platform::cjk_fallback_fonts_for(CjkScript::JapaneseOrHan) {
                load_font_spec(ctx, &spec, FontPriority::Lowest);
            }
        }
        if need_kr && (self.loaded_cjk_scripts & 0b10) == 0 {
            self.loaded_cjk_scripts |= 0b10;
            for spec in platform::cjk_fallback_fonts_for(CjkScript::Korean) {
                load_font_spec(ctx, &spec, FontPriority::Lowest);
            }
        }
    }

    fn ensure_cjk_fonts_for_lyrics(&mut self, ctx: &egui::Context) {
        let lines_iter: Vec<String> = match &self.lyrics {
            LyricsData::Synced(lines) => lines.iter().map(|l| l.text.clone()).collect(),
            LyricsData::Plain(lines) => lines.iter().map(|l| l.text.clone()).collect(),
            _ => return,
        };
        for text in lines_iter {
            self.ensure_cjk_font_loaded_for_text(ctx, &text);
            if self.loaded_cjk_scripts == 0b11 {
                break;
            }
        }
    }

    fn ensure_lyrics_requested_for_media(&mut self, ctx: &egui::Context) {
        let Some((artist, title, has_dur)) = self
            .media
            .as_ref()
            .map(|t| (t.artist.clone(), t.title.clone(), t.duration_ms > 0))
        else {
            return;
        };
        let key = (artist.clone(), title.clone());
        let key_changed = self.last_lyrics_key.as_ref() != Some(&key);
        let duration_arrived = !key_changed
            && has_dur
            && !self.last_lyrics_had_duration
            && matches!(self.lyrics, LyricsData::NotFound | LyricsData::Plain(_));
        if key_changed || duration_arrived {
            self.ensure_cjk_font_loaded_for_text(ctx, &title);
            self.ensure_cjk_font_loaded_for_text(ctx, &artist);
            self.last_lyrics_key = Some(key);
            self.last_lyrics_had_duration = has_dur;
            self.request_lyrics_for_current();
        }
    }

    fn request_lyrics_for_current(&mut self) {
        let Some((artist, title)) = self.last_lyrics_key.clone() else {
            return;
        };
        let translate_lang = (self.config.translate_lyrics
            || self.config.karaoke_lyric_mode == KaraokeLyricMode::Translate)
            .then(|| self.config.lyric_translate_lang.clone());
        let duration_secs = self
            .media
            .as_ref()
            .and_then(|m| (m.duration_ms > 0).then_some(m.duration_ms / 1000));
        self.lyrics = LyricsData::Loading;
        let _ = self.channels.lyrics_req_tx.send(LyricsRequest {
            artist,
            title,
            duration_secs,
            translate_lang,
            need_romaji: self.config.romaji_lyrics
                || self.config.karaoke_lyric_mode == KaraokeLyricMode::Romaji,
        });
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn system_metrics(&self) -> &SystemMetrics {
        &self.system_metrics
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn config(&self) -> &Config {
        &self.config
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn is_fullscreen_hidden(&self) -> bool {
        self.state.hide_in_fullscreen && self.state.fullscreen_active
    }

    fn sync_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.update_status(&self.state.listener_status, self.state.paused);
        }
    }

    fn prune_alerted_events(&mut self, now_utc: i64) {
        retain_recent_alerts(&mut self.alerted_events, now_utc);
    }

    fn current_playback_ms(&self, now: Instant) -> u64 {
        let Some(track) = &self.media else {
            return 0;
        };
        let raw = if track.is_playing {
            track.position_ms + now.saturating_duration_since(track.updated_at).as_millis() as u64
        } else {
            track.position_ms
        };
        if track.duration_ms > 0 {
            raw.min(track.duration_ms)
        } else {
            raw
        }
    }

    fn current_lyric_ms(&self, now: Instant) -> u64 {
        let base = self.current_playback_ms(now) as i64;
        (base + self.config.lyric_offset_ms).max(0) as u64
    }

    fn current_pill_lyric_snapshot(
        &self,
        now: Instant,
    ) -> Option<(String, String, usize, String, f32, Option<String>)> {
        let track = self.media.as_ref()?;
        let k = active_karaoke(&self.lyrics, self.current_lyric_ms(now))?;
        let main_text = match self.config.karaoke_lyric_mode {
            KaraokeLyricMode::Romaji => k.romaji.unwrap_or(k.text),
            KaraokeLyricMode::Translate => k.translation.unwrap_or(k.text),
            KaraokeLyricMode::Original => k.text,
        };
        let tr_text = if self.config.show_pill_translation && self.config.translate_lyrics {
            k.translation.map(str::to_owned)
        } else {
            None
        };
        let prog = if self.config.karaoke_lyrics {
            k.progress
        } else {
            1.0
        };
        Some((
            track.artist.clone(),
            track.title.clone(),
            k.line_index,
            main_text.to_owned(),
            prog,
            tr_text,
        ))
    }

    fn update_pill_lyric_transition(&mut self, now: Instant) -> bool {
        let Some((artist, title, line_index, text, progress, translation)) =
            self.current_pill_lyric_snapshot(now)
        else {
            self.pill_lyric_anim = PillLyricTransition::default();
            return false;
        };

        let key = (artist, title, line_index, text);
        if let Some(prev_key) = &self.pill_lyric_anim.active_key {
            if prev_key != &key {
                let same_song = prev_key.0 == key.0 && prev_key.1 == key.1;
                if same_song {
                    self.pill_lyric_anim.prev_text = Some(prev_key.3.clone());
                    self.pill_lyric_anim.prev_progress = self.pill_lyric_anim.curr_progress;
                    self.pill_lyric_anim.prev_translation =
                        self.pill_lyric_anim.curr_translation.take();
                    self.pill_lyric_anim.started_at = Some(now);
                    self.pill_lyric_anim.direction =
                        if line_index < prev_key.2 { -1.0 } else { 1.0 };
                } else {
                    self.pill_lyric_anim.prev_text = None;
                    self.pill_lyric_anim.prev_progress = 1.0;
                    self.pill_lyric_anim.prev_translation = None;
                    self.pill_lyric_anim.started_at = None;
                }
                self.pill_lyric_anim.active_key = Some(key);
            }
        } else {
            self.pill_lyric_anim.active_key = Some(key);
        }
        self.pill_lyric_anim.curr_progress = progress;
        self.pill_lyric_anim.curr_translation = translation;

        let active_anim = lyric_transition_state(
            self.pill_lyric_anim.started_at,
            self.pill_lyric_anim.direction,
            now,
        )
        .is_some();
        if !active_anim {
            self.pill_lyric_anim.prev_text = None;
            self.pill_lyric_anim.prev_translation = None;
            self.pill_lyric_anim.started_at = None;
        }
        active_anim
    }

    fn handle_events(&mut self, ctx: &egui::Context, now: Instant) -> (bool, bool) {
        let mut want_exit = false;
        let mut open_settings_req = false;
        if let Some(demo_at) = self.demo_deadline
            && now >= demo_at
        {
            self.demo_deadline = None;
            self.record_notification(
                NotificationItem {
                    id: "demo-1".to_owned(),
                    app_name: "Liebert Demo".to_owned(),
                    title: "Dynamic Island Ready".to_owned(),
                    body: "Hover untuk expand, lihat Lyrics/Todo/Calendar, atau klik Dismiss."
                        .to_owned(),
                    icon_rgba: None,
                    timestamp: 0,
                },
                now,
            );
        }

        let mut status_changed = false;
        while let Ok(ev) = self.channels.event_rx.try_recv() {
            match ev {
                PlatformEvent::Notification(item) => {
                    if self.config.show_notifications {
                        self.record_notification(item, now);
                    }
                }
                PlatformEvent::Removed(id) => self.state.remove_by_id(&id, now),
                PlatformEvent::Status(status) => {
                    self.state.listener_status = status;
                    status_changed = true;
                }
                PlatformEvent::Fullscreen(exclusive, windowed) => {
                    self.last_fullscreen_exclusive = exclusive;
                    self.last_fullscreen_windowed = windowed;
                }
                PlatformEvent::Media(track) => {
                    let next_track = track.map(|mut t| {
                        // Suppress small backward timeline jitter from WinRT SMTC / MPRIS2 polls
                        // on the same playing track so white karaoke highlight never blinks backward.
                        if t.is_playing
                            && self
                                .pending_user_seek_until
                                .is_none_or(|until| now >= until)
                            && let Some(prev) = &self.media
                            && prev.is_playing
                            && prev.artist == t.artist
                            && prev.title == t.title
                        {
                            let prev_ms = self.current_playback_ms(now);
                            if t.position_ms < prev_ms && prev_ms - t.position_ms < 750 {
                                t.position_ms = prev_ms;
                                t.updated_at = now;
                            }
                        }
                        t
                    });
                    self.media = next_track;
                    if self.media.is_some() {
                        self.ensure_bold_lyrics_fonts_loaded(ctx);
                        self.ensure_lyrics_requested_for_media(ctx);
                    } else {
                        self.last_lyrics_key = None;
                        self.last_lyrics_had_duration = false;
                        self.lyrics = LyricsData::NotFound;
                        self.music_scroll_focus_key = None;
                    }
                }
                PlatformEvent::LyricsLoaded {
                    artist,
                    title,
                    lyrics,
                } => {
                    if self.last_lyrics_key.as_ref() == Some(&(artist, title)) {
                        self.lyrics = lyrics;
                        self.ensure_cjk_fonts_for_lyrics(ctx);
                    }
                }
                PlatformEvent::CalendarUpdated(res) => match res {
                    Ok(events) => {
                        self.calendar_status = format!("Synced ({} events)", events.len());
                        self.prune_alerted_events(chrono::Utc::now().timestamp());
                        self.calendar_events = events;
                    }
                    Err(err) => {
                        self.calendar_status = err;
                    }
                },
                PlatformEvent::SystemMetrics(metrics) => {
                    if let Some((title, body)) = self.battery_alerts.check(
                        metrics.battery,
                        self.config.battery_low_alerts,
                        self.config.battery_warn_pct,
                        self.config.battery_critical_pct,
                    ) {
                        self.record_notification(
                            NotificationItem {
                                id: format!(
                                    "battery-low-{}",
                                    metrics.battery.map_or(0, |b| b.percent)
                                ),
                                app_name: "Battery".to_owned(),
                                title: title.to_owned(),
                                body,
                                icon_rgba: None,
                                timestamp: 0,
                            },
                            now,
                        );
                    }
                    self.system_metrics = metrics;
                }
            }
        }

        let tray_events = self
            .tray
            .as_ref()
            .map(|tray| (tray.poll_actions(), tray.is_auto_hide_checked()));
        if let Some((actions, auto_hide_checked)) = tray_events {
            for action in actions {
                match action {
                    TrayAction::OpenSettings => {
                        self.open_settings(SettingsTab::Appearance, ctx);
                        open_settings_req = true;
                    }
                    TrayAction::ToggleDockMode => {
                        self.toggle_dock_mode();
                        ctx.request_repaint();
                    }
                    TrayAction::TestNotification => {
                        self.push_test_notification(now);
                        ctx.request_repaint();
                    }
                    TrayAction::TogglePause => {
                        self.state.set_paused(!self.state.paused);
                        status_changed = true;
                    }
                    TrayAction::ToggleAutoHide => {
                        self.state.set_auto_hide(auto_hide_checked, now);
                        self.config.auto_hide_when_idle = auto_hide_checked;
                        if let Err(err) = self.config.save() {
                            eprintln!("Failed to save config: {err}");
                        }
                    }
                    TrayAction::Exit => {
                        want_exit = true;
                    }
                }
            }
        }

        if self.config.show_calendar && !self.calendar_events.is_empty() {
            let now_utc = chrono::Utc::now().timestamp();
            for ev in upcoming_events(&self.calendar_events, now_utc, 3) {
                let secs_until = ev.start_utc - now_utc;
                if (0..=300).contains(&secs_until) {
                    let key = (ev.summary.clone(), ev.start_utc);
                    if self.alerted_events.insert(key) {
                        let item = NotificationItem {
                            id: format!("cal-{}-{}", ev.start_utc, ev.summary),
                            app_name: "Google Calendar".to_owned(),
                            title: ev.summary.clone(),
                            body: format!(
                                "{} • {}",
                                format_local_time(ev.start_utc),
                                format_countdown(ev.start_utc, ev.end_utc, now_utc)
                            ),
                            icon_rgba: None,
                            timestamp: now_utc.max(0) as u64,
                        };
                        push_notification_history(&mut self.notification_history, item.clone());
                        self.state.push_notification(item, now);
                    }
                }
            }
        }

        if status_changed {
            self.sync_tray();
        }
        if let Some(ctrl_rx) = &self.channels.ui_ctrl_rx {
            let mut pending = Vec::new();
            while let Ok(ev) = ctrl_rx.try_recv() {
                pending.push(ev);
            }
            for ev in pending {
                match ev {
                    UiControlEvent::ConfigUpdated(cfg) => {
                        self.apply_external_config(*cfg, ctx);
                    }
                    UiControlEvent::SetPaused(paused) => {
                        self.state.set_paused(paused);
                    }
                    UiControlEvent::OpenSettings => {
                        self.open_settings(SettingsTab::Appearance, ctx);
                        open_settings_req = true;
                    }
                    UiControlEvent::TestNotification => {
                        self.push_test_notification(now);
                        ctx.request_repaint();
                    }
                    UiControlEvent::Shutdown => {
                        want_exit = true;
                        self.allow_close = true;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
            }
        }
        (want_exit, open_settings_req)
    }

    pub fn apply_snapshot(&mut self, snap: DaemonSnapshot, ctx: &egui::Context) {
        let now = Instant::now();
        self.apply_external_config(snap.config, ctx);
        self.state.set_paused(snap.paused);
        self.state.listener_status = snap.listener_status;
        if let Some(item) = snap.active_notification {
            self.record_notification(item, now);
        }
        self.media = snap.media.map(MediaTrack::from);
        if let Some((title, artist)) = self
            .media
            .as_ref()
            .map(|t| (t.title.clone(), t.artist.clone()))
        {
            self.ensure_bold_lyrics_fonts_loaded(ctx);
            self.ensure_cjk_font_loaded_for_text(ctx, &title);
            self.ensure_cjk_font_loaded_for_text(ctx, &artist);
        }
        self.last_lyrics_key = snap.lyrics_key;
        self.lyrics = LyricsData::from(snap.lyrics);
        self.ensure_cjk_fonts_for_lyrics(ctx);
        if self.media.is_some() && self.last_lyrics_key.is_none() {
            self.ensure_lyrics_requested_for_media(ctx);
        }
        self.calendar_events = snap.calendar_events;
        self.calendar_status = snap.calendar_status;
        self.last_fullscreen_exclusive = snap.fullscreen_exclusive;
        self.last_fullscreen_windowed = snap.fullscreen_windowed;
        if snap.open_settings {
            self.open_settings(SettingsTab::Appearance, ctx);
        }
        if snap.test_notification {
            self.push_test_notification(now);
        }
        ctx.request_repaint();
    }

    fn open_settings(&mut self, tab: SettingsTab, ctx: &egui::Context) {
        self.settings_tab = tab;
        self.show_settings = true;
        self.settings_focus_requested = true;
        self.settings_os_focus_pending = true;
        ctx.request_repaint();
    }

    fn bg_color32(&self) -> Color32 {
        let [r, g, b, a] = self.config.bg_color;
        Color32::from_rgba_unmultiplied(r, g, b, a)
    }

    fn effective_accent_rgb(&self) -> [u8; 3] {
        if self.config.dynamic_album_color
            && let Some(track) = &self.media
            && let Some(art) = &track.album_art
            && let Some(dom) = art.dominant_rgb
        {
            return dom;
        }
        self.config.accent_color
    }

    fn effective_accent(&self) -> Color32 {
        let [r, g, b] = self.effective_accent_rgb();
        Color32::from_rgb(r, g, b)
    }

    fn ensure_gradient_texture(
        &mut self,
        ctx: &egui::Context,
        base_bg: [u8; 4],
        accent: [u8; 3],
        pill_mode: bool,
    ) {
        let key = gradient_cache_key(base_bg, accent, pill_mode);
        if self.gradient_key == Some(key) && self.gradient_tex.is_some() {
            return;
        }
        // Tone-map loud album/user colors toward a neutral slate with a brightness cap so
        // the bottom wash stays pleasant behind bright pill text. Cache key remains on the
        // raw color; the mapping is deterministic, so animation uploads are unchanged.
        let soft = soft_accent_rgb(accent);
        let pixels = if pill_mode {
            // Dark neutral base with a gentle wash starting a bit earlier.
            build_vertical_gradient_pixels(base_bg, soft, 0.25, 0.55)
        } else {
            // Cool neutral highlight instead of a forced rose tint; keeps the user's hue.
            let glow = [
                lerp_u8(soft[0], 214, 0.22),
                lerp_u8(soft[1], 210, 0.22),
                lerp_u8(soft[2], 208, 0.22),
            ];
            build_vertical_gradient_pixels(base_bg, glow, 0.30, 0.55)
        };
        let img = ColorImage::new([1, pixels.len()], pixels);
        if let Some(tex) = &mut self.gradient_tex {
            tex.set(img, TextureOptions::LINEAR);
        } else {
            self.gradient_tex =
                Some(ctx.load_texture("music_bg_grad", img, TextureOptions::LINEAR));
        }
        self.gradient_key = Some(key);
    }

    fn sync_album_art_texture(&mut self, ctx: &egui::Context) {
        let Some(track) = &self.media else {
            self.album_art_tex = None;
            self.album_art_key = None;
            self.album_art_fingerprint = None;
            self.album_art_hash = None;
            self.gradient_tex = None;
            self.gradient_key = None;
            return;
        };
        // IPC delivers a fresh Arc for every media update, so pointer identity alone is not a
        // reliable cache key. Cache the full hash by a cheap pointer+edge-byte fingerprint to
        // avoid re-hashing the same cover on every repaint.
        let fingerprint = track.album_art.as_ref().map(|art| {
            let first = art
                .rgba
                .get(..8)
                .and_then(|b| b.try_into().ok())
                .map(u64::from_ne_bytes)
                .unwrap_or(0);
            let last = art
                .rgba
                .get(art.rgba.len().saturating_sub(8)..)
                .and_then(|b| b.try_into().ok())
                .map(u64::from_ne_bytes)
                .unwrap_or(0);
            (Arc::as_ptr(art) as usize, art.rgba.len(), first, last)
        });
        let art_key = if let Some(fp) = fingerprint {
            let hash = if self.album_art_fingerprint == Some(fp) {
                self.album_art_hash.unwrap_or(0)
            } else {
                use std::hash::{Hash, Hasher};
                let art = track
                    .album_art
                    .as_ref()
                    .expect("fingerprint implies album art");
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                art.width.hash(&mut hasher);
                art.height.hash(&mut hasher);
                hasher.write(&art.rgba);
                art.dominant_rgb.hash(&mut hasher);
                let hash = hasher.finish();
                self.album_art_fingerprint = Some(fp);
                self.album_art_hash = Some(hash);
                hash
            };
            Some(hash)
        } else {
            self.album_art_fingerprint = None;
            self.album_art_hash = None;
            None
        };
        if let Some(prev) = &self.album_art_key
            && prev.0 == track.artist
            && prev.1 == track.title
            && prev.2 == art_key
        {
            return;
        }
        self.album_art_key = Some((track.artist.clone(), track.title.clone(), art_key));
        if let Some(art) = &track.album_art {
            let img = ColorImage::from_rgba_unmultiplied([art.width, art.height], &art.rgba);
            if let Some(tex) = &mut self.album_art_tex {
                tex.set(img, TextureOptions::LINEAR);
            } else {
                self.album_art_tex =
                    Some(ctx.load_texture("album_art", img, TextureOptions::LINEAR));
            }
        } else {
            self.album_art_tex = None;
        }
    }

    fn text_color32(&self) -> Color32 {
        let [r, g, b] = self.config.text_color;
        Color32::from_rgb(r, g, b)
    }

    fn muted_color32(&self) -> Color32 {
        let [r, g, b] = self.config.text_color;
        Color32::from_rgba_unmultiplied(r, g, b, 170)
    }

    fn text_align(&self) -> egui::Align {
        egui_align(self.config.text_align)
    }

    fn sync_runtime_config(
        &mut self,
        ctx: &egui::Context,
        prev_lyric_opts: (bool, KaraokeLyricMode, bool, String),
        now: Instant,
    ) {
        if let Some(tx) = &self.channels.sysmon_cmd_tx {
            let _ = tx.send(SysmonCommand::SetConfig {
                enabled: self.config.taskbar_monitor_enabled,
                interval_ms: self.config.taskbar_interval_ms,
                collect_disk: self.config.taskbar_show_disk,
                collect_net: self.config.taskbar_show_network,
            });
        }
        let new_lyric_opts = (
            self.config.romaji_lyrics,
            self.config.karaoke_lyric_mode,
            self.config.translate_lyrics,
            self.config.lyric_translate_lang.clone(),
        );
        if new_lyric_opts != prev_lyric_opts {
            self.request_lyrics_for_current();
        }
        if self.config.font_name != self.applied_font_name {
            self.applied_font_name = self.config.font_name.clone();
            apply_configured_fonts_light(ctx, &self.applied_font_name);
            self.bold_fonts_loaded = false;
            self.loaded_cjk_scripts = 0;
            if self.media.is_some() {
                self.ensure_bold_lyrics_fonts_loaded(ctx);
            }
        }
        self.pomodoro.sync_durations(
            self.config.pomodoro_focus_mins,
            self.config.pomodoro_short_break_mins,
            self.config.pomodoro_long_break_mins,
        );
        self.state
            .set_auto_hide(self.config.auto_hide_when_idle, now);
        self.state
            .set_auto_hide_delay(Duration::from_secs(self.config.auto_hide_delay_secs));
        self.state
            .set_hide_in_fullscreen(self.config.hide_in_fullscreen);
        self.state.set_fullscreen_active(
            self.config.hide_in_fullscreen
                && (self.last_fullscreen_exclusive
                    || (self.config.hide_in_windowed_fullscreen && self.last_fullscreen_windowed)),
        );
        self.state
            .set_collapse_timeout(Duration::from_secs(self.config.collapse_timeout_secs));
        if let Some(tray) = &self.tray {
            tray.set_auto_hide_checked(self.config.auto_hide_when_idle);
            tray.update_dock_mode(self.config.notch_dock_mode);
        }
    }

    #[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
    pub fn apply_external_config(&mut self, cfg: Config, ctx: &egui::Context) {
        let prev_lyric_opts = (
            self.config.romaji_lyrics,
            self.config.karaoke_lyric_mode,
            self.config.translate_lyrics,
            self.config.lyric_translate_lang.clone(),
        );
        self.config = cfg;
        self.calendar_url_input = self.config.google_calendar_ics_url.clone();
        self.sync_runtime_config(ctx, prev_lyric_opts, Instant::now());
        ctx.request_repaint();
    }

    #[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
    pub fn reload_todos(&mut self, ctx: &egui::Context) {
        self.todos = TodoStore::load();
        ctx.request_repaint();
    }

    #[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
    pub fn trigger_test_notification(&mut self, ctx: &egui::Context) {
        self.push_test_notification(Instant::now());
        ctx.request_repaint();
    }

    #[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
    pub fn send_calendar_command(&self, cmd: CalendarCommand) {
        let _ = self.channels.calendar_cmd_tx.send(cmd);
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn send_config_to_daemon(&self, cfg: Config) {
        if let Some(tx) = &self.channels.daemon_tx {
            let _ = tx.send(UiToDaemon::ConfigSaved(cfg));
        }
    }

    pub fn step_logic(
        &mut self,
        ctx: &egui::Context,
        mode: OverlayBackendMode,
        external_hovered: bool,
    ) -> OverlayStep {
        if mode == OverlayBackendMode::Eframe
            && !self.no_activate_applied
            && platform::apply_overlay_window_setup(WINDOW_TITLE)
        {
            let screen_w = platform::primary_monitor_logical_width(ctx.pixels_per_point());
            let x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
            let top_y = self.effective_top_offset();
            platform::sync_overlay_window(
                WINDOW_TITLE,
                x,
                top_y,
                self.current_size.x,
                self.current_size.y,
                self.config.notch_dock_mode,
            );
            self.no_activate_applied = true;
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.001, 0.05);
        self.last_frame = now;

        // Tab transitions only request fast repaints while in flight; idle cadence is untouched.
        let tab_switching = match self.tab_switch {
            Some(sw) if tab_transition_state(&sw, now).is_none() => {
                self.tab_switch = None;
                false
            }
            Some(_) => true,
            None => false,
        };

        let (want_exit, tray_open_settings) = self.handle_events(ctx, now);

        let target_morph = if self.config.notch_dock_mode {
            1.0
        } else {
            0.0
        };
        let (next_morph, next_morph_vel) = step_spring(
            self.notch_morph,
            self.notch_morph_vel,
            target_morph,
            14.5,
            0.76,
            dt,
        );
        self.notch_morph = next_morph.clamp(0.0, 1.0);
        self.notch_morph_vel =
            if (target_morph - self.notch_morph).abs() < 0.002 && next_morph_vel.abs() < 0.02 {
                self.notch_morph = target_morph;
                0.0
            } else {
                next_morph_vel
            };
        let morphing =
            (target_morph - self.notch_morph).abs() >= 0.002 || self.notch_morph_vel.abs() >= 0.02;

        let fs = self.config.hide_in_fullscreen
            && (self.last_fullscreen_exclusive
                || (self.config.hide_in_windowed_fullscreen && self.last_fullscreen_windowed));
        let fs_changed = self.state.set_fullscreen_active(fs);

        if let Some(finished_phase) = self.pomodoro.tick(now) {
            let (title, body) = match finished_phase {
                PomodoroPhase::Focus => (
                    "Focus Session Complete",
                    format!(
                        "Completed {} focus session(s). Time for a {}!",
                        self.pomodoro.completed_focus,
                        self.pomodoro.phase.label()
                    ),
                ),
                PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => (
                    "Break Finished",
                    "Ready to start the next Focus session.".to_owned(),
                ),
            };
            self.record_notification(
                NotificationItem {
                    id: format!("pomodoro-{}", self.pomodoro.completed_focus),
                    app_name: "Pomodoro".to_owned(),
                    title: title.to_owned(),
                    body,
                    icon_rgba: None,
                    timestamp: chrono::Utc::now().timestamp().max(0) as u64,
                },
                now,
            );
        }

        // Win32 WS_EX_NOACTIVATE windows can drop pointer-hover events; poll exact drawn pill
        // bounds and top-edge trigger zone so envelope animations stay completely glitch-free.
        let top_y = self.effective_top_offset();
        let ppp = ctx.pixels_per_point().max(1.0);
        let screen_w = ctx
            .input(|i| i.viewport().monitor_size.map(|m| m.x))
            .filter(|w| *w > 100.0)
            .unwrap_or_else(|| platform::primary_monitor_logical_width(ppp));
        let trigger_w = self.config.idle_width.max(180.0) * self.config.scale;
        let is_animating_prev = (self.target_size() - self.current_size).length_sq() > 0.25;
        let hovered = if self.state.hide_in_fullscreen
            && self.state.fullscreen_active
            && self.state.active.is_none()
        {
            false
        } else if cfg!(windows) {
            let top_trigger = platform::is_cursor_in_top_trigger(top_y, trigger_w, ppp);
            if !self.last_visible {
                top_trigger.unwrap_or(false)
            } else {
                let pill_x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
                let pad = if self.state.hovered { 8.0 } else { 2.0 };
                let over_pill = platform::is_cursor_in_screen_rect(
                    ((pill_x - pad) * ppp) as i32,
                    ((top_y - pad) * ppp).max(0.0) as i32,
                    ((pill_x + self.current_size.x + pad) * ppp) as i32,
                    ((top_y + self.current_size.y + pad) * ppp) as i32,
                )
                .unwrap_or(self.last_egui_hovered);
                over_pill || (self.state.hovered && top_trigger.unwrap_or(false))
            }
        } else {
            let pointer_busy = ctx.input(|i| i.pointer.any_down());
            let ctx_hovered = external_hovered
                || self.last_egui_hovered
                || ctx.input(|i| i.pointer.hover_pos().is_some());
            platform::detect_overlay_hover(HoverPollInput {
                window_title: WINDOW_TITLE,
                ctx_hovered,
                pointer_busy,
                currently_hovered: self.state.hovered,
                visible: self.last_visible,
                animating: is_animating_prev || morphing,
                top_offset_px: top_y,
                trigger_width_px: trigger_w,
                pixels_per_point: ppp,
            })
        };
        let hover_changed = self.state.set_hovered(hovered, now);

        self.state.tick(now);

        let want_visible = self.state.should_be_visible(now);
        let target_vis = if want_visible { 1.0 } else { 0.0 };
        let vis_diff = target_vis - self.visibility_anim;
        let vis_animating = vis_diff.abs() >= 0.004;
        if vis_animating {
            self.visibility_anim += vis_diff * (1.0 - (-15.0 * dt).exp());
            if (target_vis - self.visibility_anim).abs() < 0.004 {
                self.visibility_anim = target_vis;
            }
        } else {
            self.visibility_anim = target_vis;
        }
        let visible = want_visible || self.visibility_anim > 0.01;
        let became_visible = visible && !self.last_visible;
        let visibility_changed = visible != self.last_visible;
        if visibility_changed {
            self.last_visible = visible;
            // On Eframe (Windows/X11), keep the OS window shown and rely on transparent
            // rendering + WS_EX_TRANSPARENT passthrough (or 1x1 sizing on Linux) when hidden;
            // calling SW_HIDE suppresses WM_PAINT so request_repaint() cannot wake logic() again.
        }
        if mode == OverlayBackendMode::Eframe {
            if became_visible {
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
                platform::ensure_overlay_topmost(WINDOW_TITLE, true);
            } else if visible && !(self.state.hide_in_fullscreen && self.state.fullscreen_active) {
                platform::ensure_overlay_topmost(WINDOW_TITLE, false);
            }

            if self.show_settings
                && self.settings_os_focus_pending
                && !self.settings_focus_requested
            {
                if platform::bring_window_to_front(SETTINGS_WINDOW_TITLE) {
                    self.settings_os_focus_pending = false;
                } else {
                    ctx.request_repaint();
                }
            }
        }

        let target = self.target_size();
        if became_visible {
            let s = self.config.scale;
            self.current_size = Vec2::new(
                (target.x * 0.78).max(48.0 * s),
                (target.y * 0.62).max(18.0 * s),
            );
            self.size_velocity = Vec2::new(180.0 * s, 90.0 * s);
        }
        let expanding_x = target.x > self.current_size.x + 1.0;
        let expanding_y = target.y > self.current_size.y + 1.0;
        let (omega_x, zeta_x) = if expanding_x {
            (16.5, 0.68)
        } else {
            (15.5, 0.76)
        };
        let (omega_y, zeta_y) = if expanding_y {
            (14.8, 0.70)
        } else {
            (14.0, 0.78)
        };

        let (nx, vx) = step_spring(
            self.current_size.x,
            self.size_velocity.x,
            target.x,
            omega_x,
            zeta_x,
            dt,
        );
        let (ny, vy) = step_spring(
            self.current_size.y,
            self.size_velocity.y,
            target.y,
            omega_y,
            zeta_y,
            dt,
        );
        let min_w = (60.0 * self.config.scale).min(target.x);
        let min_h = (22.0 * self.config.scale).min(target.y);
        self.current_size = Vec2::new(nx.max(min_w), ny.max(min_h));
        self.size_velocity = Vec2::new(vx, vy);

        let animating =
            (target - self.current_size).length_sq() > 0.09 || self.size_velocity.length_sq() > 1.0;
        if !animating {
            self.current_size = target;
            self.size_velocity = Vec2::ZERO;
        }

        let rounded_size = Vec2::new(self.current_size.x.round(), self.current_size.y.round());
        if mode == OverlayBackendMode::Eframe {
            if cfg!(windows) {
                // On Windows, keep one permanent transparent canvas at y = 0: opening, closing and
                // notch morphing only repaint inside it, so the OS window is never resized mid-animation.
                let canvas_size = permanent_canvas_size(&self.config);
                if (canvas_size - self.applied_window_size).length_sq() > 0.25 {
                    self.applied_window_size = canvas_size;
                    let win_x = ((screen_w - canvas_size.x) * 0.5).max(0.0);
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(canvas_size));
                    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(win_x, 0.0)));
                }
                if visible {
                    platform::pin_window_top_y(WINDOW_TITLE, 0);
                }
                let passthrough = !want_visible || !hovered;
                if self.last_passthrough != Some(passthrough) {
                    self.last_passthrough = Some(passthrough);
                    platform::set_overlay_click_through(WINDOW_TITLE, passthrough);
                }
            } else {
                let target_window_size = if visible {
                    rounded_size
                } else {
                    Vec2::splat(1.0)
                };
                if (target_window_size - self.applied_size).length_sq() > 0.25
                    || (top_y - self.applied_top_y).abs() > 0.25
                    || hover_changed
                    || became_visible
                    || visibility_changed
                {
                    self.applied_size = target_window_size;
                    self.applied_top_y = top_y;
                    let x = ((screen_w - rounded_size.x) * 0.5).max(0.0);
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(target_window_size));
                    if platform::should_send_outer_position() {
                        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(x, top_y)));
                    }
                    if visible {
                        platform::sync_overlay_window(
                            WINDOW_TITLE,
                            x,
                            top_y,
                            rounded_size.x,
                            rounded_size.y,
                            self.config.notch_dock_mode,
                        );
                    }
                }
            }
        }

        self.sync_notif_icon_textures(ctx);
        if self.config.background_material == BackgroundMaterial::Acrylic {
            self.ensure_acrylic_grain_texture(ctx);
        }

        let mascot_in_home = visible
            && self.state.visual == IslandVisualState::Expanded
            && self.state.active.is_none()
            && self.active_tab == IslandTab::Overview
            && self.config.show_idle_mascot
            && !self.state.paused;
        let mascot_active = visible && self.is_mascot_visible_in_idle();
        let mascot_shown = mascot_active || mascot_in_home;
        let mut mascot_animating = false;
        let mut mascot_blinking = false;
        let mut mascot_cursor_recent = false;
        if mascot_shown {
            if let Some(info) = platform::cursor_screen_info(ppp) {
                let moved = self.mascot_last_cursor.is_none_or(|(px, py)| {
                    (px - info.cursor_x).abs() > 1.5 || (py - info.cursor_y).abs() > 1.5
                });
                if moved {
                    self.mascot_last_cursor = Some((info.cursor_x, info.cursor_y));
                    self.mascot_cursor_moved_at = Some(now);
                }
                let island_cx = info.screen_x + info.screen_w * 0.5;
                let island_cy = info.screen_y + top_y + self.current_size.y * 0.5;
                let dx = ((info.cursor_x - island_cx) / (info.screen_w * 0.34).max(120.0))
                    .clamp(-1.0, 1.0);
                let dy = ((info.cursor_y - island_cy) / (info.screen_h * 0.42).max(120.0))
                    .clamp(-1.0, 1.0);
                let v = Vec2::new(dx, dy);
                self.mascot_eye_target = if v.length() > 1.0 { v.normalized() } else { v };
            } else if let Some(local_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                let center = self.current_size * 0.5;
                let dx = ((local_pos.x - center.x) / (self.current_size.x * 0.45).max(20.0))
                    .clamp(-1.0, 1.0);
                let dy = ((local_pos.y - center.y) / (self.current_size.y * 0.45).max(16.0))
                    .clamp(-1.0, 1.0);
                let v = Vec2::new(dx, dy);
                self.mascot_eye_target = if v.length() > 1.0 { v.normalized() } else { v };
                self.mascot_cursor_moved_at = Some(now);
            }

            let diff = self.mascot_eye_target - self.mascot_eye_offset;
            if diff.length() > 0.008 {
                let blend = 1.0 - (-18.0 * dt).exp();
                self.mascot_eye_offset += diff * blend;
                mascot_animating = true;
            } else {
                self.mascot_eye_offset = self.mascot_eye_target;
            }
            mascot_cursor_recent = self
                .mascot_cursor_moved_at
                .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_millis(250));

            if let Some(started) = self.mascot_blink_started_at {
                let elapsed_ms = now.saturating_duration_since(started).as_secs_f32() * 1000.0;
                if elapsed_ms >= 170.0 {
                    self.mascot_blink_started_at = None;
                    self.mascot_eye_open = 1.0;
                    let jitter = ((now.saturating_duration_since(started).as_nanos()
                        ^ self.last_frame.elapsed().as_nanos())
                        % 1700) as u64;
                    self.mascot_blink_next_at = now + Duration::from_millis(3600 + jitter);
                } else {
                    self.mascot_eye_open = mascot_blink_open_factor(elapsed_ms);
                    mascot_blinking = true;
                }
            } else if now >= self.mascot_blink_next_at {
                self.mascot_blink_started_at = Some(now);
                self.mascot_eye_open = mascot_blink_open_factor(8.0);
                mascot_blinking = true;
            } else {
                self.mascot_eye_open = 1.0;
            }
        } else {
            self.mascot_blink_started_at = None;
            self.mascot_eye_open = 1.0;
        }

        let playing_music =
            self.config.show_lyrics && self.media.as_ref().is_some_and(|m| m.is_playing);
        let playing_lyrics = playing_music && matches!(self.lyrics, LyricsData::Synced(_));
        let showing_idle_bars = self.state.visual == IslandVisualState::Idle && playing_music;
        let pill_lyric_animating = if self.config.show_lyrics
            && self.media.is_some()
            && self.state.visual == IslandVisualState::Idle
        {
            self.update_pill_lyric_transition(now)
        } else {
            let _ = self.update_pill_lyric_transition(now);
            false
        };
        let showing_live_seconds = self.state.visual == IslandVisualState::Idle
            && self.effective_idle_mode() == IdleDisplayMode::Clock
            && self.config.clock_show_seconds;
        let pomodoro_running = self.pomodoro.running;
        let battery_charging_animating = self.active_tab == IslandTab::Battery
            && self.system_metrics.battery.is_some_and(|bat| {
                matches!(
                    bat.status,
                    crate::battery::BatteryStatus::Charging | crate::battery::BatteryStatus::Full
                )
            });

        let mut repaint_after = if animating
            || morphing
            || vis_animating
            || hover_changed
            || fs_changed
            || pill_lyric_animating
            || tab_switching
        {
            Duration::ZERO
        } else if !visible {
            Duration::from_millis(250)
        } else if (playing_lyrics && self.config.karaoke_lyrics) || mascot_blinking {
            Duration::from_millis(16)
        } else if showing_idle_bars || battery_charging_animating || mascot_animating {
            Duration::from_millis(33)
        } else if mascot_shown && mascot_cursor_recent {
            Duration::from_millis(45)
        } else if mascot_shown {
            Duration::from_millis(90)
        } else if playing_lyrics || showing_live_seconds || pomodoro_running {
            Duration::from_millis(250)
        } else if let Some(deadline) = self
            .state
            .next_deadline()
            .or_else(|| self.state.auto_hide_deadline())
        {
            deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(80))
        } else if let Some(demo_at) = self.demo_deadline {
            demo_at.saturating_duration_since(now)
        } else {
            Duration::from_millis(80)
        };
        if mascot_shown && !mascot_blinking && self.mascot_blink_next_at > now {
            repaint_after =
                repaint_after.min(self.mascot_blink_next_at.saturating_duration_since(now));
        }

        let mut want_exit = want_exit;
        if mode == OverlayBackendMode::Eframe || mode == OverlayBackendMode::WindowsLayered {
            if self.channels.daemon_tx.is_some() {
                if !visible && !self.show_settings && self.demo_deadline.is_none() {
                    let since = *self.hidden_since.get_or_insert(now);
                    if now.duration_since(since) >= Duration::from_millis(1500) {
                        if let Some(tx) = &self.channels.daemon_tx {
                            let _ = tx.send(UiToDaemon::UiClosing);
                        }
                        self.allow_close = true;
                        want_exit = true;
                        if mode == OverlayBackendMode::Eframe {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    }
                } else {
                    self.hidden_since = None;
                }
            }
            if mode == OverlayBackendMode::Eframe {
                if repaint_after.is_zero() {
                    ctx.request_repaint();
                } else {
                    ctx.request_repaint_after(repaint_after);
                }
            }
        }

        let open_settings_requested = if mode != OverlayBackendMode::Eframe {
            let req = self.show_settings || tray_open_settings;
            self.show_settings = false;
            self.settings_focus_requested = false;
            self.settings_os_focus_pending = false;
            req
        } else {
            false
        };

        OverlayStep {
            visible,
            size: rounded_size,
            top_y,
            repaint_after,
            want_exit,
            open_settings_requested,
        }
    }

    pub fn render_island_ui(&mut self, ui: &mut egui::Ui, allow_inline_settings: bool) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();

        // Left/Right arrows switch tabs in expanded mode. Skip when a text field owns focus.
        if self.state.visual == IslandVisualState::Expanded && self.state.active.is_none() {
            let left = ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));
            let right = ui.input(|i| i.key_pressed(egui::Key::ArrowRight));
            let typing = ui.ctx().memory(|m| m.focused().is_some());
            if !typing && (left || right) {
                let dir = if left { -1 } else { 1 };
                let next = navigate_tab(self.active_tab, dir, &self.config);
                self.switch_tab(next, dir as f32, &ctx);
            }
        }

        if self.effective_idle_mode() == IdleDisplayMode::Clock {
            let s = self.config.scale;
            let with_mascot = self.config.show_idle_mascot && !self.state.paused;
            let (time_str, tz_opt) = clock_display_parts(&self.config);
            let stable_time = stabilized_clock_measure_text(&time_str);
            if self
                .clock_width_cache
                .as_ref()
                .is_none_or(|(cached, cached_m, _)| {
                    cached != &stable_time || *cached_m != with_mascot
                })
            {
                // Width changes at most once per displayed second; avoid re-laying out the
                // clock glyphs on every repaint (a per-frame allocation in the idle pill).
                let job = build_clock_layout_job(
                    &stable_time,
                    tz_opt.as_deref(),
                    Color32::WHITE,
                    Color32::WHITE,
                    s,
                );
                let text_w = ui.painter().layout_job(job).size().x;
                let lead_and_gap = if with_mascot {
                    (20.0 * s) + (7.0 * s)
                } else {
                    (4.2 * s) * 4.0
                };
                let chrome_and_pad = 26.0 + 16.0 * s;
                let measured = (lead_and_gap + text_w + chrome_and_pad)
                    .ceil()
                    .max(96.0 * s);
                self.clock_width_cache = Some((stable_time, with_mascot, measured));
            }
            let measured = self
                .clock_width_cache
                .as_ref()
                .map(|(_, _, w)| *w)
                .unwrap_or(96.0 * s);
            if self
                .clock_auto_width
                .is_none_or(|prev| (prev - measured).abs() > 0.5)
            {
                self.clock_auto_width = Some(measured);
                ctx.request_repaint();
            }
        }

        let is_music_card = self.state.visual == IslandVisualState::Expanded
            && self.state.active.is_none()
            && self.active_tab == IslandTab::Music
            && self.media.is_some();

        let ear_w = self.notch_ear_width();
        let vis = self.visibility_anim.clamp(0.0, 1.0);
        if vis > 0.005 {
            let top_y = self.effective_top_offset();
            let slide_y = -((self.current_size.y + top_y) * (1.0 - vis)) * 0.65;

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    let canvas = ui.max_rect();
                    let card_rect = if canvas.width() > self.current_size.x + 2.0
                        || canvas.height() > self.current_size.y + 2.0
                    {
                        let card_x =
                            canvas.left() + ((canvas.width() - self.current_size.x) * 0.5).max(0.0);
                        let card_y = canvas.top() + top_y + slide_y;
                        Rect::from_min_size(Pos2::new(card_x, card_y), self.current_size)
                    } else {
                        canvas
                    };
                    self.last_egui_hovered = ui.rect_contains_pointer(card_rect);

                    let body_w = (card_rect.width() - ear_w * 2.0).max(1.0);
                    let max_r = ((card_rect.height() * 0.5)
                        .floor()
                        .min((body_w * 0.5).floor()))
                    .clamp(0.0, 28.0) as u8;
                    let pill_radius = (card_rect.height() * 0.5).clamp(12.0, 28.0);
                    let notch_radius = (14.0 * self.config.scale).clamp(10.0, 20.0);
                    let raw_bot_radius = if self.state.visual == IslandVisualState::Expanded {
                        self.config.corner_radius.min(26)
                    } else {
                        lerp_u8(
                            pill_radius.round() as u8,
                            notch_radius.round() as u8,
                            self.notch_morph,
                        )
                    };
                    let bot_radius = raw_bot_radius.min(max_r);
                    let top_corner_factor = (1.0 - self.notch_morph * 2.0).clamp(0.0, 1.0);
                    let top_radius = ((bot_radius as f32) * top_corner_factor).round() as u8;
                    let border_alpha =
                        ((self.config.border_alpha as f32) * top_corner_factor * vis).round() as u8;
                    let material = self.config.background_material;
                    let surface_accent = self.effective_accent();
                    let docked = ear_w > 0.5;

                    let (pad_x, pad_y) = if is_music_card {
                        let floating_bg = [
                            self.config.bg_color[0],
                            self.config.bg_color[1],
                            self.config.bg_color[2],
                            self.config.bg_color[3].max(235),
                        ];
                        let docked_bg = [14, 15, 19, 252];
                        let raw_bg = Color32::from_rgba_unmultiplied(
                            lerp_u8(floating_bg[0], docked_bg[0], self.notch_morph),
                            lerp_u8(floating_bg[1], docked_bg[1], self.notch_morph),
                            lerp_u8(floating_bg[2], docked_bg[2], self.notch_morph),
                            ((lerp_u8(floating_bg[3], docked_bg[3], self.notch_morph) as f32) * vis)
                                .round() as u8,
                        );
                        let (fill, mat_border) =
                            material_surface_colors(raw_bg, surface_accent, material, docked);
                        let base_bg = [fill.r(), fill.g(), fill.b(), fill.a()];
                        let accent_rgb = self.effective_accent_rgb();
                        self.ensure_gradient_texture(&ctx, base_bg, accent_rgb, false);
                        paint_island_surface(
                            ui.painter(),
                            card_rect,
                            SurfaceStyle {
                                ear_w,
                                top_radius,
                                bot_radius,
                                fill,
                                grad_tex: self.gradient_tex.as_ref(),
                                grain_tex: self.acrylic_grain_tex.as_ref(),
                                accent: surface_accent,
                                material,
                                border_alpha: border_alpha.max(mat_border),
                            },
                        );
                        (16.0 + ear_w, 2.0 * self.config.scale)
                    } else {
                        let pill_album_rgb = if matches!(
                            self.state.visual,
                            IslandVisualState::Idle | IslandVisualState::Compact
                        ) && self.config.compact_album_gradient
                            && self.config.show_lyrics
                        {
                            self.media
                                .as_ref()
                                .and_then(|m| m.album_art.as_ref().and_then(|a| a.dominant_rgb))
                        } else {
                            None
                        };

                        let (fill, grad_tex, mat_border) = if let Some(album_rgb) = pill_album_rgb {
                            let raw_alpha = if self.state.visual == IslandVisualState::Idle {
                                lerp_u8(self.config.bg_color[3].max(240), 252, self.notch_morph)
                            } else {
                                self.config.bg_color[3].max(240)
                            };
                            let alpha = ((raw_alpha as f32) * vis).round() as u8;
                            let (fill, mat_border) = material_surface_colors(
                                Color32::from_rgba_unmultiplied(10, 10, 13, alpha),
                                surface_accent,
                                material,
                                docked,
                            );
                            let base_tint = [fill.r(), fill.g(), fill.b(), fill.a()];
                            self.ensure_gradient_texture(&ctx, base_tint, album_rgb, true);
                            (fill, self.gradient_tex.as_ref(), mat_border)
                        } else {
                            let base = self.bg_color32();
                            let raw_fill = if self.state.visual == IslandVisualState::Idle {
                                let a = ((lerp_u8(base.a(), 252, self.notch_morph) as f32) * vis)
                                    .round() as u8;
                                Color32::from_rgba_unmultiplied(
                                    lerp_u8(base.r(), 10, self.notch_morph),
                                    lerp_u8(base.g(), 10, self.notch_morph),
                                    lerp_u8(base.b(), 13, self.notch_morph),
                                    a,
                                )
                            } else {
                                Color32::from_rgba_unmultiplied(
                                    base.r(),
                                    base.g(),
                                    base.b(),
                                    ((base.a() as f32) * vis).round() as u8,
                                )
                            };
                            let (fill, mat_border) =
                                material_surface_colors(raw_fill, surface_accent, material, docked);
                            (fill, None, mat_border)
                        };

                        paint_island_surface(
                            ui.painter(),
                            card_rect,
                            SurfaceStyle {
                                ear_w,
                                top_radius,
                                bot_radius,
                                fill,
                                grad_tex,
                                grain_tex: self.acrylic_grain_tex.as_ref(),
                                accent: surface_accent,
                                material,
                                border_alpha: border_alpha.max(mat_border),
                            },
                        );
                        // Expanded content sits almost flush with the card top; the previous
                        // symmetric 9 px inset read as an empty strip above the header.
                        let py = match self.state.visual {
                            IslandVisualState::Expanded => 2.0 * self.config.scale,
                            IslandVisualState::Idle => 5.0,
                            IslandVisualState::Compact => 9.0,
                        };
                        (13.0 + ear_w, py)
                    };

                    let inner_rect = card_rect.shrink2(Vec2::new(pad_x, pad_y));
                    if inner_rect.width() >= 10.0 && inner_rect.height() >= 8.0 {
                        ui.scope_builder(egui::UiBuilder::new().max_rect(inner_rect), |ui| {
                            ui.set_clip_rect(card_rect);
                            ui.multiply_opacity(vis);
                            match self.state.visual {
                                IslandVisualState::Expanded => {
                                    let expand_alpha = ((card_rect.height()
                                        - 30.0 * self.config.scale)
                                        / (32.0 * self.config.scale))
                                        .clamp(0.0, 1.0);
                                    if expand_alpha > 0.02 {
                                        ui.multiply_opacity(expand_alpha);
                                        self.render_expanded(ui, &ctx, now);
                                    }
                                }
                                IslandVisualState::Compact => self.render_compact(ui),
                                IslandVisualState::Idle => self.render_idle(ui, &ctx, now),
                            }
                        });
                    }
                });
        }

        if !allow_inline_settings {
            self.installed_fonts = None;
            return;
        }
        if !self.show_settings {
            self.installed_fonts = None;
        }

        let prev_lyric_opts = (
            self.config.romaji_lyrics,
            self.config.karaoke_lyric_mode,
            self.config.translate_lyrics,
            self.config.lyric_translate_lang.clone(),
        );
        let mut config_dirty = false;
        let mut test_notif_req = false;
        render_settings_viewport(
            &ctx,
            SettingsWindowState {
                open: &mut self.show_settings,
                focus_requested: &mut self.settings_focus_requested,
                tab: &mut self.settings_tab,
                config: &mut self.config,
                todos: &mut self.todos,
                new_todo_input: &mut self.new_todo_input,
                calendar_url_input: &mut self.calendar_url_input,
                calendar_events: &self.calendar_events,
                calendar_status: &self.calendar_status,
                calendar_cmd_tx: &self.channels.calendar_cmd_tx,
                config_dirty: &mut config_dirty,
                test_notification_requested: &mut test_notif_req,
                installed_fonts: &mut self.installed_fonts,
                font_search: &mut self.font_search,
            },
        );
        if test_notif_req {
            self.push_test_notification(now);
            ctx.request_repaint();
        }
        if config_dirty {
            self.sync_runtime_config(&ctx, prev_lyric_opts, now);
            if let Some(tx) = &self.channels.sysmon_cmd_tx {
                let _ = tx.send(SysmonCommand::SetConfig {
                    enabled: self.config.taskbar_monitor_enabled,
                    interval_ms: self.config.taskbar_interval_ms,
                    collect_disk: self.config.taskbar_show_disk,
                    collect_net: self.config.taskbar_show_network,
                });
            }
            if let Some(tx) = &self.channels.daemon_tx {
                let _ = tx.send(UiToDaemon::ConfigSaved(self.config.clone()));
            } else {
                if let Err(err) = self.config.save() {
                    eprintln!("Failed to save config: {err}");
                }
                if let Err(err) = platform::set_autostart(self.config.start_with_os) {
                    eprintln!("Failed to apply autostart setting: {err}");
                }
            }
            ctx.request_repaint();
        }
    }
}

impl eframe::App for IslandApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let step = self.step_logic(ctx, OverlayBackendMode::Eframe, false);
        // Only the tray "Exit" or intentional on-demand UI close may terminate the viewport.
        if step.want_exit {
            self.allow_close = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.allow_close && ui.input(|i| i.viewport().close_requested()) {
            ui.ctx().send_viewport_cmd(ViewportCommand::CancelClose);
        }
        self.render_island_ui(ui, true);
    }
}

impl IslandApp {
    fn render_idle(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let align = self.text_align();
        let s = self.config.scale;

        if let ListenerStatus::AccessDenied(msg) = &self.state.listener_status {
            ui.horizontal_centered(|ui| {
                ui.label(
                    RichText::new("!")
                        .color(Color32::from_rgb(255, 190, 90))
                        .strong(),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(msg)
                            .font(FontId::proportional(11.0 * s))
                            .color(text_col),
                    )
                    .truncate(),
                );
            });
            return;
        }

        match self.effective_idle_mode() {
            IdleDisplayMode::Lyrics if self.config.show_lyrics && self.media.is_some() => {
                self.sync_album_art_texture(ctx);
                if let Some(track) = &self.media {
                    let pos_ms = self.current_lyric_ms(now);
                    let active = active_karaoke(&self.lyrics, pos_ms);

                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0 * s;
                        // Left mini album cover or soft circular music disc.
                        let badge_sz = 18.0 * s;
                        let (badge_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(badge_sz), Sense::hover());
                        let badge_rad = CornerRadius::same((5.0 * s).round() as u8);
                        if let Some(tex) = &self.album_art_tex {
                            let shape = RectShape::filled(badge_rect, badge_rad, Color32::WHITE)
                                .with_texture(
                                    tex.id(),
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                );
                            ui.painter().add(shape);
                        } else {
                            ui.painter().rect_filled(
                                badge_rect,
                                badge_rad,
                                Color32::from_rgba_unmultiplied(36, 36, 42, 235),
                            );
                            ui.painter().circle_filled(
                                badge_rect.center(),
                                badge_sz * 0.34,
                                Color32::from_rgba_unmultiplied(235, 235, 240, 225),
                            );
                            paint_icon(
                                ui.painter(),
                                Rect::from_center_size(badge_rect.center(), Vec2::splat(10.0 * s)),
                                IconKind::MusicNote,
                                Color32::from_rgb(20, 20, 24),
                            );
                        }

                        ui.add_space(3.0 * s);
                        let bars_w = 18.0 * s;
                        let bars_reserve = 22.0 * s;
                        let text_w = (ui.available_width() - bars_reserve).max(40.0);
                        let lyric_anim = lyric_transition_state(
                            self.pill_lyric_anim.started_at,
                            self.pill_lyric_anim.direction,
                            now,
                        );
                        let pill_tr =
                            if self.config.show_pill_translation && self.config.translate_lyrics {
                                active.and_then(|k| k.translation)
                            } else {
                                None
                            };
                        let has_tr_space = pill_tr.is_some()
                            || (lyric_anim.is_some()
                                && self.pill_lyric_anim.prev_translation.is_some());
                        let text_col_h = if has_tr_space { 32.0 * s } else { 20.0 * s };
                        let show_note = align == egui::Align::LEFT;
                        ui.allocate_ui_with_layout(
                            Vec2::new(text_w, text_col_h),
                            egui::Layout::top_down(align),
                            |ui| {
                                ui.set_clip_rect(ui.max_rect());
                                ui.spacing_mut().item_spacing.y = 1.0 * s;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 5.0 * s;
                                    if show_note {
                                        let (note_rect, _) = ui.allocate_exact_size(
                                            Vec2::splat(11.0 * s),
                                            Sense::hover(),
                                        );
                                        paint_icon(
                                            ui.painter(),
                                            note_rect,
                                            IconKind::MusicNote,
                                            muted,
                                        );
                                    }
                                    if let Some(k) = active {
                                        let lyric_text = match self.config.karaoke_lyric_mode {
                                            KaraokeLyricMode::Romaji => k.romaji.unwrap_or(k.text),
                                            KaraokeLyricMode::Translate => {
                                                k.translation.unwrap_or(k.text)
                                            }
                                            KaraokeLyricMode::Original => k.text,
                                        };
                                        let font = lyrics_bold_font(11.5 * s);
                                        if let Some((ease, out_frac, in_frac)) = lyric_anim {
                                            let avail_w = ui.available_width().max(24.0);
                                            let (row_rect, _) = ui.allocate_exact_size(
                                                Vec2::new(avail_w, 16.0 * s),
                                                Sense::hover(),
                                            );
                                            let shift_px = 11.0 * s;
                                            if let Some(prev) = &self.pill_lyric_anim.prev_text {
                                                let out_dim = scale_color_alpha(muted, 1.0 - ease);
                                                let out_lit =
                                                    scale_color_alpha(text_col, 1.0 - ease);
                                                let prev_prog = if self.config.karaoke_lyrics {
                                                    self.pill_lyric_anim.prev_progress
                                                } else {
                                                    1.0
                                                };
                                                paint_karaoke_in_rect(
                                                    ui.painter(),
                                                    row_rect,
                                                    prev,
                                                    prev_prog,
                                                    font.clone(),
                                                    out_dim,
                                                    out_lit,
                                                    out_frac * shift_px,
                                                    align,
                                                );
                                            }
                                            let in_dim = scale_color_alpha(muted, ease);
                                            let in_lit = scale_color_alpha(text_col, ease);
                                            let prog = if self.config.karaoke_lyrics {
                                                k.progress
                                            } else {
                                                1.0
                                            };
                                            paint_karaoke_in_rect(
                                                ui.painter(),
                                                row_rect,
                                                lyric_text,
                                                prog,
                                                font,
                                                in_dim,
                                                in_lit,
                                                in_frac * shift_px,
                                                align,
                                            );
                                        } else if self.config.karaoke_lyrics {
                                            render_karaoke_line(
                                                ui, lyric_text, k.progress, font, muted, text_col,
                                                align,
                                            );
                                        } else {
                                            render_aligned_truncated_text(
                                                ui, lyric_text, font, text_col, align,
                                            );
                                        }
                                    } else {
                                        let fallback = if track.artist.is_empty() {
                                            track.title.clone()
                                        } else {
                                            format!("{} — {}", track.artist, track.title)
                                        };
                                        render_aligned_truncated_text(
                                            ui,
                                            &fallback,
                                            FontId::proportional(11.5 * s),
                                            text_col,
                                            align,
                                        );
                                    }
                                });
                                if let Some((ease, out_frac, in_frac)) = lyric_anim {
                                    if self.pill_lyric_anim.prev_translation.is_some()
                                        || pill_tr.is_some()
                                    {
                                        let avail_w = ui.available_width().max(24.0);
                                        let (tr_rect, _) = ui.allocate_exact_size(
                                            Vec2::new(avail_w, 12.0 * s),
                                            Sense::hover(),
                                        );
                                        let tr_font = FontId::proportional(9.5 * s);
                                        let tr_base = Color32::from_rgba_unmultiplied(
                                            text_col.r(),
                                            text_col.g(),
                                            text_col.b(),
                                            185,
                                        );
                                        let tr_shift = 8.0 * s;
                                        if let Some(prev_tr) =
                                            &self.pill_lyric_anim.prev_translation
                                        {
                                            let out_col = scale_color_alpha(tr_base, 1.0 - ease);
                                            paint_karaoke_in_rect(
                                                ui.painter(),
                                                tr_rect,
                                                prev_tr,
                                                1.0,
                                                tr_font.clone(),
                                                out_col,
                                                out_col,
                                                out_frac * tr_shift,
                                                align,
                                            );
                                        }
                                        if let Some(tr) = pill_tr {
                                            let in_col = scale_color_alpha(tr_base, ease);
                                            paint_karaoke_in_rect(
                                                ui.painter(),
                                                tr_rect,
                                                tr,
                                                1.0,
                                                tr_font,
                                                in_col,
                                                in_col,
                                                in_frac * tr_shift,
                                                align,
                                            );
                                        }
                                    }
                                } else if let Some(tr) = pill_tr {
                                    let tr_col = Color32::from_rgba_unmultiplied(
                                        text_col.r(),
                                        text_col.g(),
                                        text_col.b(),
                                        185,
                                    );
                                    render_aligned_truncated_text(
                                        ui,
                                        tr,
                                        FontId::proportional(9.5 * s),
                                        tr_col,
                                        align,
                                    );
                                }
                            },
                        );

                        ui.allocate_ui_with_layout(
                            Vec2::new(bars_w, 20.0 * s),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                let secs = ui.input(|i| i.time) as f32;
                                render_audio_bars(
                                    ui,
                                    secs,
                                    track.is_playing,
                                    Color32::from_gray(225),
                                    s,
                                );
                            },
                        );
                    });
                    return;
                }
            }
            IdleDisplayMode::Mascot => {
                let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
                let m_sz = (rect.height() * 0.88).clamp(16.0 * s, 28.0 * s);
                let m_rect = Rect::from_center_size(rect.center(), Vec2::splat(m_sz));
                let col = if self.state.paused { muted } else { text_col };
                let glow = (!self.state.paused).then(|| mascot_glow_color(accent, col));
                paint_mascot_fx(
                    ui.painter(),
                    m_rect,
                    self.mascot_eye_offset,
                    self.mascot_eye_open,
                    glow,
                    col,
                );
                return;
            }
            IdleDisplayMode::Clock => {
                let (time_str, tz_opt) = clock_display_parts(&self.config);
                let dot_color = if self.state.paused {
                    Color32::from_gray(140)
                } else {
                    accent
                };
                let job = build_clock_layout_job(&time_str, tz_opt.as_deref(), text_col, muted, s);
                if self.config.show_idle_mascot && !self.state.paused {
                    render_mascot_label(
                        ui,
                        20.0 * s,
                        self.mascot_eye_offset,
                        self.mascot_eye_open,
                        Some(mascot_glow_color(accent, text_col)),
                        text_col,
                        job,
                        egui::Align::Center,
                    );
                } else {
                    render_dot_label(ui, dot_color, 4.2 * s, job, egui::Align::Center);
                }
                return;
            }
            IdleDisplayMode::NextEvent if self.config.show_calendar => {
                let now_utc = chrono::Utc::now().timestamp();
                if let Some(ev) = upcoming_events(&self.calendar_events, now_utc, 1).first() {
                    let cd = format_countdown(ev.start_utc, ev.end_utc, now_utc);
                    let avail_w = ui.available_width().max(40.0);
                    let cd_font = FontId::proportional(10.5 * s);
                    let sum_font = FontId::proportional(11.0 * s);
                    let cd_w = ui
                        .painter()
                        .layout_no_wrap(format!("{cd}  "), cd_font.clone(), accent)
                        .size()
                        .x;
                    let sum_budget = (avail_w - cd_w).max(20.0);
                    let shown_summary = truncate_with_ellipsis(&ev.summary, |t| {
                        ui.painter()
                            .layout_no_wrap(t.to_owned(), sum_font.clone(), text_col)
                            .size()
                            .x
                            <= sum_budget
                    });
                    let mut job = LayoutJob::default();
                    job.append(
                        &cd,
                        0.0,
                        TextFormat {
                            font_id: cd_font,
                            color: accent,
                            valign: egui::Align::Center,
                            ..Default::default()
                        },
                    );
                    job.append(
                        &format!("  {shown_summary}"),
                        0.0,
                        TextFormat {
                            font_id: sum_font,
                            color: text_col,
                            valign: egui::Align::Center,
                            ..Default::default()
                        },
                    );
                    let galley = ui.painter().layout_job(job);
                    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
                    let x = aligned_galley_x(rect.left(), rect.width(), galley.size().x, align);
                    let y = rect.center().y - galley.size().y * 0.5;
                    ui.painter().with_clip_rect(rect).galley(
                        Pos2::new(x, y),
                        galley,
                        Color32::TRANSPARENT,
                    );
                    return;
                }
            }
            IdleDisplayMode::Pomodoro if self.config.show_pomodoro => {
                let phase_col = match self.pomodoro.phase {
                    PomodoroPhase::Focus => accent,
                    PomodoroPhase::ShortBreak => Color32::from_rgb(96, 186, 150),
                    PomodoroPhase::LongBreak => Color32::from_rgb(136, 158, 214),
                };
                let dot_color = if self.pomodoro.running {
                    phase_col
                } else {
                    Color32::from_gray(145)
                };
                let mut job = LayoutJob::default();
                job.append(
                    &self.pomodoro.format_mm_ss(),
                    0.0,
                    TextFormat {
                        font_id: FontId::monospace(12.0 * s),
                        color: text_col,
                        ..Default::default()
                    },
                );
                job.append(
                    &format!("  {}", self.pomodoro.phase.short_label()),
                    0.0,
                    TextFormat {
                        font_id: FontId::proportional(9.5 * s),
                        color: muted,
                        ..Default::default()
                    },
                );
                render_dot_label(ui, dot_color, 3.0 * s, job, align);
                return;
            }
            IdleDisplayMode::Battery => {
                if let Some(bat) = self.system_metrics.battery {
                    let charging = matches!(
                        bat.status,
                        crate::battery::BatteryStatus::Charging
                            | crate::battery::BatteryStatus::Full
                            | crate::battery::BatteryStatus::PluggedNotCharging
                    );
                    let fill = match bat.status {
                        crate::battery::BatteryStatus::Charging
                        | crate::battery::BatteryStatus::Full => Color32::from_rgb(96, 186, 150),
                        _ if bat.percent <= self.config.battery_critical_pct => {
                            Color32::from_rgb(206, 116, 110)
                        }
                        _ if bat.percent <= self.config.battery_warn_pct => {
                            Color32::from_rgb(214, 168, 112)
                        }
                        _ => accent,
                    };
                    let mut job = LayoutJob::default();
                    job.append(
                        &format!("{}%", bat.percent),
                        0.0,
                        TextFormat {
                            font_id: FontId::monospace(12.0 * s),
                            color: text_col,
                            ..Default::default()
                        },
                    );
                    job.append(
                        &format!(
                            "  {}",
                            if charging {
                                bat.status.label()
                            } else {
                                "Battery"
                            }
                        ),
                        0.0,
                        TextFormat {
                            font_id: FontId::proportional(9.5 * s),
                            color: muted,
                            ..Default::default()
                        },
                    );
                    render_dot_label(ui, fill, 3.0 * s, job, align);
                } else {
                    let mut job = LayoutJob::default();
                    job.append(
                        "No battery",
                        0.0,
                        TextFormat {
                            font_id: FontId::proportional(11.0 * s),
                            color: muted,
                            valign: egui::Align::Center,
                            ..Default::default()
                        },
                    );
                    render_dot_label(ui, Color32::from_gray(140), 3.0 * s, job, align);
                }
                return;
            }
            _ => {}
        }

        let (dot_color, text) = if self.state.paused {
            (Color32::from_gray(140), "Paused")
        } else {
            (accent, "Liebert")
        };
        let mut job = LayoutJob::default();
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: FontId::proportional(11.0 * s),
                color: muted,
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
        if self.config.show_idle_mascot && !self.state.paused {
            render_mascot_label(
                ui,
                19.0 * s,
                self.mascot_eye_offset,
                self.mascot_eye_open,
                Some(mascot_glow_color(accent, text_col)),
                text_col,
                job,
                egui::Align::Center,
            );
        } else {
            render_dot_label(ui, dot_color, 4.2 * s, job, egui::Align::Center);
        }
    }

    fn render_notif_icon(
        &self,
        ui: &mut egui::Ui,
        item: &NotificationItem,
        accent: Color32,
        size: f32,
        scale: f32,
    ) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        paint_notif_avatar_in_rect(
            ui.painter(),
            rect,
            &item.app_name,
            self.lookup_notif_icon(item),
            accent,
            scale,
        );
    }

    fn render_compact(&self, ui: &mut egui::Ui) {
        let Some(item) = &self.state.active else {
            return;
        };
        let accent = self.effective_accent();
        let brand = app_brand_color(&item.app_name, accent);
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        // Notification layout always stays cleanly left-aligned regardless of config.text_align.
        let align = egui::Align::Min;
        let s = self.config.scale;
        let queued = self.state.queued_count();

        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(7.0 * s, 2.0 * s);
            self.render_notif_icon(ui, item, brand, 28.0 * s, s);
            let badge_reserve = if queued > 0 { 36.0 * s } else { 0.0 };
            let text_w = (ui.available_width() - badge_reserve).max(60.0);
            ui.allocate_ui_with_layout(
                Vec2::new(text_w, 34.0 * s),
                egui::Layout::top_down(align),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.spacing_mut().item_spacing.y = 2.0 * s;
                    let show_app_badge = !item.app_name.eq_ignore_ascii_case(&item.title);
                    let app_font = ui_bold_font(8.8 * s);
                    let title_font = ui_bold_font(12.2 * s);
                    let badge_pad_x = 10.0;
                    let gap = 5.0 * s;
                    let badge_total_w = if show_app_badge {
                        let gw = ui
                            .painter()
                            .layout_no_wrap(item.app_name.clone(), app_font.clone(), brand)
                            .size()
                            .x;
                        gw + badge_pad_x + gap
                    } else {
                        0.0
                    };
                    let title_budget = (text_w - badge_total_w).max(20.0);
                    let shown_title = truncate_with_ellipsis(&item.title, |t| {
                        ui.painter()
                            .layout_no_wrap(t.to_owned(), title_font.clone(), text_col)
                            .size()
                            .x
                            <= title_budget
                    });
                    ui.allocate_ui_with_layout(
                        Vec2::new(text_w, 17.0 * s),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            if show_app_badge {
                                egui::Frame::new()
                                    .fill(Color32::from_rgba_unmultiplied(
                                        brand.r(),
                                        brand.g(),
                                        brand.b(),
                                        36,
                                    ))
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(egui::Margin::symmetric(5, 1))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(&item.app_name)
                                                .font(app_font)
                                                .color(brand),
                                        );
                                    });
                                ui.add_space(gap);
                            }
                            ui.label(RichText::new(shown_title).font(title_font).color(text_col));
                        },
                    );
                    let sub = if item.body.is_empty() {
                        &item.app_name
                    } else {
                        &item.body
                    };
                    render_aligned_truncated_text(
                        ui,
                        sub,
                        FontId::proportional(10.8 * s),
                        muted,
                        align,
                    );
                },
            );
            if queued > 0 {
                ui.allocate_ui_with_layout(
                    Vec2::new(30.0 * s, 28.0 * s),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        egui::Frame::new()
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 22))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(egui::Margin::symmetric(7, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("+{queued}"))
                                        .font(ui_bold_font(10.0 * s))
                                        .color(text_col),
                                );
                            });
                    },
                );
            }
        });
    }

    /// Single entry point for every tab change: keeps the transition timer and focus
    /// reset consistent across keyboard, side arrows, and Home click-through.
    fn switch_tab(&mut self, next: IslandTab, direction: f32, ctx: &egui::Context) {
        if next == self.active_tab {
            return;
        }
        if self.active_tab == IslandTab::Music || next == IslandTab::Music {
            self.music_scroll_focus_key = None;
        }
        self.tab_switch = Some(TabSwitch {
            direction: if direction < 0.0 { -1.0 } else { 1.0 },
            started_at: Instant::now(),
        });
        self.active_tab = next;
        // Input arrives in ui(), after step_logic() already chose the repaint cadence.
        ctx.request_repaint();
    }

    fn render_expanded(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let nav_card = ui.max_rect();
        let (tab_opacity, tab_dx) = match self.tab_switch {
            Some(sw) => {
                let (ease, slide) = tab_transition_state(&sw, now).unwrap_or((1.0, 0.0));
                (ease.clamp(0.15, 1.0), slide * 12.0 * s)
            }
            None => (1.0, 0.0),
        };

        if let Some(item) = &self.state.active {
            let brand = app_brand_color(&item.app_name, accent);
            let row_h = 26.0 * s;
            let right_w = 180.0 * s;
            let left_w = (ui.available_width() - right_w - 6.0 * s).max(80.0);
            let mut open_settings_clicked = false;
            let mut open_inbox_clicked = false;
            let mut dismiss_clicked = false;
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left_w, row_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_clip_rect(ui.max_rect());
                        ui.spacing_mut().item_spacing.x = 6.0 * s;
                        self.render_notif_icon(ui, item, brand, 23.0 * s, s);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&item.app_name)
                                    .font(ui_bold_font(11.5 * s))
                                    .color(brand),
                            )
                            .truncate(),
                        );
                        let queued = self.state.queued_count();
                        if queued > 0 {
                            ui.label(
                                RichText::new(format!("+{queued}"))
                                    .font(ui_bold_font(10.0 * s))
                                    .color(muted),
                            );
                        }
                    },
                );
                ui.add_space(6.0 * s);
                ui.allocate_ui_with_layout(
                    Vec2::new(right_w, row_h),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.set_clip_rect(ui.max_rect());
                        ui.spacing_mut().item_spacing.x = 4.0 * s;
                        if icon_button(
                            ui,
                            IconKind::Settings,
                            Vec2::new(26.0 * s, 23.0 * s),
                            13.0 * s,
                            muted,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 16),
                            CornerRadius::same(10),
                        )
                        .on_hover_text("Open Settings")
                        .clicked()
                        {
                            open_settings_clicked = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Dismiss")
                                        .font(FontId::proportional(11.0 * s))
                                        .color(Color32::WHITE),
                                )
                                .min_size(Vec2::new(56.0 * s, 23.0 * s))
                                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 24))
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .clicked()
                        {
                            dismiss_clicked = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("All")
                                        .font(FontId::proportional(10.5 * s))
                                        .color(muted),
                                )
                                .min_size(Vec2::new(34.0 * s, 23.0 * s))
                                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text("View all notifications")
                            .clicked()
                        {
                            open_inbox_clicked = true;
                        }
                    },
                );
            });
            ui.add_space(6.0);
            let avail_w = ui.available_width();
            // Notifications stay strictly left-aligned regardless of config.text_align.
            render_aligned_wrapped_text(
                ui,
                &item.title,
                ui_bold_font(13.5 * s),
                text_col,
                avail_w,
                egui::Align::Min,
            );
            if !item.body.is_empty() {
                ui.add_space(3.0);
                render_aligned_wrapped_text(
                    ui,
                    &item.body,
                    FontId::proportional(11.8 * s),
                    muted,
                    avail_w,
                    egui::Align::Min,
                );
            }
            if open_settings_clicked {
                self.open_settings(SettingsTab::Features, ctx);
            }
            if open_inbox_clicked {
                self.state.dismiss_active(now);
                if let Some(tx) = &self.channels.daemon_tx {
                    let _ = tx.send(UiToDaemon::DismissNotification);
                }
                self.switch_tab(IslandTab::Notifications, 1.0, ctx);
            } else if dismiss_clicked {
                self.state.dismiss_active(now);
                if let Some(tx) = &self.channels.daemon_tx {
                    let _ = tx.send(UiToDaemon::DismissNotification);
                }
                ctx.request_repaint();
            }
            self.music_scroll_focus_key = None;
            return;
        }

        let is_music_card = self.active_tab == IslandTab::Music && self.media.is_some();
        if !is_music_card {
            self.music_scroll_focus_key = None;
        }
        // Reserve left/right gutters matching side arrows so content never sits under them.
        let reserve = 26.0 * s;
        let content_rect = Rect::from_min_max(
            Pos2::new(nav_card.min.x + reserve, nav_card.min.y),
            Pos2::new(
                (nav_card.max.x - reserve).max(nav_card.min.x + reserve),
                nav_card.max.y,
            ),
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(content_rect.translate(Vec2::new(tab_dx, 0.0))),
            |ui| {
                ui.set_clip_rect(content_rect);
                ui.multiply_opacity(tab_opacity);
                let row_h = 24.0 * s;
                let right_w = 30.0 * s;
                let left_w = (ui.available_width() - right_w - 6.0 * s).max(80.0 * s);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.allocate_ui_with_layout(
                        Vec2::new(left_w, row_h),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.set_clip_rect(ui.max_rect());
                            ui.label(
                                RichText::new(self.active_tab.label())
                                    .font(ui_bold_font(11.0 * s))
                                    .color(text_col),
                            );
                        },
                    );
                    ui.add_space(6.0 * s);
                    ui.allocate_ui_with_layout(
                        Vec2::new(right_w, row_h),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.set_clip_rect(ui.max_rect());
                            if icon_button(
                                ui,
                                IconKind::Settings,
                                Vec2::new(26.0 * s, 23.0 * s),
                                13.0 * s,
                                muted,
                                Color32::from_rgba_unmultiplied(255, 255, 255, 16),
                                CornerRadius::same(10),
                            )
                            .on_hover_text("Open Settings")
                            .clicked()
                            {
                                self.open_settings(SettingsTab::Appearance, ctx);
                            }
                        },
                    );
                });
                ui.add_space(if is_music_card { 2.0 * s } else { 4.0 * s });

                match self.active_tab {
                    IslandTab::Overview => self.render_overview_tab(ui, now),
                    IslandTab::Notifications => self.render_notifications_tab(ui, ctx, now),
                    IslandTab::Music => self.render_music_tab(ui, ctx, now),
                    IslandTab::Todo => self.render_todo_tab(ui),
                    IslandTab::Calendar => self.render_calendar_tab(ui),
                    IslandTab::Pomodoro => self.render_pomodoro_tab(ui, now),
                    IslandTab::System => self.render_system_tab(ui, ctx),
                    IslandTab::Battery => self.render_battery_tab(ui, ctx),
                }
            },
        );
        self.draw_side_nav(ui, ctx, nav_card);
    }

    /// Left/Right tab controls sit at the vertical middle of the expanded card edges,
    /// replacing the old top tab strip and header chevrons.
    fn draw_side_nav(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, card: Rect) {
        let s = self.config.scale;
        let btn = Vec2::new(20.0 * s, 32.0 * s);
        let y = card.center().y - btn.y * 0.5;
        let col = Color32::from_rgba_unmultiplied(255, 255, 255, 150);
        for (kind, dir, x) in [
            (IconKind::ChevronLeft, -1, card.min.x + 2.0 * s),
            (IconKind::ChevronRight, 1, card.max.x - btn.x - 2.0 * s),
        ] {
            let rect = Rect::from_min_size(Pos2::new(x, y), btn);
            let resp = ui.interact(
                rect,
                ui.scope_id().with(format!("side_nav_{dir}")),
                Sense::click(),
            );
            let bg = if resp.hovered() {
                Color32::from_rgba_unmultiplied(255, 255, 255, 42)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
            };
            ui.painter()
                .rect_filled(rect, CornerRadius::same((8.0 * s).round() as u8), bg);
            paint_icon(ui.painter(), rect, kind, col);
            if resp.clicked() {
                let next = navigate_tab(self.active_tab, dir, &self.config);
                self.switch_tab(next, dir as f32, ctx);
            }
        }
    }

    fn render_notifications_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        let prev_lyric_opts = (
            self.config.romaji_lyrics,
            self.config.karaoke_lyric_mode,
            self.config.translate_lyrics,
            self.config.lyric_translate_lang.clone(),
        );
        let total = self.notification_history.len();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0 * s;
            ui.label(
                RichText::new(format!("{total} recent"))
                    .font(ui_bold_font(10.5 * s))
                    .color(text_col),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0 * s;
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Settings")
                                .font(FontId::proportional(9.5 * s))
                                .color(muted),
                        )
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                        .corner_radius(CornerRadius::same(8)),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.open_settings(SettingsTab::Features, ctx);
                }
                if total > 0
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new("Clear")
                                    .font(FontId::proportional(9.5 * s))
                                    .color(muted),
                            )
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                            .corner_radius(CornerRadius::same(8)),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                {
                    self.notification_history.clear();
                    self.notif_icon_cache.clear();
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Test")
                                .font(FontId::proportional(9.5 * s))
                                .color(text_col),
                        )
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 18))
                        .corner_radius(CornerRadius::same(8)),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.push_test_notification(now);
                }
                let paused = self.state.paused;
                let dnd_fill = if paused {
                    Color32::from_rgba_unmultiplied(230, 148, 96, 46)
                } else {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(if paused { "DND: On" } else { "DND: Off" })
                                .font(FontId::proportional(9.5 * s))
                                .color(if paused {
                                    Color32::from_rgb(245, 182, 128)
                                } else {
                                    muted
                                }),
                        )
                        .fill(dnd_fill)
                        .corner_radius(CornerRadius::same(8)),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.state.set_paused(!paused);
                    if let Some(tx) = &self.channels.daemon_tx {
                        let _ = tx.send(UiToDaemon::TogglePause);
                    }
                    self.sync_tray();
                }
                let popups_on = self.config.show_notifications;
                let pop_fill = if popups_on {
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 44)
                } else {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(if popups_on {
                                "Popups: On"
                            } else {
                                "Popups: Off"
                            })
                            .font(FontId::proportional(9.5 * s))
                            .color(if popups_on {
                                text_col
                            } else {
                                muted
                            }),
                        )
                        .fill(pop_fill)
                        .corner_radius(CornerRadius::same(8)),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.config.show_notifications = !self.config.show_notifications;
                    self.sync_runtime_config(ctx, prev_lyric_opts, now);
                    let _ = self.config.save();
                    self.sync_tray();
                }
            });
        });

        ui.add_space(4.0 * s);

        if self.notification_history.is_empty() {
            let empty_h = ui.available_height().max(72.0 * s);
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width().max(120.0), empty_h),
                Sense::hover(),
            );
            ui.painter().rect_filled(
                rect,
                CornerRadius::same((11.0 * s).round() as u8),
                Color32::from_rgba_unmultiplied(255, 255, 255, 10),
            );
            paint_icon(
                ui.painter(),
                Rect::from_center_size(
                    Pos2::new(rect.center().x, rect.center().y - 12.0 * s),
                    Vec2::splat(18.0 * s),
                ),
                IconKind::Bell,
                muted,
            );
            ui.painter().text(
                Pos2::new(rect.center().x, rect.center().y + 10.0 * s),
                Align2::CENTER_CENTER,
                "No notifications captured yet",
                FontId::proportional(10.5 * s),
                muted,
            );
            return;
        }

        let now_unix = chrono::Utc::now().timestamp().max(0) as u64;
        let card_bg = Color32::from_rgba_unmultiplied(255, 255, 255, 12);
        let card_border = Color32::from_rgba_unmultiplied(255, 255, 255, 22);
        let card_rad = CornerRadius::same((10.0 * s).round() as u8);
        let row_w = ui.available_width().max(120.0);

        // Notification list is strictly left-aligned regardless of config.text_align.
        egui::ScrollArea::vertical()
            .id_salt("notif_history_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 5.0 * s;
                    for item in &self.notification_history {
                        let brand = app_brand_color(&item.app_name, accent);
                        let icon_tex = self.lookup_notif_icon(item);
                        let age_lbl = format_notif_age(now_unix, item.timestamp);
                        let has_body = !item.body.is_empty();
                        let card_h = if has_body { 50.0 * s } else { 36.0 * s };
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(row_w, card_h), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(rect, card_rad, card_bg);
                        p.rect_stroke(
                            rect,
                            card_rad,
                            Stroke::new(1.0, card_border),
                            StrokeKind::Inside,
                        );
                        let inner = rect.shrink2(Vec2::new(8.0 * s, 6.0 * s));
                        let icon_sz = 24.0 * s;
                        let icon_rect = Rect::from_min_size(
                            Pos2::new(inner.left(), inner.center().y - icon_sz * 0.5),
                            Vec2::splat(icon_sz),
                        );
                        paint_notif_avatar_in_rect(
                            p,
                            icon_rect,
                            &item.app_name,
                            icon_tex,
                            brand,
                            s,
                        );
                        let text_left = icon_rect.right() + 8.0 * s;
                        let text_w = (inner.right() - text_left).max(40.0);

                        let age_galley =
                            p.layout_no_wrap(age_lbl, FontId::proportional(8.5 * s), muted);
                        let app_budget = (text_w - age_galley.size().x - 8.0 * s).max(30.0);
                        let app_font = ui_bold_font(9.2 * s);
                        let shown_app = truncate_with_ellipsis(&item.app_name, |t| {
                            p.layout_no_wrap(t.to_owned(), app_font.clone(), brand)
                                .size()
                                .x
                                <= app_budget
                        });
                        let app_galley = p.layout_no_wrap(shown_app, app_font, brand);
                        let row1_y = inner.top();
                        p.galley(
                            Pos2::new(text_left, row1_y),
                            app_galley,
                            Color32::TRANSPARENT,
                        );
                        p.galley(
                            Pos2::new(inner.right() - age_galley.size().x, row1_y),
                            age_galley,
                            Color32::TRANSPARENT,
                        );

                        let title_font = ui_bold_font(11.0 * s);
                        let shown_title = truncate_with_ellipsis(&item.title, |t| {
                            p.layout_no_wrap(t.to_owned(), title_font.clone(), text_col)
                                .size()
                                .x
                                <= text_w
                        });
                        let title_galley = p.layout_no_wrap(shown_title, title_font, text_col);
                        let row2_y = row1_y + 12.5 * s;
                        p.with_clip_rect(inner).galley(
                            Pos2::new(text_left, row2_y),
                            title_galley,
                            Color32::TRANSPARENT,
                        );

                        if has_body {
                            let body_font = FontId::proportional(9.8 * s);
                            let shown_body = truncate_with_ellipsis(&item.body, |t| {
                                p.layout_no_wrap(t.to_owned(), body_font.clone(), muted)
                                    .size()
                                    .x
                                    <= text_w
                            });
                            let body_galley = p.layout_no_wrap(shown_body, body_font, muted);
                            let row3_y = row2_y + 14.0 * s;
                            p.with_clip_rect(inner).galley(
                                Pos2::new(text_left, row3_y),
                                body_galley,
                                Color32::TRANSPARENT,
                            );
                        }
                    }
                });
            });
    }

    fn render_overview_tab(&mut self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let ctx = ui.ctx().clone();

        let avail_w = ui.available_width().max(200.0 * s);
        let avail_h = ui.available_height().max(132.0 * s);
        let gap = 6.5 * s;

        let card_bg = Color32::from_rgba_unmultiplied(255, 255, 255, 11);
        let card_hover = Color32::from_rgba_unmultiplied(255, 255, 255, 22);
        let card_border = Color32::from_rgba_unmultiplied(255, 255, 255, 22);
        let hero_well = Color32::from_rgba_unmultiplied(255, 255, 255, 15);
        let track_col = Color32::from_rgba_unmultiplied(255, 255, 255, 24);
        let radius = CornerRadius::same((12.0 * s).round() as u8);

        #[derive(Clone, Copy)]
        enum GalleryTile {
            Clock,
            Notifications,
            Music,
            System,
            Todo,
            Pomodoro,
            Calendar,
            Battery,
        }

        let mut tiles = vec![GalleryTile::Clock, GalleryTile::Notifications];
        if self.config.show_lyrics {
            tiles.push(GalleryTile::Music);
        }
        tiles.push(GalleryTile::System);
        if self.config.show_todo {
            tiles.push(GalleryTile::Todo);
        }
        if self.config.show_pomodoro {
            tiles.push(GalleryTile::Pomodoro);
        }
        if self.config.show_calendar {
            tiles.push(GalleryTile::Calendar);
        }
        if self.config.show_battery {
            tiles.push(GalleryTile::Battery);
        }

        let placements = gallery_grid_layout(tiles.len(), avail_w, avail_h, gap, s);
        let (grid_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, avail_h), Sense::hover());

        let mut switch_target: Option<IslandTab> = None;
        self.sync_album_art_texture(&ctx);

        let (time_str, tz_opt) = clock_display_parts(&self.config);
        let now_utc = chrono::Utc::now().timestamp();
        let next_event = upcoming_events(&self.calendar_events, now_utc, 1);

        for (idx, rel_rect) in placements {
            let tile = tiles[idx];
            let rect = rel_rect.translate(grid_rect.min.to_vec2());
            let target_tab = match tile {
                GalleryTile::Clock => None,
                GalleryTile::Notifications => Some(IslandTab::Notifications),
                GalleryTile::Music => Some(IslandTab::Music),
                GalleryTile::System => Some(IslandTab::System),
                GalleryTile::Todo => Some(IslandTab::Todo),
                GalleryTile::Pomodoro => Some(IslandTab::Pomodoro),
                GalleryTile::Calendar => Some(IslandTab::Calendar),
                GalleryTile::Battery => Some(IslandTab::Battery),
            };
            let sense = if target_tab.is_some() {
                Sense::click()
            } else {
                Sense::hover()
            };
            let resp = ui.interact(rect, ui.scope_id().with(("home_tile", idx)), sense);
            let hovered = target_tab.is_some() && resp.hovered();
            if target_tab.is_some() {
                let _ = resp.clone().on_hover_cursor(CursorIcon::PointingHand);
            }
            let bg = if hovered { card_hover } else { card_bg };
            let border = if hovered {
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 116)
            } else {
                card_border
            };

            let p = ui.painter();
            p.rect_filled(rect, radius, bg);
            p.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);

            let inner = rect.shrink2(Vec2::new(8.0 * s, 6.0 * s));
            if inner.width() < 24.0 || inner.height() < 24.0 {
                continue;
            }
            let clip = p.with_clip_rect(inner);

            // Uniform tile header strip
            let (hdr_icon, hdr_label, hdr_badge): (IconKind, &str, Option<(String, Color32)>) =
                match tile {
                    GalleryTile::Clock => (
                        IconKind::Timer,
                        "CLOCK",
                        Some((tz_opt.clone().unwrap_or_else(|| "Local".to_owned()), accent)),
                    ),
                    GalleryTile::Notifications => (
                        IconKind::Bell,
                        "ALERTS",
                        Some((
                            self.notification_history.len().to_string(),
                            if self.notification_history.is_empty() {
                                muted
                            } else {
                                accent
                            },
                        )),
                    ),
                    GalleryTile::Music => (
                        IconKind::MusicNote,
                        "MUSIC",
                        self.media.as_ref().map(|t| {
                            if t.is_playing {
                                ("LIVE".to_owned(), Color32::from_rgb(96, 186, 150))
                            } else {
                                ("PAUSE".to_owned(), Color32::from_gray(150))
                            }
                        }),
                    ),
                    GalleryTile::System => (
                        IconKind::Cpu,
                        "SYSTEM",
                        Some((format!("{:.0}%", self.system_metrics.cpu_pct), accent)),
                    ),
                    GalleryTile::Todo => {
                        let total = self.todos.items.len();
                        let done = total.saturating_sub(self.todos.pending_count());
                        (
                            IconKind::TodoCheck,
                            "TASKS",
                            Some((format!("{done}/{total}"), accent)),
                        )
                    }
                    GalleryTile::Pomodoro => {
                        let phase_col = match self.pomodoro.phase {
                            PomodoroPhase::Focus => accent,
                            PomodoroPhase::ShortBreak => Color32::from_rgb(96, 186, 150),
                            PomodoroPhase::LongBreak => Color32::from_rgb(136, 158, 214),
                        };
                        (
                            IconKind::Timer,
                            "FOCUS",
                            Some((
                                if self.pomodoro.running { "RUN" } else { "WAIT" }.to_owned(),
                                phase_col,
                            )),
                        )
                    }
                    GalleryTile::Calendar => (
                        IconKind::Calendar,
                        "EVENTS",
                        next_event.first().map(|ev| {
                            (format_countdown(ev.start_utc, ev.end_utc, now_utc), accent)
                        }),
                    ),
                    GalleryTile::Battery => (
                        IconKind::Dot,
                        "POWER",
                        self.system_metrics.battery.map(|b| {
                            (b.status.label().to_owned(), Color32::from_rgb(96, 186, 150))
                        }),
                    ),
                };

            let hdr_h = 13.0 * s;
            let hdr_rect =
                Rect::from_min_max(inner.min, Pos2::new(inner.max.x, inner.min.y + hdr_h));
            paint_gallery_header_in_rect(
                &clip,
                hdr_rect,
                hdr_icon,
                hdr_label,
                hdr_badge.as_ref().map(|(t, c)| (t.as_str(), *c)),
                accent,
                muted,
                s,
            );

            let has_bar = matches!(
                tile,
                GalleryTile::Music
                    | GalleryTile::Todo
                    | GalleryTile::Pomodoro
                    | GalleryTile::Battery
            );
            let bar_h = 4.0 * s;
            let body_bottom = if has_bar {
                (inner.bottom() - bar_h - 3.0 * s).max(hdr_rect.bottom() + 4.0 * s)
            } else {
                inner.bottom()
            };
            let body_rect = Rect::from_min_max(
                Pos2::new(inner.left(), hdr_rect.bottom() + 3.0 * s),
                Pos2::new(inner.right(), body_bottom),
            );
            let bar_rect = has_bar.then(|| {
                Rect::from_min_max(
                    Pos2::new(inner.left(), inner.bottom() - bar_h),
                    Pos2::new(inner.right(), inner.bottom()),
                )
            });

            match tile {
                GalleryTile::Clock => {
                    let well_sz = body_rect.height().min(28.0 * s).max(16.0 * s);
                    let w_rect = Rect::from_min_size(
                        Pos2::new(body_rect.left(), body_rect.center().y - well_sz * 0.5),
                        Vec2::splat(well_sz),
                    );
                    clip.rect_filled(
                        w_rect,
                        CornerRadius::same((8.0 * s).round() as u8),
                        hero_well,
                    );
                    if self.config.show_idle_mascot {
                        let glow =
                            (!self.state.paused).then(|| mascot_glow_color(accent, text_col));
                        paint_mascot_fx(
                            &clip,
                            w_rect.shrink(3.5 * s),
                            self.mascot_eye_offset,
                            self.mascot_eye_open,
                            glow,
                            if self.state.paused { muted } else { text_col },
                        );
                    } else {
                        paint_icon(
                            &clip,
                            Rect::from_center_size(w_rect.center(), Vec2::splat(well_sz * 0.5)),
                            IconKind::Timer,
                            accent,
                        );
                    }
                    let tx_x = w_rect.right() + 7.0 * s;
                    let tx_w = (body_rect.right() - tx_x).max(20.0);
                    let time_font = ui_bold_font((14.5 * s).min(tx_w * 0.28).max(10.0 * s));
                    let shown_time = truncate_with_ellipsis(&time_str, |t| {
                        clip.layout_no_wrap(t.to_owned(), time_font.clone(), text_col)
                            .size()
                            .x
                            <= tx_w
                    });
                    let t_galley = clip.layout_no_wrap(shown_time, time_font, text_col);
                    let sub_txt = if self.config.notch_dock_mode {
                        "Top Notch"
                    } else {
                        "Floating Pill"
                    };
                    let s_galley = clip.layout_no_wrap(
                        sub_txt.to_owned(),
                        FontId::proportional(8.3 * s),
                        muted,
                    );
                    let stack_h = t_galley.size().y + 1.0 * s + s_galley.size().y;
                    let top_y = (body_rect.center().y - stack_h * 0.5).max(body_rect.top());
                    clip.galley(Pos2::new(tx_x, top_y), t_galley, Color32::TRANSPARENT);
                    clip.galley(
                        Pos2::new(tx_x, top_y + stack_h - s_galley.size().y),
                        s_galley,
                        Color32::TRANSPARENT,
                    );
                }
                GalleryTile::Notifications => {
                    if let Some(latest) = self.notification_history.front() {
                        let brand = app_brand_color(&latest.app_name, accent);
                        let icon_sz = body_rect.height().min(22.0 * s).max(14.0 * s);
                        let i_rect = Rect::from_min_size(
                            Pos2::new(body_rect.left(), body_rect.center().y - icon_sz * 0.5),
                            Vec2::splat(icon_sz),
                        );
                        paint_notif_avatar_in_rect(
                            &clip,
                            i_rect,
                            &latest.app_name,
                            self.lookup_notif_icon(latest),
                            brand,
                            s,
                        );
                        let tx_x = i_rect.right() + 6.0 * s;
                        let tx_w = (body_rect.right() - tx_x).max(20.0);
                        let ttl_font = ui_bold_font(9.6 * s);
                        let shown_ttl = truncate_with_ellipsis(&latest.title, |t| {
                            clip.layout_no_wrap(t.to_owned(), ttl_font.clone(), text_col)
                                .size()
                                .x
                                <= tx_w
                        });
                        let ttl_gal = clip.layout_no_wrap(shown_ttl, ttl_font, text_col);
                        let sub_src = if latest.body.is_empty() {
                            &latest.app_name
                        } else {
                            &latest.body
                        };
                        let sub_font = FontId::proportional(8.4 * s);
                        let shown_sub = truncate_with_ellipsis(sub_src, |t| {
                            clip.layout_no_wrap(t.to_owned(), sub_font.clone(), muted)
                                .size()
                                .x
                                <= tx_w
                        });
                        let sub_gal = clip.layout_no_wrap(shown_sub, sub_font, muted);
                        let stack_h = ttl_gal.size().y + 1.5 * s + sub_gal.size().y;
                        let top_y = (body_rect.center().y - stack_h * 0.5).max(body_rect.top());
                        let ttl_h = ttl_gal.size().y;
                        clip.galley(Pos2::new(tx_x, top_y), ttl_gal, Color32::TRANSPARENT);
                        clip.galley(
                            Pos2::new(tx_x, top_y + ttl_h + 1.5 * s),
                            sub_gal,
                            Color32::TRANSPARENT,
                        );
                    } else {
                        let g = clip.layout_no_wrap(
                            "No recent alerts".to_owned(),
                            FontId::proportional(9.5 * s),
                            muted,
                        );
                        clip.galley(
                            Pos2::new(body_rect.left(), body_rect.center().y - g.size().y * 0.5),
                            g,
                            Color32::TRANSPARENT,
                        );
                    }
                }
                GalleryTile::Music => {
                    let art_sz = body_rect.height().min(24.0 * s).max(16.0 * s);
                    let a_rect = Rect::from_min_size(
                        Pos2::new(body_rect.left(), body_rect.center().y - art_sz * 0.5),
                        Vec2::splat(art_sz),
                    );
                    let a_rad = CornerRadius::same((6.0 * s).round() as u8);
                    let (headline, subline, frac) = if let Some(track) = &self.media {
                        if let Some(tex) = &self.album_art_tex {
                            clip.add(
                                RectShape::filled(a_rect, a_rad, Color32::WHITE).with_texture(
                                    tex.id(),
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                ),
                            );
                        } else {
                            clip.rect_filled(a_rect, a_rad, hero_well);
                            paint_icon(
                                &clip,
                                Rect::from_center_size(a_rect.center(), Vec2::splat(art_sz * 0.5)),
                                IconKind::MusicNote,
                                accent,
                            );
                        }
                        let pos_ms = self.current_playback_ms(now);
                        let frac = if track.duration_ms > 0 {
                            pos_ms as f32 / track.duration_ms as f32
                        } else {
                            0.0
                        };
                        let sub = if track.artist.is_empty() {
                            track.title.clone()
                        } else {
                            track.artist.clone()
                        };
                        (track.title.clone(), sub, frac)
                    } else {
                        clip.rect_filled(a_rect, a_rad, hero_well);
                        paint_icon(
                            &clip,
                            Rect::from_center_size(a_rect.center(), Vec2::splat(art_sz * 0.5)),
                            IconKind::MusicNote,
                            muted,
                        );
                        ("No media".to_owned(), "Idle".to_owned(), 0.0)
                    };
                    let tx_x = a_rect.right() + 6.0 * s;
                    let tx_w = (body_rect.right() - tx_x).max(20.0);
                    let h_font = ui_bold_font(9.5 * s);
                    let s_font = FontId::proportional(8.3 * s);
                    let h_gal = clip.layout_no_wrap(
                        truncate_with_ellipsis(&headline, |t| {
                            clip.layout_no_wrap(t.to_owned(), h_font.clone(), text_col)
                                .size()
                                .x
                                <= tx_w
                        }),
                        h_font,
                        text_col,
                    );
                    let s_gal = clip.layout_no_wrap(
                        truncate_with_ellipsis(&subline, |t| {
                            clip.layout_no_wrap(t.to_owned(), s_font.clone(), muted)
                                .size()
                                .x
                                <= tx_w
                        }),
                        s_font,
                        muted,
                    );
                    let stack_h = h_gal.size().y + 1.5 * s + s_gal.size().y;
                    let top_y = (body_rect.center().y - stack_h * 0.5).max(body_rect.top());
                    clip.galley(Pos2::new(tx_x, top_y), h_gal, Color32::TRANSPARENT);
                    clip.galley(
                        Pos2::new(tx_x, top_y + stack_h - s_gal.size().y),
                        s_gal,
                        Color32::TRANSPARENT,
                    );
                    if let Some(br) = bar_rect {
                        paint_pill_bar(&clip, br, frac, track_col, accent);
                    }
                }
                GalleryTile::System => {
                    let m = &self.system_metrics;
                    let dsk = m.disk_activity_pct.or(m.disk_used_pct).unwrap_or(0.0);
                    let rows = [
                        ("CPU", m.cpu_pct / 100.0, accent),
                        ("RAM", m.ram_pct / 100.0, Color32::from_rgb(126, 190, 160)),
                        ("DSK", dsk / 100.0, Color32::from_rgb(214, 168, 126)),
                    ];
                    let slot_h = body_rect.height() / 3.0;
                    let lbl_w = 22.0 * s;
                    let val_w = 24.0 * s;
                    for (r_i, (lbl, frac, col)) in rows.into_iter().enumerate() {
                        let cy = body_rect.top() + slot_h * (r_i as f32 + 0.5);
                        clip.text(
                            Pos2::new(body_rect.left(), cy),
                            Align2::LEFT_CENTER,
                            lbl,
                            FontId::proportional(8.0 * s),
                            muted,
                        );
                        let pct_txt = format!("{:.0}%", (frac * 100.0).clamp(0.0, 100.0));
                        clip.text(
                            Pos2::new(body_rect.right(), cy),
                            Align2::RIGHT_CENTER,
                            pct_txt,
                            FontId::monospace(8.0 * s),
                            text_col,
                        );
                        let b_left = body_rect.left() + lbl_w;
                        let b_right = (body_rect.right() - val_w).max(b_left + 10.0);
                        paint_pill_bar(
                            &clip,
                            Rect::from_min_max(
                                Pos2::new(b_left, cy - 2.0 * s),
                                Pos2::new(b_right, cy + 2.0 * s),
                            ),
                            frac,
                            track_col,
                            col,
                        );
                    }
                }
                GalleryTile::Todo => {
                    let total = self.todos.items.len();
                    let pending = self.todos.pending_count();
                    let done = total.saturating_sub(pending);
                    let frac = if total > 0 {
                        done as f32 / total as f32
                    } else {
                        0.0
                    };
                    let next_task = self
                        .todos
                        .items
                        .iter()
                        .find(|t| !t.done)
                        .map(|t| t.text.as_str())
                        .unwrap_or(if total == 0 {
                            "No tasks yet"
                        } else {
                            "All tasks done"
                        });
                    let main_txt = if pending == 0 && total > 0 {
                        "Clear"
                    } else {
                        next_task
                    };
                    let t_font = ui_bold_font(9.6 * s);
                    let s_font = FontId::proportional(8.4 * s);
                    let t_gal = clip.layout_no_wrap(
                        truncate_with_ellipsis(main_txt, |t| {
                            clip.layout_no_wrap(t.to_owned(), t_font.clone(), text_col)
                                .size()
                                .x
                                <= body_rect.width()
                        }),
                        t_font,
                        if pending > 0 { text_col } else { muted },
                    );
                    let s_gal = clip.layout_no_wrap(format!("{pending} pending"), s_font, muted);
                    let stack_h = t_gal.size().y + 1.5 * s + s_gal.size().y;
                    let top_y = (body_rect.center().y - stack_h * 0.5).max(body_rect.top());
                    clip.galley(
                        Pos2::new(body_rect.left(), top_y),
                        t_gal,
                        Color32::TRANSPARENT,
                    );
                    clip.galley(
                        Pos2::new(body_rect.left(), top_y + stack_h - s_gal.size().y),
                        s_gal,
                        Color32::TRANSPARENT,
                    );
                    if let Some(br) = bar_rect {
                        paint_pill_bar(&clip, br, frac, track_col, accent);
                    }
                }
                GalleryTile::Pomodoro => {
                    let phase_col = match self.pomodoro.phase {
                        PomodoroPhase::Focus => accent,
                        PomodoroPhase::ShortBreak => Color32::from_rgb(96, 186, 150),
                        PomodoroPhase::LongBreak => Color32::from_rgb(136, 158, 214),
                    };
                    let t_gal = clip.layout_no_wrap(
                        self.pomodoro.format_mm_ss(),
                        ui_bold_font(13.5 * s),
                        text_col,
                    );
                    let s_gal = clip.layout_no_wrap(
                        self.pomodoro.phase.short_label().to_owned(),
                        FontId::proportional(8.4 * s),
                        muted,
                    );
                    let stack_h = t_gal.size().y + 1.0 * s + s_gal.size().y;
                    let top_y = (body_rect.center().y - stack_h * 0.5).max(body_rect.top());
                    clip.galley(
                        Pos2::new(body_rect.left(), top_y),
                        t_gal,
                        Color32::TRANSPARENT,
                    );
                    clip.galley(
                        Pos2::new(body_rect.left(), top_y + stack_h - s_gal.size().y),
                        s_gal,
                        Color32::TRANSPARENT,
                    );
                    if let Some(br) = bar_rect {
                        paint_pill_bar(&clip, br, self.pomodoro.progress(), track_col, phase_col);
                    }
                }
                GalleryTile::Calendar => {
                    if let Some(ev) = next_event.first() {
                        let e_font = ui_bold_font(9.6 * s);
                        let shown = truncate_with_ellipsis(&ev.summary, |t| {
                            clip.layout_no_wrap(t.to_owned(), e_font.clone(), text_col)
                                .size()
                                .x
                                <= body_rect.width()
                        });
                        let e_gal = clip.layout_no_wrap(shown, e_font, text_col);
                        let t_gal = clip.layout_no_wrap(
                            format_local_time(ev.start_utc),
                            FontId::proportional(8.4 * s),
                            accent,
                        );
                        let stack_h = e_gal.size().y + 1.5 * s + t_gal.size().y;
                        let top_y = (body_rect.center().y - stack_h * 0.5).max(body_rect.top());
                        clip.galley(
                            Pos2::new(body_rect.left(), top_y),
                            e_gal,
                            Color32::TRANSPARENT,
                        );
                        clip.galley(
                            Pos2::new(body_rect.left(), top_y + stack_h - t_gal.size().y),
                            t_gal,
                            Color32::TRANSPARENT,
                        );
                    } else {
                        let msg = if self.config.google_calendar_ics_url.is_empty() {
                            "Connect .ics"
                        } else {
                            "No events"
                        };
                        let g = clip.layout_no_wrap(
                            msg.to_owned(),
                            FontId::proportional(9.5 * s),
                            muted,
                        );
                        clip.galley(
                            Pos2::new(body_rect.left(), body_rect.center().y - g.size().y * 0.5),
                            g,
                            Color32::TRANSPARENT,
                        );
                    }
                }
                GalleryTile::Battery => {
                    if let Some(b) = self.system_metrics.battery {
                        let bat_col = Color32::from_rgb(96, 186, 150);
                        let b_gal = clip.layout_no_wrap(
                            format!("{}%", b.percent),
                            ui_bold_font(14.0 * s),
                            text_col,
                        );
                        let top_y =
                            (body_rect.center().y - b_gal.size().y * 0.5).max(body_rect.top());
                        clip.galley(
                            Pos2::new(body_rect.left(), top_y),
                            b_gal,
                            Color32::TRANSPARENT,
                        );
                        if let Some(br) = bar_rect {
                            paint_pill_bar(&clip, br, b.percent as f32 / 100.0, track_col, bat_col);
                        }
                    } else {
                        let g = clip.layout_no_wrap(
                            "AC / No battery".to_owned(),
                            FontId::proportional(9.2 * s),
                            muted,
                        );
                        clip.galley(
                            Pos2::new(body_rect.left(), body_rect.center().y - g.size().y * 0.5),
                            g,
                            Color32::TRANSPARENT,
                        );
                        if let Some(br) = bar_rect {
                            paint_pill_bar(&clip, br, 0.0, track_col, muted);
                        }
                    }
                }
            }

            if target_tab.is_some() && resp.clicked() {
                switch_target = target_tab;
            }
        }

        if let Some(next_tab) = switch_target {
            self.switch_tab(next_tab, 1.0, &ctx);
        }
    }

    /// Small vector ring used by CPU / RAM / Disk and the Battery tab.
    /// `highlight` optionally paints a moving segment (used while charging).
    fn paint_donut(
        ui: &egui::Ui,
        center: Pos2,
        radius: f32,
        thickness: f32,
        frac: f32,
        fill: Color32,
        highlight: Option<f32>,
    ) {
        let painter = ui.painter();
        let track = Color32::from_rgba_unmultiplied(255, 255, 255, 30);
        painter.circle_stroke(center, radius, Stroke::new(thickness, track));
        let frac = frac.clamp(0.0, 1.0);
        if frac > 0.0 {
            let steps = (48.0 * frac).ceil() as usize;
            let stroke = Stroke::new(thickness, fill);
            for i in 0..steps {
                let start = -TAU / 4.0 + TAU * frac * (i as f32 / steps as f32);
                let end = -TAU / 4.0 + TAU * frac * ((i + 1) as f32 / steps as f32);
                painter.line_segment(
                    [
                        center + Vec2::new(start.cos(), start.sin()) * radius,
                        center + Vec2::new(end.cos(), end.sin()) * radius,
                    ],
                    stroke,
                );
            }
        }
        if let Some(phase) = highlight {
            let angle = -TAU / 4.0 + TAU * phase.rem_euclid(1.0);
            let pos = center + Vec2::new(angle.cos(), angle.sin()) * radius;
            painter.circle_filled(pos, thickness * 0.85, Color32::WHITE);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_stat_donut(
        ui: &mut egui::Ui,
        icon: IconKind,
        label: &str,
        value: &str,
        frac: f32,
        col: Color32,
        s: f32,
    ) {
        let size = Vec2::new(66.0 * s, 80.0 * s);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let center = rect.min + Vec2::new(rect.width() * 0.5, 31.0 * s);
        Self::paint_donut(ui, center, 24.0 * s, 5.0 * s, frac, col, None);
        let painter = ui.painter();
        painter.text(
            center,
            Align2::CENTER_CENTER,
            value,
            ui_bold_font(10.5 * s),
            col,
        );
        let lbl_galley = painter.layout_no_wrap(
            label.to_owned(),
            FontId::proportional(10.0 * s),
            Color32::from_gray(185),
        );
        let icon_sz = 10.5 * s;
        let gap = 3.5 * s;
        let total_w = icon_sz + gap + lbl_galley.size().x;
        let x0 = rect.center().x - total_w * 0.5;
        let cy = rect.top() + 69.0 * s;
        paint_icon(
            painter,
            Rect::from_center_size(Pos2::new(x0 + icon_sz * 0.5, cy), Vec2::splat(icon_sz)),
            icon,
            col,
        );
        painter.galley(
            Pos2::new(x0 + icon_sz + gap, cy - lbl_galley.size().y * 0.5),
            lbl_galley,
            Color32::TRANSPARENT,
        );
    }

    fn render_system_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let m = &self.system_metrics;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0 * s;
            Self::render_stat_donut(
                ui,
                IconKind::Cpu,
                "CPU",
                &format!("{:.0}%", m.cpu_pct),
                m.cpu_pct / 100.0,
                accent,
                s,
            );
            Self::render_stat_donut(
                ui,
                IconKind::Memory,
                "RAM",
                &format!("{:.0}%", m.ram_pct),
                m.ram_pct / 100.0,
                Color32::from_rgb(126, 190, 160),
                s,
            );
            let disk_pct = m.disk_activity_pct.or(m.disk_used_pct).unwrap_or(0.0);
            let disk_label = if cfg!(target_os = "linux") {
                "Disk /"
            } else {
                "Disk C:"
            };
            Self::render_stat_donut(
                ui,
                IconKind::Disk,
                disk_label,
                &format!("{disk_pct:.0}%"),
                disk_pct / 100.0,
                Color32::from_rgb(214, 168, 126),
                s,
            );
        });
        ui.add_space(2.0 * s);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0 * s;
            let (net_i, _) = ui.allocate_exact_size(Vec2::splat(11.5 * s), Sense::hover());
            paint_icon(ui.painter(), net_i, IconKind::Dot, accent);
            ui.label(
                RichText::new("Network")
                    .font(FontId::proportional(11.0 * s))
                    .color(muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0 * s;
                ui.label(
                    RichText::new(crate::sysmon::format_speed_bps(m.net_up_bps))
                        .font(FontId::proportional(11.0 * s))
                        .color(text_col),
                );
                let (up_i, _) = ui.allocate_exact_size(Vec2::splat(10.5 * s), Sense::hover());
                paint_icon(ui.painter(), up_i, IconKind::ArrowUp, accent);
                ui.add_space(4.0 * s);
                ui.label(
                    RichText::new(crate::sysmon::format_speed_bps(m.net_down_bps))
                        .font(FontId::proportional(11.0 * s))
                        .color(text_col),
                );
                let (dn_i, _) = ui.allocate_exact_size(Vec2::splat(10.5 * s), Sense::hover());
                paint_icon(
                    ui.painter(),
                    dn_i,
                    IconKind::ArrowDown,
                    Color32::from_rgb(96, 186, 150),
                );
            });
        });

        ui.add_space(6.0 * s);
        ui.horizontal(|ui| {
            let tb_label = if self.config.taskbar_monitor_enabled {
                "Taskbar: On"
            } else {
                "Taskbar: Off"
            };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(tb_label)
                            .font(FontId::proportional(10.5 * s))
                            .color(text_col),
                    )
                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 20))
                    .corner_radius(CornerRadius::same(10)),
                )
                .clicked()
            {
                let prev_lyric_opts = (
                    self.config.romaji_lyrics,
                    self.config.karaoke_lyric_mode,
                    self.config.translate_lyrics,
                    self.config.lyric_translate_lang.clone(),
                );
                self.config.taskbar_monitor_enabled = !self.config.taskbar_monitor_enabled;
                self.sync_runtime_config(ctx, prev_lyric_opts, Instant::now());
                let _ = self.config.save();
                ctx.request_repaint();
            }

            if self.config.show_battery
                && ui
                    .add(
                        egui::Button::new(
                            RichText::new("Battery")
                                .font(FontId::proportional(10.5 * s))
                                .color(muted),
                        )
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                        .corner_radius(CornerRadius::same(10)),
                    )
                    .clicked()
            {
                self.switch_tab(IslandTab::Battery, -1.0, ctx);
            }

            if ui
                .add(
                    egui::Button::new(
                        RichText::new("Settings")
                            .font(FontId::proportional(10.5 * s))
                            .color(muted),
                    )
                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                    .corner_radius(CornerRadius::same(10)),
                )
                .clicked()
            {
                self.open_settings(SettingsTab::System, ctx);
            }
        });
    }

    /// Dedicated Battery tab. Charging is shown with a moving highlight and orbiting dot;
    /// desktops without a battery get an explicit empty state.
    fn render_battery_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let Some(bat) = self.system_metrics.battery else {
            ui.vertical_centered(|ui| {
                ui.add_space(14.0 * s);
                ui.label(
                    RichText::new("No battery detected")
                        .font(FontId::proportional(12.0 * s))
                        .color(muted),
                );
            });
            return;
        };

        let charging = matches!(
            bat.status,
            crate::battery::BatteryStatus::Charging | crate::battery::BatteryStatus::Full
        );
        let time = ui.input(|i| i.time) as f32;
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(92.0 * s, 92.0 * s), Sense::hover());
            let center = rect.center();
            let radius = 34.0 * s;
            Self::paint_donut(
                ui,
                center,
                radius,
                7.0 * s,
                bat.percent as f32 / 100.0,
                accent,
                charging.then_some(time * 0.35),
            );
            ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                format!("{}%", bat.percent),
                FontId::proportional(18.0 * s),
                text_col,
            );
            if charging {
                // Orbiting dot makes the charging state visible even without a percentage change.
                let angle = -TAU / 4.0 + time * 2.6;
                let dot = center + Vec2::new(angle.cos(), angle.sin()) * radius;
                ui.painter()
                    .circle_filled(dot, 3.5 * s, Color32::from_rgb(226, 198, 144));
            }
        });

        ui.add_space(2.0 * s);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(bat.status.label())
                    .font(FontId::proportional(12.0 * s))
                    .color(if charging {
                        Color32::from_rgb(220, 186, 132)
                    } else {
                        text_col
                    }),
            );
            if let Some(secs) = bat.remaining_secs {
                ui.label(
                    RichText::new(format!(
                        "{} remaining",
                        crate::battery::format_remaining(secs)
                    ))
                    .font(FontId::proportional(10.5 * s))
                    .color(muted),
                );
            }
        });

        ui.add_space(6.0 * s);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("Settings")
                            .font(FontId::proportional(10.5 * s))
                            .color(muted),
                    )
                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                    .corner_radius(CornerRadius::same(10)),
                )
                .clicked()
            {
                self.open_settings(SettingsTab::Battery, ctx);
            }
        });
    }

    fn render_pomodoro_tab(&mut self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let phase_col = match self.pomodoro.phase {
            PomodoroPhase::Focus => accent,
            PomodoroPhase::ShortBreak => Color32::from_rgb(96, 186, 150),
            PomodoroPhase::LongBreak => Color32::from_rgb(136, 158, 214),
        };

        ui.vertical_centered(|ui| {
            ui.add_space(4.0 * s);
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(
                    phase_col.r(),
                    phase_col.g(),
                    phase_col.b(),
                    34,
                ))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(10, 3))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(self.pomodoro.phase.label())
                            .font(FontId::proportional(11.0 * s))
                            .color(phase_col)
                            .strong(),
                    );
                });

            ui.add_space(6.0 * s);
            ui.label(
                RichText::new(self.pomodoro.format_mm_ss())
                    .font(FontId::monospace(30.0 * s))
                    .color(text_col)
                    .strong(),
            );
            ui.add_space(6.0 * s);

            let bar_w = (ui.available_width() - 40.0 * s).max(120.0);
            let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, 5.0 * s), Sense::hover());
            ui.painter().rect_filled(
                bar_rect,
                CornerRadius::same(3),
                Color32::from_rgba_unmultiplied(255, 255, 255, 26),
            );
            let fill_w = (bar_rect.width() * self.pomodoro.progress()).max(0.0);
            if fill_w > 0.5 {
                let fill_rect =
                    Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, bar_rect.height()));
                ui.painter()
                    .rect_filled(fill_rect, CornerRadius::same(3), phase_col);
            }

            ui.add_space(10.0 * s);
            let gap = 6.0 * s;
            let total_btn_w = (88.0 + 66.0 + 60.0) * s + gap * 2.0;
            ui.allocate_ui_with_layout(
                Vec2::new(total_btn_w, 28.0 * s),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let toggle_label = if self.pomodoro.running {
                        "Pause"
                    } else {
                        "Start"
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(toggle_label)
                                    .font(FontId::proportional(12.0 * s))
                                    .color(Color32::WHITE)
                                    .strong(),
                            )
                            .min_size(Vec2::new(88.0 * s, 26.0 * s))
                            .fill(phase_col)
                            .corner_radius(CornerRadius::same(13)),
                        )
                        .clicked()
                    {
                        self.pomodoro.toggle(now);
                        ui.ctx().request_repaint();
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Reset")
                                    .font(FontId::proportional(11.5 * s))
                                    .color(text_col),
                            )
                            .min_size(Vec2::new(66.0 * s, 26.0 * s))
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 22))
                            .corner_radius(CornerRadius::same(13)),
                        )
                        .clicked()
                    {
                        self.pomodoro.reset();
                        ui.ctx().request_repaint();
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Skip")
                                    .font(FontId::proportional(11.5 * s))
                                    .color(muted),
                            )
                            .min_size(Vec2::new(60.0 * s, 26.0 * s))
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                            .corner_radius(CornerRadius::same(13)),
                        )
                        .clicked()
                    {
                        self.pomodoro.skip();
                        ui.ctx().request_repaint();
                    }
                },
            );

            ui.add_space(8.0 * s);
            let cycle_done = self.pomodoro.completed_focus % 4;
            ui.label(
                RichText::new(format!(
                    "Sessions: {}  ({}/4 in cycle)",
                    self.pomodoro.completed_focus, cycle_done
                ))
                .font(FontId::proportional(10.5 * s))
                .color(muted),
            );
        });
    }

    fn render_music_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
        let accent_rgb = self.effective_accent_rgb();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let align = self.text_align();
        let s = self.config.scale;

        self.ensure_bold_lyrics_fonts_loaded(ctx);
        self.ensure_lyrics_requested_for_media(ctx);
        self.sync_album_art_texture(ctx);
        let pos_ms = self.current_playback_ms(now);
        let lyric_ms = self.current_lyric_ms(now);
        let Some(track) = self.media.as_ref() else {
            let avail_w = ui.available_width();
            render_aligned_wrapped_text(
                ui,
                "Play a song on Spotify or any media app...",
                FontId::proportional(11.5 * s),
                muted,
                avail_w,
                align,
            );
            return;
        };

        let track_key = (track.artist.clone(), track.title.clone());
        let mut new_scroll_focus: Option<(String, String, usize)> = None;
        let mut pending_seek: Option<u64> = None;
        let mut pending_shuffle: Option<bool> = None;
        let mut pending_repeat: Option<RepeatMode> = None;
        let mut retry_lyrics = false;
        let total_w = ui.available_width();
        let col_gap = 14.0 * s;
        // Equal columns keep the two halves visually balanced regardless of card width.
        let left_w = ((total_w - col_gap) * 0.5).max(148.0 * s);
        let right_w = (total_w - left_w - col_gap).max(120.0 * s);
        let body_h = ui.available_height().max(140.0 * s);

        ui.horizontal_top(|ui| {
            // Left column: Album Art, centered Title & Artist, Playbar, and Transport Controls.
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, body_h),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    let art_size = (left_w - 26.0 * s).clamp(84.0 * s, 118.0 * s);
                    let (art_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(art_size), Sense::hover());
                    let art_radius = CornerRadius::same((12.0 * s).round() as u8);

                    if let Some(tex) = &self.album_art_tex {
                        let shape = RectShape::filled(art_rect, art_radius, Color32::WHITE)
                            .with_texture(
                                tex.id(),
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            );
                        ui.painter().add(shape);
                    } else {
                        ui.painter().rect_filled(
                            art_rect,
                            art_radius,
                            Color32::from_rgba_unmultiplied(45, 42, 44, 220),
                        );
                        ui.painter().circle_stroke(
                            art_rect.center(),
                            art_size * 0.24,
                            Stroke::new(2.0 * s, accent),
                        );
                        paint_icon(
                            ui.painter(),
                            Rect::from_center_size(art_rect.center(), Vec2::splat(22.0 * s)),
                            IconKind::MusicNote,
                            accent,
                        );
                    }

                    ui.add_space(6.0 * s);
                    render_aligned_truncated_text(
                        ui,
                        &track.title,
                        FontId::proportional(13.0 * s),
                        text_col,
                        align,
                    );
                    if !track.artist.is_empty() {
                        render_aligned_truncated_text(
                            ui,
                            &track.artist,
                            FontId::proportional(11.0 * s),
                            muted,
                            align,
                        );
                    }

                    ui.add_space(6.0 * s);
                    if let Some(seek_ms) =
                        render_playbar(ui, pos_ms, track.duration_ms, accent, muted, s)
                    {
                        let _ = self
                            .channels
                            .media_cmd_tx
                            .send(MediaCommand::SeekMs(seek_ms));
                        pending_seek = Some(seek_ms);
                    }

                    ui.add_space(4.0 * s);
                    ui.horizontal(|ui| {
                        let btn_sz = Vec2::splat(25.0 * s);
                        let play_sz = Vec2::splat(29.0 * s);
                        let gap = 3.0 * s;
                        // Five controls also produce five gaps; count the trailing one so
                        // the group lands exactly in the middle of the column.
                        let controls_w = btn_sz.x * 4.0 + play_sz.x + gap * 5.0;
                        let side = ((ui.available_width() - controls_w) * 0.5).max(0.0);
                        ui.add_space(side);
                        ui.spacing_mut().item_spacing.x = gap;

                        let shuf_col = if track.shuffle { accent } else { muted };
                        let btn_rad = CornerRadius::same((8.0 * s).round() as u8);
                        if icon_button(
                            ui,
                            IconKind::Shuffle,
                            btn_sz,
                            13.0 * s,
                            shuf_col,
                            Color32::TRANSPARENT,
                            btn_rad,
                        )
                        .on_hover_text("Shuffle")
                        .clicked()
                        {
                            let next_shuf = !track.shuffle;
                            let _ = self
                                .channels
                                .media_cmd_tx
                                .send(MediaCommand::SetShuffle(next_shuf));
                            pending_shuffle = Some(next_shuf);
                        }

                        if icon_button(
                            ui,
                            IconKind::Previous,
                            btn_sz,
                            14.0 * s,
                            text_col,
                            Color32::TRANSPARENT,
                            btn_rad,
                        )
                        .on_hover_text("Previous")
                        .clicked()
                        {
                            let _ = self.channels.media_cmd_tx.send(MediaCommand::Previous);
                        }

                        let play_kind = if track.is_playing {
                            IconKind::Pause
                        } else {
                            IconKind::Play
                        };
                        if icon_button(
                            ui,
                            play_kind,
                            play_sz,
                            15.5 * s,
                            Color32::WHITE,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                            CornerRadius::same((14.5 * s).round() as u8),
                        )
                        .on_hover_text("Play / Pause")
                        .clicked()
                        {
                            let _ = self
                                .channels
                                .media_cmd_tx
                                .send(MediaCommand::TogglePlayPause);
                        }

                        if icon_button(
                            ui,
                            IconKind::Next,
                            btn_sz,
                            14.0 * s,
                            text_col,
                            Color32::TRANSPARENT,
                            btn_rad,
                        )
                        .on_hover_text("Next")
                        .clicked()
                        {
                            let _ = self.channels.media_cmd_tx.send(MediaCommand::Next);
                        }

                        let (rep_kind, rep_col) = match track.repeat {
                            RepeatMode::Off => (IconKind::Repeat, muted),
                            RepeatMode::All => (IconKind::Repeat, accent),
                            RepeatMode::One => (IconKind::RepeatOne, accent),
                        };
                        if icon_button(
                            ui,
                            rep_kind,
                            btn_sz,
                            13.0 * s,
                            rep_col,
                            Color32::TRANSPARENT,
                            btn_rad,
                        )
                        .on_hover_text("Repeat (Off / All / One)")
                        .clicked()
                        {
                            let next_rep = track.repeat.next();
                            let _ = self
                                .channels
                                .media_cmd_tx
                                .send(MediaCommand::SetRepeat(next_rep));
                            pending_repeat = Some(next_rep);
                        }
                    });
                },
            );

            ui.add_space(col_gap);

            // Right column: All lyrics with bold active karaoke focus & blurred prev/next lines.
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, body_h),
                egui::Layout::top_down(align),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    // Pin the wrap width to the allocated column so a scrollbar appearing
                    // cannot shift centered lines sideways.
                    let wrap_w = right_w.max(60.0);
                    egui::ScrollArea::vertical()
                        .id_salt("music_lyrics_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(align), |ui| {
                                match &self.lyrics {
                                    LyricsData::Loading => {
                                        render_aligned_wrapped_text(
                                            ui,
                                            "Fetching lyrics...",
                                            FontId::proportional(12.0 * s),
                                            muted,
                                            wrap_w,
                                            align,
                                        );
                                    }
                                    LyricsData::NotFound => {
                                        render_aligned_wrapped_text(
                                            ui,
                                            "Lyrics not found for this track",
                                            FontId::proportional(12.0 * s),
                                            muted,
                                            wrap_w,
                                            align,
                                        );
                                        ui.add_space(6.0 * s);
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("Retry Fetch")
                                                        .font(FontId::proportional(10.5 * s))
                                                        .color(text_col),
                                                )
                                                .fill(Color32::from_rgba_unmultiplied(
                                                    255, 255, 255, 18,
                                                ))
                                                .corner_radius(CornerRadius::same(7)),
                                            )
                                            .on_hover_cursor(CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            retry_lyrics = true;
                                        }
                                    }
                                    LyricsData::Synced(lines) => {
                                        if lines.is_empty() {
                                            render_aligned_wrapped_text(
                                                ui,
                                                "No synced lyrics for this track",
                                                FontId::proportional(12.0 * s),
                                                muted,
                                                wrap_w,
                                                align,
                                            );
                                            return;
                                        }
                                        let active = active_karaoke(&self.lyrics, lyric_ms);
                                        let curr_idx = active.as_ref().map(|k| k.line_index);
                                        let active_prog =
                                            active.as_ref().map(|k| k.progress).unwrap_or(0.0);
                                        let dim_white =
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 90);
                                        let lit_romaji = Color32::from_rgb(
                                            accent_rgb[0],
                                            accent_rgb[1],
                                            accent_rgb[2],
                                        );
                                        let dim_romaji = Color32::from_rgba_unmultiplied(
                                            (accent_rgb[0] as u16 / 2) as u8,
                                            (accent_rgb[1] as u16 / 2) as u8,
                                            (accent_rgb[2] as u16 / 2) as u8,
                                            135,
                                        );
                                        let orig_prog = if self.config.karaoke_lyrics {
                                            active_prog
                                        } else {
                                            1.0
                                        };
                                        let pad_v = (body_h * 0.28).max(16.0 * s);
                                        ui.add_space(pad_v);
                                        for (idx, line) in lines.iter().enumerate() {
                                            let (primary, romaji_sub, trans_sub) =
                                                resolve_lyric_line_texts(
                                                    line.text.as_str(),
                                                    line.romaji.as_deref(),
                                                    line.translation.as_deref(),
                                                    self.config.karaoke_lyric_mode,
                                                    self.config.romaji_lyrics,
                                                    self.config.translate_lyrics,
                                                );
                                            if Some(idx) == curr_idx {
                                                let resp = render_wrapped_karaoke(
                                                    ui,
                                                    primary,
                                                    orig_prog,
                                                    lyrics_bold_font(14.5 * s),
                                                    text_col,
                                                    dim_white,
                                                    wrap_w,
                                                    align,
                                                );
                                                if let Some(sub) = romaji_sub {
                                                    ui.add_space(2.0 * s);
                                                    render_wrapped_karaoke(
                                                        ui,
                                                        sub,
                                                        orig_prog,
                                                        FontId::proportional(11.5 * s),
                                                        lit_romaji,
                                                        dim_romaji,
                                                        wrap_w,
                                                        align,
                                                    );
                                                }
                                                if let Some(tr) = trans_sub {
                                                    ui.add_space(2.0 * s);
                                                    render_aligned_wrapped_text(
                                                        ui,
                                                        tr,
                                                        FontId::proportional(10.5 * s),
                                                        Color32::from_gray(215),
                                                        wrap_w,
                                                        align,
                                                    );
                                                }
                                                let want_focus =
                                                    (track_key.0.clone(), track_key.1.clone(), idx);
                                                if self.music_scroll_focus_key.as_ref()
                                                    != Some(&want_focus)
                                                {
                                                    resp.scroll_to_me(Some(egui::Align::Center));
                                                    new_scroll_focus = Some(want_focus);
                                                }
                                            } else {
                                                let dist = curr_idx.map_or(1, |c| idx.abs_diff(c));
                                                let (blur_r, fade) = match dist {
                                                    1 => (1.85 * s, 0.72),
                                                    2 => (2.40 * s, 0.52),
                                                    _ => (2.80 * s, 0.36),
                                                };
                                                render_blurred_aligned_text(
                                                    ui,
                                                    primary,
                                                    lyrics_bold_font(13.0 * s),
                                                    text_col,
                                                    wrap_w,
                                                    align,
                                                    blur_r,
                                                    fade,
                                                );
                                                if let Some(sub) = romaji_sub {
                                                    render_blurred_aligned_text(
                                                        ui,
                                                        sub,
                                                        FontId::proportional(10.5 * s),
                                                        lit_romaji,
                                                        wrap_w,
                                                        align,
                                                        blur_r * 0.80,
                                                        fade * 0.85,
                                                    );
                                                }
                                                if let Some(tr) = trans_sub {
                                                    render_blurred_aligned_text(
                                                        ui,
                                                        tr,
                                                        FontId::proportional(10.0 * s),
                                                        Color32::from_gray(215),
                                                        wrap_w,
                                                        align,
                                                        blur_r * 0.80,
                                                        fade * 0.80,
                                                    );
                                                }
                                            }
                                            ui.add_space(7.0 * s);
                                        }
                                        ui.add_space(pad_v);
                                    }
                                    LyricsData::Plain(lines) => {
                                        let lit_romaji = Color32::from_rgb(
                                            accent_rgb[0],
                                            accent_rgb[1],
                                            accent_rgb[2],
                                        );
                                        for line in lines.iter() {
                                            let (primary, romaji_sub, trans_sub) =
                                                resolve_lyric_line_texts(
                                                    line.text.as_str(),
                                                    line.romaji.as_deref(),
                                                    line.translation.as_deref(),
                                                    self.config.karaoke_lyric_mode,
                                                    self.config.romaji_lyrics,
                                                    self.config.translate_lyrics,
                                                );
                                            render_aligned_wrapped_text(
                                                ui,
                                                primary,
                                                lyrics_bold_font(12.5 * s),
                                                text_col,
                                                wrap_w,
                                                align,
                                            );
                                            if let Some(sub) = romaji_sub {
                                                render_aligned_wrapped_text(
                                                    ui,
                                                    sub,
                                                    FontId::proportional(10.5 * s),
                                                    lit_romaji,
                                                    wrap_w,
                                                    align,
                                                );
                                            }
                                            if let Some(tr) = trans_sub {
                                                render_aligned_wrapped_text(
                                                    ui,
                                                    tr,
                                                    FontId::proportional(10.0 * s),
                                                    Color32::from_gray(215),
                                                    wrap_w,
                                                    align,
                                                );
                                            }
                                            ui.add_space(6.0 * s);
                                        }
                                    }
                                }
                            });
                        });
                },
            );
        });
        if let Some(focus) = new_scroll_focus {
            self.music_scroll_focus_key = Some(focus);
        }
        if let Some(m) = &mut self.media {
            if let Some(seek_ms) = pending_seek {
                m.position_ms = seek_ms;
                m.updated_at = now;
                self.pending_user_seek_until = Some(now + Duration::from_millis(900));
            }
            if let Some(next_shuf) = pending_shuffle {
                m.shuffle = next_shuf;
            }
            if let Some(next_rep) = pending_repeat {
                m.repeat = next_rep;
            }
        }
        if retry_lyrics {
            self.request_lyrics_for_current();
        }
    }

    fn render_todo_tab(&mut self, ui: &mut egui::Ui) {
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let align = self.text_align();
        let s = self.config.scale;

        ui.horizontal(|ui| {
            let total = self.todos.items.len();
            let pending = self.todos.pending_count();
            ui.label(
                RichText::new(format!("{pending} pending • {} done", total - pending))
                    .font(FontId::proportional(11.0 * s))
                    .color(muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("+ Manage / Add")
                                .font(FontId::proportional(10.5 * s))
                                .color(text_col),
                        )
                        .min_size(Vec2::new(92.0 * s, 21.0 * s))
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 16))
                        .corner_radius(CornerRadius::same(8)),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.open_settings(SettingsTab::Todo, ui.ctx());
                }
            });
        });
        let total = self.todos.items.len();
        if total > 0 {
            let done = total - self.todos.pending_count();
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 3.0 * s), Sense::hover());
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(2),
                Color32::from_rgba_unmultiplied(255, 255, 255, 24),
            );
            let filled = Rect::from_min_size(
                rect.min,
                Vec2::new(rect.width() * (done as f32 / total as f32), rect.height()),
            );
            ui.painter()
                .rect_filled(filled, CornerRadius::same(2), self.effective_accent());
            ui.add_space(4.0 * s);
        }

        let mut toggle_id = None;
        let mut remove_id = None;
        let mut dirty = false;

        egui::ScrollArea::vertical()
            .max_height(84.0 * s)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.todos.items.is_empty() {
                    let avail_w = ui.available_width();
                    render_aligned_wrapped_text(
                        ui,
                        "No tasks yet. Click '+ Manage / Add'.",
                        FontId::proportional(11.0 * s),
                        muted,
                        avail_w,
                        align,
                    );
                }
                for item in &self.todos.items {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(
                            255,
                            255,
                            255,
                            if item.done { 8 } else { 16 },
                        ))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let mut done = item.done;
                                let label = if item.done {
                                    RichText::new(&item.text)
                                        .font(FontId::proportional(11.5 * s))
                                        .color(muted)
                                        .strikethrough()
                                } else {
                                    RichText::new(&item.text)
                                        .font(FontId::proportional(11.5 * s))
                                        .color(text_col)
                                };
                                if ui
                                    .checkbox(&mut done, label)
                                    .on_hover_cursor(CursorIcon::PointingHand)
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
                                            Vec2::splat(20.0 * s),
                                            10.0 * s,
                                            muted,
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 12),
                                            CornerRadius::same(6),
                                        )
                                        .on_hover_text("Remove task")
                                        .clicked()
                                        {
                                            remove_id = Some(item.id);
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(2.0 * s);
                }
            });

        if let Some(id) = toggle_id {
            self.todos.toggle(id);
            dirty = true;
        }
        if let Some(id) = remove_id {
            self.todos.remove(id);
            dirty = true;
        }
        if dirty && let Err(err) = self.todos.save() {
            eprintln!("Failed to save todos: {err}");
        }
    }

    fn render_calendar_tab(&mut self, ui: &mut egui::Ui) {
        use chrono::TimeZone;

        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let now_utc = chrono::Utc::now().timestamp();
        let today = chrono::Local::now().date_naive();
        let (vy, vm) = self.calendar_view_ym;
        let (first_offset, days_in_month) = calendar_month_cells(vy, vm);

        let event_days: HashSet<u32> = self
            .calendar_events
            .iter()
            .filter_map(|ev| chrono::Local.timestamp_opt(ev.start_utc, 0).single())
            .filter_map(|dt| {
                let d = dt.date_naive();
                (d.year() == vy && d.month() == vm).then_some(d.day())
            })
            .collect();

        let avail_w = ui.available_width().max(260.0 * s);
        let avail_h = ui.available_height().max(118.0 * s);
        let gap = 10.0 * s;
        let cal_w = (avail_w * 0.45).clamp(156.0 * s, 198.0 * s);
        let right_w = (avail_w - cal_w - gap).max(120.0 * s);
        let card_bg = Color32::from_rgba_unmultiplied(255, 255, 255, 10);
        let card_rad = CornerRadius::same((11.0 * s).round() as u8);

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;

            // Left Column: Interactive Month Calendar Component
            let (cal_rect, _) = ui.allocate_exact_size(Vec2::new(cal_w, avail_h), Sense::hover());
            ui.painter().rect_filled(cal_rect, card_rad, card_bg);
            ui.painter().rect_stroke(
                cal_rect,
                card_rad,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
                StrokeKind::Inside,
            );
            let cal_inner = cal_rect.shrink2(Vec2::new(7.0 * s, 5.0 * s));

            let nav_h = 18.0 * s;
            let prev_rect = Rect::from_min_size(cal_inner.min, Vec2::new(18.0 * s, nav_h));
            let next_rect = Rect::from_min_size(
                Pos2::new(cal_inner.right() - 18.0 * s, cal_inner.top()),
                Vec2::new(18.0 * s, nav_h),
            );
            let prev_resp = ui
                .interact(prev_rect, ui.scope_id().with("cal_prev"), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let next_resp = ui
                .interact(next_rect, ui.scope_id().with("cal_next"), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            paint_icon(
                ui.painter(),
                prev_rect.shrink(3.5 * s),
                IconKind::ChevronLeft,
                if prev_resp.hovered() { text_col } else { muted },
            );
            paint_icon(
                ui.painter(),
                next_rect.shrink(3.5 * s),
                IconKind::ChevronRight,
                if next_resp.hovered() { text_col } else { muted },
            );

            let month_title = format!("{} {}", month_short_name(vm), vy);
            let title_rect = Rect::from_min_max(
                Pos2::new(prev_rect.right(), cal_inner.top()),
                Pos2::new(next_rect.left(), cal_inner.top() + nav_h),
            );
            let title_resp = ui
                .interact(title_rect, ui.scope_id().with("cal_today"), Sense::click())
                .on_hover_text("Jump to Today")
                .on_hover_cursor(CursorIcon::PointingHand);
            ui.painter().text(
                title_rect.center(),
                Align2::CENTER_CENTER,
                month_title,
                ui_bold_font(10.2 * s),
                text_col,
            );

            if prev_resp.clicked() {
                self.calendar_view_ym = step_year_month(vy, vm, -1);
                self.calendar_selected_day = None;
            } else if next_resp.clicked() {
                self.calendar_view_ym = step_year_month(vy, vm, 1);
                self.calendar_selected_day = None;
            } else if title_resp.clicked() {
                self.calendar_view_ym = (today.year(), today.month());
                self.calendar_selected_day = Some((today.year(), today.month(), today.day()));
            }

            let wk_top = cal_inner.top() + nav_h + 2.0 * s;
            let wk_h = 12.0 * s;
            let cell_w = cal_inner.width() / 7.0;
            for (i, wk) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
                ui.painter().text(
                    Pos2::new(
                        cal_inner.left() + cell_w * (i as f32 + 0.5),
                        wk_top + wk_h * 0.5,
                    ),
                    Align2::CENTER_CENTER,
                    *wk,
                    ui_bold_font(7.8 * s),
                    muted,
                );
            }

            let grid_top = wk_top + wk_h + 2.0 * s;
            let row_h = ((cal_inner.bottom() - grid_top) / 6.0).clamp(11.0 * s, 19.0 * s);
            for day in 1..=days_in_month {
                let idx = first_offset + (day as usize - 1);
                let col = (idx % 7) as f32;
                let row = (idx / 7) as f32;
                if row >= 6.0 {
                    break;
                }
                let cell_rect = Rect::from_min_size(
                    Pos2::new(cal_inner.left() + col * cell_w, grid_top + row * row_h),
                    Vec2::new(cell_w, row_h),
                );
                let pill_sz = Vec2::new((cell_w - 2.0 * s).max(10.0), (row_h - 1.5 * s).max(10.0));
                let pill_rect = Rect::from_center_size(cell_rect.center(), pill_sz);
                let d_resp = ui
                    .interact(
                        cell_rect,
                        ui.scope_id().with(("cal_day", day)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                let is_today = today.year() == vy && today.month() == vm && today.day() == day;
                let is_selected = self.calendar_selected_day == Some((vy, vm, day));
                let r_cr = CornerRadius::same((5.0 * s).round() as u8);
                if is_selected {
                    ui.painter().rect_filled(pill_rect, r_cr, accent);
                } else if is_today {
                    ui.painter().rect_stroke(
                        pill_rect,
                        r_cr,
                        Stroke::new(1.1, accent),
                        StrokeKind::Inside,
                    );
                } else if d_resp.hovered() {
                    ui.painter().rect_filled(
                        pill_rect,
                        r_cr,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                    );
                }
                ui.painter().text(
                    pill_rect.center() - Vec2::new(0.0, 0.5 * s),
                    Align2::CENTER_CENTER,
                    day.to_string(),
                    if is_selected || is_today {
                        ui_bold_font(8.6 * s)
                    } else {
                        FontId::proportional(8.4 * s)
                    },
                    if is_selected {
                        Color32::from_rgb(12, 13, 18)
                    } else if is_today {
                        accent
                    } else {
                        text_col
                    },
                );
                if event_days.contains(&day) {
                    ui.painter().circle_filled(
                        Pos2::new(pill_rect.center().x, pill_rect.bottom() - 2.0 * s),
                        1.4 * s,
                        if is_selected {
                            Color32::from_rgb(12, 13, 18)
                        } else {
                            accent
                        },
                    );
                }
                if d_resp.clicked() {
                    let target = (vy, vm, day);
                    self.calendar_selected_day = if is_selected { None } else { Some(target) };
                }
            }

            // Right Column: Selected Day / Upcoming Events + Actions
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, avail_h),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.horizontal(|ui| {
                        let right_title = if let Some((sy, sm, sd)) = self.calendar_selected_day {
                            if sy == vy && sm == vm {
                                format!("{} {sd}", month_short_name(sm))
                            } else {
                                "Selected Day".to_owned()
                            }
                        } else {
                            "Upcoming".to_owned()
                        };
                        ui.label(
                            RichText::new(right_title)
                                .font(ui_bold_font(10.5 * s))
                                .color(text_col),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0 * s;
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Setup")
                                            .font(FontId::proportional(9.0 * s))
                                            .color(muted),
                                    )
                                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                                    .corner_radius(CornerRadius::same(7)),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.open_settings(SettingsTab::Calendar, ui.ctx());
                            }
                            if !self.config.google_calendar_ics_url.is_empty()
                                && ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Sync")
                                                .font(FontId::proportional(9.0 * s))
                                                .color(text_col),
                                        )
                                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 18))
                                        .corner_radius(CornerRadius::same(7)),
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .clicked()
                            {
                                let _ =
                                    self.channels.calendar_cmd_tx.send(CalendarCommand::Refresh);
                            }
                        });
                    });
                    ui.add_space(4.0 * s);

                    let shown_events: Vec<&CalendarEvent> =
                        if let Some((sy, sm, sd)) = self.calendar_selected_day {
                            self.calendar_events
                                .iter()
                                .filter(|ev| {
                                    chrono::Local
                                        .timestamp_opt(ev.start_utc, 0)
                                        .single()
                                        .is_some_and(|dt| {
                                            let d = dt.date_naive();
                                            d.year() == sy && d.month() == sm && d.day() == sd
                                        })
                                })
                                .take(5)
                                .collect()
                        } else {
                            upcoming_events(&self.calendar_events, now_utc, 4)
                        };

                    if self.config.google_calendar_ics_url.is_empty() && shown_events.is_empty() {
                        ui.label(
                            RichText::new(
                                "Connect Google Calendar (.ics) in Setup to show live events.",
                            )
                            .font(FontId::proportional(10.0 * s))
                            .color(muted),
                        );
                    } else if shown_events.is_empty() {
                        ui.label(
                            RichText::new(if self.calendar_selected_day.is_some() {
                                "No events on this day."
                            } else {
                                "No upcoming events."
                            })
                            .font(FontId::proportional(10.0 * s))
                            .color(muted),
                        );
                    } else {
                        for ev in shown_events {
                            let row_w = ui.available_width();
                            let cd_w = 52.0 * s;
                            let left_w = (row_w - cd_w - 4.0 * s).max(70.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(row_w, 21.0 * s),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(left_w, 21.0 * s),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_clip_rect(ui.max_rect());
                                            ui.spacing_mut().item_spacing.x = 5.0 * s;
                                            ui.label(
                                                RichText::new(format_local_time(ev.start_utc))
                                                    .font(ui_bold_font(9.5 * s))
                                                    .color(accent),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&ev.summary)
                                                        .font(FontId::proportional(10.2 * s))
                                                        .color(text_col),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(cd_w, 21.0 * s),
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.set_clip_rect(ui.max_rect());
                                            ui.label(
                                                RichText::new(format_countdown(
                                                    ev.start_utc,
                                                    ev.end_utc,
                                                    now_utc,
                                                ))
                                                .font(FontId::proportional(9.0 * s))
                                                .color(muted),
                                            );
                                        },
                                    );
                                },
                            );
                        }
                    }
                },
            );
        });
    }
}

fn clock_display_parts(config: &Config) -> (String, Option<String>) {
    let (time_str, tz_label) = format_clock(
        chrono::Utc::now(),
        &config.clock_timezone,
        config.clock_24h,
        config.clock_show_seconds,
    );
    let show_tz = !config.clock_timezone.eq_ignore_ascii_case("local") && !tz_label.is_empty();
    (time_str, show_tz.then_some(tz_label))
}

fn stabilized_clock_measure_text(time_str: &str) -> String {
    let mut out: String = time_str
        .chars()
        .map(|c| if c.is_ascii_digit() { '8' } else { c })
        .collect();
    if out.ends_with(" AM") {
        let len = out.len();
        out.replace_range(len - 3.., " PM");
    }
    out
}

fn build_clock_layout_job(
    time_str: &str,
    tz_label: Option<&str>,
    time_col: Color32,
    muted_col: Color32,
    scale: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        time_str,
        0.0,
        TextFormat {
            font_id: ui_bold_font(12.5 * scale),
            color: time_col,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    if let Some(tz) = tz_label {
        job.append(
            &format!("  {tz}"),
            0.0,
            TextFormat {
                font_id: FontId::proportional(9.5 * scale),
                color: muted_col,
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
    }
    job
}

fn estimate_clock_pill_width(
    time_str: &str,
    tz_label: Option<&str>,
    scale: f32,
    with_mascot: bool,
) -> f32 {
    let lead_and_gap = if with_mascot {
        27.0 * scale
    } else {
        16.8 * scale
    };
    let time_w = (time_str.chars().count() as f32) * (7.6 * scale);
    let tz_w = tz_label
        .map(|tz| (2.0 + tz.chars().count() as f32) * (6.2 * scale))
        .unwrap_or(0.0);
    let chrome_and_pad = 26.0 + 16.0 * scale;
    (lead_and_gap + time_w + tz_w + chrome_and_pad)
        .ceil()
        .max(96.0 * scale)
}

fn resolve_lyric_line_texts<'a>(
    text: &'a str,
    romaji: Option<&'a str>,
    translation: Option<&'a str>,
    mode: KaraokeLyricMode,
    show_romaji: bool,
    show_translate: bool,
) -> (&'a str, Option<&'a str>, Option<&'a str>) {
    let romaji_primary = mode == KaraokeLyricMode::Romaji && romaji.is_some();
    let translate_primary = mode == KaraokeLyricMode::Translate && translation.is_some();
    let primary = if romaji_primary {
        romaji.unwrap_or(text)
    } else if translate_primary {
        translation.unwrap_or(text)
    } else {
        text
    };
    let secondary_romaji = if romaji_primary || translate_primary {
        Some(text)
    } else if show_romaji {
        romaji
    } else {
        None
    };
    let secondary_trans = if !translate_primary && show_translate {
        translation
    } else {
        None
    };
    (primary, secondary_romaji, secondary_trans)
}

fn make_aligned_job(
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
    align: egui::Align,
) -> LayoutJob {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap.max_width = max_width.max(40.0);
    job.halign = align;
    job
}

fn galley_draw_pos(rect: Rect, wrap_w: f32, galley: &egui::Galley, align: egui::Align) -> Pos2 {
    let desired_left = aligned_galley_x(rect.left(), wrap_w, galley.rect.width(), align);
    Pos2::new(
        desired_left - galley.rect.left(),
        rect.top() - galley.rect.top(),
    )
}

fn render_aligned_wrapped_text(
    ui: &mut egui::Ui,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
    align: egui::Align,
) -> egui::Response {
    let wrap_w = max_width.max(40.0);
    let galley =
        ui.painter()
            .layout_job(make_aligned_job(text, font.clone(), color, wrap_w, align));
    let alloc_h = galley.size().y.max(font.size + 2.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(wrap_w, alloc_h), Sense::hover());
    if ui.is_rect_visible(rect) {
        let draw_pos = galley_draw_pos(rect, wrap_w, &galley, align);
        ui.painter()
            .with_clip_rect(ui.clip_rect())
            .galley(draw_pos, galley, Color32::TRANSPARENT);
    }
    resp
}

#[allow(clippy::too_many_arguments)]
fn render_blurred_aligned_text(
    ui: &mut egui::Ui,
    text: &str,
    font: FontId,
    base_color: Color32,
    max_width: f32,
    align: egui::Align,
    blur_radius: f32,
    opacity: f32,
) -> egui::Response {
    // 12-tap dual-ring isotropic Gaussian-style kernel (no sharp center pass) works uniformly on
    // both Windows SoftCanvas and Linux wl_shm / OpenGL without GPU shader dependencies.
    const PASSES: [(f32, f32, f32); 12] = [
        (1.0, 0.0, 1.0),
        (0.707, 0.707, 1.0),
        (0.0, 1.0, 1.0),
        (-0.707, 0.707, 1.0),
        (-1.0, 0.0, 1.0),
        (-0.707, -0.707, 1.0),
        (0.0, -1.0, 1.0),
        (0.707, -0.707, 1.0),
        (0.45, 0.45, 0.5),
        (-0.45, 0.45, 0.5),
        (-0.45, -0.45, 0.5),
        (0.45, -0.45, 0.5),
    ];
    let wrap_w = max_width.max(40.0);
    let pass_alpha = ((base_color.a() as f32) * opacity.clamp(0.0, 1.0) * 0.14)
        .clamp(4.0, 48.0)
        .round() as u8;
    let pass_col =
        Color32::from_rgba_unmultiplied(base_color.r(), base_color.g(), base_color.b(), pass_alpha);
    let galley = ui.painter().layout_job(make_aligned_job(
        text,
        font.clone(),
        pass_col,
        wrap_w,
        align,
    ));
    let alloc_h = galley.size().y.max(font.size + 2.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(wrap_w, alloc_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    if blur_radius <= 0.1 {
        let sharp_col = scale_color_alpha(base_color, opacity);
        let sharp_galley = ui
            .painter()
            .layout_job(make_aligned_job(text, font, sharp_col, wrap_w, align));
        let draw_pos = galley_draw_pos(rect, wrap_w, &sharp_galley, align);
        ui.painter().with_clip_rect(ui.clip_rect()).galley(
            draw_pos,
            sharp_galley,
            Color32::TRANSPARENT,
        );
        return resp;
    }

    let draw_pos = galley_draw_pos(rect, wrap_w, &galley, align);
    let clip = ui.clip_rect().expand(blur_radius + 1.0);
    let painter = ui.painter().with_clip_rect(clip);
    for (dx, dy, weight) in PASSES {
        let offset = Vec2::new(dx * blur_radius * weight, dy * blur_radius * weight);
        painter.galley(draw_pos + offset, galley.clone(), Color32::TRANSPARENT);
    }
    resp
}

#[allow(clippy::too_many_arguments)]
fn render_wrapped_karaoke(
    ui: &mut egui::Ui,
    text: &str,
    progress: f32,
    font: FontId,
    lit_color: Color32,
    dim_color: Color32,
    max_width: f32,
    align: egui::Align,
) -> egui::Response {
    let wrap_w = max_width.max(40.0);
    let dim_galley = ui.painter().layout_job(make_aligned_job(
        text,
        font.clone(),
        dim_color,
        wrap_w,
        align,
    ));
    let lit_galley = ui
        .painter()
        .layout_job(make_aligned_job(text, font, lit_color, wrap_w, align));
    let alloc_h = dim_galley.size().y.max(1.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(wrap_w, alloc_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let draw_pos = galley_draw_pos(rect, wrap_w, &dim_galley, align);
    let clip = ui.clip_rect();
    let p = progress.clamp(0.0, 1.0);

    if p <= 0.001 {
        ui.painter()
            .with_clip_rect(clip)
            .galley(draw_pos, dim_galley, Color32::TRANSPARENT);
        return resp;
    }
    if p >= 0.999 {
        ui.painter()
            .with_clip_rect(clip)
            .galley(draw_pos, lit_galley, Color32::TRANSPARENT);
        return resp;
    }

    let row_count = dim_galley.rows.len();
    let total_w: f32 = dim_galley.rows.iter().map(|r| r.rect().width()).sum();
    let mut rem = total_w * p;

    for (idx, placed) in dim_galley.rows.iter().enumerate() {
        let rr = placed.rect().translate(draw_pos.to_vec2());
        let band_top = if idx == 0 {
            rect.top().min(rr.top()) - 2.0
        } else {
            let prev_rr = dim_galley.rows[idx - 1]
                .rect()
                .translate(draw_pos.to_vec2());
            (prev_rr.bottom() + rr.top()) * 0.5
        };
        let band_bot = if idx + 1 >= row_count {
            rect.bottom().max(rr.bottom()) + 2.0
        } else {
            let next_rr = dim_galley.rows[idx + 1]
                .rect()
                .translate(draw_pos.to_vec2());
            (rr.bottom() + next_rr.top()) * 0.5
        };

        let rw = rr.width().max(0.0);
        let lit_w = rem.clamp(0.0, rw);
        rem = (rem - rw).max(0.0);

        if lit_w <= 0.5 {
            let dim_band = Rect::from_min_max(
                Pos2::new(rect.left().min(rr.left()) - 2.0, band_top),
                Pos2::new(rect.right().max(rr.right()) + 2.0, band_bot),
            )
            .intersect(clip);
            if dim_band.width() > 0.0 && dim_band.height() > 0.0 {
                ui.painter().with_clip_rect(dim_band).galley(
                    draw_pos,
                    dim_galley.clone(),
                    Color32::TRANSPARENT,
                );
            }
        } else if lit_w >= rw - 0.5 {
            let lit_band = Rect::from_min_max(
                Pos2::new(rect.left().min(rr.left()) - 2.0, band_top),
                Pos2::new(rect.right().max(rr.right()) + 2.0, band_bot),
            )
            .intersect(clip);
            if lit_band.width() > 0.0 && lit_band.height() > 0.0 {
                ui.painter().with_clip_rect(lit_band).galley(
                    draw_pos,
                    lit_galley.clone(),
                    Color32::TRANSPARENT,
                );
            }
        } else {
            // Mutually exclusive split on the active row prevents alpha double-blending on edges.
            let split_x = rr.left() + lit_w;
            let lit_band = Rect::from_min_max(
                Pos2::new(rect.left().min(rr.left()) - 2.0, band_top),
                Pos2::new(split_x, band_bot),
            )
            .intersect(clip);
            if lit_band.width() > 0.0 && lit_band.height() > 0.0 {
                ui.painter().with_clip_rect(lit_band).galley(
                    draw_pos,
                    lit_galley.clone(),
                    Color32::TRANSPARENT,
                );
            }
            let dim_band = Rect::from_min_max(
                Pos2::new(split_x, band_top),
                Pos2::new(rect.right().max(rr.right()) + 2.0, band_bot),
            )
            .intersect(clip);
            if dim_band.width() > 0.0 && dim_band.height() > 0.0 {
                ui.painter().with_clip_rect(dim_band).galley(
                    draw_pos,
                    dim_galley.clone(),
                    Color32::TRANSPARENT,
                );
            }
        }
    }
    resp
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    ((a as f32) + ((b as f32) - (a as f32)) * t).round() as u8
}

/// Pull loud album/accent colors toward a neutral slate and cap brightness so the
/// gradient wash stays comfortable to look at behind white text.
fn soft_accent_rgb(rgb: [u8; 3]) -> [u8; 3] {
    const NEUTRAL: [u8; 3] = [126, 132, 146];
    [
        lerp_u8(rgb[0], NEUTRAL[0], 0.30).min(190),
        lerp_u8(rgb[1], NEUTRAL[1], 0.30).min(190),
        lerp_u8(rgb[2], NEUTRAL[2], 0.30).min(190),
    ]
}

fn egui_align(align: TextAlign) -> egui::Align {
    match align {
        TextAlign::Left => egui::Align::LEFT,
        TextAlign::Center => egui::Align::Center,
        TextAlign::Right => egui::Align::RIGHT,
    }
}

fn quantize_channel(value: u8) -> u8 {
    (value / 8).saturating_mul(8)
}

/// Quantize animated colors so a notch spring does not re-upload the gradient texture every frame.
fn gradient_cache_key(
    base_bg: [u8; 4],
    accent: [u8; 3],
    pill_mode: bool,
) -> ([u8; 4], [u8; 3], bool) {
    (
        base_bg.map(quantize_channel),
        accent.map(quantize_channel),
        pill_mode,
    )
}

/// X position of a manually painted galley inside a column, clamped when it is wider than the column.
fn aligned_galley_x(origin: f32, available: f32, content: f32, align: egui::Align) -> f32 {
    origin + (available - content).max(0.0) * align.to_factor()
}

fn retain_recent_alerts(alerts: &mut HashSet<(String, i64)>, now_utc: i64) {
    let cutoff = now_utc - ALERT_RETENTION_SECS;
    alerts.retain(|(_, start_utc)| *start_utc >= cutoff);
}

/// Underdamped harmonic spring step (macOS/iOS Dynamic Island style bounce) with sub-stepping.
fn step_spring(pos: f32, vel: f32, target: f32, omega: f32, zeta: f32, dt: f32) -> (f32, f32) {
    const SUBSTEPS: usize = 4;
    let sub_dt = (dt / SUBSTEPS as f32).clamp(0.0, 0.02);
    let stiffness = omega * omega;
    let damping = 2.0 * zeta * omega;
    let mut p = pos;
    let mut v = vel;
    for _ in 0..SUBSTEPS {
        let accel = stiffness * (target - p) - damping * v;
        v += accel * sub_dt;
        p += v * sub_dt;
    }
    (p, v)
}

fn build_vertical_gradient_pixels(
    base_bg: [u8; 4],
    bottom_rgb: [u8; 3],
    start_norm: f32,
    max_strength: f32,
) -> Vec<Color32> {
    const H: usize = 64;
    let span = (1.0 - start_norm).max(0.01);
    let mut pixels = Vec::with_capacity(H);
    for y in 0..H {
        let norm = y as f32 / (H - 1) as f32;
        let t = ((norm - start_norm) / span).clamp(0.0, 1.0);
        let ease = t * t * (3.0 - 2.0 * t) * max_strength;
        let r = lerp_u8(base_bg[0], bottom_rgb[0], ease);
        let g = lerp_u8(base_bg[1], bottom_rgb[1], ease);
        let b = lerp_u8(base_bg[2], bottom_rgb[2], ease);
        pixels.push(Color32::from_rgba_unmultiplied(r, g, b, base_bg[3]));
    }
    pixels
}

struct SurfaceStyle<'a> {
    ear_w: f32,
    top_radius: u8,
    bot_radius: u8,
    fill: Color32,
    grad_tex: Option<&'a TextureHandle>,
    grain_tex: Option<&'a TextureHandle>,
    accent: Color32,
    material: BackgroundMaterial,
    border_alpha: u8,
}

fn material_surface_colors(
    base: Color32,
    accent: Color32,
    material: BackgroundMaterial,
    docked: bool,
) -> (Color32, u8) {
    let a = base.a() as f32;
    match material {
        BackgroundMaterial::Solid => (base, 0),
        BackgroundMaterial::Acrylic => {
            let mul = if docked { 0.88 } else { 0.78 };
            let out_a = (a * mul).round().clamp(146.0, 248.0) as u8;
            let tinted = Color32::from_rgba_unmultiplied(
                lerp_u8(base.r(), accent.r(), 0.08),
                lerp_u8(base.g(), accent.g(), 0.08),
                lerp_u8(base.b(), accent.b(), 0.08),
                out_a,
            );
            (tinted, 38)
        }
        BackgroundMaterial::FrostedGlass => {
            let mul = if docked { 0.84 } else { 0.74 };
            let out_a = (a * mul).round().clamp(138.0, 244.0) as u8;
            let tinted = Color32::from_rgba_unmultiplied(
                lerp_u8(base.r(), 32, 0.34),
                lerp_u8(base.g(), 38, 0.34),
                lerp_u8(base.b(), 48, 0.36),
                out_a,
            );
            (tinted, 54)
        }
        BackgroundMaterial::LiquidGlass => {
            let mul = if docked { 0.80 } else { 0.68 };
            let out_a = (a * mul).round().clamp(128.0, 238.0) as u8;
            let tinted = Color32::from_rgba_unmultiplied(
                lerp_u8(base.r(), accent.r(), 0.06),
                lerp_u8(base.g(), accent.g(), 0.06),
                lerp_u8(base.b(), accent.b().max(42), 0.12),
                out_a,
            );
            (tinted, 76)
        }
    }
}

fn paint_material_surface_fx(
    painter: &egui::Painter,
    body_rect: Rect,
    body_corners: CornerRadius,
    grain_tex: Option<&TextureHandle>,
    accent: Color32,
    material: BackgroundMaterial,
    docked: bool,
) {
    if body_rect.width() <= 2.0 || body_rect.height() <= 2.0 {
        return;
    }
    match material {
        BackgroundMaterial::Solid => {}
        BackgroundMaterial::Acrylic => {
            if let Some(grain) = grain_tex {
                let u_rep = (body_rect.width() / 32.0).max(1.0);
                let v_rep = (body_rect.height() / 32.0).max(1.0);
                let shape = RectShape::filled(
                    body_rect,
                    body_corners,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 22),
                )
                .with_texture(
                    grain.id(),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(u_rep, v_rep)),
                );
                painter.add(shape);
            }
            if !docked {
                painter.rect_stroke(
                    body_rect.shrink(1.0),
                    body_corners,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 16)),
                    StrokeKind::Inside,
                );
            }
        }
        BackgroundMaterial::FrostedGlass => {
            let clipped = painter.with_clip_rect(body_rect);
            let top_mist = Rect::from_min_max(
                body_rect.min,
                Pos2::new(body_rect.max.x, body_rect.min.y + body_rect.height() * 0.45),
            );
            clipped.rect_filled(
                top_mist,
                body_corners,
                Color32::from_rgba_unmultiplied(230, 240, 255, 18),
            );
            clipped.rect_filled(
                body_rect,
                body_corners,
                Color32::from_rgba_unmultiplied(180, 200, 230, 10),
            );
            if !docked {
                painter.rect_stroke(
                    body_rect.shrink(1.0),
                    body_corners,
                    Stroke::new(1.1, Color32::from_rgba_unmultiplied(235, 245, 255, 26)),
                    StrokeKind::Inside,
                );
            }
        }
        BackgroundMaterial::LiquidGlass => {
            let clipped = painter.with_clip_rect(body_rect);
            let sheen_h = (body_rect.height() * 0.34).clamp(6.0, 44.0);
            let top_sheen = Rect::from_min_max(
                Pos2::new(body_rect.left() + 2.0, body_rect.top() + 1.0),
                Pos2::new(body_rect.right() - 2.0, body_rect.top() + sheen_h),
            );
            let sheen_r = CornerRadius {
                nw: body_corners.nw,
                ne: body_corners.ne,
                sw: (sheen_h * 0.4).min(12.0) as u8,
                se: (sheen_h * 0.4).min(12.0) as u8,
            };
            clipped.rect_filled(
                top_sheen,
                sheen_r,
                Color32::from_rgba_unmultiplied(255, 255, 255, 24),
            );
            let caustic_w = (body_rect.width() * 0.56).max(16.0);
            let caustic_cx = body_rect.left() + body_rect.width() * 0.36;
            clipped.rect_filled(
                Rect::from_min_max(
                    Pos2::new(caustic_cx - caustic_w * 0.5, body_rect.top() + 1.0),
                    Pos2::new(caustic_cx + caustic_w * 0.5, body_rect.top() + 3.2),
                ),
                CornerRadius::same(2),
                Color32::from_rgba_unmultiplied(255, 255, 255, 78),
            );
            clipped.rect_filled(
                Rect::from_min_max(
                    Pos2::new(body_rect.left() + 8.0, body_rect.bottom() - 3.0),
                    Pos2::new(body_rect.right() - 8.0, body_rect.bottom() - 1.0),
                ),
                CornerRadius::same(2),
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 44),
            );
            if !docked {
                painter.rect_stroke(
                    body_rect.shrink(1.0),
                    body_corners,
                    Stroke::new(1.1, Color32::from_rgba_unmultiplied(255, 255, 255, 42)),
                    StrokeKind::Inside,
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
struct NotchSliceRow {
    y: f32,
    x_left: f32,
    x_right: f32,
    n_left: Vec2,
    n_right: Vec2,
}

fn build_notch_surface_mesh(
    rect: Rect,
    ear_w: f32,
    bot_radius: u8,
    fill: Color32,
    grad_tex_id: Option<TextureId>,
) -> Mesh {
    let top_y = if cfg!(windows) {
        rect.top() - 1.0
    } else {
        rect.top()
    };
    // Leave 0.5px inside rect.bottom() for anti-aliasing fringe so GL viewport never clips it.
    let bottom_y = (rect.bottom() - 0.5).max(top_y + 1.0);
    let total_h = (bottom_y - top_y).max(1.0);
    let ear_r = ear_w
        .min(total_h * 0.45)
        .min((rect.width() * 0.25).max(0.0));
    let body_left = rect.left() + ear_w;
    let body_right = (rect.right() - ear_w).max(body_left + 1.0);
    let body_w = (body_right - body_left).max(1.0);
    let bot_r = (bot_radius as f32)
        .min((total_h - ear_r).max(0.0))
        .min(body_w * 0.5)
        .max(0.0);
    let feather = 0.75f32.min(ear_r * 0.5).max(0.0);

    const EAR_SEGS: usize = 16;
    const BOT_SEGS: usize = 16;
    let mut rows = Vec::with_capacity(EAR_SEGS + BOT_SEGS + 3);

    for i in 0..=EAR_SEGS {
        let t = i as f32 / EAR_SEGS as f32;
        let angle = -FRAC_PI_2 + FRAC_PI_2 * t;
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let y = top_y + ear_r * (1.0 + sin_a);
        let dx = ear_r * (1.0 - cos_a);
        rows.push(NotchSliceRow {
            y,
            x_left: body_left - dx,
            x_right: body_right + dx,
            n_left: Vec2::new(-cos_a, -sin_a),
            n_right: Vec2::new(cos_a, -sin_a),
        });
    }

    let straight_bottom_y = (bottom_y - bot_r).max(top_y + ear_r);
    if straight_bottom_y > top_y + ear_r + 0.25 {
        rows.push(NotchSliceRow {
            y: straight_bottom_y,
            x_left: body_left,
            x_right: body_right,
            n_left: Vec2::new(-1.0, 0.0),
            n_right: Vec2::new(1.0, 0.0),
        });
    }

    if bot_r > 0.5 {
        for i in 1..=BOT_SEGS {
            let t = i as f32 / BOT_SEGS as f32;
            let angle = FRAC_PI_2 * t;
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            let y = straight_bottom_y + bot_r * sin_a;
            let inset = bot_r * (1.0 - cos_a);
            rows.push(NotchSliceRow {
                y,
                x_left: body_left + inset,
                x_right: body_right - inset,
                n_left: Vec2::new(-cos_a, sin_a),
                n_right: Vec2::new(cos_a, sin_a),
            });
        }
    }

    let (mut mesh, inner_color) = if let Some(tex_id) = grad_tex_id {
        (Mesh::with_texture(tex_id), Color32::WHITE)
    } else {
        (Mesh::default(), fill)
    };
    let uv_for_y = |y: f32| -> Pos2 {
        if grad_tex_id.is_some() {
            Pos2::new(0.5, ((y - top_y) / total_h).clamp(0.0, 1.0))
        } else {
            WHITE_UV
        }
    };

    for row in &rows {
        let p_li = Pos2::new(row.x_left, row.y);
        let p_ri = Pos2::new(row.x_right, row.y);
        let p_lo = Pos2::new(
            (row.x_left + feather * row.n_left.x).clamp(rect.left(), rect.right()),
            (row.y + feather * row.n_left.y).clamp(top_y, rect.bottom()),
        );
        let p_ro = Pos2::new(
            (row.x_right + feather * row.n_right.x).clamp(rect.left(), rect.right()),
            (row.y + feather * row.n_right.y).clamp(top_y, rect.bottom()),
        );
        let uv_i = uv_for_y(row.y);
        let uv_lo = uv_for_y(p_lo.y);
        let uv_ro = uv_for_y(p_ro.y);

        mesh.vertices.push(Vertex {
            pos: p_lo,
            uv: uv_lo,
            color: Color32::TRANSPARENT,
        });
        mesh.vertices.push(Vertex {
            pos: p_li,
            uv: uv_i,
            color: inner_color,
        });
        mesh.vertices.push(Vertex {
            pos: p_ri,
            uv: uv_i,
            color: inner_color,
        });
        mesh.vertices.push(Vertex {
            pos: p_ro,
            uv: uv_ro,
            color: Color32::TRANSPARENT,
        });
    }

    for k in 0..rows.len().saturating_sub(1) {
        let b0 = (k * 4) as u32;
        let b1 = ((k + 1) * 4) as u32;
        let (lo0, li0, ri0, ro0) = (b0, b0 + 1, b0 + 2, b0 + 3);
        let (lo1, li1, ri1, ro1) = (b1, b1 + 1, b1 + 2, b1 + 3);

        // Interior fill
        mesh.add_triangle(li0, ri0, ri1);
        mesh.add_triangle(li0, ri1, li1);

        // Left anti-aliasing fringe
        mesh.add_triangle(lo0, li0, li1);
        mesh.add_triangle(lo0, li1, lo1);

        // Right anti-aliasing fringe
        mesh.add_triangle(ri0, ro0, ro1);
        mesh.add_triangle(ri0, ro1, ri1);
    }

    if let Some(last) = rows.last() {
        let last_base = ((rows.len() - 1) * 4) as u32;
        let li_last = last_base + 1;
        let ri_last = last_base + 2;
        let bot_y_outer = (last.y + feather).min(rect.bottom());
        let uv_bot = uv_for_y(bot_y_outer);
        let bl_out = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos: Pos2::new(last.x_left, bot_y_outer),
            uv: uv_bot,
            color: Color32::TRANSPARENT,
        });
        let br_out = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos: Pos2::new(last.x_right, bot_y_outer),
            uv: uv_bot,
            color: Color32::TRANSPARENT,
        });
        mesh.add_triangle(li_last, ri_last, br_out);
        mesh.add_triangle(li_last, br_out, bl_out);
    }

    mesh
}

fn paint_island_surface(painter: &egui::Painter, rect: Rect, style: SurfaceStyle<'_>) {
    let SurfaceStyle {
        ear_w,
        top_radius,
        bot_radius,
        fill,
        grad_tex,
        grain_tex,
        accent,
        material,
        border_alpha,
    } = style;

    let max_r = ((rect.height() * 0.5)
        .floor()
        .min((rect.width() * 0.5).floor()))
    .clamp(0.0, 28.0) as u8;
    let clamped_bot = bot_radius.min(max_r);
    let clamped_top = top_radius.min(max_r);

    if ear_w > 0.5 {
        let mesh =
            build_notch_surface_mesh(rect, ear_w, bot_radius, fill, grad_tex.map(|tex| tex.id()));
        painter.add(Shape::mesh(mesh));
        let inner_body = Rect::from_min_max(
            Pos2::new(rect.left() + ear_w, rect.top()),
            Pos2::new(rect.right() - ear_w, rect.bottom() - 0.5),
        );
        let notch_corners = CornerRadius {
            nw: 0,
            ne: 0,
            sw: clamped_bot,
            se: clamped_bot,
        };
        paint_material_surface_fx(
            painter,
            inner_body,
            notch_corners,
            grain_tex,
            accent,
            material,
            true,
        );
        return;
    }

    let body_corners = CornerRadius {
        nw: clamped_top,
        ne: clamped_top,
        sw: clamped_bot,
        se: clamped_bot,
    };

    let body_rect = rect;

    if let Some(tex) = grad_tex {
        let shape = RectShape::filled(body_rect, body_corners, Color32::WHITE).with_texture(
            tex.id(),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        );
        painter.add(shape);
    } else {
        painter.rect_filled(body_rect, body_corners, fill);
    }

    paint_material_surface_fx(
        painter,
        body_rect,
        body_corners,
        grain_tex,
        accent,
        material,
        false,
    );

    if border_alpha > 0 {
        painter.rect_stroke(
            body_rect,
            body_corners,
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, border_alpha),
            ),
            StrokeKind::Inside,
        );
    }
}

fn app_brand_color(app_name: &str, fallback: Color32) -> Color32 {
    let lower = app_name.to_ascii_lowercase();
    if lower.contains("whatsapp") {
        Color32::from_rgb(37, 211, 102)
    } else if lower.contains("discord") {
        Color32::from_rgb(88, 101, 242)
    } else if lower.contains("gmail") || lower == "mail" {
        Color32::from_rgb(234, 67, 53)
    } else if lower.contains("outlook") {
        Color32::from_rgb(0, 120, 212)
    } else if lower.contains("chrome") {
        Color32::from_rgb(66, 133, 244)
    } else if lower.contains("edge") {
        Color32::from_rgb(0, 120, 215)
    } else if lower.contains("firefox") {
        Color32::from_rgb(255, 113, 57)
    } else if lower.contains("brave") {
        Color32::from_rgb(251, 84, 43)
    } else if lower.contains("telegram") {
        Color32::from_rgb(42, 171, 238)
    } else if lower.contains("slack") {
        Color32::from_rgb(224, 30, 90)
    } else if lower.contains("spotify") {
        Color32::from_rgb(29, 185, 84)
    } else {
        fallback
    }
}

fn render_audio_bars(
    ui: &mut egui::Ui,
    time_secs: f32,
    is_playing: bool,
    color: Color32,
    scale: f32,
) {
    let bar_w = 2.4 * scale;
    let gap = 2.2 * scale;
    let total_w = bar_w * 3.0 + gap * 2.0;
    let max_h = 12.0 * scale;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, max_h), Sense::hover());
    let painter = ui.painter();

    for i in 0..3 {
        let h_frac = if is_playing {
            let phase = time_secs * (5.2 + i as f32 * 1.4) + i as f32 * 1.9;
            0.35 + 0.65 * (0.5 + 0.5 * phase.sin())
        } else {
            [0.35, 0.55, 0.35][i]
        };
        let h = (max_h * h_frac).max(3.0 * scale);
        let x = rect.left() + i as f32 * (bar_w + gap);
        let bar_rect = Rect::from_center_size(
            Pos2::new(x + bar_w * 0.5, rect.center().y),
            Vec2::new(bar_w, h),
        );
        painter.rect_filled(bar_rect, CornerRadius::same(2), color);
    }
}

fn paint_notif_avatar_in_rect(
    painter: &egui::Painter,
    rect: Rect,
    app_name: &str,
    tex_id: Option<TextureId>,
    accent: Color32,
    scale: f32,
) {
    let size = rect.width().min(rect.height()).max(8.0);
    if let Some(id) = tex_id {
        let cr = CornerRadius::same((size * 0.26).round().clamp(3.0, 9.0) as u8);
        painter.add(
            RectShape::filled(rect, cr, Color32::WHITE)
                .with_texture(id, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))),
        );
        painter.rect_stroke(
            rect,
            cr,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 28)),
            StrokeKind::Inside,
        );
        return;
    }
    let initial = app_name
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('L')
        .to_uppercase()
        .to_string();
    painter.circle_filled(
        rect.center(),
        size * 0.5,
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 46),
    );
    painter.circle_stroke(rect.center(), size * 0.5, Stroke::new(1.0, accent));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        ui_bold_font((size * 0.46).max(8.5 * scale)),
        Color32::WHITE,
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_gallery_header_in_rect(
    painter: &egui::Painter,
    rect: Rect,
    icon: IconKind,
    label: &str,
    badge: Option<(&str, Color32)>,
    accent: Color32,
    muted: Color32,
    scale: f32,
) {
    let icon_sz = 9.5 * scale;
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + icon_sz * 0.5, rect.center().y),
        Vec2::splat(icon_sz),
    );
    paint_icon(painter, icon_rect, icon, accent);

    let badge_w = if let Some((text, col)) = badge {
        let b_font = ui_bold_font(7.6 * scale);
        let b_gal = painter.layout_no_wrap(text.to_owned(), b_font, col);
        let pill_w = (b_gal.size().x + 8.0 * scale).min(rect.width() * 0.46);
        let pill_h = (b_gal.size().y + 2.0 * scale).min(rect.height());
        let pill_rect = Rect::from_min_size(
            Pos2::new(rect.right() - pill_w, rect.center().y - pill_h * 0.5),
            Vec2::new(pill_w, pill_h),
        );
        painter.rect_filled(
            pill_rect,
            CornerRadius::same((pill_h * 0.45).round().clamp(3.0, 7.0) as u8),
            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 30),
        );
        painter.with_clip_rect(pill_rect).galley(
            Pos2::new(
                pill_rect.center().x - b_gal.size().x.min(pill_w) * 0.5,
                pill_rect.center().y - b_gal.size().y * 0.5,
            ),
            b_gal,
            Color32::TRANSPARENT,
        );
        pill_w + 4.0 * scale
    } else {
        0.0
    };

    let lbl_left = icon_rect.right() + 3.5 * scale;
    let lbl_max_w = (rect.right() - badge_w - lbl_left).max(10.0);
    let lbl_font = ui_bold_font(8.5 * scale);
    let shown_lbl = truncate_with_ellipsis(label, |t| {
        painter
            .layout_no_wrap(t.to_owned(), lbl_font.clone(), muted)
            .size()
            .x
            <= lbl_max_w
    });
    let lbl_gal = painter.layout_no_wrap(shown_lbl, lbl_font, muted);
    painter.galley(
        Pos2::new(lbl_left, rect.center().y - lbl_gal.size().y * 0.5),
        lbl_gal,
        Color32::TRANSPARENT,
    );
}

fn gallery_grid_layout(
    count: usize,
    avail_w: f32,
    avail_h: f32,
    gap: f32,
    scale: f32,
) -> Vec<(usize, Rect)> {
    if count == 0 {
        return Vec::new();
    }
    let cols = match count {
        1 => 1,
        2 | 4 => 2,
        3 | 5 | 6 => 3,
        _ => 4,
    };
    let rows = count.div_ceil(cols).max(1);
    let tile_h = ((avail_h - gap * (rows.saturating_sub(1) as f32)) / rows as f32)
        .floor()
        .max(54.0 * scale);
    let mut out = Vec::with_capacity(count);
    for r in 0..rows {
        let start = r * cols;
        let end = (start + cols).min(count);
        let row_len = end - start;
        let tile_w = ((avail_w - gap * (row_len.saturating_sub(1) as f32)) / row_len as f32)
            .floor()
            .max(60.0 * scale);
        let y = r as f32 * (tile_h + gap);
        for c in 0..row_len {
            let x = c as f32 * (tile_w + gap);
            out.push((
                start + c,
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(tile_w, tile_h)),
            ));
        }
    }
    out
}

fn format_notif_age(now_unix: u64, ts_unix: u64) -> String {
    let diff = now_unix.saturating_sub(ts_unix);
    if diff < 45 {
        "now".to_owned()
    } else if diff < 3600 {
        format!("{}m", diff / 60)
    } else if diff < 86_400 {
        format!("{}h", diff / 3600)
    } else {
        format!("{}d", diff / 86_400)
    }
}

fn calendar_month_cells(year: i32, month: u32) -> (usize, u32) {
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let (ny, nm) = step_year_month(year, month, 1);
    let last_day = chrono::NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map_or(30, |d| d.day());
    (first.weekday().num_days_from_monday() as usize, last_day)
}

fn step_year_month(year: i32, month: u32, delta_months: i32) -> (i32, u32) {
    let total = year * 12 + (month as i32 - 1) + delta_months;
    let ny = total.div_euclid(12);
    let nm = total.rem_euclid(12) as u32 + 1;
    (ny, nm)
}

fn month_short_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    }
}

fn truncate_with_ellipsis(text: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if fits(&cand) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

fn render_aligned_truncated_text(
    ui: &mut egui::Ui,
    text: &str,
    font: FontId,
    color: Color32,
    align: egui::Align,
) {
    let max_w = ui.available_width();
    let shown = truncate_with_ellipsis(text, |t| {
        ui.painter()
            .layout_no_wrap(t.to_owned(), font.clone(), color)
            .size()
            .x
            <= max_w
    });
    let galley = ui.painter().layout_no_wrap(shown, font, color);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(max_w, galley.size().y), Sense::hover());
    let x = aligned_galley_x(rect.left(), max_w, galley.size().x, align);
    let pos = Pos2::new(x, rect.top());
    ui.painter()
        .with_clip_rect(rect)
        .galley(pos, galley, Color32::TRANSPARENT);
}

fn render_dot_label(
    ui: &mut egui::Ui,
    dot_color: Color32,
    dot_radius: f32,
    job: LayoutJob,
    align: egui::Align,
) {
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    let gap = dot_radius * 2.0;
    let total_w = dot_radius * 2.0 + gap + galley.size().x;
    let start_x = aligned_galley_x(rect.left(), rect.width(), total_w, align);
    let cy = rect.center().y;
    ui.painter()
        .circle_filled(Pos2::new(start_x + dot_radius, cy), dot_radius, dot_color);
    ui.painter().galley(
        Pos2::new(start_x + dot_radius * 2.0 + gap, cy - galley.size().y * 0.5),
        galley,
        Color32::TRANSPARENT,
    );
}

fn mascot_glow_color(accent: Color32, col: Color32) -> Color32 {
    Color32::from_rgb(
        lerp_u8(col.r(), accent.r().max(130), 0.42),
        lerp_u8(col.g(), accent.g().max(150), 0.42),
        lerp_u8(col.b(), accent.b().max(195), 0.42),
    )
}

fn mascot_blink_open_factor(elapsed_ms: f32) -> f32 {
    if elapsed_ms <= 0.0 || elapsed_ms >= 170.0 {
        1.0
    } else if elapsed_ms < 72.0 {
        let t = (elapsed_ms / 72.0).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        1.0 - smooth * 0.92
    } else if elapsed_ms < 96.0 {
        0.08
    } else {
        let t = ((elapsed_ms - 96.0) / 74.0).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        0.08 + smooth * 0.92
    }
}

fn push_notification_history(history: &mut VecDeque<NotificationItem>, mut item: NotificationItem) {
    if item.timestamp == 0 {
        item.timestamp = chrono::Utc::now().timestamp().max(0) as u64;
    }
    history.retain(|old| old.id != item.id);
    history.push_front(item);
    while history.len() > 30 {
        history.pop_back();
    }
}

fn hash_notif_icon(app_name: &str, rgba: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    app_name.hash(&mut h);
    rgba.hash(&mut h);
    h.finish()
}

#[allow(clippy::too_many_arguments)]
fn render_mascot_label(
    ui: &mut egui::Ui,
    mascot_size: f32,
    eye_offset: Vec2,
    eye_open: f32,
    glow_color: Option<Color32>,
    mascot_color: Color32,
    job: LayoutJob,
    align: egui::Align,
) {
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    let gap = (mascot_size * 0.35).max(5.0);
    let total_w = mascot_size + gap + galley.size().x;
    let start_x = aligned_galley_x(rect.left(), rect.width(), total_w, align);
    let cy = rect.center().y;
    let m_rect = Rect::from_center_size(
        Pos2::new(start_x + mascot_size * 0.5, cy),
        Vec2::splat(mascot_size),
    );
    paint_mascot_fx(
        ui.painter(),
        m_rect,
        eye_offset,
        eye_open,
        glow_color,
        mascot_color,
    );
    ui.painter().galley(
        Pos2::new(start_x + mascot_size + gap, cy - galley.size().y * 0.5),
        galley,
        Color32::TRANSPARENT,
    );
}

fn paint_pill_bar(
    painter: &egui::Painter,
    rect: Rect,
    frac: f32,
    track_col: Color32,
    fill_col: Color32,
) {
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return;
    }
    let r = (rect.height() * 0.5).ceil().clamp(1.0, 16.0) as u8;
    let corner = CornerRadius::same(r);
    painter.rect_filled(rect, corner, track_col);
    let f = frac.clamp(0.0, 1.0);
    if f > 0.01 {
        let fill_w = (rect.width() * f).max(rect.height().min(rect.width()));
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
        painter.rect_filled(fill_rect, corner, fill_col);
    }
}

fn scale_color_alpha(color: Color32, alpha: f32) -> Color32 {
    let a = ((color.a() as f32) * alpha.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

/// Returns `(ease_0_to_1, incoming_x_frac)` while a tab switch is in flight.
/// The incoming content slides in from the direction the user navigated.
fn tab_transition_state(sw: &TabSwitch, now: Instant) -> Option<(f32, f32)> {
    let dur = Duration::from_millis(TAB_TRANSITION_MS).as_secs_f32();
    let elapsed = now.saturating_duration_since(sw.started_at).as_secs_f32();
    if elapsed >= dur {
        return None;
    }
    let t = (elapsed / dur).clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - t).powi(3);
    let dir = if sw.direction < 0.0 { -1.0 } else { 1.0 };
    Some((ease, -dir * (1.0 - ease)))
}

/// Returns `(ease_0_to_1, outgoing_y_frac, incoming_y_frac)` while a pill lyric transition is active.
fn lyric_transition_state(
    started_at: Option<Instant>,
    direction: f32,
    now: Instant,
) -> Option<(f32, f32, f32)> {
    let start = started_at?;
    let dur = Duration::from_millis(PILL_LYRIC_TRANSITION_MS).as_secs_f32();
    let elapsed = now.saturating_duration_since(start).as_secs_f32();
    if elapsed >= dur {
        return None;
    }
    let t = (elapsed / dur).clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - t).powi(3);
    let dir = if direction < 0.0 { -1.0 } else { 1.0 };
    Some((ease, -dir * ease, dir * (1.0 - ease)))
}

#[allow(clippy::too_many_arguments)]
fn paint_karaoke_in_rect(
    painter: &egui::Painter,
    rect: Rect,
    text: &str,
    progress: f32,
    font: FontId,
    dim_color: Color32,
    lit_color: Color32,
    y_offset: f32,
    align: egui::Align,
) {
    let avail_w = rect.width().max(24.0);
    let p = progress.clamp(0.0, 1.0);
    let lit_galley = painter.layout_no_wrap(text.to_owned(), font.clone(), lit_color);
    let full_w = lit_galley.size().x;
    let shown_w = full_w.min(avail_w);
    let scroll_x = if full_w > avail_w {
        let max_scroll = full_w - avail_w;
        -(max_scroll * p)
    } else {
        0.0
    };
    let draw_y = rect.center().y - lit_galley.size().y * 0.5 + y_offset;
    let draw_x = aligned_galley_x(rect.left(), avail_w, shown_w, align) + scroll_x;
    let draw_pos = Pos2::new(draw_x, draw_y);
    let clipped = painter.with_clip_rect(rect);

    if p <= 0.001 {
        let dim_galley = painter.layout_no_wrap(text.to_owned(), font, dim_color);
        clipped.galley(draw_pos, dim_galley, Color32::TRANSPARENT);
        return;
    }
    if p >= 0.999 {
        clipped.galley(draw_pos, lit_galley, Color32::TRANSPARENT);
        return;
    }

    // Use full unclipped galley width so scrolled long lines track the exact character
    // progress, and split mutually exclusively at split_x to prevent double-blended edges.
    let split_x = draw_x + full_w * p;
    let lit_clip = Rect::from_min_max(rect.min, Pos2::new(split_x, rect.bottom())).intersect(rect);
    if lit_clip.width() > 0.0 && lit_clip.height() > 0.0 {
        painter
            .with_clip_rect(lit_clip)
            .galley(draw_pos, lit_galley, Color32::TRANSPARENT);
    }
    let dim_clip = Rect::from_min_max(Pos2::new(split_x, rect.top()), rect.max).intersect(rect);
    if dim_clip.width() > 0.0 && dim_clip.height() > 0.0 {
        let dim_galley = painter.layout_no_wrap(text.to_owned(), font, dim_color);
        painter
            .with_clip_rect(dim_clip)
            .galley(draw_pos, dim_galley, Color32::TRANSPARENT);
    }
}

fn render_karaoke_line(
    ui: &mut egui::Ui,
    text: &str,
    progress: f32,
    font: FontId,
    dim_color: Color32,
    lit_color: Color32,
    align: egui::Align,
) {
    let avail_w = ui.available_width().max(24.0);
    let row_h = 16.0f32.max(font.size + 3.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::hover());
    paint_karaoke_in_rect(
        ui.painter(),
        rect,
        text,
        progress,
        font,
        dim_color,
        lit_color,
        0.0,
        align,
    );
}

fn render_playbar(
    ui: &mut egui::Ui,
    pos_ms: u64,
    duration_ms: u64,
    accent: Color32,
    muted: Color32,
    scale: f32,
) -> Option<u64> {
    let mut seek_target = None;
    let clamped_pos = if duration_ms > 0 {
        pos_ms.min(duration_ms)
    } else {
        pos_ms
    };
    let frac = if duration_ms > 0 {
        (clamped_pos as f32 / duration_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let row_w = ui.available_width();
    let time_w = 32.0 * scale;
    let gap = 6.0 * scale;
    let bar_w = (row_w - time_w * 2.0 - gap * 2.0).max(40.0);

    ui.allocate_ui_with_layout(
        Vec2::new(row_w, 16.0 * scale),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                Vec2::new(time_w, 16.0 * scale),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.label(
                        RichText::new(format_mm_ss(clamped_pos))
                            .font(FontId::monospace(9.5 * scale))
                            .color(muted),
                    );
                },
            );

            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(bar_w, 16.0 * scale), Sense::click_and_drag());
            let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
            let cy = rect.center().y;
            let track_h = 3.5 * scale;
            let track_rect =
                Rect::from_center_size(Pos2::new(rect.center().x, cy), Vec2::new(bar_w, track_h));
            let painter = ui.painter();
            painter.rect_filled(
                track_rect,
                CornerRadius::same(2),
                Color32::from_rgba_unmultiplied(255, 255, 255, 42),
            );

            let fill_w = bar_w * frac;
            if fill_w > 0.5 {
                let fill_rect = Rect::from_min_size(track_rect.min, Vec2::new(fill_w, track_h));
                painter.rect_filled(fill_rect, CornerRadius::same(2), accent);
            }
            if duration_ms > 0 && (resp.hovered() || resp.dragged()) {
                painter.circle_filled(Pos2::new(rect.left() + fill_w, cy), 4.5 * scale, accent);
            }

            if duration_ms > 0
                && (resp.clicked() || resp.dragged())
                && let Some(pointer) = resp.interact_pointer_pos()
            {
                let ratio = ((pointer.x - rect.left()) / bar_w).clamp(0.0, 1.0);
                seek_target = Some((duration_ms as f64 * ratio as f64).round() as u64);
            }

            ui.allocate_ui_with_layout(
                Vec2::new(time_w, 16.0 * scale),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        RichText::new(format_mm_ss(duration_ms))
                            .font(FontId::monospace(9.5 * scale))
                            .color(muted),
                    );
                },
            );
        },
    );

    seek_target
}

fn format_mm_ss(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_with_ellipsis_fits_budget_and_keeps_short_text() {
        assert_eq!(
            truncate_with_ellipsis("short", |t| t.chars().count() <= 8),
            "short"
        );
        let cut = truncate_with_ellipsis("a very long song title", |t| t.chars().count() <= 8);
        assert!(cut.ends_with('…') && cut.chars().count() <= 8, "got: {cut}");
        assert_eq!(truncate_with_ellipsis("abc", |t| t.is_empty()), "…");
    }

    #[test]
    fn aligned_galley_x_respects_left_center_right_and_overflow() {
        assert_eq!(aligned_galley_x(10.0, 100.0, 40.0, egui::Align::LEFT), 10.0);
        assert_eq!(
            aligned_galley_x(10.0, 100.0, 40.0, egui::Align::Center),
            40.0
        );
        assert_eq!(
            aligned_galley_x(10.0, 100.0, 40.0, egui::Align::RIGHT),
            70.0
        );
        assert_eq!(
            aligned_galley_x(10.0, 100.0, 140.0, egui::Align::RIGHT),
            10.0
        );
        assert_eq!(egui_align(TextAlign::Center), egui::Align::Center);
    }

    #[test]
    fn gradient_cache_key_quantizes_animated_colors() {
        let a = gradient_cache_key([16, 16, 20, 240], [110, 195, 255], false);
        let b = gradient_cache_key([19, 22, 21, 244], [111, 198, 251], false);
        assert_eq!(a, b);
    }

    #[test]
    fn alerted_events_are_pruned_to_the_retention_window() {
        let mut alerts = HashSet::new();
        alerts.insert(("old".to_owned(), 100));
        alerts.insert(("recent".to_owned(), 1_000_000));
        retain_recent_alerts(&mut alerts, 1_000_000);
        assert!(!alerts.iter().any(|(name, _)| name == "old"));
        assert!(alerts.iter().any(|(name, _)| name == "recent"));
    }

    #[test]
    fn app_brand_color_maps_known_apps_and_falls_back() {
        let fb = Color32::from_rgb(10, 20, 30);
        assert_eq!(
            app_brand_color("WhatsApp", fb),
            Color32::from_rgb(37, 211, 102)
        );
        assert_eq!(
            app_brand_color("Discord", fb),
            Color32::from_rgb(88, 101, 242)
        );
        assert_eq!(app_brand_color("Gmail", fb), Color32::from_rgb(234, 67, 53));
        assert_eq!(
            app_brand_color("Google Chrome", fb),
            Color32::from_rgb(66, 133, 244)
        );
        assert_eq!(app_brand_color("Unknown App", fb), fb);
    }

    #[test]
    fn notch_surface_mesh_has_valid_indices_and_bounds() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(164.0, 30.0));
        let mesh = build_notch_surface_mesh(
            rect,
            12.0,
            14,
            Color32::from_rgba_unmultiplied(7, 7, 9, 252),
            None,
        );
        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.indices.is_empty());
        assert_eq!(mesh.indices.len() % 3, 0);
        for &idx in &mesh.indices {
            assert!((idx as usize) < mesh.vertices.len());
        }
        for v in &mesh.vertices {
            assert!(v.pos.x.is_finite() && v.pos.y.is_finite());
            assert!(v.pos.x >= rect.left() && v.pos.x <= rect.right());
            assert!(v.pos.y >= rect.top() - 1.0 && v.pos.y <= rect.bottom());
        }

        let grad_mesh = build_notch_surface_mesh(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(492.0, 286.0)),
            12.0,
            22,
            Color32::WHITE,
            Some(TextureId::Managed(1)),
        );
        assert_eq!(grad_mesh.texture_id, TextureId::Managed(1));
        assert_eq!(grad_mesh.indices.len() % 3, 0);
    }

    #[test]
    fn pill_gradient_starts_black_and_reaches_album_color_at_bottom() {
        let pixels = build_vertical_gradient_pixels([0, 0, 0, 252], [200, 80, 120], 0.35, 0.85);
        assert_eq!(pixels.len(), 64);
        assert_eq!(pixels[0], Color32::from_rgba_unmultiplied(0, 0, 0, 252));
        let bottom = pixels[63].to_srgba_unmultiplied();
        assert_eq!(bottom[3], 252);
        assert!(bottom[0] >= 165 && bottom[1] >= 65 && bottom[2] >= 95);
    }

    #[test]
    fn soft_accent_mutes_loud_colors_without_losing_hue() {
        let soft = soft_accent_rgb([255, 20, 200]);
        assert!(soft.iter().all(|c| *c <= 190));
        assert!(soft[0] > soft[1], "red hue must remain dominant");
        assert!(soft[2] > soft[1], "blue hue must remain dominant");
    }

    #[test]
    fn macos_spring_overshoots_and_settles_smoothly() {
        let mut pos = 164.0;
        let mut vel = 0.0;
        let target = 454.0;
        let mut max_pos = pos;
        for _ in 0..90 {
            (pos, vel) = step_spring(pos, vel, target, 16.5, 0.68, 1.0 / 60.0);
            max_pos = max_pos.max(pos);
        }
        assert!(
            max_pos > target + 4.0,
            "expected bounce overshoot, got {max_pos}"
        );
        assert!(
            (pos - target).abs() < 0.5,
            "expected settle near target, got {pos}"
        );
        assert!(vel.abs() < 2.0);
    }

    #[test]
    fn lyric_transition_fades_up_and_down_and_finishes() {
        let t0 = Instant::now();
        let (ease_up, out_up, in_up) =
            lyric_transition_state(Some(t0), 1.0, t0 + Duration::from_millis(80))
                .expect("active fade-up");
        assert!(ease_up > 0.0 && ease_up < 1.0);
        assert!(out_up < 0.0, "outgoing line moves up, got {out_up}");
        assert!(in_up > 0.0, "incoming line enters from below, got {in_up}");

        let (_, out_down, in_down) =
            lyric_transition_state(Some(t0), -1.0, t0 + Duration::from_millis(80))
                .expect("active fade-down");
        assert!(out_down > 0.0, "outgoing line moves down, got {out_down}");
        assert!(
            in_down < 0.0,
            "incoming line enters from above, got {in_down}"
        );

        assert!(
            lyric_transition_state(
                Some(t0),
                1.0,
                t0 + Duration::from_millis(PILL_LYRIC_TRANSITION_MS + 10)
            )
            .is_none()
        );
    }

    #[test]
    fn tab_transition_slides_in_from_nav_side_and_finishes() {
        let t0 = Instant::now();
        let sw_right = TabSwitch {
            direction: 1.0,
            started_at: t0,
        };
        let (ease, slide) = tab_transition_state(&sw_right, t0 + Duration::from_millis(80))
            .expect("active forward transition");
        assert!(ease > 0.0 && ease < 1.0);
        assert!(slide < 0.0, "incoming content enters from the left");

        let sw_left = TabSwitch {
            direction: -1.0,
            started_at: t0,
        };
        let (_, slide_left) = tab_transition_state(&sw_left, t0 + Duration::from_millis(80))
            .expect("active backward transition");
        assert!(slide_left > 0.0, "incoming content enters from the right");

        assert!(
            tab_transition_state(
                &sw_right,
                t0 + Duration::from_millis(TAB_TRANSITION_MS + 10)
            )
            .is_none()
        );
    }

    #[test]
    fn clock_width_scales_automatically_and_stabilizes_digits() {
        let short = estimate_clock_pill_width("12:34", None, 1.0, false);
        let secs = estimate_clock_pill_width("12:34:56", None, 1.0, false);
        let full_tz = estimate_clock_pill_width("11:34:56 PM", Some("WIB"), 1.0, false);
        let with_mascot = estimate_clock_pill_width("12:34", None, 1.0, true);
        assert!(
            secs > short,
            "seconds should widen clock pill ({secs} <= {short})"
        );
        assert!(
            full_tz > secs,
            "12h+tz should widen clock pill ({full_tz} <= {secs})"
        );
        assert!(
            with_mascot > short,
            "mascot badge should widen clock pill ({with_mascot} <= {short})"
        );
        assert_eq!(stabilized_clock_measure_text("01:23:45 AM"), "88:88:88 PM");
        assert_eq!(stabilized_clock_measure_text("09:05"), "88:88");
    }

    #[test]
    fn galley_draw_pos_centers_single_and_wrapped_lines_in_column() {
        let ctx = egui::Context::default();
        apply_configured_fonts(&ctx, "");
        let rect = Rect::from_min_size(Pos2::new(20.0, 10.0), Vec2::new(200.0, 40.0));
        let mut actual_mid = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let wrapped_galley = ui.painter().layout_job(make_aligned_job(
                "Short line\nAnother longer wrapped lyric line",
                FontId::proportional(13.0),
                Color32::WHITE,
                200.0,
                egui::Align::Center,
            ));
            let draw_pos = galley_draw_pos(rect, 200.0, &wrapped_galley, egui::Align::Center);
            let placed = wrapped_galley.rect.translate(draw_pos.to_vec2());
            actual_mid = Some(placed.center().x);
        });
        out.textures_delta.clear();
        let expected_mid = rect.center().x;
        let actual_mid = actual_mid.expect("galley computed inside run_ui");
        assert!(
            (actual_mid - expected_mid).abs() < 1.5,
            "expected centered bbox around {expected_mid}, got {actual_mid}"
        );
    }

    #[test]
    fn mascot_and_karaoke_paint_without_empty_or_overlapping_clips() {
        let ctx = egui::Context::default();
        apply_configured_fonts_light(&ctx, "");
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(140.0, 24.0));
            crate::ui::icons::paint_mascot(
                ui.painter(),
                Rect::from_center_size(Pos2::new(12.0, 12.0), Vec2::splat(20.0)),
                Vec2::new(2.0, -2.0),
                Color32::WHITE,
            );
            paint_karaoke_in_rect(
                ui.painter(),
                rect,
                "A very long scrolling karaoke line that exceeds pill width by a large margin",
                0.45,
                FontId::proportional(12.0),
                Color32::GRAY,
                Color32::WHITE,
                0.0,
                egui::Align::LEFT,
            );
            render_wrapped_karaoke(
                ui,
                "Wrapped karaoke multi-line highlight check across two rows",
                0.55,
                FontId::proportional(13.0),
                Color32::WHITE,
                Color32::GRAY,
                140.0,
                egui::Align::Center,
            );
        });
        assert!(!out.shapes.is_empty());
        out.textures_delta.clear();
    }

    #[test]
    fn apply_configured_fonts_registers_bold_lyrics_family_and_line_resolution() {
        let ctx = egui::Context::default();
        apply_configured_fonts(&ctx, "");
        let mut has_bold_family = false;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            has_bold_family = ui.fonts(|f| !f.families().is_empty());
        });
        out.textures_delta.clear();
        assert!(has_bold_family);
        assert_eq!(lyrics_bold_font(14.0).family, lyrics_bold_family());

        let (p, r, t) = resolve_lyric_line_texts(
            "夜に駆ける",
            Some("Yoru ni kakeru"),
            Some("Racing into the night"),
            KaraokeLyricMode::Original,
            true,
            true,
        );
        assert_eq!(p, "夜に駆ける");
        assert_eq!(r, Some("Yoru ni kakeru"));
        assert_eq!(t, Some("Racing into the night"));
    }

    #[test]
    fn gallery_grid_and_calendar_and_blink_helpers_are_consistent() {
        let slots8 = gallery_grid_layout(8, 400.0, 160.0, 6.0, 1.0);
        assert_eq!(slots8.len(), 8);
        assert!((slots8[0].1.width() - slots8[7].1.width()).abs() < 1.0);
        assert!(slots8[4].1.top() > slots8[0].1.bottom());

        let (offset, days) = calendar_month_cells(2026, 2);
        assert_eq!(days, 28);
        assert!(offset < 7);
        assert_eq!(step_year_month(2026, 1, -1), (2025, 12));
        assert_eq!(step_year_month(2026, 12, 1), (2027, 1));

        assert!((mascot_blink_open_factor(0.0) - 1.0).abs() < f32::EPSILON);
        assert!(mascot_blink_open_factor(80.0) < 0.15);
        assert!((mascot_blink_open_factor(170.0) - 1.0).abs() < f32::EPSILON);

        let mut hist = VecDeque::new();
        for i in 0..35u32 {
            push_notification_history(
                &mut hist,
                NotificationItem {
                    id: format!("id-{i}"),
                    app_name: "Test".to_owned(),
                    title: format!("Title {i}"),
                    body: String::new(),
                    icon_rgba: None,
                    timestamp: 100 + i as u64,
                },
            );
        }
        assert_eq!(hist.len(), 30);
        assert_eq!(hist.front().map(|n| n.id.as_str()), Some("id-34"));
    }
}
