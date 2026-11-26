use std::collections::HashMap;
use std::future::pending;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use zbus::{Connection, Dict, ObjectPath, OwnedVariant, Proxy, Variant, connection, interface};

use crate::platform::{
    AlbumArt, FontFamilyTarget, FontFileSpec, HoverPollInput, MediaCommand, MediaTrack,
    PlatformEvent, RepeatMode, decode_album_art,
};
use crate::state::{ListenerStatus, NotificationItem};

// shortcut: Non-Hyprland Wayland compositors lack wlr-layer-shell positioning in winit/eframe; upgrade to smithay-client-toolkit or iced_layershell for compositor-agnostic layer surfaces.

#[derive(Clone, Copy)]
struct MonitorInfo {
    x: f32,
    y: f32,
    logical_width: f32,
    scale: f32,
}

struct LinuxOverlayCache {
    monitor: Option<(Instant, MonitorInfo)>,
    last_geometry: Option<(i32, i32, i32, i32)>,
    last_topmost_sync: Option<Instant>,
    last_hover_probe: Option<(Instant, bool)>,
    last_trigger_probe: Option<(Instant, bool)>,
}

fn overlay_cache() -> &'static Mutex<LinuxOverlayCache> {
    static CACHE: OnceLock<Mutex<LinuxOverlayCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LinuxOverlayCache {
            monitor: None,
            last_geometry: None,
            last_topmost_sync: None,
            last_hover_probe: None,
            last_trigger_probe: None,
        })
    })
}

fn hypr_socket_path() -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let mut path = PathBuf::from(runtime);
    path.push("hypr");
    path.push(sig);
    path.push(".socket.sock");
    path.exists().then_some(path)
}

fn hypr_ipc(cmd: &str) -> Option<String> {
    let sock_path = hypr_socket_path()?;
    let mut stream = UnixStream::connect(sock_path).ok()?;
    let timeout = Some(Duration::from_millis(100));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    stream.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    stream.read_to_string(&mut out).ok()?;
    Some(out)
}

fn hypr_eval_or_fallback(lua: &str, legacy_cmd: &str) {
    if let Some(resp) = hypr_ipc(&format!("/eval {lua}"))
        && resp.trim() == "ok"
    {
        return;
    }
    let _ = hypr_ipc(legacy_cmd);
}

fn escape_lua_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn hypr_focused_monitor_cached() -> Option<MonitorInfo> {
    if let Ok(guard) = overlay_cache().lock()
        && let Some((at, mon)) = guard.monitor
        && at.elapsed() < Duration::from_millis(500)
    {
        return Some(mon);
    }

    let raw = hypr_ipc("j/monitors")?;
    let list: Vec<serde_json::Value> = serde_json::from_str(&raw).ok()?;
    let mon = list
        .iter()
        .find(|m| m.get("focused").and_then(|v| v.as_bool()) == Some(true))
        .or_else(|| list.first())?;
    let x = mon.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let y = mon.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let width = mon.get("width").and_then(|v| v.as_f64()).unwrap_or(1920.0) as f32;
    let scale = (mon.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32).max(0.5);
    let info = MonitorInfo {
        x,
        y,
        logical_width: width / scale,
        scale,
    };
    if let Ok(mut guard) = overlay_cache().lock() {
        guard.monitor = Some((Instant::now(), info));
    }
    Some(info)
}

fn hypr_cursor_pos() -> Option<(f32, f32)> {
    let raw = hypr_ipc("cursorpos")?;
    let (xs, ys) = raw.trim().split_once(',')?;
    let x = xs.trim().parse::<f32>().ok()?;
    let y = ys.trim().parse::<f32>().ok()?;
    Some((x, y))
}

pub fn should_send_outer_position() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_none()
}

pub fn primary_monitor_logical_width(_pixels_per_point: f32) -> f32 {
    if let Some(mon) = hypr_focused_monitor_cached() {
        return mon.logical_width;
    }
    1920.0
}

pub fn prepare_layer_shell_rules(namespace: &str) {
    if hypr_socket_path().is_none() {
        return;
    }
    let escaped = escape_lua_str(namespace);
    let lua = format!(
        "hl.layer_rule({{name=\"liebert-dynamic-island-layer\", match={{namespace=\"^{escaped}$\"}}, no_anim=true, blur=false, above_lock=false, order=-1}})"
    );
    let legacy = format!(
        "[[BATCH]]/keyword layerrule noanim,^({namespace})$ ; /keyword layerrule order -1,^({namespace})$"
    );
    hypr_eval_or_fallback(&lua, &legacy);
}

pub fn prepare_overlay_rules(window_title: &str, top_offset_px: f32) {
    if hypr_socket_path().is_none() {
        return;
    }
    let y = top_offset_px.round() as i32;
    let escaped = escape_lua_str(window_title);
    let lua = format!(
        "hl.window_rule({{name=\"liebert-dynamic-island\", match={{class=\"^liebert-dynamic-island$\"}}, float=true, pin=true, no_initial_focus=true, no_blur=true, no_shadow=true, no_dim=true, no_glow=true, no_wobble=true, no_anim=true, decorate=false, border_size=0, rounding=0, move=\"(monitor_w*0.5-window_w*0.5) {y}\"}}); hl.window_rule({{name=\"liebert-dynamic-island-title\", match={{title=\"^{escaped}$\"}}, float=true, pin=true, no_initial_focus=true, no_blur=true, no_shadow=true, no_dim=true, no_glow=true, no_wobble=true, no_anim=true, decorate=false, border_size=0, rounding=0, move=\"(monitor_w*0.5-window_w*0.5) {y}\"}})"
    );
    let legacy = format!(
        "[[BATCH]]/keyword windowrulev2 float,title:^({window_title})$ ; /keyword windowrulev2 pin,title:^({window_title})$ ; /keyword windowrulev2 noinitialfocus,title:^({window_title})$ ; /keyword windowrulev2 noborder,title:^({window_title})$ ; /keyword windowrulev2 noshadow,title:^({window_title})$ ; /keyword windowrulev2 noblur,title:^({window_title})$ ; /keyword windowrulev2 nodim,title:^({window_title})$ ; /keyword windowrulev2 noanim,title:^({window_title})$ ; /keyword windowrulev2 rounding 0,title:^({window_title})$ ; /keyword windowrulev2 move (monitor_w*0.5-window_w*0.5) {y},title:^({window_title})$"
    );
    hypr_eval_or_fallback(&lua, &legacy);
}

fn hypr_client_flags(window_title: &str) -> Option<(bool, bool)> {
    let raw = hypr_ipc("j/clients")?;
    let clients: Vec<serde_json::Value> = serde_json::from_str(&raw).ok()?;
    let client = clients.iter().find(|c| {
        (c.get("title").and_then(|v| v.as_str()) == Some(window_title)
            || c.get("class").and_then(|v| v.as_str()) == Some("liebert-dynamic-island"))
            && c.get("mapped").and_then(|v| v.as_bool()) == Some(true)
    })?;
    let floating = client.get("floating").and_then(|v| v.as_bool()).unwrap_or(false);
    let pinned = client.get("pinned").and_then(|v| v.as_bool()).unwrap_or(false);
    Some((floating, pinned))
}

fn dispatch_hypr_topmost(window_title: &str, check_legacy_flags: bool) {
    let escaped = escape_lua_str(window_title);
    let lua = format!(
        "hl.dispatch(hl.dsp.window.float({{action=\"set\", window=\"title:^{escaped}$\"}})); hl.dispatch(hl.dsp.window.pin({{action=\"set\", window=\"title:^{escaped}$\"}})); hl.dispatch(hl.dsp.window.alter_zorder({{mode=\"top\", window=\"title:^{escaped}$\"}}))"
    );
    if let Some(resp) = hypr_ipc(&format!("/eval {lua}"))
        && resp.trim() == "ok"
    {
        return;
    }

    let mut legacy_cmds = Vec::new();
    if check_legacy_flags && let Some((floating, pinned)) = hypr_client_flags(window_title) {
        if !floating {
            legacy_cmds.push(format!("/dispatch setfloating title:^({window_title})$"));
        }
        if !pinned {
            legacy_cmds.push(format!("/dispatch pin title:^({window_title})$"));
        }
    }
    legacy_cmds.push(format!("/dispatch alterzorder top,title:^({window_title})$"));
    let _ = hypr_ipc(&format!("[[BATCH]]{}", legacy_cmds.join(" ; ")));
}

pub fn apply_no_activate_hook(window_title: &str) -> bool {
    if hypr_socket_path().is_none() {
        return true;
    }
    if hypr_client_flags(window_title).is_none() {
        return false;
    }
    dispatch_hypr_topmost(window_title, true);
    if let Ok(mut guard) = overlay_cache().lock() {
        guard.last_topmost_sync = Some(Instant::now());
    }
    true
}

pub fn ensure_overlay_topmost(window_title: &str, force: bool) {
    if hypr_socket_path().is_none() {
        return;
    }
    if !force
        && let Ok(guard) = overlay_cache().lock()
        && let Some(at) = guard.last_topmost_sync
        && at.elapsed() < Duration::from_millis(1000)
    {
        return;
    }
    if let Ok(mut guard) = overlay_cache().lock() {
        guard.last_topmost_sync = Some(Instant::now());
    }
    dispatch_hypr_topmost(window_title, force);
}

pub fn sync_overlay_window(
    window_title: &str,
    x_logical: f32,
    y_logical: f32,
    w_logical: f32,
    h_logical: f32,
    _notch_dock_mode: bool,
) {
    if hypr_socket_path().is_none() {
        return;
    }
    let (mon_x, mon_y) = hypr_focused_monitor_cached()
        .map(|m| (m.x, m.y))
        .unwrap_or((0.0, 0.0));
    let tx = (mon_x + x_logical).round() as i32;
    let ty = (mon_y + y_logical).round() as i32;
    let tw = w_logical.round().max(1.0) as i32;
    let th = h_logical.round().max(1.0) as i32;
    let geom = (tx, ty, tw, th);

    if let Ok(mut guard) = overlay_cache().lock() {
        if guard.last_geometry == Some(geom) {
            return;
        }
        guard.last_geometry = Some(geom);
        guard.last_topmost_sync = Some(Instant::now());
    }

    let escaped = escape_lua_str(window_title);
    let lua = format!(
        "hl.dispatch(hl.dsp.window.float({{action=\"set\", window=\"title:^{escaped}$\"}})); hl.dispatch(hl.dsp.window.pin({{action=\"set\", window=\"title:^{escaped}$\"}})); hl.dispatch(hl.dsp.window.resize({{x={tw}, y={th}, exact=true, window=\"title:^{escaped}$\"}})); hl.dispatch(hl.dsp.window.move({{x={tx}, y={ty}, exact=true, window=\"title:^{escaped}$\"}})); hl.dispatch(hl.dsp.window.alter_zorder({{mode=\"top\", window=\"title:^{escaped}$\"}}))"
    );
    let legacy = format!(
        "[[BATCH]]/dispatch resizewindowpixel exact {tw} {th},title:^({window_title})$ ; /dispatch movewindowpixel exact {tx} {ty},title:^({window_title})$ ; /dispatch alterzorder top,title:^({window_title})$"
    );
    hypr_eval_or_fallback(&lua, &legacy);
}

pub fn bring_window_to_front(window_title: &str) -> bool {
    if hypr_socket_path().is_none() {
        return true;
    }
    let escaped = escape_lua_str(window_title);
    let lua = format!("hl.dispatch(hl.dsp.focus({{window=\"title:^{escaped}$\"}}))");
    let legacy = format!("/dispatch focuswindow title:^({window_title})$");
    hypr_eval_or_fallback(&lua, &legacy);
    true
}

fn is_cursor_over_window(window_title: &str, padding_px: i32) -> Option<bool> {
    hypr_socket_path()?;
    let (cx, cy) = hypr_cursor_pos()?;
    let mon_y = hypr_focused_monitor_cached().map(|m| m.y).unwrap_or(0.0);
    if cy > mon_y + 480.0 {
        return Some(false);
    }
    let raw = hypr_ipc("j/clients")?;
    let clients: Vec<serde_json::Value> = serde_json::from_str(&raw).ok()?;
    let client = clients.iter().find(|c| {
        c.get("title").and_then(|v| v.as_str()) == Some(window_title)
            && c.get("mapped").and_then(|v| v.as_bool()) == Some(true)
            && c.get("hidden").and_then(|v| v.as_bool()) != Some(true)
    })?;
    let at = client.get("at")?.as_array()?;
    let size = client.get("size")?.as_array()?;
    let x = at.first()?.as_f64()? as f32;
    let y = at.get(1)?.as_f64()? as f32;
    let w = size.first()?.as_f64()? as f32;
    let h = size.get(1)?.as_f64()? as f32;
    let pad = padding_px as f32;
    Some(cx >= x - pad && cx <= x + w + pad && cy >= y - pad && cy <= y + h + pad)
}

pub fn is_cursor_in_top_trigger(
    top_offset_px: f32,
    trigger_width_px: f32,
    _pixels_per_point: f32,
) -> Option<bool> {
    hypr_socket_path()?;
    let (cx, cy) = hypr_cursor_pos()?;
    let mon = hypr_focused_monitor_cached()?;
    let _ = mon.scale;
    let center_x = mon.x + mon.logical_width * 0.5;
    let half_w = trigger_width_px.max(120.0) * 0.5;
    let trigger_bottom = mon.y + top_offset_px.max(0.0) + 12.0;
    Some(cx >= center_x - half_w && cx <= center_x + half_w && cy >= mon.y && cy <= trigger_bottom)
}

pub fn detect_overlay_hover(input: HoverPollInput<'_>) -> bool {
    if !input.visible {
        if let Ok(guard) = overlay_cache().lock()
            && let Some((at, cached)) = guard.last_trigger_probe
            && at.elapsed() < Duration::from_millis(75)
        {
            return cached;
        }
        let result = is_cursor_in_top_trigger(input.top_offset_px, input.trigger_width_px, 1.0)
            .unwrap_or(false);
        if let Ok(mut guard) = overlay_cache().lock() {
            guard.last_trigger_probe = Some((Instant::now(), result));
        }
        return result;
    }

    // Fast path: trust active pointer interactions and native Wayland/X11 pointer-inside events with 0 IPC calls.
    if input.pointer_busy || input.ctx_hovered {
        return true;
    }

    if hypr_socket_path().is_none() {
        return false;
    }

    if let Ok(guard) = overlay_cache().lock()
        && let Some((at, cached)) = guard.last_hover_probe
        && at.elapsed() < Duration::from_millis(33)
    {
        return cached;
    }

    let pad = if input.animating {
        24
    } else if input.currently_hovered {
        8
    } else {
        2
    };
    let over = is_cursor_over_window(input.window_title, pad).unwrap_or(false)
        || (input.currently_hovered
            && input.animating
            && is_cursor_in_top_trigger(input.top_offset_px, input.trigger_width_px, 1.0)
                .unwrap_or(false));
    if let Ok(mut guard) = overlay_cache().lock() {
        guard.last_hover_probe = Some((Instant::now(), over));
    }
    over
}

pub fn spawn_fullscreen_watcher(
    tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    if hypr_socket_path().is_none() {
        return;
    }
    std::thread::Builder::new()
        .name("linux-fullscreen-watcher".to_owned())
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
        .expect("spawn linux fullscreen watcher thread");
}

pub fn detect_fullscreen_state() -> (bool, bool) {
    let Some(raw) = hypr_ipc("j/activewindow") else {
        return (false, false);
    };
    let Ok(win) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return (false, false);
    };
    if win.get("title").and_then(|v| v.as_str()) == Some(crate::ui::WINDOW_TITLE) {
        return (false, false);
    }
    let fs_mode = win.get("fullscreen").and_then(|v| v.as_i64()).unwrap_or(0);
    // Hyprland fullscreen mode: 0 = none, 1 = maximize (windowed), 2/3 = true fullscreen.
    (fs_mode >= 2, fs_mode != 0)
}

pub fn is_foreground_fullscreen() -> bool {
    detect_fullscreen_state().1
}

fn first_existing_path(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|p| Path::new(p).is_file())
        .map(|p| (*p).to_owned())
}

fn fc_match_file(pattern: &str) -> Option<String> {
    let out = Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!path.is_empty() && Path::new(&path).is_file()).then_some(path)
}

pub fn base_fallback_fonts() -> Vec<FontFileSpec> {
    let mut specs = Vec::new();

    if let Some(path) = first_existing_path(&[
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ])
    .or_else(|| fc_match_file("sans-serif"))
    {
        specs.push(FontFileSpec {
            name: "linux_sans".to_owned(),
            path,
            family: FontFamilyTarget::Proportional,
            face_indices: &[0],
        });
    }

    if let Some(path) = first_existing_path(&[
        "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ])
    .or_else(|| fc_match_file("monospace"))
    {
        specs.push(FontFileSpec {
            name: "linux_mono".to_owned(),
            path,
            family: FontFamilyTarget::Monospace,
            face_indices: &[0],
        });
    }

    specs
}

pub fn cjk_fallback_fonts_for(script: super::CjkScript) -> Vec<FontFileSpec> {
    let (lang, ttc_face): (&str, &'static [u32]) = match script {
        super::CjkScript::JapaneseOrHan => (":lang=ja", &[0]),
        super::CjkScript::Korean => (":lang=ko", &[1]),
    };
    if let Some(path) = first_existing_path(&[
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    ])
    .or_else(|| fc_match_file(lang))
    {
        let faces = if path.ends_with(".ttc") {
            ttc_face
        } else {
            &[0][..]
        };
        let suffix = match script {
            super::CjkScript::JapaneseOrHan => "jp",
            super::CjkScript::Korean => "kr",
        };
        return vec![FontFileSpec {
            name: format!("linux_noto_cjk_{suffix}"),
            path,
            family: FontFamilyTarget::Both,
            face_indices: faces,
        }];
    }
    Vec::new()
}

struct NotificationServer {
    next_id: AtomicU32,
    tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<&str>,
        _hints: HashMap<&str, zbus::OwnedVariant>,
        _expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id.fetch_add(1, Ordering::Relaxed)
        };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let item = NotificationItem {
            id: id.to_string(),
            app_name: if app_name.is_empty() {
                "Linux".to_owned()
            } else {
                app_name.to_owned()
            },
            title: summary.trim().to_owned(),
            body: body.trim().to_owned(),
            icon_rgba: None,
            timestamp,
        };
        let _ = self.tx.send(PlatformEvent::Notification(item));
        (self.on_wake)();
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.tx.send(PlatformEvent::Removed(id.to_string()));
        (self.on_wake)();
    }

    fn get_capabilities(&self) -> Vec<&'static str> {
        vec!["body", "persistence"]
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("liebert-dynamic-island", "Liebert", "0.1.1", "1.2")
    }
}

pub fn run_notification_spike() -> Result<String, String> {
    zbus::block_on(async {
        let conn = Connection::session()
            .await
            .map_err(|e| format!("D-Bus session connection failed: {e}"))?;

        let proxy = zbus::fdo::DBusProxy::new(&conn)
            .await
            .map_err(|e| format!("DBusProxy failed: {e}"))?;
        let name = "org.freedesktop.Notifications"
            .try_into()
            .map_err(|e| format!("Invalid bus name: {e}"))?;
        let has_owner = proxy
            .name_has_owner(name)
            .await
            .map_err(|e| format!("name_has_owner failed: {e}"))?;

        if has_owner {
            Ok("D-Bus session OK; org.freedesktop.Notifications currently owned by another daemon (stop dunst/mako to bind directly)".to_owned())
        } else {
            Ok("D-Bus session OK; org.freedesktop.Notifications is free to claim".to_owned())
        }
    })
}

pub fn spawn_listener(tx: mpsc::Sender<PlatformEvent>, on_wake: Arc<dyn Fn() + Send + Sync>) {
    std::thread::Builder::new()
        .name("linux-dbus-notifications".to_owned())
        .spawn(move || {
            let server = NotificationServer {
                next_id: AtomicU32::new(1),
                tx: tx.clone(),
                on_wake: on_wake.clone(),
            };

            let async_tx = tx.clone();
            let async_on_wake = on_wake.clone();
            let res = zbus::block_on(async move {
                let _conn = connection::Builder::session()
                    .name("org.freedesktop.Notifications")
                    .serve_at("/org/freedesktop/Notifications", server)
                    .build()
                    .await?;

                let _ = async_tx.send(PlatformEvent::Status(ListenerStatus::Listening));
                async_on_wake();
                pending::<()>().await;
                Ok::<(), zbus::Error>(())
            });

            if let Err(err) = res {
                let _ = tx.send(PlatformEvent::Status(ListenerStatus::Error(format!(
                    "D-Bus daemon error (another notification daemon active?): {err}"
                ))));
                on_wake();
            }
        })
        .expect("spawn linux notification thread");
}

pub fn spawn_media_listener(
    tx: mpsc::Sender<PlatformEvent>,
    cmd_rx: mpsc::Receiver<MediaCommand>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("linux-mpris-media".to_owned())
        .spawn(move || {
            let mut sync_state: Option<MprisSyncState> = None;
            let mut cached_art: CachedAlbumArt = None;
            let mut active_bus: Option<String> = None;
            let mut active_track_id: Option<String> = None;
            let _ = zbus::block_on(async move {
                let conn = Connection::session().await?;
                let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
                let mut props_proxies: HashMap<String, Proxy<'static>> = HashMap::new();
                let mut player_proxies: HashMap<String, Proxy<'static>> = HashMap::new();

                loop {
                    let mpris_names: Vec<String> = dbus
                        .list_names()
                        .await
                        .ok()
                        .map(|names| {
                            names
                                .into_iter()
                                .map(|n| n.to_string())
                                .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
                                .collect()
                        })
                        .unwrap_or_default();

                    props_proxies.retain(|bus, _| mpris_names.contains(bus));
                    player_proxies.retain(|bus, _| mpris_names.contains(bus));

                    for bus in &mpris_names {
                        if !props_proxies.contains_key(bus)
                            && let Ok(proxy) = zbus::proxy::Builder::<Proxy<'static>>::new(&conn)
                                .destination(bus.clone())
                                .path("/org/mpris/MediaPlayer2")
                                .interface("org.freedesktop.DBus.Properties")
                                .cache_properties(zbus::proxy::CacheProperties::No)
                                .build()
                                .await
                        {
                            props_proxies.insert(bus.clone(), proxy);
                        }
                    }

                    let chosen_bus = pick_active_mpris_bus(
                        &mpris_names,
                        &props_proxies,
                        active_bus.as_deref(),
                    )
                    .await;
                    active_bus = chosen_bus.clone();

                    let sample = if let Some(ref bus) = chosen_bus
                        && let Some(props) = props_proxies.get(bus)
                    {
                        read_mpris_sample(props, &mut cached_art).await
                    } else {
                        cached_art = None;
                        active_track_id = None;
                        None
                    };

                    let now = Instant::now();
                    let cur_pos_ms = sample.as_ref().map(|s| s.track.position_ms).unwrap_or(0);
                    let cur_rate = sample.as_ref().map(|s| s.rate).unwrap_or(1.0);
                    if let Some(ref s) = sample {
                        active_track_id = s.track_id.clone();
                    }

                    let poll_interval = match &sample {
                        Some(s) if s.track.is_playing => Duration::from_millis(350),
                        Some(_) => Duration::from_millis(800),
                        None => Duration::from_millis(1200),
                    };

                    let current_track = sample.as_ref().map(|s| s.track.clone());
                    if evaluate_mpris_sync(&mut sync_state, current_track.as_ref(), cur_rate, now) {
                        let _ = tx.send(PlatformEvent::Media(current_track));
                        on_wake();
                    }

                    match cmd_rx.recv_timeout(poll_interval) {
                        Ok(cmd) => {
                            if let Some(ref bus) = chosen_bus {
                                match cmd {
                                    MediaCommand::TogglePlayPause
                                    | MediaCommand::Next
                                    | MediaCommand::Previous => {
                                        if let Some(player) = get_or_build_player_proxy(
                                            &conn,
                                            &mut player_proxies,
                                            bus,
                                        )
                                        .await
                                        {
                                            let method = match cmd {
                                                MediaCommand::TogglePlayPause => "PlayPause",
                                                MediaCommand::Next => "Next",
                                                MediaCommand::Previous => "Previous",
                                                _ => unreachable!(),
                                            };
                                            let _: Result<(), _> = player.call(method, &()).await;
                                        }
                                    }
                                    MediaCommand::SetShuffle(shuffle) => {
                                        if let Some(props) = props_proxies.get(bus) {
                                            let _: Result<(), _> = props
                                                .call(
                                                    "Set",
                                                    &(
                                                        "org.mpris.MediaPlayer2.Player",
                                                        "Shuffle",
                                                        Variant::Bool(shuffle),
                                                    ),
                                                )
                                                .await;
                                        }
                                    }
                                    MediaCommand::SetRepeat(mode) => {
                                        if let Some(props) = props_proxies.get(bus) {
                                            let loop_str = match mode {
                                                RepeatMode::Off => "None",
                                                RepeatMode::All => "Playlist",
                                                RepeatMode::One => "Track",
                                            };
                                            let _: Result<(), _> = props
                                                .call(
                                                    "Set",
                                                    &(
                                                        "org.mpris.MediaPlayer2.Player",
                                                        "LoopStatus",
                                                        Variant::from(loop_str),
                                                    ),
                                                )
                                                .await;
                                        }
                                    }
                                    MediaCommand::SeekMs(target_ms) => {
                                        if let Some(player) = get_or_build_player_proxy(
                                            &conn,
                                            &mut player_proxies,
                                            bus,
                                        )
                                        .await
                                        {
                                            let target_us = (target_ms as i64).saturating_mul(1000);
                                            let set_pos_ok = if let Some(ref raw_id) = active_track_id
                                                && let Ok(obj_path) = ObjectPath::try_from(raw_id.as_str())
                                            {
                                                player
                                                    .call::<_, _, ()>("SetPosition", &(obj_path, target_us))
                                                    .await
                                                    .is_ok()
                                            } else {
                                                false
                                            };
                                            if !set_pos_ok {
                                                let live_pos_ms = if let Some(props) = props_proxies.get(bus) {
                                                    read_live_position_us(props)
                                                        .await
                                                        .map(|us| (us / 1000).max(0) as u64)
                                                        .unwrap_or(cur_pos_ms)
                                                } else {
                                                    cur_pos_ms
                                                };
                                                let delta_us = (target_ms as i64 - live_pos_ms as i64)
                                                    .saturating_mul(1000);
                                                let _: Result<(), _> =
                                                    player.call("Seek", &(delta_us,)).await;
                                            }
                                        }
                                    }
                                }
                            }
                            sync_state = None;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                Ok::<(), zbus::Error>(())
            });
        })
        .expect("spawn linux media thread");
}

#[derive(Debug, Clone)]
struct MprisSyncState {
    title: String,
    artist: String,
    is_playing: bool,
    shuffle: bool,
    repeat: RepeatMode,
    duration_ms: u64,
    has_art: bool,
    position_ms: u64,
    updated_at: Instant,
    emitted_at: Instant,
    rate: f64,
    lag_count: u8,
}

fn evaluate_mpris_sync(
    prev: &mut Option<MprisSyncState>,
    current: Option<&MediaTrack>,
    rate: f64,
    now: Instant,
) -> bool {
    let Some(track) = current else {
        return prev.take().is_some();
    };
    let has_art = track.album_art.is_some();

    let Some(st) = prev.as_mut() else {
        *prev = Some(MprisSyncState {
            title: track.title.clone(),
            artist: track.artist.clone(),
            is_playing: track.is_playing,
            shuffle: track.shuffle,
            repeat: track.repeat,
            duration_ms: track.duration_ms,
            has_art,
            position_ms: track.position_ms,
            updated_at: track.updated_at,
            emitted_at: now,
            rate,
            lag_count: 0,
        });
        return true;
    };

    let meta_or_state_changed = st.title != track.title
        || st.artist != track.artist
        || st.is_playing != track.is_playing
        || st.shuffle != track.shuffle
        || st.repeat != track.repeat
        || (st.has_art != has_art && has_art)
        || (st.duration_ms == 0 && track.duration_ms > 0)
        || st.duration_ms.abs_diff(track.duration_ms) >= 1000;

    if meta_or_state_changed {
        *st = MprisSyncState {
            title: track.title.clone(),
            artist: track.artist.clone(),
            is_playing: track.is_playing,
            shuffle: track.shuffle,
            repeat: track.repeat,
            duration_ms: track.duration_ms,
            has_art,
            position_ms: track.position_ms,
            updated_at: track.updated_at,
            emitted_at: now,
            rate,
            lag_count: 0,
        };
        return true;
    }

    if !track.is_playing {
        if st.position_ms.abs_diff(track.position_ms) >= 40 {
            st.position_ms = track.position_ms;
            st.updated_at = track.updated_at;
            st.emitted_at = now;
            st.lag_count = 0;
            return true;
        }
        return false;
    }

    let elapsed_ms = now.saturating_duration_since(st.updated_at).as_millis() as f64;
    let expected_ms = st.position_ms.saturating_add((elapsed_ms * st.rate).round() as u64);
    let actual_ms = track.position_ms;
    let diff = actual_ms.abs_diff(expected_ms);

    let forward_seek = actual_ms > expected_ms.saturating_add(300);
    let rewind_or_restart = actual_ms.saturating_add(900) < expected_ms
        || actual_ms < st.position_ms.saturating_sub(250);
    let persistent_lag = if expected_ms.saturating_sub(actual_ms) >= 250 {
        st.lag_count = st.lag_count.saturating_add(1);
        st.lag_count >= 2
    } else {
        st.lag_count = 0;
        false
    };
    let periodic_lock = now.saturating_duration_since(st.emitted_at) >= Duration::from_millis(2500)
        && diff >= 80;

    if forward_seek || rewind_or_restart || persistent_lag || periodic_lock {
        st.position_ms = actual_ms;
        st.updated_at = track.updated_at;
        st.emitted_at = now;
        st.rate = rate;
        st.lag_count = 0;
        return true;
    }
    false
}

async fn get_or_build_player_proxy<'a>(
    conn: &Connection,
    cache: &'a mut HashMap<String, Proxy<'static>>,
    bus: &str,
) -> Option<&'a Proxy<'static>> {
    if !cache.contains_key(bus)
        && let Ok(proxy) = zbus::proxy::Builder::<Proxy<'static>>::new(conn)
            .destination(bus.to_owned())
            .path("/org/mpris/MediaPlayer2")
            .interface("org.mpris.MediaPlayer2.Player")
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
    {
        cache.insert(bus.to_owned(), proxy);
    }
    cache.get(bus)
}

async fn pick_active_mpris_bus(
    names: &[String],
    props_proxies: &HashMap<String, Proxy<'static>>,
    last_bus: Option<&str>,
) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    if names.len() == 1 {
        return Some(names[0].clone());
    }

    type PlayerScore = (u8, bool, bool, bool, bool);
    let mut best: Option<(PlayerScore, &String)> = None;
    for bus in names {
        let mut status_rank = 0u8;
        let mut has_title = false;
        if let Some(props) = props_proxies.get(bus)
            && let Ok(all) = props
                .call::<_, _, HashMap<String, OwnedVariant>>(
                    "GetAll",
                    &("org.mpris.MediaPlayer2.Player",),
                )
                .await
        {
            let status = all
                .get("PlaybackStatus")
                .and_then(|v| variant_to_str(v))
                .unwrap_or_default();
            status_rank = match status.as_str() {
                "Playing" => 2,
                "Paused" => 1,
                _ => 0,
            };
            if let Some(meta) = all.get("Metadata")
                && let Variant::Dict(dict) = &**meta
            {
                has_title = dict_str(dict, "xesam:title").is_some();
            }
        }
        let lower = bus.to_ascii_lowercase();
        let is_playerctld = lower == "org.mpris.mediaplayer2.playerctld";
        let non_browser = !lower.contains("chromium")
            && !lower.contains("chrome")
            && !lower.contains("firefox")
            && !lower.contains("plasma-browser-integration");
        let sticky = last_bus == Some(bus.as_str());
        let score = (
            if has_title { status_rank } else { 0 },
            has_title,
            non_browser,
            sticky,
            is_playerctld,
        );
        if best.as_ref().is_none_or(|(best_score, _)| score > *best_score) {
            best = Some((score, bus));
        }
    }
    best.map(|(_, bus)| bus.clone())
}

type CachedAlbumArt = Option<((String, String), Option<Arc<AlbumArt>>)>;

struct MprisSample {
    track: MediaTrack,
    track_id: Option<String>,
    rate: f64,
}

async fn read_live_position_us(props: &Proxy<'_>) -> Option<i64> {
    let val: OwnedVariant = props
        .call("Get", &("org.mpris.MediaPlayer2.Player", "Position"))
        .await
        .ok()?;
    variant_to_micros(&val)
}

async fn read_mpris_sample(
    props: &Proxy<'_>,
    cached_art: &mut CachedAlbumArt,
) -> Option<MprisSample> {
    let all: HashMap<String, OwnedVariant> = props
        .call("GetAll", &("org.mpris.MediaPlayer2.Player",))
        .await
        .ok()?;

    // MPRIS2 lets players omit or stale-cache `Position` inside `GetAll`; query explicitly and
    // stamp `sampled_at` immediately before any slow cover-art fetch.
    let live_pos_us = read_live_position_us(props).await;
    let mut sampled_at = Instant::now();
    let position_us = live_pos_us
        .or_else(|| all.get("Position").and_then(|v| variant_to_micros(v)))
        .unwrap_or(0);
    let mut position_ms = (position_us / 1000).max(0) as u64;

    let status = all
        .get("PlaybackStatus")
        .and_then(|v| variant_to_str(v))
        .unwrap_or_else(|| "Stopped".to_owned());
    let is_playing = status == "Playing";
    let rate = all
        .get("Rate")
        .and_then(|v| variant_to_f64(v))
        .filter(|r| *r > 0.05)
        .unwrap_or(1.0)
        .clamp(0.25, 4.0);
    let shuffle = all
        .get("Shuffle")
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false);
    let repeat = all
        .get("LoopStatus")
        .and_then(|v| variant_to_str(v))
        .map(|s| match s.as_str() {
            "Track" => RepeatMode::One,
            "Playlist" => RepeatMode::All,
            _ => RepeatMode::Off,
        })
        .unwrap_or(RepeatMode::Off);

    let meta_val = all.get("Metadata")?;
    let Variant::Dict(dict) = &**meta_val else {
        return None;
    };

    let (title, artist, duration_ms, track_id, art_url) = extract_metadata_fields(dict)?;

    let key = (title.clone(), artist.clone());
    let album_art = if let Some((cached_key, art)) = cached_art.as_ref()
        && *cached_key == key
    {
        art.clone()
    } else {
        let fetch_started = Instant::now();
        let art = art_url.as_deref().and_then(load_art_from_url);
        *cached_art = Some((key, art.clone()));
        // Re-sample Position if album art fetch spent network/disk time so timeline stays locked.
        if is_playing && fetch_started.elapsed() > Duration::from_millis(40)
            && let Some(fresh_us) = read_live_position_us(props).await
        {
            sampled_at = Instant::now();
            position_ms = (fresh_us / 1000).max(0) as u64;
        }
        art
    };

    Some(MprisSample {
        track: MediaTrack {
            title,
            artist,
            is_playing,
            position_ms,
            duration_ms,
            shuffle,
            repeat,
            album_art,
            updated_at: sampled_at,
        },
        track_id,
        rate,
    })
}

type ExtractedMeta = (String, String, u64, Option<String>, Option<String>);

fn extract_metadata_fields(dict: &Dict<'_, '_>) -> Option<ExtractedMeta> {
    let title = dict_str(dict, "xesam:title")
        .or_else(|| dict_str(dict, "xesam:url").and_then(|u| file_stem_from_uri(&u)))?;

    let mut artists = dict_string_list(dict, "xesam:artist");
    if artists.is_empty() {
        artists = dict_string_list(dict, "xesam:albumArtist");
    }
    if artists.is_empty()
        && let Some(creator) = dict_str(dict, "xesam:creator")
    {
        artists.push(creator);
    }
    let artist = artists.join(", ");

    let duration_ms = dict_micros(dict, "mpris:length")
        .map(|us| (us / 1000).max(0) as u64)
        .unwrap_or(0);
    let track_id = dict_str(dict, "mpris:trackid");
    let art_url = dict_str(dict, "mpris:artUrl");

    Some((title, artist, duration_ms, track_id, art_url))
}

fn unwrap_variant<'a>(mut v: &'a Variant<'a>) -> &'a Variant<'a> {
    while let Variant::Variant(inner) = v {
        v = inner;
    }
    v
}

fn variant_to_micros(v: &Variant<'_>) -> Option<i64> {
    match unwrap_variant(v) {
        Variant::I64(n) => Some(*n),
        Variant::U64(n) => i64::try_from(*n).ok(),
        Variant::I32(n) => Some(i64::from(*n)),
        Variant::U32(n) => Some(i64::from(*n)),
        Variant::F64(n) if n.is_finite() => Some(*n as i64),
        _ => None,
    }
}

fn variant_to_f64(v: &Variant<'_>) -> Option<f64> {
    match unwrap_variant(v) {
        Variant::F64(n) if n.is_finite() => Some(*n),
        Variant::I64(n) => Some(*n as f64),
        Variant::U64(n) => Some(*n as f64),
        Variant::I32(n) => Some(f64::from(*n)),
        Variant::U32(n) => Some(f64::from(*n)),
        _ => None,
    }
}

fn variant_to_str(v: &Variant<'_>) -> Option<String> {
    match unwrap_variant(v) {
        Variant::Str(s) => {
            let trimmed = s.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        Variant::ObjectPath(p) => {
            let trimmed = p.as_str().trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        _ => None,
    }
}

fn variant_to_string_list(v: &Variant<'_>) -> Vec<String> {
    match unwrap_variant(v) {
        Variant::Str(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_owned()]
            }
        }
        Variant::Array(arr) => arr
            .inner()
            .iter()
            .filter_map(variant_to_str)
            .collect(),
        _ => Vec::new(),
    }
}

fn dict_str(dict: &Dict<'_, '_>, key: &str) -> Option<String> {
    dict.get::<&str, Variant>(&key)
        .ok()
        .flatten()
        .and_then(|v| variant_to_str(&v))
}

fn dict_string_list(dict: &Dict<'_, '_>, key: &str) -> Vec<String> {
    dict.get::<&str, Variant>(&key)
        .ok()
        .flatten()
        .map(|v| variant_to_string_list(&v))
        .unwrap_or_default()
}

fn dict_micros(dict: &Dict<'_, '_>, key: &str) -> Option<i64> {
    dict.get::<&str, Variant>(&key)
        .ok()
        .flatten()
        .and_then(|v| variant_to_micros(&v))
}

fn decode_file_uri(uri: &str) -> Option<String> {
    let raw = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(raw.len());
    let src = raw.as_bytes();
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'%'
            && i + 2 < src.len()
            && let Ok(hex) = std::str::from_utf8(&src[i + 1..i + 3])
            && let Ok(val) = u8::from_str_radix(hex, 16)
        {
            bytes.push(val);
            i += 3;
            continue;
        }
        bytes.push(src[i]);
        i += 1;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn file_stem_from_uri(uri: &str) -> Option<String> {
    let decoded = decode_file_uri(uri)?;
    let stem = Path::new(&decoded).file_stem()?.to_str()?.trim();
    (!stem.is_empty()).then(|| stem.to_owned())
}

fn load_art_from_url(url: &str) -> Option<Arc<AlbumArt>> {
    if let Some(path) = decode_file_uri(url) {
        let bytes = std::fs::read(path).ok()?;
        return decode_album_art(&bytes);
    }
    if url.starts_with("https://") || url.starts_with("http://") {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(3)))
            .build();
        let agent: ureq::Agent = config.into();
        let bytes = agent
            .get(url)
            .call()
            .ok()?
            .body_mut()
            .with_config()
            .limit(4 * 1024 * 1024)
            .read_to_vec()
            .ok()?;
        return decode_album_art(&bytes);
    }
    None
}

pub fn list_installed_fonts() -> Vec<super::InstalledFont> {
    let mut dirs = vec![
        std::path::PathBuf::from("/usr/share/fonts"),
        std::path::PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(std::path::PathBuf::from(home).join(".local/share/fonts"));
    }
    super::collect_fonts_from_dirs(&dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_and_cjk_font_discovery_finds_existing_files() {
        let base = base_fallback_fonts();
        for spec in &base {
            assert!(Path::new(&spec.path).is_file(), "missing font {}", spec.path);
            assert!(!spec.face_indices.is_empty());
        }
        let cjk = cjk_fallback_fonts_for(crate::platform::CjkScript::JapaneseOrHan);
        for spec in &cjk {
            assert!(Path::new(&spec.path).is_file(), "missing cjk font {}", spec.path);
            assert!(!spec.face_indices.is_empty());
        }
    }

    #[test]
    fn detect_overlay_hover_fast_paths_avoid_false_collapse() {
        let busy = detect_overlay_hover(HoverPollInput {
            window_title: "Nonexistent",
            ctx_hovered: false,
            pointer_busy: true,
            currently_hovered: true,
            visible: true,
            animating: false,
            top_offset_px: 0.0,
            trigger_width_px: 180.0,
            pixels_per_point: 1.0,
        });
        assert!(busy);

        let ctx_hit = detect_overlay_hover(HoverPollInput {
            window_title: "Nonexistent",
            ctx_hovered: true,
            pointer_busy: false,
            currently_hovered: false,
            visible: true,
            animating: false,
            top_offset_px: 0.0,
            trigger_width_px: 180.0,
            pixels_per_point: 1.0,
        });
        assert!(ctx_hit);
    }

    #[test]
    fn mpris_metadata_extracts_unsigned_length_and_single_string_artist() {
        use zbus::{Signature, zvariant::Type};

        let mut dict = Dict::new(String::SIGNATURE, &Signature::Variant);
        dict.add("xesam:title", Variant::new("Idol")).unwrap();
        // Several Linux players send a single string instead of a string array.
        dict.add("xesam:artist", Variant::new("YOASOBI")).unwrap();
        // Players such as Chromium/mpDris2 send mpris:length as uint64 (`t`).
        dict.add("mpris:length", Variant::new(213_000_000u64)).unwrap();

        let (title, artist, duration_ms, _, _) =
            extract_metadata_fields(&dict).expect("metadata extracted");
        assert_eq!(title, "Idol");
        assert_eq!(artist, "YOASOBI");
        assert_eq!(duration_ms, 213_000);
    }

    #[test]
    fn mpris_sync_tracker_detects_seeks_and_ignores_coarse_jitter() {
        let t0 = Instant::now();
        let make = |pos_ms: u64, playing: bool, at: Instant| MediaTrack {
            title: "Song".to_owned(),
            artist: "Artist".to_owned(),
            is_playing: playing,
            position_ms: pos_ms,
            duration_ms: 200_000,
            shuffle: false,
            repeat: RepeatMode::Off,
            album_art: None,
            updated_at: at,
        };

        let mut state = None;
        let first = make(10_000, true, t0);
        assert!(evaluate_mpris_sync(&mut state, Some(&first), 1.0, t0));

        // 400 ms later with player ticking steadily: within deadband, do not reset UI smooth clock.
        let t1 = t0 + Duration::from_millis(400);
        let steady = make(10_380, true, t1);
        assert!(!evaluate_mpris_sync(&mut state, Some(&steady), 1.0, t1));

        // Explicit forward seek > 300 ms ahead emits immediately.
        let t2 = t0 + Duration::from_millis(800);
        let seek_fwd = make(25_000, true, t2);
        assert!(evaluate_mpris_sync(&mut state, Some(&seek_fwd), 1.0, t2));

        // Explicit backward seek / loop restart emits immediately.
        let t3 = t0 + Duration::from_millis(1200);
        let seek_back = make(12_000, true, t3);
        assert!(evaluate_mpris_sync(&mut state, Some(&seek_back), 1.0, t3));
    }

    #[test]
    fn file_uri_decoding_handles_percent_encoding() {
        assert_eq!(
            decode_file_uri("file:///home/user/My%20Music/cover%231.jpg").as_deref(),
            Some("/home/user/My Music/cover#1.jpg")
        );
        assert_eq!(
            file_stem_from_uri("file:///tmp/Track%2001.flac").as_deref(),
            Some("Track 01")
        );
    }
}
