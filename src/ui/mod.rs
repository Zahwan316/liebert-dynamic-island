mod icons;
mod settings;

use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, CursorIcon, FontId, Mesh, Pos2, Rect, RichText,
    Sense, Shape, Stroke, StrokeKind, TextureHandle, TextureId, TextureOptions, Vec2,
    ViewportCommand,
    text::{LayoutJob, TextFormat},
};
use eframe::epaint::{RectShape, Vertex, WHITE_UV};
use eframe::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

use crate::calendar::{
    CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events,
};
use crate::clock::format_clock;
use crate::config::{Config, IdleDisplayMode, KaraokeLyricMode};
use crate::lyrics::{LyricsData, LyricsRequest, active_karaoke};
use crate::platform::{
    self, CjkScript, FontFamilyTarget, FontFileSpec, HoverPollInput, InstalledFont, MediaCommand,
    MediaTrack, PlatformEvent, RepeatMode,
};
use crate::pomodoro::{PomodoroPhase, PomodoroTimer};
use crate::state::{IslandState, IslandVisualState, ListenerStatus, NotificationItem};
use crate::todo::TodoStore;
use crate::tray::{TrayAction, TrayController};
use crate::ui::icons::{IconKind, icon_button, paint_icon};
#[cfg(target_os = "linux")]
pub use crate::ui::settings::run_standalone_settings;
use crate::ui::settings::{
    SETTINGS_WINDOW_TITLE, SettingsTab, SettingsWindowState, render_settings_viewport,
};

pub const WINDOW_TITLE: &str = "Liebert Dynamic Island";

pub fn apply_configured_fonts(ctx: &egui::Context, font_name: &str) {
    let mut defs = egui::FontDefinitions::default();
    if !font_name.trim().is_empty()
        && let Some(found) = platform::list_installed_fonts()
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(font_name.trim()))
        && let Ok(bytes) = std::fs::read(&found.path)
    {
        let key = "user_selected_font".to_owned();
        defs.font_data
            .insert(key.clone(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        if let Some(fam) = defs.families.get_mut(&egui::FontFamily::Proportional) {
            fam.insert(0, key);
        }
    }
    ctx.set_fonts(defs);
    install_base_fallback_fonts(ctx);
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayBackendMode {
    Eframe,
    WaylandLayerShell,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
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
    for spec in platform::base_fallback_fonts() {
        load_font_spec(ctx, &spec);
    }
}

fn load_font_spec(ctx: &egui::Context, spec: &FontFileSpec) -> bool {
    let Ok(bytes) = std::fs::read(&spec.path) else {
        return false;
    };
    let blob: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes);
    let families_base = match spec.family {
        FontFamilyTarget::Proportional => vec![InsertFontFamily {
            family: egui::FontFamily::Proportional,
            priority: FontPriority::Lowest,
        }],
        FontFamilyTarget::Monospace => vec![InsertFontFamily {
            family: egui::FontFamily::Monospace,
            priority: FontPriority::Lowest,
        }],
        FontFamilyTarget::Both => vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: FontPriority::Lowest,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: FontPriority::Lowest,
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
    Music,
    Todo,
    Calendar,
    Pomodoro,
}

/// Fixed transparent window size big enough for the largest card, so hover/notch animations
/// never resize the OS window (the cause of open/close flicker).
pub fn permanent_canvas_size(config: &Config) -> Vec2 {
    let s = config.scale;
    let ear_pad = (12.0 * s).ceil() * 2.0;
    Vec2::new(
        (config.expanded_width.max(468.0) * s + ear_pad).ceil(),
        (292.0 * s + config.top_offset_px.max(12.0)).ceil(),
    )
}

pub struct AppChannels {
    pub event_rx: mpsc::Receiver<PlatformEvent>,
    pub lyrics_req_tx: mpsc::Sender<LyricsRequest>,
    pub media_cmd_tx: mpsc::Sender<MediaCommand>,
    pub calendar_cmd_tx: mpsc::Sender<CalendarCommand>,
}

pub struct IslandApp {
    config: Config,
    state: IslandState,
    channels: AppChannels,
    tray: Option<TrayController>,
    media: Option<MediaTrack>,
    lyrics: LyricsData,
    last_lyrics_key: Option<(String, String)>,
    album_art_tex: Option<TextureHandle>,
    album_art_key: Option<(String, String)>,
    gradient_tex: Option<TextureHandle>,
    gradient_key: Option<([u8; 4], [u8; 3], bool)>,
    loaded_cjk_scripts: u8,
    todos: TodoStore,
    calendar_events: Vec<CalendarEvent>,
    calendar_status: String,
    alerted_events: HashSet<(String, i64)>,
    active_tab: IslandTab,
    show_settings: bool,
    settings_focus_requested: bool,
    settings_os_focus_pending: bool,
    settings_tab: SettingsTab,
    new_todo_input: String,
    calendar_url_input: String,
    current_size: Vec2,
    applied_window_size: Vec2,
    last_passthrough: Option<bool>,
    notch_morph: f32,
    visibility_anim: f32,
    test_notif_seq: u32,
    last_frame: Instant,
    last_fullscreen_exclusive: bool,
    last_fullscreen_windowed: bool,
    no_activate_applied: bool,
    last_visible: bool,
    last_egui_hovered: bool,
    demo_deadline: Option<Instant>,
    pomodoro: PomodoroTimer,
    installed_fonts: Vec<InstalledFont>,
    font_search: String,
    applied_font_name: String,
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
        let initial_w = config.idle_width * config.scale + ear_pad;
        let initial_h = 30.0 * config.scale;
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
            album_art_tex: None,
            album_art_key: None,
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
            active_tab: IslandTab::Overview,
            show_settings: false,
            settings_focus_requested: false,
            settings_os_focus_pending: false,
            settings_tab: SettingsTab::Appearance,
            new_todo_input: String::new(),
            calendar_url_input,
            current_size: initial_size,
            applied_window_size: Vec2::ZERO,
            last_passthrough: None,
            notch_morph,
            visibility_anim: 1.0,
            test_notif_seq: 0,
            last_frame: now,
            last_fullscreen_exclusive: false,
            last_fullscreen_windowed: false,
            no_activate_applied: false,
            last_visible: true,
            last_egui_hovered: false,
            demo_deadline: demo_mode.then(|| now + Duration::from_secs(1)),
            pomodoro,
            installed_fonts: Vec::new(),
            font_search: String::new(),
            applied_font_name,
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

    fn push_test_notification(&mut self, now: Instant) {
        if self.state.paused {
            self.state.set_paused(false);
            self.sync_tray();
        }
        let samples = [
            ("WhatsApp", "Budi", "Meeting jam 14:00 jadi pindah ke Discord?"),
            ("Discord", "#general — Alex", "Deploy staging barusan sukses, cek log!"),
            ("Gmail", "GitHub", "[liebert-dynamic-island] PR #12 merged"),
            ("Google Chrome", "Download Complete", "release-v0.1.0-x86_64.zip siap dibuka"),
        ];
        let idx = (self.test_notif_seq as usize) % samples.len();
        self.test_notif_seq = self.test_notif_seq.wrapping_add(1);
        let (app_name, title, body) = samples[idx];
        self.state.push_notification(
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
                                && active_karaoke(&self.lyrics, self.current_lyric_ms(self.last_frame))
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
                        IdleDisplayMode::Clock => {
                            let needs_wide = self.config.clock_show_seconds
                                || !self.config.clock_24h
                                || !self.config.clock_timezone.eq_ignore_ascii_case("local");
                            let w = if needs_wide {
                                self.config.idle_width.max(162.0)
                            } else {
                                self.config.idle_width
                            };
                            Vec2::new(w * s, 30.0 * s)
                        }
                        _ => Vec2::new(self.config.idle_width * s, 30.0 * s),
                    }
                }
            }
            IslandVisualState::Compact => Vec2::new(348.0 * s, 60.0 * s),
            IslandVisualState::Expanded => {
                if self.state.active.is_none()
                    && self.active_tab == IslandTab::Music
                    && self.media.is_some()
                {
                    Vec2::new(self.config.expanded_width.max(468.0) * s, 286.0 * s)
                } else {
                    Vec2::new(self.config.expanded_width.max(430.0) * s, 184.0 * s)
                }
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
                load_font_spec(ctx, &spec);
            }
        }
        if need_kr && (self.loaded_cjk_scripts & 0b10) == 0 {
            self.loaded_cjk_scripts |= 0b10;
            for spec in platform::cjk_fallback_fonts_for(CjkScript::Korean) {
                load_font_spec(ctx, &spec);
            }
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

    fn sync_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.update_status(&self.state.listener_status, self.state.paused);
        }
    }

    fn current_playback_ms(&self, now: Instant) -> u64 {
        let Some(track) = &self.media else {
            return 0;
        };
        if track.is_playing {
            track.position_ms + now.saturating_duration_since(track.updated_at).as_millis() as u64
        } else {
            track.position_ms
        }
    }

    fn current_lyric_ms(&self, now: Instant) -> u64 {
        const KARAOKE_SYNC_LEAD_MS: i64 = 250;
        let base = self.current_playback_ms(now) as i64;
        (base + KARAOKE_SYNC_LEAD_MS + self.config.lyric_offset_ms).max(0) as u64
    }

    fn handle_events(&mut self, ctx: &egui::Context, now: Instant) -> (bool, bool) {
        let mut want_exit = false;
        let mut open_settings_req = false;
        if let Some(demo_at) = self.demo_deadline
            && now >= demo_at
        {
            self.demo_deadline = None;
            self.state.push_notification(
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
                        self.state.push_notification(item, now);
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
                PlatformEvent::Media(mut track) => {
                    if let (Some(prev), Some(next)) = (&self.media, &mut track)
                        && prev.is_playing
                        && next.is_playing
                        && prev.artist == next.artist
                        && prev.title == next.title
                    {
                        let cur_ms = self.current_playback_ms(now);
                        if next.position_ms.abs_diff(cur_ms) <= 450 {
                            next.position_ms = prev.position_ms;
                            next.updated_at = prev.updated_at;
                        }
                    }
                    self.media = track;
                    if let Some((artist, title)) = self
                        .media
                        .as_ref()
                        .map(|t| (t.artist.clone(), t.title.clone()))
                    {
                        let key = (artist.clone(), title.clone());
                        if self.last_lyrics_key.as_ref() != Some(&key) {
                            self.ensure_cjk_font_loaded_for_text(ctx, &title);
                            self.ensure_cjk_font_loaded_for_text(ctx, &artist);
                            self.last_lyrics_key = Some(key);
                            self.request_lyrics_for_current();
                        }
                    } else {
                        self.last_lyrics_key = None;
                        self.lyrics = LyricsData::NotFound;
                    }
                }
                PlatformEvent::LyricsLoaded {
                    artist,
                    title,
                    lyrics,
                } => {
                    if self.last_lyrics_key.as_ref() == Some(&(artist, title)) {
                        match &lyrics {
                            LyricsData::Synced(lines) => {
                                for l in lines.iter() {
                                    self.ensure_cjk_font_loaded_for_text(ctx, &l.text);
                                    if self.loaded_cjk_scripts == 0b11 {
                                        break;
                                    }
                                }
                            }
                            LyricsData::Plain(lines) => {
                                for l in lines.iter() {
                                    self.ensure_cjk_font_loaded_for_text(ctx, &l.text);
                                    if self.loaded_cjk_scripts == 0b11 {
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                        self.lyrics = lyrics;
                    }
                }
                PlatformEvent::CalendarUpdated(res) => match res {
                    Ok(events) => {
                        self.calendar_status = format!("Synced ({} events)", events.len());
                        self.calendar_events = events;
                    }
                    Err(err) => {
                        self.calendar_status = err;
                    }
                },
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
                        ctx.send_viewport_cmd(ViewportCommand::Close);
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
                        self.state.push_notification(
                            NotificationItem {
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
                            },
                            now,
                        );
                    }
                }
            }
        }

        if status_changed {
            self.sync_tray();
        }
        (want_exit, open_settings_req)
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

    fn accent_color32(&self) -> Color32 {
        let [r, g, b] = self.config.accent_color;
        Color32::from_rgb(r, g, b)
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
        let key = (base_bg, accent, pill_mode);
        if self.gradient_key == Some(key) && self.gradient_tex.is_some() {
            return;
        }
        let pixels = if pill_mode {
            build_vertical_gradient_pixels(base_bg, accent, 0.35, 0.85)
        } else {
            // Blend accent toward warm soft rose/coral highlight for bottom wash matching reference.
            let glow = [
                lerp_u8(accent[0], 246, 0.32),
                lerp_u8(accent[1], 195, 0.32),
                lerp_u8(accent[2], 198, 0.32),
            ];
            build_vertical_gradient_pixels(base_bg, glow, 0.50, 0.85)
        };
        let img = ColorImage::new([1, pixels.len()], pixels);
        if let Some(tex) = &mut self.gradient_tex {
            tex.set(img, TextureOptions::LINEAR);
        } else {
            self.gradient_tex = Some(ctx.load_texture("music_bg_grad", img, TextureOptions::LINEAR));
        }
        self.gradient_key = Some(key);
    }

    fn sync_album_art_texture(&mut self, ctx: &egui::Context) {
        let Some(track) = &self.media else {
            return;
        };
        if let Some(prev) = &self.album_art_key
            && prev.0 == track.artist
            && prev.1 == track.title
            && (track.album_art.is_none() || self.album_art_tex.is_some())
        {
            return;
        }
        self.album_art_key = Some((track.artist.clone(), track.title.clone()));
        if let Some(art) = &track.album_art {
            let img = ColorImage::from_rgba_unmultiplied([art.width, art.height], &art.rgba);
            if let Some(tex) = &mut self.album_art_tex {
                tex.set(img, TextureOptions::LINEAR);
            } else {
                self.album_art_tex = Some(ctx.load_texture("album_art", img, TextureOptions::LINEAR));
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

    fn sync_runtime_config(&mut self, prev_lyric_opts: (bool, KaraokeLyricMode, bool, String), now: Instant) {
        let new_lyric_opts = (
            self.config.romaji_lyrics,
            self.config.karaoke_lyric_mode,
            self.config.translate_lyrics,
            self.config.lyric_translate_lang.clone(),
        );
        if new_lyric_opts != prev_lyric_opts {
            self.request_lyrics_for_current();
        }
        self.state.set_auto_hide(self.config.auto_hide_when_idle, now);
        self.state
            .set_auto_hide_delay(Duration::from_secs(self.config.auto_hide_delay_secs));
        self.state
            .set_hide_in_fullscreen(self.config.hide_in_fullscreen);
        self.state
            .set_fullscreen_active(
                self.config.hide_in_fullscreen
                    && (self.last_fullscreen_exclusive
                        || (self.config.hide_in_windowed_fullscreen
                            && self.last_fullscreen_windowed)),
            );
        self.state
            .set_collapse_timeout(Duration::from_secs(self.config.collapse_timeout_secs));
        if let Some(tray) = &self.tray {
            tray.set_auto_hide_checked(self.config.auto_hide_when_idle);
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn apply_external_config(&mut self, cfg: Config, ctx: &egui::Context) {
        let prev_lyric_opts = (
            self.config.romaji_lyrics,
            self.config.karaoke_lyric_mode,
            self.config.translate_lyrics,
            self.config.lyric_translate_lang.clone(),
        );
        self.config = cfg;
        self.calendar_url_input = self.config.google_calendar_ics_url.clone();
        self.sync_runtime_config(prev_lyric_opts, Instant::now());
        ctx.request_repaint();
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn reload_todos(&mut self, ctx: &egui::Context) {
        self.todos = TodoStore::load();
        ctx.request_repaint();
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn trigger_test_notification(&mut self, ctx: &egui::Context) {
        self.push_test_notification(Instant::now());
        ctx.request_repaint();
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn send_calendar_command(&self, cmd: CalendarCommand) {
        let _ = self.channels.calendar_cmd_tx.send(cmd);
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

        let (want_exit, tray_open_settings) = self.handle_events(ctx, now);

        let target_morph = if self.config.notch_dock_mode { 1.0 } else { 0.0 };
        let morph_diff = target_morph - self.notch_morph;
        let morphing = morph_diff.abs() >= 0.002;
        if morphing {
            let m_t = 1.0 - (-13.0 * dt).exp();
            self.notch_morph += morph_diff * m_t;
            if (target_morph - self.notch_morph).abs() < 0.002 {
                self.notch_morph = target_morph;
            }
        } else {
            self.notch_morph = target_morph;
        }

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
            self.state.push_notification(
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
        let screen_w = platform::primary_monitor_logical_width(ppp);
        let trigger_w = self.config.idle_width.max(180.0) * self.config.scale;
        let pointer_busy = ctx.input(|i| i.pointer.any_down());
        let ctx_hovered = external_hovered
            || self.last_egui_hovered
            || ctx.input(|i| i.pointer.hover_pos().is_some());
        let is_animating_prev = (self.target_size() - self.current_size).length_sq() > 0.25;
        let hovered = if self.state.hide_in_fullscreen
            && self.state.fullscreen_active
            && self.state.active.is_none()
        {
            false
        } else if !self.last_visible {
            platform::is_cursor_in_top_trigger(top_y, trigger_w, ppp).unwrap_or(false)
        } else {
            let pill_x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
            let pad = if self.state.hovered { 4.0 } else { 0.0 };
            let win_over = platform::is_cursor_in_screen_rect(
                ((pill_x - pad) * ppp) as i32,
                ((top_y - pad) * ppp).max(0.0) as i32,
                ((pill_x + self.current_size.x + pad) * ppp) as i32,
                ((top_y + self.current_size.y + pad) * ppp) as i32,
            );
            match win_over {
                Some(over_pill) => {
                    over_pill
                        || (self.state.hovered
                            && platform::is_cursor_in_top_trigger(top_y, trigger_w, ppp)
                                .unwrap_or(false))
                }
                None => platform::detect_overlay_hover(HoverPollInput {
                    window_title: WINDOW_TITLE,
                    ctx_hovered,
                    pointer_busy,
                    currently_hovered: self.state.hovered,
                    visible: self.last_visible,
                    animating: is_animating_prev || morphing,
                    top_offset_px: top_y,
                    trigger_width_px: trigger_w,
                    pixels_per_point: ctx.pixels_per_point(),
                }),
            }
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
        if !visible && !vis_animating {
            self.album_art_tex = None;
            self.album_art_key = None;
            self.gradient_tex = None;
            self.gradient_key = None;
        }
        if visible != self.last_visible {
            self.last_visible = visible;
            // On Eframe (Windows/X11), keep the OS window shown and rely on transparent
            // rendering + WS_EX_TRANSPARENT passthrough when hidden; calling SW_HIDE
            // suppresses WM_PAINT so request_repaint() cannot wake logic() again.
            if mode == OverlayBackendMode::WaylandLayerShell {
                let _ = platform::set_overlay_visible(WINDOW_TITLE, visible);
            }
        }
        if mode == OverlayBackendMode::Eframe {
            if became_visible {
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
                platform::ensure_overlay_topmost(WINDOW_TITLE, true);
            } else if visible && !(self.state.hide_in_fullscreen && self.state.fullscreen_active) {
                platform::ensure_overlay_topmost(WINDOW_TITLE, false);
            }

            if self.show_settings && self.settings_os_focus_pending && !self.settings_focus_requested
            {
                if platform::bring_window_to_front(SETTINGS_WINDOW_TITLE) {
                    self.settings_os_focus_pending = false;
                } else {
                    ctx.request_repaint();
                }
            }
        }

        let target = self.target_size();
        let t = 1.0 - (-16.0 * dt).exp();
        self.current_size += (target - self.current_size) * t;
        let animating = (target - self.current_size).length_sq() > 0.0225;
        if !animating {
            self.current_size = target;
        }

        // The overlay keeps one permanent transparent canvas at y = 0: opening, closing and
        // notch morphing only repaint inside it, so the OS window is never resized mid-animation.
        let canvas_size = permanent_canvas_size(&self.config);
        if (canvas_size - self.applied_window_size).length_sq() > 0.25 {
            self.applied_window_size = canvas_size;
            let win_x = ((screen_w - canvas_size.x) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(canvas_size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(win_x, 0.0)));
            platform::position_overlay_window(
                WINDOW_TITLE,
                win_x,
                0.0,
                canvas_size.x,
                canvas_size.y,
            );
        }
        if self.last_visible {
            platform::pin_window_top_y(WINDOW_TITLE, 0);
        }

        if want_visible && visible {
            let pill_x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
            platform::set_overlay_hitbox(
                (pill_x * ppp).floor() as i32,
                (top_y * ppp).floor() as i32,
                ((pill_x + self.current_size.x) * ppp).ceil() as i32,
                ((top_y + self.current_size.y) * ppp).ceil() as i32,
            );
        } else {
            platform::set_overlay_hitbox(0, 0, -1, -1);
        }

        let passthrough = !want_visible || !hovered;
        if self.last_passthrough != Some(passthrough)
            && platform::set_overlay_click_through(WINDOW_TITLE, passthrough)
        {
            self.last_passthrough = Some(passthrough);
        }
        let rounded_size = Vec2::new(self.current_size.x.round(), self.current_size.y.round());
        #[cfg(target_os = "linux")]
        if mode == OverlayBackendMode::Eframe
            && ((rounded_size - self.applied_size).length_sq() > 0.25
                || (top_y - self.applied_top_y).abs() > 0.25
                || hover_changed
                || became_visible)
        {
            self.applied_size = rounded_size;
            self.applied_top_y = top_y;
            let screen_w = platform::primary_monitor_logical_width(ctx.pixels_per_point());
            let x = ((screen_w - rounded_size.x) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(rounded_size));
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

        let playing_music =
            self.config.show_lyrics && self.media.as_ref().is_some_and(|m| m.is_playing);
        let playing_lyrics = playing_music && matches!(self.lyrics, LyricsData::Synced(_));
        let showing_idle_bars = self.state.visual == IslandVisualState::Idle && playing_music;
        let showing_live_seconds = self.state.visual == IslandVisualState::Idle
            && self.effective_idle_mode() == IdleDisplayMode::Clock
            && self.config.clock_show_seconds;
        let pomodoro_running = self.pomodoro.running;

        let repaint_after =
            if animating || morphing || vis_animating || hover_changed || fs_changed {
            Duration::ZERO
        } else if !visible {
            Duration::from_millis(250)
        } else if playing_lyrics && self.config.karaoke_lyrics {
            Duration::from_millis(16)
        } else if showing_idle_bars {
            Duration::from_millis(33)
        } else if playing_lyrics {
            Duration::from_millis(100)
        } else if showing_live_seconds || pomodoro_running {
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
        } else if self.state.hovered {
            Duration::from_millis(500)
        } else {
            Duration::from_millis(200)
        };

        if mode == OverlayBackendMode::Eframe {
            if repaint_after.is_zero() {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(repaint_after);
            }
        }

        let open_settings_requested = if mode == OverlayBackendMode::WaylandLayerShell {
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
        let vis = self.visibility_anim.clamp(0.0, 1.0);
        if vis <= 0.005 && !(allow_inline_settings && self.show_settings) {
            return;
        }

        let ctx = ui.ctx().clone();
        let now = Instant::now();

        let ear_w = self.notch_ear_width();
        let card_rect = ui.max_rect();
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
        let border_alpha = ((self.config.border_alpha as f32) * top_corner_factor).round() as u8;

        let is_music_card = self.state.visual == IslandVisualState::Expanded
            && self.state.active.is_none()
            && self.active_tab == IslandTab::Music
            && self.media.is_some();

        let vis = self.visibility_anim.clamp(0.0, 1.0);
        if vis > 0.005 {
            let top_y = self.effective_top_offset();
            let slide_y = -((self.current_size.y + top_y) * (1.0 - vis)) * 0.65;
            let border_alpha = ((border_alpha as f32) * vis).round() as u8;

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                let canvas = ui.max_rect();
                let card_x = canvas.left() + ((canvas.width() - self.current_size.x) * 0.5).max(0.0);
                let card_y = canvas.top() + top_y + slide_y;
                let card_rect = Rect::from_min_size(Pos2::new(card_x, card_y), self.current_size);
                self.last_egui_hovered = ui.rect_contains_pointer(card_rect);

                let (pad_x, pad_y) = if is_music_card {
                    let base_bg = [
                        self.config.bg_color[0],
                        self.config.bg_color[1],
                        self.config.bg_color[2],
                        ((self.config.bg_color[3].max(235) as f32) * vis).round() as u8,
                    ];
                    let accent_rgb = self.effective_accent_rgb();
                    self.ensure_gradient_texture(&ctx, base_bg, accent_rgb, self.notch_morph < 0.5);
                    paint_island_surface(
                        ui.painter(),
                        card_rect,
                        SurfaceStyle {
                            ear_w,
                            top_radius,
                            bot_radius,
                            fill: Color32::from_rgba_unmultiplied(
                                base_bg[0], base_bg[1], base_bg[2], base_bg[3],
                            ),
                            grad_tex: self.gradient_tex.as_ref(),
                            border_alpha,
                        },
                    );
                    (16.0 + ear_w, 10.0)
                } else {
                    let base = self.bg_color32();
                    let fill = Color32::from_rgba_unmultiplied(
                        base.r(),
                        base.g(),
                        base.b(),
                        ((base.a() as f32) * vis).round() as u8,
                    );
                    paint_island_surface(
                        ui.painter(),
                        card_rect,
                        SurfaceStyle {
                            ear_w,
                            top_radius,
                            bot_radius,
                            fill,
                            grad_tex: None,
                            border_alpha,
                        },
                    );
                    let py = if self.state.visual == IslandVisualState::Idle {
                        5.0
                    } else {
                        9.0
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
                                let expand_alpha = ((self.current_size.y - 60.0 * self.config.scale)
                                    / (56.0 * self.config.scale))
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
            return;
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
                apply_configured_fonts(&ctx, &self.applied_font_name);
                self.loaded_cjk_scripts = 0;
            }
            self.pomodoro.sync_durations(
                self.config.pomodoro_focus_mins,
                self.config.pomodoro_short_break_mins,
                self.config.pomodoro_long_break_mins,
            );
            self.state.set_auto_hide(self.config.auto_hide_when_idle, now);
            self.state
                .set_auto_hide_delay(Duration::from_secs(self.config.auto_hide_delay_secs));
            self.state
                .set_hide_in_fullscreen(self.config.hide_in_fullscreen);
            self.state.set_fullscreen_active(
                self.config.hide_in_fullscreen
                    && (self.last_fullscreen_exclusive
                        || (self.config.hide_in_windowed_fullscreen
                            && self.last_fullscreen_windowed)),
            );
            self.state
                .set_collapse_timeout(Duration::from_secs(self.config.collapse_timeout_secs));
            if let Some(tray) = &self.tray {
                tray.set_auto_hide_checked(self.config.auto_hide_when_idle);
            }
            self.sync_runtime_config(prev_lyric_opts, now);
            if let Err(err) = self.config.save() {
                eprintln!("Failed to save config: {err}");
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
        let _ = self.step_logic(ctx, OverlayBackendMode::Eframe, false);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_island_ui(ui, true);
    }
}

impl IslandApp {
    fn render_idle(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
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
                            let shape =
                                RectShape::filled(badge_rect, badge_rad, Color32::WHITE)
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

                        let bars_w = 18.0 * s;
                        ui.add_space(3.0 * s);
                        let bars_reserve = 22.0 * s;
                        let text_w = (ui.available_width() - bars_reserve).max(40.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(text_w, 20.0 * s),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_clip_rect(ui.max_rect());
                                ui.spacing_mut().item_spacing.x = 4.0 * s;
                                let (note_rect, _) = ui.allocate_exact_size(
                                    Vec2::new(12.0 * s, 18.0 * s),
                                    Sense::hover(),
                                );
                                ui.painter().text(
                                    note_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "♪",
                                    FontId::proportional(11.0 * s),
                                    muted,
                                );
                                if let Some(k) = active {
                                    let lyric_text = match self.config.karaoke_lyric_mode {
                                        KaraokeLyricMode::Romaji => k.romaji.unwrap_or(k.text),
                                        KaraokeLyricMode::Translate => {
                                            k.translation.unwrap_or(k.text)
                                        }
                                        KaraokeLyricMode::Original => k.text,
                                    };
                                    if self.config.karaoke_lyrics {
                                        render_karaoke_line(
                                            ui,
                                            lyric_text,
                                            k.progress,
                                            FontId::proportional(11.5 * s),
                                            muted,
                                            text_col,
                                        );
                                    } else {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(lyric_text)
                                                    .font(FontId::proportional(11.5 * s))
                                                    .color(text_col),
                                            )
                                            .truncate(),
                                        );
                                    }
                                } else {
                                    let fallback = if track.artist.is_empty() {
                                        track.title.clone()
                                    } else {
                                        format!("{} — {}", track.artist, track.title)
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(fallback)
                                                .font(FontId::proportional(11.5 * s))
                                                .color(text_col),
                                        )
                                        .truncate(),
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
            IdleDisplayMode::Clock => {
                let (time_str, tz_label) = format_clock(
                    chrono::Utc::now(),
                    &self.config.clock_timezone,
                    self.config.clock_24h,
                    self.config.clock_show_seconds,
                );
                let show_tz = !self.config.clock_timezone.eq_ignore_ascii_case("local");
                let dot_color = if self.state.paused {
                    Color32::from_gray(140)
                } else {
                    accent
                };
                let mut job = LayoutJob::default();
                job.append(
                    &time_str,
                    0.0,
                    TextFormat {
                        font_id: FontId::monospace(12.0 * s),
                        color: text_col,
                        ..Default::default()
                    },
                );
                if show_tz {
                    job.append(
                        &format!("  {tz_label}"),
                        0.0,
                        TextFormat {
                            font_id: FontId::proportional(9.5 * s),
                            color: muted,
                            ..Default::default()
                        },
                    );
                }
                render_dot_label(ui, dot_color, 3.0 * s, job);
                ui.centered_and_justified(|ui| {
                    ui.horizontal(|ui| {
                        let (dot_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(9.0 * s), Sense::hover());
                        paint_icon(
                            ui.painter(),
                            dot_rect,
                            IconKind::Dot,
                            if self.state.paused {
                                Color32::from_gray(140)
                            } else {
                                accent
                            },
                        );
                        ui.label(
                            RichText::new(time_str)
                                .font(FontId::monospace(12.0 * s))
                                .color(text_col)
                                .strong(),
                        );
                        if show_tz {
                            ui.label(
                                RichText::new(tz_label)
                                    .font(FontId::proportional(9.5 * s))
                                    .color(muted),
                            );
                        }
                    });
                });
                return;
            }
            IdleDisplayMode::NextEvent if self.config.show_calendar => {
                let now_utc = chrono::Utc::now().timestamp();
                if let Some(ev) = upcoming_events(&self.calendar_events, now_utc, 1).first() {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0 * s;
                        ui.label(
                            RichText::new(format_countdown(ev.start_utc, ev.end_utc, now_utc))
                                .font(FontId::proportional(10.5 * s))
                                .color(accent)
                                .strong(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&ev.summary)
                                    .font(FontId::proportional(11.0 * s))
                                    .color(text_col),
                            )
                            .truncate(),
                        );
                    });
                    return;
                }
            }
            IdleDisplayMode::Pomodoro if self.config.show_pomodoro => {
                let phase_col = match self.pomodoro.phase {
                    PomodoroPhase::Focus => accent,
                    PomodoroPhase::ShortBreak => Color32::from_rgb(80, 210, 145),
                    PomodoroPhase::LongBreak => Color32::from_rgb(135, 175, 255),
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
                render_dot_label(ui, dot_color, 3.0 * s, job);
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
                ..Default::default()
            },
        );
        render_dot_label(ui, dot_color, 3.0 * s, job);
        ui.centered_and_justified(|ui| {
            let (dot_color, text) = if self.state.paused {
                (Color32::from_gray(140), "Paused")
            } else {
                (accent, "Liebert")
            };
            ui.horizontal(|ui| {
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(9.5 * s), Sense::hover());
                paint_icon(ui.painter(), dot_rect, IconKind::Dot, dot_color);
                ui.label(
                    RichText::new(text)
                        .font(FontId::proportional(11.0 * s))
                        .color(muted),
                );
            });
        });
    }

    fn render_compact(&self, ui: &mut egui::Ui) {
        let Some(item) = &self.state.active else {
            return;
        };
        let accent = self.effective_accent();
        let brand = app_brand_color(&item.app_name, accent);
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let queued = self.state.queued_count();

        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0 * s, 2.0 * s);
            render_app_avatar(ui, &item.app_name, brand, 28.0 * s, s);
            let badge_reserve = if queued > 0 { 36.0 * s } else { 0.0 };
            let text_w = (ui.available_width() - badge_reserve).max(60.0);
            ui.allocate_ui_with_layout(
                Vec2::new(text_w, 34.0 * s),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.spacing_mut().item_spacing.y = 2.0 * s;
                    ui.allocate_ui_with_layout(
                        Vec2::new(text_w, 17.0 * s),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 5.0 * s;
                            if !item.app_name.eq_ignore_ascii_case(&item.title) {
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
                                                .font(FontId::proportional(9.0 * s))
                                                .color(brand)
                                                .strong(),
                                        );
                                    });
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&item.title)
                                        .font(FontId::proportional(12.5 * s))
                                        .color(text_col)
                                        .strong(),
                                )
                                .truncate(),
                            );
                        },
                    );
                    let sub = if item.body.is_empty() {
                        &item.app_name
                    } else {
                        &item.body
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(sub)
                                .font(FontId::proportional(11.0 * s))
                                .color(muted),
                        )
                        .truncate(),
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
                                        .font(FontId::proportional(10.5 * s))
                                        .color(text_col),
                                );
                            });
                    },
                );
            }
        });
    }

    fn render_expanded(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        if let Some(item) = &self.state.active {
            let brand = app_brand_color(&item.app_name, accent);
            let row_h = 26.0 * s;
            let right_w = 96.0 * s;
            let left_w = (ui.available_width() - right_w - 6.0 * s).max(80.0);
            let mut open_settings_clicked = false;
            let mut dismiss_clicked = false;
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left_w, row_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_clip_rect(ui.max_rect());
                        ui.spacing_mut().item_spacing.x = 6.0 * s;
                        render_app_avatar(ui, &item.app_name, brand, 22.0 * s, s);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&item.app_name)
                                    .font(FontId::proportional(11.5 * s))
                                    .color(brand)
                                    .strong(),
                            )
                            .truncate(),
                        );
                        let queued = self.state.queued_count();
                        if queued > 0 {
                            ui.label(
                                RichText::new(format!("+{queued}"))
                                    .font(FontId::proportional(10.5 * s))
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
                                .min_size(Vec2::new(58.0 * s, 24.0 * s))
                                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 24))
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .clicked()
                        {
                            dismiss_clicked = true;
                        }
                    },
                );
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(&item.title)
                    .font(FontId::proportional(13.5 * s))
                    .color(text_col)
                    .strong(),
            );
            if !item.body.is_empty() {
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(&item.body)
                            .font(FontId::proportional(12.0 * s))
                            .color(muted),
                    )
                    .wrap(),
                );
            }
            if open_settings_clicked {
                self.open_settings(SettingsTab::Appearance, ctx);
            }
            if dismiss_clicked {
                self.state.dismiss_active(now);
                ctx.request_repaint();
            }
            return;
        }

        let is_music_card = self.active_tab == IslandTab::Music && self.media.is_some();
        let row_h = 24.0 * s;
        let right_w = 28.0 * s;
        let left_w = (ui.available_width() - right_w - 6.0 * s).max(160.0);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, row_h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.spacing_mut().item_spacing.x = 4.0 * s;
                    render_pill_tab(
                        ui,
                        &mut self.active_tab,
                        IslandTab::Overview,
                        None,
                        "Home",
                        accent,
                        s,
                    );
                    if self.config.show_lyrics {
                        render_pill_tab(
                            ui,
                            &mut self.active_tab,
                            IslandTab::Music,
                            Some(IconKind::MusicNote),
                            "Music",
                            accent,
                            s,
                        );
                    }
                    if self.config.show_todo {
                        let label = format!("{}", self.todos.pending_count());
                        render_pill_tab(
                            ui,
                            &mut self.active_tab,
                            IslandTab::Todo,
                            Some(IconKind::TodoCheck),
                            &label,
                            accent,
                            s,
                        );
                    }
                    if self.config.show_calendar {
                        render_pill_tab(
                            ui,
                            &mut self.active_tab,
                            IslandTab::Calendar,
                            Some(IconKind::Calendar),
                            "Calendar",
                            accent,
                            s,
                        );
                    }
                    if self.config.show_pomodoro {
                        render_pill_tab(
                            ui,
                            &mut self.active_tab,
                            IslandTab::Pomodoro,
                            None,
                            "⏱ Timer",
                            accent,
                            s,
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
                    if render_icon_button(
                        ui,
                        Vec2::splat(24.0 * s),
                        "⚙",
                        12.0 * s,
                        muted,
                        Some((Color32::from_rgba_unmultiplied(255, 255, 255, 16), 10)),
                        "Open Settings",
                    ) {
                        self.open_settings(SettingsTab::Appearance, ctx);
                    }
                },
            );
        });
        if !is_music_card {
            ui.add_space(2.0);
            ui.separator();
            ui.add_space(2.0);
        } else {
            ui.add_space(6.0);
        }

        match self.active_tab {
            IslandTab::Overview => self.render_overview_tab(ui, now),
            IslandTab::Music => self.render_music_tab(ui, ctx, now),
            IslandTab::Todo => self.render_todo_tab(ui),
            IslandTab::Calendar => self.render_calendar_tab(ui),
            IslandTab::Pomodoro => self.render_pomodoro_tab(ui, now),
        }
    }

    fn render_overview_tab(&mut self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        let (time_str, tz_label) = format_clock(
            chrono::Utc::now(),
            &self.config.clock_timezone,
            self.config.clock_24h,
            self.config.clock_show_seconds,
        );
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 22.0 * s),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0 * s;
                ui.label(
                    RichText::new(time_str)
                        .font(FontId::monospace(15.0 * s))
                        .color(text_col)
                        .strong(),
                );
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 16))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(tz_label)
                                .font(FontId::proportional(9.5 * s))
                                .color(muted),
                        );
                    });
            },
        );
        ui.add_space(4.0);

        if self.config.show_lyrics {
            if let Some(track) = &self.media {
                let pos_ms = self.current_lyric_ms(now);
                let lyric = active_karaoke(&self.lyrics, pos_ms)
                    .map(|k| match self.config.karaoke_lyric_mode {
                        KaraokeLyricMode::Romaji => k.romaji.unwrap_or(k.text),
                        KaraokeLyricMode::Translate => k.translation.unwrap_or(k.text),
                        KaraokeLyricMode::Original => k.text,
                    })
                    .unwrap_or("No synced lyric");
                if render_overview_row(
                    ui,
                    IconKind::MusicNote,
                    &format!("{} — {}", track.title, lyric),
                    accent,
                    s,
                    true,
                ) {
                    self.active_tab = IslandTab::Music;
                }
            } else {
                render_overview_row(ui, IconKind::MusicNote, "No music playing", muted, s, false);
            }
        }

        if self.config.show_calendar {
            let now_utc = chrono::Utc::now().timestamp();
            let next = upcoming_events(&self.calendar_events, now_utc, 1);
            let cal_text = if let Some(ev) = next.first() {
                format!(
                    "{} ({})",
                    ev.summary,
                    format_countdown(ev.start_utc, ev.end_utc, now_utc)
                )
            } else if self.config.google_calendar_ics_url.is_empty() {
                "Connect Google Calendar in Settings".to_owned()
            } else {
                "No upcoming events".to_owned()
            };
            if render_overview_summary_row(ui, "📅", accent, &cal_text, text_col, s, true) {
                self.active_tab = IslandTab::Calendar;
            }
        }

        if self.config.show_todo {
            let pending = self.todos.pending_count();
            if render_overview_summary_row(
                ui,
                "☑",
                muted,
                &format!("{pending} pending task(s)"),
                muted,
                s,
                true,
            ) {
                self.active_tab = IslandTab::Todo;
            }
        }

        if self.config.show_pomodoro {
            let status = if self.pomodoro.running { "Running" } else { "Paused" };
            let pom_text = format!(
                "{} • {} ({status})",
                self.pomodoro.phase.label(),
                self.pomodoro.format_mm_ss()
            );
            if render_overview_summary_row(ui, "⏱", accent, &pom_text, text_col, s, true) {
                self.active_tab = IslandTab::Pomodoro;
            }
        }
    }

    fn render_pomodoro_tab(&mut self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let phase_col = match self.pomodoro.phase {
            PomodoroPhase::Focus => accent,
            PomodoroPhase::ShortBreak => Color32::from_rgb(80, 210, 145),
            PomodoroPhase::LongBreak => Color32::from_rgb(135, 175, 255),
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
                let fill_rect = Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, bar_rect.height()));
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
                    let toggle_label = if self.pomodoro.running { "Pause" } else { "Start" };
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
        let s = self.config.scale;

        self.sync_album_art_texture(ctx);
        let pos_ms = self.current_playback_ms(now);
        let lyric_ms = self.current_lyric_ms(now);
        let Some(track) = self.media.as_ref() else {
            ui.label(
                RichText::new("Play a song on Spotify or any media app...")
                    .font(FontId::proportional(11.5 * s))
                    .color(muted),
            );
            return;
        };

        let mut pending_seek: Option<u64> = None;
        let mut pending_shuffle: Option<bool> = None;
        let mut pending_repeat: Option<RepeatMode> = None;
        let total_w = ui.available_width();
        let col_gap = 14.0 * s;
        let left_w = (total_w * 0.43).max(148.0 * s);
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
                    render_centered_truncated_text(
                        ui,
                        &track.title,
                        FontId::proportional(13.0 * s),
                        text_col,
                    );
                    if !track.artist.is_empty() {
                        render_centered_truncated_text(
                            ui,
                            &track.artist,
                            FontId::proportional(11.0 * s),
                            muted,
                        );
                    }

                    ui.add_space(6.0 * s);
                    if let Some(seek_ms) =
                        render_playbar(ui, pos_ms, track.duration_ms, accent, muted, s)
                    {
                        let _ = self.channels.media_cmd_tx.send(MediaCommand::SeekMs(seek_ms));
                        pending_seek = Some(seek_ms);
                    }

                    ui.add_space(4.0 * s);
                    let controls_w = 146.0 * s;
                    ui.allocate_ui_with_layout(
                        Vec2::new(controls_w, 30.0 * s),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0 * s;
                            let shuf_col = if track.shuffle { accent } else { muted };
                            if render_icon_button(
                                ui,
                                Vec2::splat(24.0 * s),
                                "🔀",
                                12.0 * s,
                                shuf_col,
                                None,
                                "Shuffle",
                            ) {
                                let next_shuf = !track.shuffle;
                                let _ = self
                                    .channels
                                    .media_cmd_tx
                                    .send(MediaCommand::SetShuffle(next_shuf));
                                pending_shuffle = Some(next_shuf);
                            }

                            if render_icon_button(
                                ui,
                                Vec2::splat(26.0 * s),
                                "⏮",
                                13.5 * s,
                                text_col,
                                None,
                                "Previous",
                            ) {
                                let _ = self.channels.media_cmd_tx.send(MediaCommand::Previous);
                            }

                            let play_icon = if track.is_playing { "⏸" } else { "▶" };
                            if render_icon_button(
                                ui,
                                Vec2::splat(30.0 * s),
                                play_icon,
                                14.5 * s,
                                Color32::WHITE,
                                Some((Color32::from_rgba_unmultiplied(255, 255, 255, 26), 15)),
                                "Play / Pause",
                            ) {
                                let _ = self
                                    .channels
                                    .media_cmd_tx
                                    .send(MediaCommand::TogglePlayPause);
                            }

                            if render_icon_button(
                                ui,
                                Vec2::splat(26.0 * s),
                                "⏭",
                                13.5 * s,
                                text_col,
                                None,
                                "Next",
                            ) {
                                let _ = self.channels.media_cmd_tx.send(MediaCommand::Next);
                            }

                            let (rep_icon, rep_col) = match track.repeat {
                                RepeatMode::Off => ("🔁", muted),
                                RepeatMode::All => ("🔁", accent),
                                RepeatMode::One => ("🔂", accent),
                            };
                            if render_icon_button(
                                ui,
                                Vec2::splat(24.0 * s),
                                rep_icon,
                                12.0 * s,
                                rep_col,
                                None,
                                "Repeat (Off / All / One)",
                            ) {
                                let next_rep = track.repeat.next();
                                let _ = self
                                    .channels
                                    .media_cmd_tx
                                    .send(MediaCommand::SetRepeat(next_rep));
                                pending_repeat = Some(next_rep);
                            }
                        },
                    );
                },
            );

            ui.add_space(col_gap);

            // Right column: Multi-line wrapped Karaoke Lyric, Romaji, Translation & Upcoming preview.
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, body_h),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    egui::ScrollArea::vertical()
                        .id_salt("music_lyrics_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match &self.lyrics {
                            LyricsData::Loading => {
                                ui.label(
                                    RichText::new("Fetching lyrics...")
                                        .font(FontId::proportional(12.0 * s))
                                        .color(muted),
                                );
                            }
                            LyricsData::NotFound => {
                                ui.label(
                                    RichText::new("Lyrics not found for this track")
                                        .font(FontId::proportional(12.0 * s))
                                        .color(muted),
                                );
                            }
                            LyricsData::Synced(_) | LyricsData::Plain(_) => {
                                if let Some(k) = active_karaoke(&self.lyrics, lyric_ms) {
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
                                        k.progress
                                    } else {
                                        1.0
                                    };
                                    let romaji_primary = self.config.karaoke_lyric_mode
                                        == KaraokeLyricMode::Romaji
                                        && k.romaji.is_some();
                                    let translate_primary = self.config.karaoke_lyric_mode
                                        == KaraokeLyricMode::Translate
                                        && k.translation.is_some();
                                    let primary_text = if romaji_primary {
                                        k.romaji.unwrap_or(k.text)
                                    } else if translate_primary {
                                        k.translation.unwrap_or(k.text)
                                    } else {
                                        k.text
                                    };
                                    let wrap_w = ui.available_width().max(60.0);
                                    render_wrapped_karaoke(
                                        ui,
                                        primary_text,
                                        orig_prog,
                                        FontId::proportional(14.0 * s),
                                        text_col,
                                        dim_white,
                                        wrap_w,
                                    );

                                    if romaji_primary || translate_primary {
                                        ui.add_space(2.0 * s);
                                        render_wrapped_karaoke(
                                            ui,
                                            k.text,
                                            orig_prog,
                                            FontId::proportional(11.5 * s),
                                            lit_romaji,
                                            dim_romaji,
                                            wrap_w,
                                        );
                                    } else if self.config.romaji_lyrics
                                        && let Some(rom) = k.romaji
                                    {
                                        ui.add_space(2.0 * s);
                                        render_wrapped_karaoke(
                                            ui,
                                            rom,
                                            orig_prog,
                                            FontId::proportional(11.5 * s),
                                            lit_romaji,
                                            dim_romaji,
                                            wrap_w,
                                        );
                                    }

                                    if !translate_primary
                                        && self.config.translate_lyrics
                                        && let Some(tr) = k.translation
                                    {
                                        ui.add_space(2.0 * s);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(tr)
                                                    .font(FontId::proportional(10.5 * s))
                                                    .color(Color32::from_gray(215)),
                                            )
                                            .wrap(),
                                        );
                                    }

                                    if let Some(next_line) = k.next_text {
                                        ui.add_space(8.0 * s);
                                        let (next_primary, next_secondary) = if self
                                            .config
                                            .karaoke_lyric_mode
                                            == KaraokeLyricMode::Romaji
                                            && let Some(next_rom) = k.next_romaji
                                        {
                                            (next_rom, Some(next_line))
                                        } else if self.config.karaoke_lyric_mode
                                            == KaraokeLyricMode::Translate
                                            && let Some(next_tr) = k.next_translation
                                        {
                                            (next_tr, Some(next_line))
                                        } else {
                                            (
                                                next_line,
                                                if self.config.romaji_lyrics {
                                                    k.next_romaji
                                                } else {
                                                    None
                                                },
                                            )
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(next_primary)
                                                    .font(FontId::proportional(12.5 * s))
                                                    .color(Color32::from_rgba_unmultiplied(
                                                        255, 255, 255, 58,
                                                    ))
                                                    .strong(),
                                            )
                                            .wrap(),
                                        );
                                        if let Some(next_sub) = next_secondary {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(next_sub)
                                                        .font(FontId::proportional(10.5 * s))
                                                        .color(Color32::from_rgba_unmultiplied(
                                                            accent_rgb[0],
                                                            accent_rgb[1],
                                                            accent_rgb[2],
                                                            58,
                                                        )),
                                                )
                                                .wrap(),
                                            );
                                        }
                                        if self.config.translate_lyrics
                                            && let Some(next_tr) = k.next_translation
                                        {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(next_tr)
                                                        .font(FontId::proportional(10.0 * s))
                                                        .color(Color32::from_rgba_unmultiplied(
                                                            220, 220, 220, 50,
                                                        )),
                                                )
                                                .wrap(),
                                            );
                                        }
                                    }
                                }
                            }
                        });
                },
            );
        });
        if let Some(m) = &mut self.media {
            if let Some(seek_ms) = pending_seek {
                m.position_ms = seek_ms;
                m.updated_at = now;
            }
            if let Some(next_shuf) = pending_shuffle {
                m.shuffle = next_shuf;
            }
            if let Some(next_rep) = pending_repeat {
                m.repeat = next_rep;
            }
        }
    }

    fn render_todo_tab(&mut self, ui: &mut egui::Ui) {
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} pending", self.todos.pending_count()))
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

        let mut toggle_id = None;
        let mut remove_id = None;
        let mut dirty = false;

        egui::ScrollArea::vertical()
            .max_height(72.0 * s)
            .show(ui, |ui| {
                if self.todos.items.is_empty() {
                    ui.label(
                        RichText::new("No tasks yet. Click '+ Manage / Add'.")
                            .font(FontId::proportional(11.0 * s))
                            .color(muted),
                    );
                }
                for item in &self.todos.items {
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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        });
                    });
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
        let accent = self.accent_color32();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        if self.config.google_calendar_ics_url.is_empty() {
            ui.label(
                RichText::new("Connect your Google Calendar (.ics) to see upcoming events.")
                    .font(FontId::proportional(11.5 * s))
                    .color(muted),
            );
            ui.add_space(4.0);
            if ui
                .button("Configure Google Calendar")
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                self.open_settings(SettingsTab::Calendar, ui.ctx());
            }
            return;
        }

        let now_utc = chrono::Utc::now().timestamp();
        let upcoming = upcoming_events(&self.calendar_events, now_utc, 3);
        if upcoming.is_empty() {
            ui.label(
                RichText::new(format!("No upcoming events ({})", self.calendar_status))
                    .font(FontId::proportional(11.5 * s))
                    .color(muted),
            );
            if ui
                .small_button("Refresh")
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                let _ = self.channels.calendar_cmd_tx.send(CalendarCommand::Refresh);
            }
            return;
        }

        for ev in upcoming {
            let row_w = ui.available_width();
            let cd_w = 72.0 * s;
            let left_w = (row_w - cd_w - 6.0 * s).max(100.0);
            ui.allocate_ui_with_layout(
                Vec2::new(row_w, 22.0 * s),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(left_w, 22.0 * s),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_clip_rect(ui.max_rect());
                            ui.spacing_mut().item_spacing.x = 6.0 * s;
                            ui.label(
                                RichText::new(format_local_time(ev.start_utc))
                                    .font(FontId::monospace(11.0 * s))
                                    .color(accent),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&ev.summary)
                                        .font(FontId::proportional(11.5 * s))
                                        .color(text_col),
                                )
                                .truncate(),
                            );
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(cd_w, 22.0 * s),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.set_clip_rect(ui.max_rect());
                            ui.label(
                                RichText::new(format_countdown(ev.start_utc, ev.end_utc, now_utc))
                                    .font(FontId::proportional(10.5 * s))
                                    .color(muted),
                            );
                        },
                    );
                },
            );
        }
    }
}

fn render_wrapped_karaoke(
    ui: &mut egui::Ui,
    text: &str,
    progress: f32,
    font: FontId,
    lit_color: Color32,
    dim_color: Color32,
    max_width: f32,
) {
    let wrap_w = max_width.max(40.0);
    let dim_galley = ui
        .painter()
        .layout(text.to_owned(), font.clone(), dim_color, wrap_w);
    let lit_galley = ui.painter().layout(text.to_owned(), font, lit_color, wrap_w);
    let (rect, _) = ui.allocate_exact_size(dim_galley.size(), Sense::hover());

    let clip = ui.clip_rect();
    ui.painter()
        .with_clip_rect(clip)
        .galley(rect.min, dim_galley.clone(), Color32::TRANSPARENT);

    let p = progress.clamp(0.0, 1.0);
    let total_w: f32 = dim_galley.rows.iter().map(|r| r.rect().width()).sum();
    let mut rem = total_w * p;
    for placed in &dim_galley.rows {
        if rem <= 0.5 {
            break;
        }
        let rr = placed.rect().translate(rect.min.to_vec2());
        let rw = rr.width().max(0.0);
        let lit_w = rem.min(rw);
        if lit_w > 0.5 {
            let row_clip = Rect::from_min_max(rr.min, Pos2::new(rr.left() + lit_w, rr.bottom()))
                .intersect(clip);
            if row_clip.width() > 0.0 && row_clip.height() > 0.0 {
                ui.painter().with_clip_rect(row_clip).galley(
                    rect.min,
                    lit_galley.clone(),
                    Color32::TRANSPARENT,
                );
            }
        }
        rem -= rw;
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    ((a as f32) + ((b as f32) - (a as f32)) * t).round() as u8
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
    border_alpha: u8,
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
        border_alpha,
    } = style;

    if ear_w > 0.5 {
        let mesh = build_notch_surface_mesh(
            rect,
            ear_w,
            bot_radius,
            fill,
            grad_tex.map(|tex| tex.id()),
        );
        painter.add(Shape::mesh(mesh));
        return;
    }

    let max_r = ((rect.height() * 0.5)
        .floor()
        .min((rect.width() * 0.5).floor()))
    .clamp(0.0, 28.0) as u8;
    let clamped_bot = bot_radius.min(max_r);
    let clamped_top = top_radius.min(max_r);
    let body_corners = CornerRadius {
        nw: clamped_top,
        ne: clamped_top,
        sw: clamped_bot,
        se: clamped_bot,
    };

    let top_y = if ear_w > 0.5 {
        rect.top() - 1.0
    } else {
        rect.top()
    };
    let body_rect = Rect::from_min_max(
        Pos2::new(rect.left() + ear_w, top_y),
        Pos2::new(rect.right() - ear_w, rect.bottom()),
    );

    if let Some(tex) = grad_tex {
        let shape = RectShape::filled(body_rect, body_corners, Color32::WHITE)
            .with_texture(tex.id(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)));
        painter.add(shape);
    } else {
        painter.rect_filled(body_rect, body_corners, fill);
    }

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

    if ear_w <= 0.5 {
        return;
    }

    let ear_color = if grad_tex.is_some() {
        Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), fill.a())
    } else {
        fill
    };
    let r = ear_w.min(rect.height() * 0.45);
    const SEGS: usize = 14;
    let seam_overlap = 0.75;

    // Top-left concave ear (0.75px seam overlap eliminates subpixel AA gap against body_rect)
    let mut left_mesh = Mesh::default();
    let left_apex = Pos2::new(body_rect.left() + seam_overlap, top_y);
    let left_center = Pos2::new(body_rect.left() - r, top_y + r);
    left_mesh.colored_vertex(left_apex, ear_color);
    for i in 0..=SEGS {
        let t = i as f32 / SEGS as f32;
        let angle = -FRAC_PI_2 + FRAC_PI_2 * t;
        let p = Pos2::new(
            left_center.x + r * angle.cos(),
            left_center.y + r * angle.sin(),
        );
        left_mesh.colored_vertex(p, ear_color);
        if i > 0 {
            left_mesh.add_triangle(0, i as u32, (i + 1) as u32);
        }
    }
    left_mesh.colored_vertex(
        Pos2::new(body_rect.left() + seam_overlap, top_y + r),
        ear_color,
    );
    left_mesh.add_triangle(0, (SEGS + 1) as u32, (SEGS + 2) as u32);
    painter.add(Shape::mesh(left_mesh));

    // Top-right concave ear
    let mut right_mesh = Mesh::default();
    let right_apex = Pos2::new(body_rect.right() - seam_overlap, top_y);
    let right_center = Pos2::new(body_rect.right() + r, top_y + r);
    right_mesh.colored_vertex(right_apex, ear_color);
    for i in 0..=SEGS {
        let t = i as f32 / SEGS as f32;
        let angle = -FRAC_PI_2 - (PI - FRAC_PI_2) * t;
        let p = Pos2::new(
            right_center.x + r * angle.cos(),
            right_center.y + r * angle.sin(),
        );
        right_mesh.colored_vertex(p, ear_color);
        if i > 0 {
            right_mesh.add_triangle(0, i as u32, (i + 1) as u32);
        }
    }
    right_mesh.colored_vertex(
        Pos2::new(body_rect.right() - seam_overlap, top_y + r),
        ear_color,
    );
    right_mesh.add_triangle(0, (SEGS + 1) as u32, (SEGS + 2) as u32);
    painter.add(Shape::mesh(right_mesh));
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

fn render_app_avatar(
    ui: &mut egui::Ui,
    app_name: &str,
    accent: Color32,
    size: f32,
    scale: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let initial = app_name
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('L')
        .to_uppercase()
        .to_string();
    let painter = ui.painter();
    painter.circle_filled(
        rect.center(),
        size * 0.5,
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 44),
    );
    painter.circle_stroke(rect.center(), size * 0.5, Stroke::new(1.0, accent));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        FontId::proportional((size * 0.48).max(9.0 * scale)),
        Color32::WHITE,
    );
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

fn render_centered_truncated_text(ui: &mut egui::Ui, text: &str, font: FontId, color: Color32) {
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
    let pos = Pos2::new(rect.center().x - galley.size().x * 0.5, rect.top());
    ui.painter()
        .with_clip_rect(rect)
        .galley(pos, galley, Color32::TRANSPARENT);
}

fn render_dot_label(ui: &mut egui::Ui, dot_color: Color32, dot_radius: f32, job: LayoutJob) {
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    let gap = dot_radius * 2.0;
    let total_w = dot_radius * 2.0 + gap + galley.size().x;
    let start_x = rect.center().x - total_w * 0.5;
    let cy = rect.center().y;
    ui.painter()
        .circle_filled(Pos2::new(start_x + dot_radius, cy), dot_radius, dot_color);
    ui.painter().galley(
        Pos2::new(start_x + dot_radius * 2.0 + gap, cy - galley.size().y * 0.5),
        galley,
        Color32::TRANSPARENT,
    );
}

fn paint_vector_icon(painter: &egui::Painter, rect: Rect, icon: &str, color: Color32) -> bool {
    let c = rect.center();
    let u = rect.width().min(rect.height());
    let bar = |cx: f32, w: f32, h: f32| {
        painter.rect_filled(
            Rect::from_center_size(Pos2::new(cx, c.y), Vec2::new(w, h)),
            CornerRadius::same(1),
            color,
        );
    };
    let tri = |pts: [Pos2; 3]| {
        painter.add(Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE));
    };
    match icon {
        "⚙" => {
            let stroke = Stroke::new((u * 0.07).max(1.2), color);
            painter.circle_stroke(c, u * 0.22, stroke);
            painter.circle_filled(c, u * 0.07, color);
            for i in 0..8 {
                let a = i as f32 * PI / 4.0;
                let (sin, cos) = a.sin_cos();
                painter.line_segment(
                    [
                        Pos2::new(c.x + cos * u * 0.27, c.y + sin * u * 0.27),
                        Pos2::new(c.x + cos * u * 0.37, c.y + sin * u * 0.37),
                    ],
                    Stroke::new((u * 0.1).max(1.6), color),
                );
            }
            true
        }
        "▶" => {
            let x = c.x + u * 0.04;
            tri([
                Pos2::new(x - u * 0.16, c.y - u * 0.22),
                Pos2::new(x - u * 0.16, c.y + u * 0.22),
                Pos2::new(x + u * 0.22, c.y),
            ]);
            true
        }
        "⏸" => {
            bar(c.x - u * 0.1, u * 0.12, u * 0.42);
            bar(c.x + u * 0.1, u * 0.12, u * 0.42);
            true
        }
        "⏮" => {
            bar(c.x - u * 0.22, u * 0.08, u * 0.4);
            tri([
                Pos2::new(c.x + u * 0.2, c.y - u * 0.2),
                Pos2::new(c.x + u * 0.2, c.y + u * 0.2),
                Pos2::new(c.x - u * 0.12, c.y),
            ]);
            true
        }
        "⏭" => {
            bar(c.x + u * 0.22, u * 0.08, u * 0.4);
            tri([
                Pos2::new(c.x - u * 0.2, c.y - u * 0.2),
                Pos2::new(c.x - u * 0.2, c.y + u * 0.2),
                Pos2::new(c.x + u * 0.12, c.y),
            ]);
            true
        }
        _ => false,
    }
}

fn render_icon_button(
    ui: &mut egui::Ui,
    size: Vec2,
    icon: &str,
    font_size: f32,
    color: Color32,
    bg: Option<(Color32, u8)>,
    hover_text: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    if let Some((fill, radius)) = bg {
        let actual_fill = if hovered {
            Color32::from_rgba_unmultiplied(
                fill.r(),
                fill.g(),
                fill.b(),
                fill.a().saturating_add(24),
            )
        } else {
            fill
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius), actual_fill);
    } else if hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((size.y * 0.5).round() as u8),
            Color32::from_rgba_unmultiplied(255, 255, 255, 20),
        );
    }
    let icon_color = if hovered { Color32::WHITE } else { color };
    if !paint_vector_icon(ui.painter(), rect, icon, icon_color) {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            FontId::proportional(font_size),
            icon_color,
        );
    }
    let clicked = resp.clicked();
    if !hover_text.is_empty() {
        resp.on_hover_text(hover_text);
    }
    clicked
}

fn render_overview_summary_row(
    ui: &mut egui::Ui,
    icon: &str,
    icon_color: Color32,
    text: &str,
    text_color: Color32,
    scale: f32,
    clickable: bool,
) -> bool {
    let mut clicked = false;
    let row_w = ui.available_width();
    ui.allocate_ui_with_layout(
        Vec2::new(row_w, 19.0 * scale),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_clip_rect(ui.max_rect());
            ui.spacing_mut().item_spacing.x = 6.0 * scale;
            let (icon_rect, _) =
                ui.allocate_exact_size(Vec2::new(16.0 * scale, 16.0 * scale), Sense::hover());
            ui.painter().text(
                icon_rect.center(),
                egui::Align2::CENTER_CENTER,
                icon,
                FontId::proportional(11.5 * scale),
                icon_color,
            );
            let mut label = egui::Label::new(
                RichText::new(text)
                    .font(FontId::proportional(11.5 * scale))
                    .color(text_color),
            )
            .truncate();
            if clickable {
                label = label.sense(Sense::click());
            }
            if ui.add(label).clicked() && clickable {
                clicked = true;
            }
        },
    );
    clicked
}

fn render_overview_row(
    ui: &mut egui::Ui,
    icon: IconKind,
    text: &str,
    color: Color32,
    scale: f32,
    clickable: bool,
) -> bool {
    let h = 21.0 * scale;
    let w = ui.available_width().max(60.0);
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(w, h), sense);
    if clickable {
        resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        if clickable && (resp.hovered() || resp.is_pointer_button_down_on()) {
            let alpha = if resp.is_pointer_button_down_on() { 24 } else { 14 };
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(7),
                Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
            );
        }
        let icon_sz = 12.0 * scale;
        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 6.0 * scale + icon_sz * 0.5, rect.center().y),
            Vec2::splat(icon_sz),
        );
        paint_icon(ui.painter(), icon_rect, icon, color);

        let text_left = icon_rect.right() + 6.0 * scale;
        let text_rect = Rect::from_min_max(Pos2::new(text_left, rect.top()), rect.max);
        let galley = ui.painter().layout(
            text.to_owned(),
            FontId::proportional(11.5 * scale),
            color,
            f32::INFINITY,
        );
        let text_y = rect.center().y - galley.size().y * 0.5;
        ui.painter()
            .with_clip_rect(text_rect.intersect(ui.clip_rect()))
            .galley(Pos2::new(text_left, text_y), galley, Color32::TRANSPARENT);
    }
    clickable && resp.clicked()
}

fn render_pill_tab(
    ui: &mut egui::Ui,
    active: &mut IslandTab,
    value: IslandTab,
    icon: Option<IconKind>,
    label: &str,
    accent: Color32,
    scale: f32,
) {
    let selected = *active == value;
    let text_col = if selected {
        Color32::WHITE
    } else {
        Color32::from_gray(185)
    };
    let font = FontId::proportional(11.0 * scale);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, text_col);
    let pad_x = 8.0 * scale;
    let icon_sz = if icon.is_some() { 11.5 * scale } else { 0.0 };
    let icon_gap = if icon.is_some() { 4.5 * scale } else { 0.0 };
    let w = (pad_x * 2.0 + icon_sz + icon_gap + galley.size().x).max(38.0 * scale);
    let h = 23.0 * scale;

    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 52)
        } else if resp.is_pointer_button_down_on() {
            Color32::from_rgba_unmultiplied(255, 255, 255, 28)
        } else if resp.hovered() {
            Color32::from_rgba_unmultiplied(255, 255, 255, 20)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 10)
        };
        let stroke = if selected {
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 150),
            )
        } else {
            Stroke::NONE
        };
        let corner = CornerRadius::same(10);
        ui.painter().rect_filled(rect, corner, fill);
        if stroke != Stroke::NONE {
            ui.painter()
                .rect_stroke(rect, corner, stroke, StrokeKind::Inside);
        }

        let mut cursor_x = rect.left() + pad_x;
        if let Some(kind) = icon {
            let icon_rect = Rect::from_center_size(
                Pos2::new(cursor_x + icon_sz * 0.5, rect.center().y),
                Vec2::splat(icon_sz),
            );
            paint_icon(ui.painter(), icon_rect, kind, text_col);
            cursor_x += icon_sz + icon_gap;
        }
        let text_pos = Pos2::new(cursor_x, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(text_pos, galley, Color32::TRANSPARENT);
    }
    if resp.clicked() {
        *active = value;
    }
}

fn render_karaoke_line(
    ui: &mut egui::Ui,
    text: &str,
    progress: f32,
    font: FontId,
    dim_color: Color32,
    lit_color: Color32,
) {
    let avail_w = ui.available_width().max(24.0);
    let dim_galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), dim_color);
    let lit_galley = ui.painter().layout_no_wrap(text.to_owned(), font, lit_color);
    let h = dim_galley.size().y.max(16.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, h), Sense::hover());

    let text_w = dim_galley.size().x.min(avail_w);
    let scroll_x = if dim_galley.size().x > avail_w {
        let max_scroll = dim_galley.size().x - avail_w;
        -(max_scroll * progress.clamp(0.0, 1.0))
    } else {
        0.0
    };
    let draw_y = rect.center().y - dim_galley.size().y * 0.5;
    let draw_pos = Pos2::new(rect.left() + scroll_x, draw_y);
    let base_painter = ui.painter().with_clip_rect(rect);
    base_painter.galley(draw_pos, dim_galley, Color32::TRANSPARENT);

    let lit_width = (text_w * progress.clamp(0.0, 1.0)).max(0.0);
    if lit_width > 0.5 {
        let lit_clip = Rect::from_min_max(rect.min, Pos2::new(rect.left() + lit_width, rect.bottom()));
        ui.painter()
            .with_clip_rect(lit_clip.intersect(rect))
            .galley(draw_pos, lit_galley, Color32::TRANSPARENT);
    }
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
            let (rect, resp) = ui.allocate_exact_size(
                Vec2::new(bar_w, 14.0 * scale),
                Sense::click_and_drag(),
            );
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
        assert_eq!(truncate_with_ellipsis("short", |t| t.chars().count() <= 8), "short");
        let cut = truncate_with_ellipsis("a very long song title", |t| t.chars().count() <= 8);
        assert!(cut.ends_with('…') && cut.chars().count() <= 8, "got: {cut}");
        assert_eq!(truncate_with_ellipsis("abc", |t| t.is_empty()), "…");
    }

    #[test]
    fn app_brand_color_maps_known_apps_and_falls_back() {
        let fb = Color32::from_rgb(10, 20, 30);
        assert_eq!(app_brand_color("WhatsApp", fb), Color32::from_rgb(37, 211, 102));
        assert_eq!(app_brand_color("Discord", fb), Color32::from_rgb(88, 101, 242));
        assert_eq!(app_brand_color("Gmail", fb), Color32::from_rgb(234, 67, 53));
        assert_eq!(app_brand_color("Google Chrome", fb), Color32::from_rgb(66, 133, 244));
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
}
