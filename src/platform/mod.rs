use std::sync::{Arc, mpsc};
use std::time::Instant;

use crate::calendar::CalendarEvent;
use crate::lyrics::LyricsData;
use crate::state::{ListenerStatus, NotificationItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumArt {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub dominant_rgb: Option<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTrack {
    pub title: String,
    pub artist: String,
    pub is_playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub album_art: Option<Arc<AlbumArt>>,
    pub updated_at: Instant,
}

pub fn decode_album_art(bytes: &[u8]) -> Option<Arc<AlbumArt>> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().ok()?;
    let resized = img.resize(160, 160, image::imageops::FilterType::Triangle);
    drop(img);
    let rgba_img = resized.into_rgba8();
    let width = rgba_img.width() as usize;
    let height = rgba_img.height() as usize;
    let rgba = rgba_img.into_raw();
    let dominant_rgb = extract_dominant_accent(&rgba);
    Some(Arc::new(AlbumArt {
        width,
        height,
        rgba,
        dominant_rgb,
    }))
}

pub fn extract_dominant_accent(rgba: &[u8]) -> Option<[u8; 3]> {
    let mut sum_r = 0.0f64;
    let mut sum_g = 0.0f64;
    let mut sum_b = 0.0f64;
    let mut weight_sum = 0.0f64;

    for &[r, g, b, a] in rgba.as_chunks::<4>().0.iter().step_by(4) {
        if a < 180 {
            continue;
        }
        let max_c = r.max(g).max(b);
        let min_c = r.min(g).min(b);
        if max_c < 45 {
            continue;
        }
        let sat = (max_c - min_c) as f64 / max_c as f64;
        if sat < 0.22 {
            continue;
        }
        let w = sat * (max_c as f64 / 255.0);
        sum_r += r as f64 * w;
        sum_g += g as f64 * w;
        sum_b += b as f64 * w;
        weight_sum += w;
    }

    if weight_sum <= 0.0 {
        return None;
    }

    let mut r = sum_r / weight_sum;
    let mut g = sum_g / weight_sum;
    let mut b = sum_b / weight_sum;
    let peak = r.max(g).max(b);
    if peak > 0.0 && peak < 180.0 {
        let boost = 180.0 / peak;
        r = (r * boost).min(255.0);
        g = (g * boost).min(255.0);
        b = (b * boost).min(255.0);
    }

    Some([r.round() as u8, g.round() as u8, b.round() as u8])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    TogglePlayPause,
    Next,
    Previous,
    SetShuffle(bool),
    SetRepeat(RepeatMode),
    SeekMs(u64),
}

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    Notification(NotificationItem),
    Removed(String),
    Status(ListenerStatus),
    Media(Option<MediaTrack>),
    Fullscreen(bool, bool),
    LyricsLoaded {
        artist: String,
        title: String,
        lyrics: LyricsData,
    },
    CalendarUpdated(Result<Vec<CalendarEvent>, String>),
}

#[allow(dead_code)] // Variants used across Linux/Windows font specs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamilyTarget {
    Proportional,
    Monospace,
    Both,
}

#[derive(Debug, Clone)]
pub struct FontFileSpec {
    pub name: String,
    pub path: String,
    pub family: FontFamilyTarget,
    pub face_indices: &'static [u32],
}

#[derive(Debug, Clone, Copy)]
pub struct HoverPollInput<'a> {
    pub window_title: &'a str,
    pub ctx_hovered: bool,
    pub pointer_busy: bool,
    pub currently_hovered: bool,
    pub visible: bool,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub animating: bool,
    pub top_offset_px: f32,
    pub trigger_width_px: f32,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub pixels_per_point: f32,
}

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub mod wayland_layer;

pub fn spawn_listener(tx: mpsc::Sender<PlatformEvent>, on_wake: Arc<dyn Fn() + Send + Sync>) {
    #[cfg(windows)]
    windows::spawn_listener(tx, on_wake);

    #[cfg(target_os = "linux")]
    linux::spawn_listener(tx, on_wake);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = tx.send(PlatformEvent::Status(ListenerStatus::Error(
            "Unsupported OS".to_owned(),
        )));
        on_wake();
    }
}

pub fn spawn_media_listener(
    tx: mpsc::Sender<PlatformEvent>,
    cmd_rx: mpsc::Receiver<MediaCommand>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    #[cfg(windows)]
    windows::spawn_media_listener(tx, cmd_rx, on_wake);

    #[cfg(target_os = "linux")]
    linux::spawn_media_listener(tx, cmd_rx, on_wake);

    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = (tx, cmd_rx, on_wake);
}

pub fn spawn_fullscreen_watcher(
    tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    #[cfg(windows)]
    windows::spawn_fullscreen_watcher(tx, on_wake);

    #[cfg(not(windows))]
    let _ = (tx, on_wake);
}

pub fn run_notification_spike() -> Result<String, String> {
    #[cfg(windows)]
    return windows::run_notification_spike();

    #[cfg(target_os = "linux")]
    return linux::run_notification_spike();

    #[cfg(not(any(windows, target_os = "linux")))]
    Err("Notification spike is only supported on Windows and Linux".to_owned())
}

pub fn overlay_resizable() -> bool {
    #[cfg(target_os = "linux")]
    return true;

    #[cfg(not(target_os = "linux"))]
    false
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn should_send_outer_position() -> bool {
    #[cfg(target_os = "linux")]
    return linux::should_send_outer_position();

    #[cfg(not(target_os = "linux"))]
    true
}

pub fn primary_monitor_logical_width(pixels_per_point: f32) -> f32 {
    #[cfg(windows)]
    return windows::primary_monitor_logical_width(pixels_per_point);

    #[cfg(target_os = "linux")]
    return linux::primary_monitor_logical_width(pixels_per_point);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = pixels_per_point;
        1920.0
    }
}

pub fn prepare_overlay_rules(window_title: &str, top_offset_px: f32) {
    #[cfg(target_os = "linux")]
    linux::prepare_overlay_rules(window_title, top_offset_px);

    #[cfg(not(target_os = "linux"))]
    let _ = (window_title, top_offset_px);
}

pub fn apply_overlay_window_setup(window_title: &str) -> bool {
    #[cfg(windows)]
    {
        windows::apply_no_activate_hook(window_title);
        true
    }

    #[cfg(target_os = "linux")]
    return linux::apply_no_activate_hook(window_title);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = window_title;
        true
    }
}

pub fn position_overlay_window(
    window_title: &str,
    x_logical: f32,
    y_logical: f32,
    w_logical: f32,
    h_logical: f32,
) {
    #[cfg(target_os = "linux")]
    linux::position_overlay_window(window_title, x_logical, y_logical, w_logical, h_logical);

    #[cfg(not(target_os = "linux"))]
    let _ = (window_title, x_logical, y_logical, w_logical, h_logical);
}

pub fn set_overlay_visible(window_title: &str, visible: bool) -> bool {
    #[cfg(windows)]
    {
        windows::set_overlay_visible(window_title, visible);
        true
    }

    #[cfg(not(windows))]
    {
        let _ = (window_title, visible);
        false
    }
}

pub fn set_overlay_click_through(window_title: &str, passthrough: bool) {
    #[cfg(windows)]
    windows::set_overlay_click_through(window_title, passthrough);

    #[cfg(not(windows))]
    let _ = (window_title, passthrough);
}

pub fn pin_window_top_y(window_title: &str, target_y_px: i32) {
    #[cfg(windows)]
    windows::pin_window_top_y(window_title, target_y_px);

    #[cfg(not(windows))]
    let _ = (window_title, target_y_px);
}

pub fn ensure_overlay_topmost(window_title: &str, force: bool) {
    #[cfg(target_os = "linux")]
    linux::ensure_overlay_topmost(window_title, force);

    #[cfg(not(target_os = "linux"))]
    let _ = (window_title, force);
}

pub fn sync_overlay_window(
    window_title: &str,
    x_logical: f32,
    y_logical: f32,
    w_logical: f32,
    h_logical: f32,
    notch_dock_mode: bool,
) {
    #[cfg(windows)]
    windows::sync_overlay_window(window_title, x_logical, y_logical, w_logical, h_logical, notch_dock_mode);

    #[cfg(target_os = "linux")]
    linux::sync_overlay_window(
        window_title,
        x_logical,
        y_logical,
        w_logical,
        h_logical,
        notch_dock_mode,
    );

    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = (window_title, x_logical, y_logical, w_logical, h_logical, notch_dock_mode);
}

pub fn is_cursor_in_screen_rect(
    left_px: i32,
    top_px: i32,
    right_px: i32,
    bottom_px: i32,
) -> Option<bool> {
    #[cfg(windows)]
    return windows::is_cursor_in_screen_rect(left_px, top_px, right_px, bottom_px);

    #[cfg(not(windows))]
    {
        let _ = (left_px, top_px, right_px, bottom_px);
        None
    }
}

pub fn is_cursor_in_top_trigger(
    top_offset_px: f32,
    trigger_width_px: f32,
    pixels_per_point: f32,
) -> Option<bool> {
    #[cfg(windows)]
    return windows::is_cursor_in_top_trigger(top_offset_px, trigger_width_px, pixels_per_point);

    #[cfg(target_os = "linux")]
    return linux::is_cursor_in_top_trigger(top_offset_px, trigger_width_px, pixels_per_point);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (top_offset_px, trigger_width_px, pixels_per_point);
        None
    }
}

pub fn detect_overlay_hover(input: HoverPollInput<'_>) -> bool {
    #[cfg(windows)]
    return windows::detect_overlay_hover(input);

    #[cfg(target_os = "linux")]
    return linux::detect_overlay_hover(input);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        input.visible && (input.pointer_busy || input.ctx_hovered)
    }
}

pub fn bring_window_to_front(window_title: &str) -> bool {
    #[cfg(windows)]
    return windows::bring_window_to_front(window_title);

    #[cfg(target_os = "linux")]
    return linux::bring_window_to_front(window_title);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = window_title;
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFont {
    pub name: String,
    pub path: String,
}

pub fn collect_fonts_from_dirs(dirs: &[std::path::PathBuf]) -> Vec<InstalledFont> {
    let mut seen = std::collections::HashSet::new();
    let mut fonts = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let sub = collect_fonts_from_dirs(&[path]);
                for f in sub {
                    if seen.insert(f.name.to_ascii_lowercase()) {
                        fonts.push(f);
                    }
                }
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let display = stem.replace(['_', '-'], " ").trim().to_owned();
            if display.is_empty() {
                continue;
            }
            if seen.insert(display.to_ascii_lowercase()) {
                fonts.push(InstalledFont {
                    name: display,
                    path: path.to_string_lossy().into_owned(),
                });
            }
        }
    }
    fonts.sort_by_key(|a| a.name.to_ascii_lowercase());
    fonts
}

pub fn list_installed_fonts() -> Vec<InstalledFont> {
    #[cfg(windows)]
    return windows::list_installed_fonts();

    #[cfg(target_os = "linux")]
    return linux::list_installed_fonts();

    #[cfg(not(any(windows, target_os = "linux")))]
    Vec::new()
}

#[allow(dead_code)] // Polled asynchronously by Windows fullscreen watcher; retained for Linux compatibility.
pub fn is_foreground_fullscreen() -> bool {
    #[cfg(windows)]
    return windows::is_foreground_fullscreen();

    #[cfg(target_os = "linux")]
    return linux::is_foreground_fullscreen();

    #[cfg(not(any(windows, target_os = "linux")))]
    false
}

pub fn base_fallback_fonts() -> Vec<FontFileSpec> {
    #[cfg(windows)]
    return windows::base_fallback_fonts();

    #[cfg(target_os = "linux")]
    return linux::base_fallback_fonts();

    #[cfg(not(any(windows, target_os = "linux")))]
    Vec::new()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CjkScript {
    JapaneseOrHan,
    Korean,
}

pub fn cjk_fallback_fonts_for(script: CjkScript) -> Vec<FontFileSpec> {
    #[cfg(windows)]
    return windows::cjk_fallback_fonts_for(script);

    #[cfg(target_os = "linux")]
    {
        let _ = script;
        linux::cjk_fallback_fonts()
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = script;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_dominant_accent_picks_vibrant_color_and_skips_grayscale() {
        let mut rgba = Vec::new();
        for _ in 0..80 {
            rgba.extend_from_slice(&[250, 250, 250, 255]);
        }
        for _ in 0..20 {
            rgba.extend_from_slice(&[215, 70, 78, 255]);
        }
        let accent = extract_dominant_accent(&rgba).expect("red accent");
        assert!(accent[0] > 180 && accent[1] < 110 && accent[2] < 120);

        let gray = [180, 180, 180, 255].repeat(32);
        assert_eq!(extract_dominant_accent(&gray), None);
    }
}
