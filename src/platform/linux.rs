use std::collections::HashMap;
use std::future::pending;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use zbus::{Connection, Dict, OwnedVariant, Proxy, Variant, connection, interface};

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

fn is_cursor_in_top_trigger(top_offset_px: f32, trigger_width_px: f32) -> Option<bool> {
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
        let result = is_cursor_in_top_trigger(input.top_offset_px, input.trigger_width_px)
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
            && is_cursor_in_top_trigger(input.top_offset_px, input.trigger_width_px)
                .unwrap_or(false));
    if let Ok(mut guard) = overlay_cache().lock() {
        guard.last_hover_probe = Some((Instant::now(), over));
    }
    over
}

pub fn is_foreground_fullscreen() -> bool {
    let Some(raw) = hypr_ipc("j/activewindow") else {
        return false;
    };
    let Ok(win) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    if win.get("title").and_then(|v| v.as_str()) == Some(crate::ui::WINDOW_TITLE) {
        return false;
    }
    win.get("fullscreen").and_then(|v| v.as_i64()).unwrap_or(0) != 0
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

    if let Some(path) = first_existing_path(&[
        "/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
    ]) {
        specs.push(FontFileSpec {
            name: "linux_symbols".to_owned(),
            path,
            family: FontFamilyTarget::Both,
            face_indices: &[0],
        });
    }

    if let Some(path) = first_existing_path(&[
        "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    ]) {
        specs.push(FontFileSpec {
            name: "linux_symbols2".to_owned(),
            path,
            family: FontFamilyTarget::Both,
            face_indices: &[0],
        });
    }

    if let Some(path) = first_existing_path(&[
        "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
    ]) {
        specs.push(FontFileSpec {
            name: "linux_arabic".to_owned(),
            path,
            family: FontFamilyTarget::Proportional,
            face_indices: &[0],
        });
    }

    specs
}

pub fn cjk_fallback_fonts() -> Vec<FontFileSpec> {
    const CJK_FACES: &[u32] = &[0, 1, 2, 3];
    if let Some(path) = first_existing_path(&[
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    ])
    .or_else(|| fc_match_file(":lang=ja"))
    {
        let faces = if path.ends_with(".ttc") {
            CJK_FACES
        } else {
            &[0][..]
        };
        return vec![FontFileSpec {
            name: "linux_noto_cjk".to_owned(),
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
        ("liebert-dynamic-island", "Liebert", "0.1.0", "1.2")
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
            let mut last_snapshot: Option<(String, String, bool, u64, bool, RepeatMode)> = None;
            let mut cached_art: CachedAlbumArt = None;
            let _ = zbus::block_on(async move {
                let conn = Connection::session().await?;
                let dbus = zbus::fdo::DBusProxy::new(&conn).await?;

                loop {
                    let player_name = dbus
                        .list_names()
                        .await
                        .ok()
                        .and_then(|names| {
                            names
                                .into_iter()
                                .map(|n| n.to_string())
                                .find(|n| n.starts_with("org.mpris.MediaPlayer2."))
                        });

                    let current = if let Some(ref bus) = player_name {
                        read_mpris_track(&conn, bus, &mut cached_art).await
                    } else {
                        None
                    };
                    let cur_pos_ms = current.as_ref().map(|t| t.position_ms).unwrap_or(0);

                    let snapshot = current.as_ref().map(|t| {
                        (
                            t.title.clone(),
                            t.artist.clone(),
                            t.is_playing,
                            t.position_ms / 1000,
                            t.shuffle,
                            t.repeat,
                        )
                    });
                    if snapshot != last_snapshot {
                        last_snapshot = snapshot;
                        let _ = tx.send(PlatformEvent::Media(current));
                        on_wake();
                    }

                    match cmd_rx.recv_timeout(Duration::from_millis(600)) {
                        Ok(cmd) => {
                            if let Some(ref bus) = player_name {
                                match cmd {
                                    MediaCommand::TogglePlayPause
                                    | MediaCommand::Next
                                    | MediaCommand::Previous => {
                                        if let Ok(player) = Proxy::new(
                                            &conn,
                                            bus.as_str(),
                                            "/org/mpris/MediaPlayer2",
                                            "org.mpris.MediaPlayer2.Player",
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
                                        if let Ok(props) = Proxy::new(
                                            &conn,
                                            bus.as_str(),
                                            "/org/mpris/MediaPlayer2",
                                            "org.freedesktop.DBus.Properties",
                                        )
                                        .await
                                        {
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
                                        if let Ok(props) = Proxy::new(
                                            &conn,
                                            bus.as_str(),
                                            "/org/mpris/MediaPlayer2",
                                            "org.freedesktop.DBus.Properties",
                                        )
                                        .await
                                        {
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
                                        if let Ok(player) = Proxy::new(
                                            &conn,
                                            bus.as_str(),
                                            "/org/mpris/MediaPlayer2",
                                            "org.mpris.MediaPlayer2.Player",
                                        )
                                        .await
                                        {
                                            let delta_us =
                                                (target_ms as i64 - cur_pos_ms as i64) * 1000;
                                            let _: Result<(), _> =
                                                player.call("Seek", &(delta_us,)).await;
                                        }
                                    }
                                }
                            }
                            last_snapshot = None;
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

type CachedAlbumArt = Option<((String, String), Option<Arc<AlbumArt>>)>;

async fn read_mpris_track(
    conn: &Connection,
    bus: &str,
    cached_art: &mut CachedAlbumArt,
) -> Option<MediaTrack> {
    let props = Proxy::new(
        conn,
        bus,
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
    )
    .await
    .ok()?;
    let all: HashMap<String, OwnedVariant> = props
        .call("GetAll", &("org.mpris.MediaPlayer2.Player",))
        .await
        .ok()?;

    let status = all
        .get("PlaybackStatus")
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or("Stopped");
    let is_playing = status == "Playing";
    let shuffle = all
        .get("Shuffle")
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false);
    let repeat = all
        .get("LoopStatus")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(|s| match s {
            "Track" => RepeatMode::One,
            "Playlist" => RepeatMode::All,
            _ => RepeatMode::Off,
        })
        .unwrap_or(RepeatMode::Off);

    let position_ms = all
        .get("Position")
        .and_then(|v| i64::try_from(v).ok())
        .map(|us| (us / 1000).max(0) as u64)
        .unwrap_or(0);

    let meta_val = all.get("Metadata")?;
    let dict = Dict::try_from(meta_val.downcast_ref::<Variant>().ok()?).ok()?;

    let title = dict
        .get::<&str, &str>(&"xesam:title")
        .ok()
        .flatten()
        .map(|s| s.trim().to_owned())?;
    if title.is_empty() {
        return None;
    }

    let artist = dict
        .get::<&str, Variant>(&"xesam:artist")
        .ok()
        .flatten()
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .map(|list| list.join(", "))
        .unwrap_or_default();

    let duration_ms = dict
        .get::<&str, i64>(&"mpris:length")
        .ok()
        .flatten()
        .map(|us| (us / 1000).max(0) as u64)
        .unwrap_or(0);

    let key = (title.clone(), artist.clone());
    let album_art = if let Some((cached_key, art)) = cached_art.as_ref()
        && *cached_key == key
    {
        art.clone()
    } else {
        let art_url = dict
            .get::<&str, &str>(&"mpris:artUrl")
            .ok()
            .flatten();
        let art = art_url.and_then(load_art_from_url);
        *cached_art = Some((key, art.clone()));
        art
    };

    Some(MediaTrack {
        title,
        artist,
        is_playing,
        position_ms,
        duration_ms,
        shuffle,
        repeat,
        album_art,
        updated_at: Instant::now(),
    })
}

fn load_art_from_url(url: &str) -> Option<Arc<AlbumArt>> {
    if let Some(path) = url.strip_prefix("file://") {
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
        let cjk = cjk_fallback_fonts();
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
}
