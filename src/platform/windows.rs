use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Media::MediaPlaybackAutoRepeatMode;
use windows::Storage::Streams::DataReader;
use windows::UI::Notifications::Management::{
    UserNotificationListener, UserNotificationListenerAccessStatus,
};
use windows::UI::Notifications::{
    KnownNotificationBindings, NotificationKinds, UserNotification,
    UserNotificationChangedEventArgs,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_EXTENDED_FRAME_BOUNDS,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmGetWindowAttribute,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DispatchMessageW, FindWindowW, GA_ROOT, GWL_EXSTYLE, GWL_STYLE, GWLP_WNDPROC,
    GetAncestor, GetClassNameW, GetCursorPos, GetDesktopWindow, GetForegroundWindow,
    GetShellWindow, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
    HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindowVisible, IsZoomed, MONITORINFOF_PRIMARY, MSG,
    PM_REMOVE, PeekMessageW, SM_CXSCREEN, SW_HIDE, SW_RESTORE, SW_SHOWNOACTIVATE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WM_NCCALCSIZE, WNDPROC,
    WS_BORDER, WS_CAPTION, WS_DLGFRAME, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};
use windows::core::HSTRING;

use crate::platform::{
    AlbumArt, FontFamilyTarget, FontFileSpec, HoverPollInput, MediaCommand, MediaTrack,
    PlatformEvent, RepeatMode, decode_album_art,
};
use crate::state::{ListenerStatus, NotificationItem};

fn init_com_mta() {
    // Ignore RPC_E_CHANGED_MODE if COM is already initialized on this thread.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn access_status_label(status: UserNotificationListenerAccessStatus) -> &'static str {
    match status {
        UserNotificationListenerAccessStatus::Allowed => "Allowed",
        UserNotificationListenerAccessStatus::Denied => "Denied",
        UserNotificationListenerAccessStatus::Unspecified => "Unspecified",
        _ => "Unknown",
    }
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn extract_notification_item(notif: &UserNotification) -> Option<NotificationItem> {
    let id = notif.Id().ok()?.to_string();
    let app_name = notif
        .AppInfo()
        .and_then(|info| info.DisplayInfo())
        .and_then(|disp| disp.DisplayName())
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "Windows".to_owned());

    let mut lines = Vec::new();
    if let Ok(visual) = notif.Notification().and_then(|n| n.Visual()) {
        if let Ok(template) = KnownNotificationBindings::ToastGeneric()
            && let Ok(binding) = visual.GetBinding(&template)
            && let Ok(elements) = binding.GetTextElements()
        {
            for i in 0..elements.Size().unwrap_or(0) {
                if let Ok(elem) = elements.GetAt(i)
                    && let Ok(text) = elem.Text()
                {
                    let trimmed = text.to_string().trim().to_owned();
                    if !trimmed.is_empty() {
                        lines.push(trimmed);
                    }
                }
            }
        }
        if lines.is_empty()
            && let Ok(bindings) = visual.Bindings()
        {
            for bi in 0..bindings.Size().unwrap_or(0) {
                if let Ok(binding) = bindings.GetAt(bi)
                    && let Ok(elements) = binding.GetTextElements()
                {
                    for i in 0..elements.Size().unwrap_or(0) {
                        if let Ok(elem) = elements.GetAt(i)
                            && let Ok(text) = elem.Text()
                        {
                            let trimmed = text.to_string().trim().to_owned();
                            if !trimmed.is_empty() {
                                lines.push(trimmed);
                            }
                        }
                    }
                    if !lines.is_empty() {
                        break;
                    }
                }
            }
        }
    }

    let title = lines
        .first()
        .cloned()
        .unwrap_or_else(|| app_name.clone());
    let body = if lines.len() > 1 {
        lines[1..].join(" — ")
    } else {
        String::new()
    };

    Some(NotificationItem {
        id,
        app_name,
        title,
        body,
        icon_rgba: None,
        timestamp: unix_timestamp_secs(),
    })
}

pub fn run_notification_spike() -> Result<String, String> {
    init_com_mta();

    let listener = UserNotificationListener::Current()
        .map_err(|e| format!("UserNotificationListener::Current() failed: {e}"))?;

    let status = listener
        .RequestAccessAsync()
        .map_err(|e| format!("RequestAccessAsync() call failed: {e}"))?
        .join()
        .map_err(|e| format!("RequestAccessAsync() await failed: {e}"))?;

    if status != UserNotificationListenerAccessStatus::Allowed {
        return Err(format!(
            "Notification access status: {} (enable in Windows Settings > Privacy > Notifications)",
            access_status_label(status)
        ));
    }

    let toasts = listener
        .GetNotificationsAsync(NotificationKinds::Toast)
        .map_err(|e| format!("GetNotificationsAsync() call failed: {e}"))?
        .join()
        .map_err(|e| format!("GetNotificationsAsync() await failed: {e}"))?;

    let count = toasts.Size().unwrap_or(0);
    let mut previews = Vec::new();
    for i in 0..count.min(3) {
        if let Ok(n) = toasts.GetAt(i)
            && let Some(item) = extract_notification_item(&n)
        {
            previews.push(format!("[{}] {}", item.app_name, item.title));
        }
    }

    Ok(format!(
        "Access: Allowed | Active toasts: {} | Sample: {}",
        count,
        if previews.is_empty() {
            "none".to_owned()
        } else {
            previews.join(", ")
        }
    ))
}

pub fn spawn_listener(tx: mpsc::Sender<PlatformEvent>, on_wake: Arc<dyn Fn() + Send + Sync>) {
    std::thread::Builder::new()
        .name("win-notification-listener".to_owned())
        .spawn(move || {
            init_com_mta();

            let listener = match UserNotificationListener::Current() {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(PlatformEvent::Status(ListenerStatus::AccessDenied(format!(
                        "Listener unavailable: {e}"
                    ))));
                    on_wake();
                    return;
                }
            };

            let access = listener
                .RequestAccessAsync()
                .and_then(|op| op.join())
                .map_err(|e| e.to_string());

            match access {
                Ok(UserNotificationListenerAccessStatus::Allowed) => {}
                Ok(other) => {
                    let _ = tx.send(PlatformEvent::Status(ListenerStatus::AccessDenied(format!(
                        "Access {} — enable Windows Notifications access",
                        access_status_label(other)
                    ))));
                    on_wake();
                    return;
                }
                Err(err) => {
                    let _ = tx.send(PlatformEvent::Status(ListenerStatus::AccessDenied(err)));
                    on_wake();
                    return;
                }
            }

            let mut seen_ids: HashSet<u32> = HashSet::new();
            if let Ok(existing) = listener
                .GetNotificationsAsync(NotificationKinds::Toast)
                .and_then(|op| op.join())
            {
                for i in 0..existing.Size().unwrap_or(0) {
                    if let Ok(n) = existing.GetAt(i)
                        && let Ok(id) = n.Id()
                    {
                        seen_ids.insert(id);
                    }
                }
            }

            let (sig_tx, sig_rx) = mpsc::channel::<()>();
            let handler = TypedEventHandler::<
                UserNotificationListener,
                UserNotificationChangedEventArgs,
            >::new(move |_, _| {
                let _ = sig_tx.send(());
                Ok(())
            });

            // Fallback 500ms polling catches toasts reliably on unpackaged Win32 executables
            // where WinRT NotificationChanged callbacks may not fire without a CoreDispatcher.
            let _token = listener.NotificationChanged(&handler).ok();

            let _ = tx.send(PlatformEvent::Status(ListenerStatus::Listening));
            on_wake();

            loop {
                match sig_rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(()) => while sig_rx.try_recv().is_ok() {},
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                let Ok(list) = listener
                    .GetNotificationsAsync(NotificationKinds::Toast)
                    .and_then(|op| op.join())
                else {
                    continue;
                };

                let mut changed = false;
                let mut current_ids = HashSet::new();
                for i in 0..list.Size().unwrap_or(0) {
                    let Ok(n) = list.GetAt(i) else { continue };
                    let Ok(id) = n.Id() else { continue };
                    current_ids.insert(id);
                    if seen_ids.insert(id)
                        && let Some(item) = extract_notification_item(&n)
                    {
                        let _ = tx.send(PlatformEvent::Notification(item));
                        changed = true;
                    }
                }

                let removed: Vec<u32> = seen_ids
                    .iter()
                    .copied()
                    .filter(|id| !current_ids.contains(id))
                    .collect();
                for id in removed {
                    seen_ids.remove(&id);
                    let _ = tx.send(PlatformEvent::Removed(id.to_string()));
                    changed = true;
                }
                if changed {
                    on_wake();
                }
            }
        })
        .expect("spawn windows notification thread");
}

pub fn spawn_media_listener(
    tx: mpsc::Sender<PlatformEvent>,
    cmd_rx: mpsc::Receiver<MediaCommand>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("win-smtc-media".to_owned())
        .spawn(move || {
            init_com_mta();
            let Ok(manager) =
                GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                    .and_then(|op| op.join())
            else {
                return;
            };

            #[derive(PartialEq, Eq)]
            struct MediaSnapshot {
                title: String,
                artist: String,
                is_playing: bool,
                raw_pos_ms: u64,
                last_updated_ticks: i64,
                duration_ms: u64,
                shuffle: bool,
                repeat: RepeatMode,
            }

            let mut last_snapshot: Option<MediaSnapshot> = None;
            let mut cached_art: CachedAlbumArt = None;
            loop {
                let current = read_smtc_track(&manager, &mut cached_art);
                let snapshot = current.as_ref().map(|(t, raw_pos_ms, last_updated_ticks)| {
                    MediaSnapshot {
                        title: t.title.clone(),
                        artist: t.artist.clone(),
                        is_playing: t.is_playing,
                        raw_pos_ms: *raw_pos_ms,
                        last_updated_ticks: *last_updated_ticks,
                        duration_ms: t.duration_ms,
                        shuffle: t.shuffle,
                        repeat: t.repeat,
                    }
                });
                if snapshot != last_snapshot {
                    last_snapshot = snapshot;
                    let _ = tx.send(PlatformEvent::Media(current.map(|(t, _, _)| t)));
                    on_wake();
                }

                let poll_ms = if last_snapshot.as_ref().is_some_and(|s| s.is_playing) {
                    200
                } else {
                    600
                };
                match cmd_rx.recv_timeout(Duration::from_millis(poll_ms)) {
                    Ok(cmd) => {
                        if let Ok(session) = manager.GetCurrentSession() {
                            match cmd {
                                MediaCommand::TogglePlayPause => {
                                    let _ = session.TryTogglePlayPauseAsync();
                                }
                                MediaCommand::Next => {
                                    let _ = session.TrySkipNextAsync();
                                }
                                MediaCommand::Previous => {
                                    let _ = session.TrySkipPreviousAsync();
                                }
                                MediaCommand::SetShuffle(shuffle) => {
                                    let _ = session.TryChangeShuffleActiveAsync(shuffle);
                                }
                                MediaCommand::SetRepeat(mode) => {
                                    let win_mode = match mode {
                                        RepeatMode::Off => MediaPlaybackAutoRepeatMode::None,
                                        RepeatMode::All => MediaPlaybackAutoRepeatMode::List,
                                        RepeatMode::One => MediaPlaybackAutoRepeatMode::Track,
                                    };
                                    let _ = session.TryChangeAutoRepeatModeAsync(win_mode);
                                }
                                MediaCommand::SeekMs(ms) => {
                                    let ticks = (ms as i64).saturating_mul(10_000);
                                    let _ = session.TryChangePlaybackPositionAsync(ticks);
                                }
                            }
                        }
                        last_snapshot = None;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("spawn windows media thread");
}

const FILETIME_UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

fn current_filetime_ticks() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| {
            FILETIME_UNIX_EPOCH_TICKS
                .saturating_add((d.as_secs() as i64).saturating_mul(10_000_000))
                .saturating_add((d.subsec_nanos() / 100) as i64)
        })
        .unwrap_or(0)
}

type CachedAlbumArt = Option<((String, String), Option<Arc<AlbumArt>>)>;

fn read_smtc_track(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    cached_art: &mut CachedAlbumArt,
) -> Option<(MediaTrack, u64, i64)> {
    let session = manager.GetCurrentSession().ok()?;
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = props.Title().ok()?.to_string().trim().to_owned();
    if title.is_empty() {
        return None;
    }
    let artist = props
        .Artist()
        .map(|s| s.to_string().trim().to_owned())
        .unwrap_or_default();

    let key = (title.clone(), artist.clone());
    let album_art = if let Some((cached_key, art)) = cached_art.as_ref()
        && *cached_key == key
    {
        art.clone()
    } else {
        let art = read_smtc_thumbnail(&props);
        *cached_art = Some((key, art.clone()));
        art
    };

    let playback = session.GetPlaybackInfo().ok();
    let is_playing = playback
        .as_ref()
        .and_then(|info| info.PlaybackStatus().ok())
        .is_some_and(|s| s == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
    let shuffle = playback
        .as_ref()
        .and_then(|info| info.IsShuffleActive().ok())
        .and_then(|r| r.Value().ok())
        .unwrap_or(false);
    let repeat = playback
        .as_ref()
        .and_then(|info| info.AutoRepeatMode().ok())
        .and_then(|r| r.Value().ok())
        .map(|m| match m {
            MediaPlaybackAutoRepeatMode::Track => RepeatMode::One,
            MediaPlaybackAutoRepeatMode::List => RepeatMode::All,
            _ => RepeatMode::Off,
        })
        .unwrap_or(RepeatMode::Off);

    let (raw_pos_ms, duration_ms, last_updated_ticks) = session
        .GetTimelineProperties()
        .map(|tl| {
            let pos = tl
                .Position()
                .map(|t| (t.Duration / 10_000).max(0) as u64)
                .unwrap_or(0);
            let dur = tl
                .EndTime()
                .map(|t| (t.Duration / 10_000).max(0) as u64)
                .unwrap_or(0);
            let updated = tl.LastUpdatedTime().map(|dt| dt.UniversalTime).unwrap_or(0);
            (pos, dur, updated)
        })
        .unwrap_or((0, 0, 0));

    let mut position_ms = raw_pos_ms;
    if is_playing && last_updated_ticks > FILETIME_UNIX_EPOCH_TICKS {
        let now_ticks = current_filetime_ticks();
        if now_ticks > last_updated_ticks {
            let elapsed_ms = ((now_ticks - last_updated_ticks) / 10_000) as u64;
            if elapsed_ms <= duration_ms.max(3_600_000) {
                position_ms = raw_pos_ms.saturating_add(elapsed_ms);
                if duration_ms > 0 {
                    position_ms = position_ms.min(duration_ms);
                }
            }
        }
    }

    Some((
        MediaTrack {
            title,
            artist,
            is_playing,
            position_ms,
            duration_ms,
            shuffle,
            repeat,
            album_art,
            updated_at: Instant::now(),
        },
        raw_pos_ms,
        last_updated_ticks,
    ))
}

fn read_smtc_thumbnail(
    props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Arc<AlbumArt>> {
    let thumb_ref = props.Thumbnail().ok()?;
    let stream = thumb_ref.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as usize;
    if size == 0 || size > 4 * 1024 * 1024 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    let loaded = reader.LoadAsync(size as u32).ok()?.join().ok()? as usize;
    if loaded == 0 {
        return None;
    }
    let mut buf = vec![0u8; loaded];
    reader.ReadBytes(&mut buf).ok()?;
    decode_album_art(&buf)
}

pub fn primary_monitor_logical_width(pixels_per_point: f32) -> f32 {
    let px = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let logical = if px > 0 { px as f32 } else { 1920.0 };
    logical / pixels_per_point.max(1.0)
}

pub fn sync_overlay_window(
    window_title: &str,
    _x_logical: f32,
    _y_logical: f32,
    _w_logical: f32,
    _h_logical: f32,
    notch_dock_mode: bool,
) {
    if notch_dock_mode {
        pin_window_top_y(window_title, 0);
    }
}

pub fn detect_overlay_hover(input: HoverPollInput<'_>) -> bool {
    let top_trigger = is_cursor_in_top_trigger(
        input.top_offset_px,
        input.trigger_width_px,
        input.pixels_per_point,
    );
    if !input.visible {
        return top_trigger.unwrap_or(false);
    }
    if input.pointer_busy {
        return true;
    }
    let pad = if input.currently_hovered { 6 } else { 0 };
    let over_win = is_cursor_over_window(input.window_title, pad).unwrap_or(input.ctx_hovered);
    over_win || (input.currently_hovered && top_trigger.unwrap_or(false))
}

pub fn base_fallback_fonts() -> Vec<FontFileSpec> {
    vec![FontFileSpec {
        name: "win_segoe_ui".to_owned(),
        path: "C:\\Windows\\Fonts\\segoeui.ttf".to_owned(),
        family: FontFamilyTarget::Both,
        face_indices: &[0],
    }]
}

pub fn cjk_fallback_fonts_for(script: super::CjkScript) -> Vec<FontFileSpec> {
    match script {
        super::CjkScript::JapaneseOrHan => vec![FontFileSpec {
            name: "win_msgothic".to_owned(),
            path: "C:\\Windows\\Fonts\\msgothic.ttc".to_owned(),
            family: FontFamilyTarget::Both,
            face_indices: &[0],
        }],
        super::CjkScript::Korean => vec![FontFileSpec {
            name: "win_malgun".to_owned(),
            path: "C:\\Windows\\Fonts\\malgun.ttf".to_owned(),
            family: FontFamilyTarget::Both,
            face_indices: &[0],
        }],
    }
}

static ORIG_WNDPROC: AtomicIsize = AtomicIsize::new(0);

static HITBOX_LEFT: AtomicI32 = AtomicI32::new(0);
static HITBOX_TOP: AtomicI32 = AtomicI32::new(0);
static HITBOX_RIGHT: AtomicI32 = AtomicI32::new(-1);
static HITBOX_BOTTOM: AtomicI32 = AtomicI32::new(-1);

pub fn set_overlay_hitbox(left_px: i32, top_px: i32, right_px: i32, bottom_px: i32) {
    HITBOX_LEFT.store(left_px, Ordering::Relaxed);
    HITBOX_TOP.store(top_px, Ordering::Relaxed);
    HITBOX_RIGHT.store(right_px, Ordering::Relaxed);
    HITBOX_BOTTOM.store(bottom_px, Ordering::Relaxed);
}

fn point_in_hitbox(x: i32, y: i32, l: i32, t: i32, r: i32, b: i32) -> bool {
    x >= l && x <= r && y >= t && y <= b
}

unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Strip the 1px top inset that winit's undecorated-shadow handler adds in WM_NCCALCSIZE.
    if msg == WM_NCCALCSIZE && wparam.0 != 0 {
        return LRESULT(0);
    }
    if msg == WM_NCHITTEST {
        let x = (lparam.0 & 0xFFFF) as i16 as i32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
        let l = HITBOX_LEFT.load(Ordering::Relaxed);
        let t = HITBOX_TOP.load(Ordering::Relaxed);
        let r = HITBOX_RIGHT.load(Ordering::Relaxed);
        let b = HITBOX_BOTTOM.load(Ordering::Relaxed);
        return if point_in_hitbox(x, y, l, t, r, b) {
            LRESULT(HTCLIENT as isize)
        } else {
            LRESULT(HTTRANSPARENT as isize)
        };
    }
    let orig = ORIG_WNDPROC.load(Ordering::Relaxed);
    if orig != 0 {
        let proc: WNDPROC = unsafe { std::mem::transmute::<isize, WNDPROC>(orig) };
        return unsafe { CallWindowProcW(proc, hwnd, msg, wparam, lparam) };
    }
    LRESULT(0)
}

pub fn apply_no_activate_hook(window_title: &str) {
    let title = HSTRING::from(window_title);
    let Ok(hwnd) = (unsafe { FindWindowW(None, &title) }) else {
        return;
    };
    if hwnd.is_invalid() {
        return;
    }

    if ORIG_WNDPROC.load(Ordering::Relaxed) == 0 {
        let prev = unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWLP_WNDPROC,
                overlay_wndproc as *const () as isize,
            )
        };
        if prev != 0 {
            ORIG_WNDPROC.store(prev, Ordering::Relaxed);
        }
    }

    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let desired_ex = ex_style | (WS_EX_NOACTIVATE.0 as isize) | (WS_EX_TOOLWINDOW.0 as isize);
    if ex_style != desired_ex {
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired_ex);
        }
    }

    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    let strip = WS_CAPTION.0
        | WS_THICKFRAME.0
        | WS_BORDER.0
        | WS_DLGFRAME.0
        | WS_SYSMENU.0
        | WS_MINIMIZEBOX.0
        | WS_MAXIMIZEBOX.0;
    let desired_style = (style & !strip) | WS_POPUP.0;
    if style != desired_style {
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_STYLE, desired_style as isize);
        }
    }

    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &DWMWCP_DONOTROUND as *const _ as *const c_void,
            std::mem::size_of_val(&DWMWCP_DONOTROUND) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &DWMWA_COLOR_NONE as *const _ as *const c_void,
            std::mem::size_of_val(&DWMWA_COLOR_NONE) as u32,
        );
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

pub fn pin_window_top_y(window_title: &str, target_y_px: i32) {
    let title = HSTRING::from(window_title);
    let Ok(hwnd) = (unsafe { FindWindowW(None, &title) }) else {
        return;
    };
    if hwnd.is_invalid() {
        return;
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() && rect.top != target_y_px {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                rect.left,
                target_y_px,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

pub fn set_overlay_visible(window_title: &str, visible: bool) {
    let title = HSTRING::from(window_title);
    let Ok(hwnd) = (unsafe { FindWindowW(None, &title) }) else {
        return;
    };
    if hwnd.is_invalid() {
        return;
    }
    // Direct ShowWindow avoids winit re-applying window styles (which drops NOACTIVATE/POPUP).
    unsafe {
        let _ = ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        if visible {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

pub fn set_overlay_click_through(window_title: &str, passthrough: bool) -> bool {
    let title = HSTRING::from(window_title);
    let Ok(hwnd) = (unsafe { FindWindowW(None, &title) }) else {
        return false;
    };
    if hwnd.is_invalid() {
        return false;
    }
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let base = ex | (WS_EX_NOACTIVATE.0 as isize) | (WS_EX_TOOLWINDOW.0 as isize);
    let pass_bits = (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0) as isize;
    let desired = if passthrough {
        base | pass_bits
    } else {
        base & !pass_bits
    };
    if desired != ex {
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
    true
}

pub fn bring_window_to_front(window_title: &str) -> bool {
    let title = HSTRING::from(window_title);
    let Ok(hwnd) = (unsafe { FindWindowW(None, &title) }) else {
        return false;
    };
    if hwnd.is_invalid() {
        return false;
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
        let _ = SetForegroundWindow(hwnd);
    }
    true
}

pub fn is_cursor_in_screen_rect(
    left_px: i32,
    top_px: i32,
    right_px: i32,
    bottom_px: i32,
) -> Option<bool> {
    let mut pt = POINT::default();
    unsafe { GetCursorPos(&mut pt) }.ok()?;
    Some(pt.x >= left_px && pt.x <= right_px && pt.y >= top_px && pt.y <= bottom_px)
}

#[allow(dead_code)] // Retained for platform facade compatibility.
pub fn is_cursor_over_window(window_title: &str, padding_px: i32) -> Option<bool> {
    let title = HSTRING::from(window_title);
    let hwnd = unsafe { FindWindowW(None, &title) }.ok()?;
    if hwnd.is_invalid() {
        return None;
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    let mut pt = POINT::default();
    unsafe { GetCursorPos(&mut pt) }.ok()?;

    Some(
        pt.x >= rect.left - padding_px
            && pt.x <= rect.right + padding_px
            && pt.y >= rect.top - padding_px
            && pt.y <= rect.bottom + padding_px,
    )
}

pub fn is_cursor_in_top_trigger(
    top_offset_px: f32,
    trigger_width_px: f32,
    pixels_per_point: f32,
) -> Option<bool> {
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    if screen_w <= 0 {
        return None;
    }
    let mut pt = POINT::default();
    unsafe { GetCursorPos(&mut pt) }.ok()?;

    let ppp = pixels_per_point.max(1.0);
    let center_x = screen_w / 2;
    let half_w = ((trigger_width_px * ppp) * 0.5) as i32;
    let max_y = ((top_offset_px + 28.0) * ppp).max(16.0) as i32;

    Some(pt.x >= center_x - half_w && pt.x <= center_x + half_w && pt.y >= 0 && pt.y <= max_y)
}

pub fn pump_os_events() {
    let mut msg = MSG::default();
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn spawn_fullscreen_watcher(
    tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("win-fullscreen-watcher".to_owned())
        .spawn(move || {
            let mut last_fs = detect_fullscreen_state();
            let _ = tx.send(PlatformEvent::Fullscreen(last_fs.0, last_fs.1));
            on_wake();
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let fs = detect_fullscreen_state();
                if fs != last_fs {
                    last_fs = fs;
                    if tx.send(PlatformEvent::Fullscreen(fs.0, fs.1)).is_err() {
                        break;
                    }
                    on_wake();
                }
            }
        })
        .expect("spawn windows fullscreen watcher thread");
}

#[allow(dead_code)] // Facade compatibility wrapper.
pub fn is_foreground_fullscreen() -> bool {
    detect_fullscreen_state().1
}

pub fn detect_fullscreen_state() -> (bool, bool) {
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_invalid() {
        return (false, false);
    }
    if fg == unsafe { GetDesktopWindow() } || fg == unsafe { GetShellWindow() } {
        return (false, false);
    }
    if unsafe { IsIconic(fg) }.as_bool() || !unsafe { IsWindowVisible(fg) }.as_bool() {
        return (false, false);
    }

    let root = unsafe { GetAncestor(fg, GA_ROOT) };
    let hwnd = if root.is_invalid() { fg } else { root };

    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == std::process::id() {
        return (false, false);
    }

    let mut class_buf = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut class_buf) };
    if len > 0 {
        let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
        if matches!(
            class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return (false, false);
        }
    }

    let hmon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(hmon, &mut info) }.as_bool()
        || (info.dwFlags & MONITORINFOF_PRIMARY) == 0
    {
        return (false, false);
    }

    if let Ok(quns) = unsafe { SHQueryUserNotificationState() }
        && matches!(
            quns,
            QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_BUSY | QUNS_PRESENTATION_MODE
        )
    {
        return (true, true);
    }

    let is_zoomed = unsafe { IsZoomed(hwnd) }.as_bool();
    let mut win_rect = RECT::default();
    let dwm_ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut win_rect as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .is_ok();
    if !dwm_ok && unsafe { GetWindowRect(hwnd, &mut win_rect) }.is_err() {
        return (false, is_zoomed);
    }

    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    let has_caption = (style & WS_CAPTION.0) == WS_CAPTION.0;
    let (exclusive, windowed) = classify_fullscreen_rects(win_rect, info.rcMonitor, info.rcWork, has_caption);
    (exclusive, windowed || is_zoomed)
}

fn classify_fullscreen_rects(
    win_rect: RECT,
    mon: RECT,
    work: RECT,
    has_caption: bool,
) -> (bool, bool) {
    const TOL: i32 = 2;
    let covers_monitor = win_rect.left <= mon.left + TOL
        && win_rect.top <= mon.top + TOL
        && win_rect.right >= mon.right - TOL
        && win_rect.bottom >= mon.bottom - TOL;
    let covers_work = win_rect.left <= work.left + TOL
        && win_rect.top <= work.top + TOL
        && win_rect.right >= work.right - TOL
        && win_rect.bottom >= work.bottom - TOL;
    let has_visible_taskbar = (mon.bottom - work.bottom) >= 20
        || (work.top - mon.top) >= 20
        || (work.left - mon.left) >= 20
        || (mon.right - work.right) >= 20;
    let covers_autohide_edge = work.bottom < mon.bottom && win_rect.bottom >= mon.bottom;
    let exclusive = covers_monitor && (has_visible_taskbar || !has_caption || covers_autohide_edge);
    (exclusive, exclusive || covers_monitor || covers_work)
}

pub fn list_installed_fonts() -> Vec<super::InstalledFont> {
    let mut dirs = vec![std::path::PathBuf::from(r"C:\Windows\Fonts")];
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        dirs.push(
            std::path::PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
        );
    }
    super::collect_fonts_from_dirs(&dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: i32, t: i32, r: i32, b: i32) -> RECT {
        RECT {
            left: l,
            top: t,
            right: r,
            bottom: b,
        }
    }

    #[test]
    fn fullscreen_rect_predicate_distinguishes_fullscreen_from_maximized() {
        let mon = rect(0, 0, 1920, 1080);
        let work_taskbar = rect(0, 0, 1920, 1040);
        let work_autohide = rect(0, 0, 1920, 1078);

        // Windowed-fullscreen / maximized covering work area with visible taskbar.
        assert_eq!(
            classify_fullscreen_rects(rect(-8, -8, 1928, 1048), mon, work_taskbar, true),
            (false, true)
        );
        // Windowed-fullscreen / maximized with auto-hidden taskbar (1078 == work.bottom).
        assert_eq!(
            classify_fullscreen_rects(rect(0, 0, 1920, 1078), mon, work_autohide, true),
            (false, true)
        );
        // Exclusive / borderless fullscreen covering full monitor (1080).
        assert_eq!(
            classify_fullscreen_rects(rect(0, 0, 1920, 1080), mon, work_autohide, false),
            (true, true)
        );
        // Window covering the monitor while a taskbar exists.
        assert_eq!(
            classify_fullscreen_rects(rect(0, 0, 1920, 1080), mon, work_taskbar, true),
            (true, true)
        );
        // Small windowed app does not trigger fullscreen auto-hide.
        assert_eq!(
            classify_fullscreen_rects(rect(100, 100, 900, 700), mon, work_taskbar, true),
            (false, false)
        );
    }

    #[test]
    fn point_in_hitbox_checks_bounds_and_empty_rect() {
        assert!(point_in_hitbox(500, 10, 400, 0, 600, 40));
        assert!(!point_in_hitbox(399, 10, 400, 0, 600, 40));
        assert!(!point_in_hitbox(500, 41, 400, 0, 600, 40));
        assert!(!point_in_hitbox(0, 0, 0, 0, -1, -1));
    }
}
