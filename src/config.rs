use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum IdleDisplayMode {
    #[default]
    Auto,
    Clock,
    Lyrics,
    NextEvent,
    Pomodoro,
    Battery,
    Minimal,
}

impl IdleDisplayMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto (Lyrics -> Clock)",
            Self::Clock => "Clock",
            Self::Lyrics => "Lyrics / Now Playing",
            Self::NextEvent => "Next Calendar Event",
            Self::Pomodoro => "Pomodoro Timer",
            Self::Battery => "Battery",
            Self::Minimal => "Minimal Dot",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum KaraokeLyricMode {
    #[default]
    Original,
    Romaji,
    Translate,
}

impl KaraokeLyricMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Romaji => "Romaji (when available)",
            Self::Translate => "Translate (when available)",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TaskbarFormat {
    #[default]
    CompactText,
    IconValue,
    MiniBar,
}

impl TaskbarFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::CompactText => "Compact Text",
            Self::IconValue => "Icon + Value",
            Self::MiniBar => "Mini Bar",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TaskbarPosition {
    #[default]
    LeftOfTray,
    RightOfTray,
    TopOfTaskbar,
}

impl TaskbarPosition {
    pub fn label(self) -> &'static str {
        match self {
            Self::LeftOfTray => "Left of Tray",
            Self::RightOfTray => "Right of Tray",
            Self::TopOfTaskbar => "Top of Taskbar",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub scale: f32,
    pub idle_width: f32,
    pub expanded_width: f32,
    pub top_offset_px: f32,
    pub notch_dock_mode: bool,
    pub collapse_timeout_secs: u64,
    pub auto_hide_when_idle: bool,
    pub auto_hide_delay_secs: u64,
    pub hide_in_fullscreen: bool,
    pub hide_in_windowed_fullscreen: bool,
    pub idle_display: IdleDisplayMode,
    pub clock_timezone: String,
    pub clock_24h: bool,
    pub clock_show_seconds: bool,
    pub bg_color: [u8; 4],
    pub accent_color: [u8; 3],
    pub text_color: [u8; 3],
    pub border_alpha: u8,
    pub corner_radius: u8,
    pub font_name: String,
    pub text_align: TextAlign,
    pub show_notifications: bool,
    pub show_lyrics: bool,
    pub dynamic_album_color: bool,
    pub compact_album_gradient: bool,
    pub show_pill_translation: bool,
    pub karaoke_lyrics: bool,
    pub karaoke_lyric_mode: KaraokeLyricMode,
    pub romaji_lyrics: bool,
    pub translate_lyrics: bool,
    pub lyric_translate_lang: String,
    pub lyric_offset_ms: i64,
    pub show_todo: bool,
    pub show_calendar: bool,
    pub show_pomodoro: bool,
    pub show_battery: bool,
    pub idle_show_network_speed: bool,
    pub start_with_os: bool,
    pub pomodoro_focus_mins: u64,
    pub pomodoro_short_break_mins: u64,
    pub pomodoro_long_break_mins: u64,
    pub google_calendar_ics_url: String,
    pub taskbar_monitor_enabled: bool,
    pub taskbar_show_cpu: bool,
    pub taskbar_show_ram: bool,
    pub taskbar_show_disk: bool,
    pub taskbar_show_network: bool,
    pub taskbar_show_battery: bool,
    pub taskbar_format: TaskbarFormat,
    pub taskbar_position: TaskbarPosition,
    pub taskbar_interval_ms: u64,
    pub battery_low_alerts: bool,
    pub battery_warn_pct: u8,
    pub battery_critical_pct: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scale: 1.0,
            idle_width: 140.0,
            expanded_width: 390.0,
            top_offset_px: 10.0,
            notch_dock_mode: true,
            collapse_timeout_secs: 5,
            auto_hide_when_idle: false,
            auto_hide_delay_secs: 5,
            hide_in_fullscreen: true,
            hide_in_windowed_fullscreen: true,
            idle_display: IdleDisplayMode::Auto,
            clock_timezone: "Local".to_owned(),
            clock_24h: true,
            clock_show_seconds: false,
            bg_color: [16, 16, 20, 240],
            accent_color: [110, 195, 255],
            text_color: [245, 245, 248],
            border_alpha: 36,
            corner_radius: 22,
            font_name: String::new(),
            text_align: TextAlign::Left,
            show_notifications: true,
            show_lyrics: true,
            dynamic_album_color: true,
            compact_album_gradient: true,
            show_pill_translation: true,
            karaoke_lyrics: true,
            karaoke_lyric_mode: KaraokeLyricMode::Original,
            romaji_lyrics: true,
            translate_lyrics: true,
            lyric_translate_lang: "id".to_owned(),
            lyric_offset_ms: 0,
            show_todo: true,
            show_calendar: true,
            show_pomodoro: true,
            show_battery: true,
            idle_show_network_speed: false,
            start_with_os: false,
            pomodoro_focus_mins: 25,
            pomodoro_short_break_mins: 5,
            pomodoro_long_break_mins: 15,
            google_calendar_ics_url: String::new(),
            taskbar_monitor_enabled: true,
            taskbar_show_cpu: true,
            taskbar_show_ram: true,
            taskbar_show_disk: false,
            taskbar_show_network: true,
            taskbar_show_battery: true,
            taskbar_format: TaskbarFormat::CompactText,
            taskbar_position: TaskbarPosition::LeftOfTray,
            taskbar_interval_ms: 1000,
            battery_low_alerts: true,
            battery_warn_pct: 20,
            battery_critical_pct: 10,
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "liebert-dynamic-island")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        Self::from_toml_str(&raw)
    }

    pub fn from_toml_str(raw: &str) -> Self {
        let mut cfg: Self = toml::from_str(raw).unwrap_or_default();
        cfg.scale = cfg.scale.clamp(0.75, 1.5);
        cfg.idle_width = cfg.idle_width.clamp(110.0, 240.0);
        cfg.expanded_width = cfg.expanded_width.clamp(320.0, 520.0);
        cfg.top_offset_px = cfg.top_offset_px.clamp(0.0, 60.0);
        cfg.collapse_timeout_secs = cfg.collapse_timeout_secs.clamp(2, 15);
        cfg.auto_hide_delay_secs = cfg.auto_hide_delay_secs.clamp(0, 60);
        cfg.corner_radius = cfg.corner_radius.clamp(8, 28);
        cfg.lyric_offset_ms = cfg.lyric_offset_ms.clamp(-5000, 5000);
        cfg.pomodoro_focus_mins = cfg.pomodoro_focus_mins.clamp(1, 120);
        cfg.pomodoro_short_break_mins = cfg.pomodoro_short_break_mins.clamp(1, 60);
        cfg.pomodoro_long_break_mins = cfg.pomodoro_long_break_mins.clamp(1, 90);
        cfg.taskbar_interval_ms = cfg.taskbar_interval_ms.clamp(500, 5000);
        cfg.battery_warn_pct = cfg.battery_warn_pct.clamp(5, 50);
        cfg.battery_critical_pct = cfg
            .battery_critical_pct
            .clamp(3, 30)
            .min(cfg.battery_warn_pct);
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, encoded)
    }

    pub fn apply_preset(&mut self, name: &str) {
        match name {
            "obsidian" => {
                self.bg_color = [16, 16, 20, 240];
                self.accent_color = [110, 195, 255];
                self.text_color = [245, 245, 248];
                self.border_alpha = 36;
            }
            "midnight" => {
                self.bg_color = [15, 23, 42, 235];
                self.accent_color = [96, 165, 250];
                self.text_color = [241, 245, 249];
                self.border_alpha = 48;
            }
            "amoled" => {
                self.bg_color = [0, 0, 0, 252];
                self.accent_color = [52, 211, 153];
                self.text_color = [255, 255, 255];
                self.border_alpha = 28;
            }
            "slate" => {
                self.bg_color = [36, 40, 48, 236];
                self.accent_color = [244, 162, 97];
                self.text_color = [248, 250, 252];
                self.border_alpha = 52;
            }
            "crimson" => {
                self.bg_color = [30, 29, 28, 248];
                self.accent_color = [224, 92, 98];
                self.text_color = [248, 246, 245];
                self.border_alpha = 28;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let cfg = Config::default();
        assert!(!cfg.auto_hide_when_idle);
        assert_eq!(cfg.scale, 1.0);
        assert_eq!(cfg.top_offset_px, 10.0);
        assert!(cfg.notch_dock_mode);
        assert_eq!(cfg.collapse_timeout_secs, 5);
        assert_eq!(cfg.auto_hide_delay_secs, 5);
        assert!(cfg.hide_in_fullscreen);
        assert_eq!(cfg.idle_display, IdleDisplayMode::Auto);
        assert_eq!(cfg.clock_timezone, "Local");
        assert!(cfg.show_notifications && cfg.show_lyrics && cfg.show_todo && cfg.show_calendar);
        assert!(cfg.dynamic_album_color && cfg.compact_album_gradient && cfg.show_pill_translation);
        assert!(cfg.karaoke_lyrics && cfg.romaji_lyrics && cfg.translate_lyrics);
        assert_eq!(cfg.karaoke_lyric_mode, KaraokeLyricMode::Original);
        assert_eq!(cfg.text_align, TextAlign::Left);
        assert_eq!(cfg.lyric_translate_lang, "id");
        assert_eq!(cfg.lyric_offset_ms, 0);
    }

    #[test]
    fn toml_round_trip() {
        let mut cfg = Config {
            auto_hide_when_idle: true,
            auto_hide_delay_secs: 12,
            hide_in_fullscreen: true,
            hide_in_windowed_fullscreen: true,
            show_notifications: false,
            compact_album_gradient: false,
            show_pill_translation: false,
            karaoke_lyrics: false,
            karaoke_lyric_mode: KaraokeLyricMode::Romaji,
            text_align: TextAlign::Right,
            romaji_lyrics: false,
            translate_lyrics: true,
            lyric_translate_lang: "en".to_owned(),
            lyric_offset_ms: -450,
            idle_display: IdleDisplayMode::Clock,
            clock_timezone: "Asia/Jakarta".to_owned(),
            clock_24h: false,
            clock_show_seconds: true,
            top_offset_px: 14.5,
            notch_dock_mode: false,
            collapse_timeout_secs: 8,
            google_calendar_ics_url: "https://calendar.google.com/calendar/ical/test/basic.ics"
                .to_owned(),
            ..Config::default()
        };
        cfg.apply_preset("amoled");
        let raw = toml::to_string_pretty(&cfg).expect("serialize config");
        let parsed = Config::from_toml_str(&raw);
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn legacy_partial_toml_loads_with_new_defaults() {
        let legacy =
            "auto_hide_when_idle = true\ntop_offset_px = 18.0\ncollapse_timeout_secs = 7\n";
        let parsed = Config::from_toml_str(legacy);
        assert!(parsed.auto_hide_when_idle);
        assert_eq!(parsed.top_offset_px, 18.0);
        assert_eq!(parsed.collapse_timeout_secs, 7);
        assert_eq!(parsed.scale, 1.0);
        assert_eq!(parsed.text_align, TextAlign::Left);
        assert_eq!(parsed.bg_color, Config::default().bg_color);
    }

    #[test]
    fn invalid_toml_falls_back_to_default() {
        let parsed = Config::from_toml_str("not valid = [toml");
        assert_eq!(parsed, Config::default());
    }
}
