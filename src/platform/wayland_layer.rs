use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use calloop::EventLoop;
use calloop::ping::make_ping;
use calloop_wayland_source::WaylandSource;
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers as KeyboardModifiers,
};
use smithay_client_toolkit::seat::pointer::{
    BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent, PointerEventKind, PointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::calendar::{self, CalendarCommand};
use crate::config::Config;
use crate::lyrics;
use crate::platform::{self, linux};
use crate::tray::TrayController;
use crate::ui::soft_render::SoftCanvas;
use crate::ui::{self, AppChannels, IslandApp, OverlayBackendMode, SETTINGS_WINDOW_TITLE};

const LAYER_NAMESPACE: &str = "liebert-dynamic-island";

enum SettingsIpcCmd {
    ConfigUpdated,
    TodosUpdated,
    TestNotification,
    CalendarSetUrl(String),
    CalendarRefresh,
}

struct WaylandState {
    registry_state: RegistryState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm_state: Shm,
    seat_state: SeatState,
    output_state: OutputState,
    layer: Option<LayerSurface>,
    empty_region: Option<Region>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    configured: bool,
    ever_configured: bool,
    surface_closed: bool,
    needs_recreate: bool,
    needs_redraw: bool,
    scale_factor: i32,
    logical_width: u32,
    logical_height: u32,
    pointer_inside: bool,
    last_pointer_pos: Pos2,
    pending_events: Vec<Event>,
}

impl WaylandState {
    fn create_layer_surface(
        &mut self,
        qh: &QueueHandle<Self>,
        width: u32,
        height: u32,
        margin_top: i32,
        visible: bool,
    ) {
        self.layer = None;
        let wl_surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            wl_surface,
            Layer::Overlay,
            Some(LAYER_NAMESPACE),
            None,
        );
        layer.set_anchor(Anchor::TOP);
        layer.set_exclusive_zone(-1);
        // shortcut: winit/eframe have no wlr-layer-shell keyboard grab, so OnDemand is used to
        // receive Left/Right only after the user clicks the island; upgrade when a proper
        // compositor-agnostic global-shortcut protocol is available.
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        layer.set_margin(margin_top, 0, 0, 0);
        layer.set_size(width.max(1), height.max(1));
        if !visible && let Some(region) = &self.empty_region {
            layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
        }
        layer.commit();

        self.configured = false;
        self.surface_closed = false;
        self.needs_recreate = false;
        self.needs_redraw = true;
        self.logical_width = width.max(1);
        self.logical_height = height.max(1);
        self.layer = Some(layer);
    }
}

impl CompositorHandler for WaylandState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        if self
            .layer
            .as_ref()
            .is_some_and(|layer| layer.wl_surface() == surface)
        {
            let next = new_factor.max(1);
            if next != self.scale_factor {
                // Defer set_buffer_scale until a matching sized wl_shm buffer is attached atomically.
                self.scale_factor = next;
                self.needs_redraw = true;
            }
        }
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.needs_redraw = true;
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if self
            .layer
            .as_ref()
            .is_some_and(|layer| layer.wl_surface() == surface)
            && let Some(info) = self.output_state.info(output)
        {
            let next = info.scale_factor.max(1);
            if next != self.scale_factor {
                self.scale_factor = next;
                self.needs_redraw = true;
            }
        }
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for WaylandState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        if self.layer.is_none() {
            self.needs_recreate = true;
        }
        self.needs_redraw = true;
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        self.needs_redraw = true;
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        self.needs_redraw = true;
    }
}

impl ShmHandler for WaylandState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl LayerShellHandler for WaylandState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        if self.layer.as_ref().is_some_and(|active| active == layer) {
            self.layer = None;
            self.configured = false;
            self.surface_closed = true;
            self.needs_recreate = true;
            if self.pointer_inside {
                self.pointer_inside = false;
                self.pending_events.push(Event::PointerGone);
            }
            self.needs_redraw = true;
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if self.layer.as_ref().is_some_and(|active| active == layer) {
            if configure.new_size.0 > 0 {
                self.logical_width = configure.new_size.0;
            }
            if configure.new_size.1 > 0 {
                self.logical_height = configure.new_size.1;
            }
            self.configured = true;
            self.ever_configured = true;
            self.needs_redraw = true;
        }
    }
}

impl SeatHandler for WaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && self.pointer.is_none()
            && let Ok(pointer) = self.seat_state.get_pointer(qh, &seat)
        {
            self.pointer = Some(pointer);
        }
        if capability == Capability::Keyboard
            && self.keyboard.is_none()
            && let Ok(keyboard) = self.seat_state.get_keyboard(qh, &seat, None)
        {
            self.keyboard = Some(keyboard);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for WaylandState {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let key = match event.keysym {
            Keysym::Left => Key::ArrowLeft,
            Keysym::Right => Key::ArrowRight,
            _ => return,
        };
        self.pending_events.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        self.pending_events.push(Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        self.needs_redraw = true;
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _modifiers: KeyboardModifiers,
        _layout: u32,
    ) {
    }
}

delegate_keyboard!(WaylandState);

impl PointerHandler for WaylandState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(active_surface) = self.layer.as_ref().map(|layer| layer.wl_surface()) else {
            return;
        };
        for event in events {
            if &event.surface != active_surface {
                continue;
            }
            let pos = Pos2::new(event.position.0 as f32, event.position.1 as f32);
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    self.pointer_inside = true;
                    self.last_pointer_pos = pos;
                    self.pending_events.push(Event::PointerMoved(pos));
                    self.needs_redraw = true;
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_inside = false;
                    self.pending_events.push(Event::PointerGone);
                    self.needs_redraw = true;
                }
                PointerEventKind::Motion { .. } => {
                    self.pointer_inside = true;
                    self.last_pointer_pos = pos;
                    self.pending_events.push(Event::PointerMoved(pos));
                    self.needs_redraw = true;
                }
                PointerEventKind::Press { button, .. } => {
                    if let Some(btn) = map_wayland_button(button) {
                        self.last_pointer_pos = pos;
                        self.pending_events.push(Event::PointerButton {
                            pos,
                            button: btn,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        });
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    if let Some(btn) = map_wayland_button(button) {
                        self.last_pointer_pos = pos;
                        self.pending_events.push(Event::PointerButton {
                            pos,
                            button: btn,
                            pressed: false,
                            modifiers: Modifiers::NONE,
                        });
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    let dx = if horizontal.absolute.abs() > 0.0 {
                        -(horizontal.absolute as f32)
                    } else {
                        -(horizontal.discrete as f32) * 20.0
                    };
                    let dy = if vertical.absolute.abs() > 0.0 {
                        -(vertical.absolute as f32)
                    } else {
                        -(vertical.discrete as f32) * 20.0
                    };
                    if dx != 0.0 || dy != 0.0 {
                        self.pending_events.push(Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: Vec2::new(dx, dy),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                            modifiers: Modifiers::NONE,
                        });
                        self.needs_redraw = true;
                    }
                }
            }
        }
    }
}

impl ProvidesRegistryState for WaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(WaylandState);
delegate_output!(WaylandState);
delegate_shm!(WaylandState);
delegate_layer!(WaylandState);
delegate_seat!(WaylandState);
delegate_pointer!(WaylandState);
delegate_registry!(WaylandState);

fn map_wayland_button(button: u32) -> Option<PointerButton> {
    match button {
        BTN_LEFT => Some(PointerButton::Primary),
        BTN_RIGHT => Some(PointerButton::Secondary),
        BTN_MIDDLE => Some(PointerButton::Middle),
        _ => None,
    }
}

/// Skip egui tessellation/raster work when nothing changed and no animation deadline passed.
fn should_draw(needs_redraw: bool, repaint_after: Duration, since_last_draw: Duration) -> bool {
    needs_redraw || repaint_after.is_zero() || since_last_draw >= repaint_after
}

fn spawn_settings_ipc_listener(
    tx: mpsc::Sender<SettingsIpcCmd>,
    ping: calloop::ping::Ping,
) -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let sock_path = runtime.join(format!("liebert-settings-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path).ok()?;
    std::thread::Builder::new()
        .name("settings-ipc-listener".to_owned())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let reader = BufReader::new(stream);
                for line in reader.lines().map_while(Result::ok) {
                    let cmd = match line.as_str() {
                        "config" => Some(SettingsIpcCmd::ConfigUpdated),
                        "todos" => Some(SettingsIpcCmd::TodosUpdated),
                        "test_notif" => Some(SettingsIpcCmd::TestNotification),
                        "cal_refresh" => Some(SettingsIpcCmd::CalendarRefresh),
                        other => other
                            .strip_prefix("cal_url:")
                            .map(|url| SettingsIpcCmd::CalendarSetUrl(url.to_owned())),
                    };
                    if let Some(cmd) = cmd {
                        let _ = tx.send(cmd);
                        ping.ping();
                    }
                }
            }
        })
        .ok()?;
    Some(sock_path)
}

fn launch_or_focus_settings(child: &mut Option<Child>, ipc_sock: Option<&Path>) {
    if let Some(proc) = child.as_mut() {
        match proc.try_wait() {
            Ok(None) => {
                let _ = linux::bring_window_to_front(SETTINGS_WINDOW_TITLE);
                return;
            }
            Ok(Some(_)) | Err(_) => {
                *child = None;
            }
        }
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.arg("--settings");
    if let Some(sock) = ipc_sock {
        cmd.arg("--ipc-sock").arg(sock);
    }
    if let Ok(spawned) = cmd.spawn() {
        *child = Some(spawned);
    }
}

struct SocketCleanup(Option<PathBuf>, Option<Child>);

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        if let Some(mut child) = self.1.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn try_run_layer_shell(
    config: Config,
    tray: Option<TrayController>,
    demo_mode: bool,
    initial_w: f32,
    initial_h: f32,
    initial_y: f32,
) -> Result<(), String> {
    let conn =
        Connection::connect_to_env().map_err(|e| format!("Wayland connection unavailable: {e}"))?;
    let (globals, mut event_queue) =
        registry_queue_init(&conn).map_err(|e| format!("Wayland registry init failed: {e}"))?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| format!("wl_compositor unavailable: {e}"))?;
    let layer_shell =
        LayerShell::bind(&globals, &qh).map_err(|e| format!("wlr-layer-shell unavailable: {e}"))?;
    let shm_state = Shm::bind(&globals, &qh).map_err(|e| format!("wl_shm unavailable: {e}"))?;

    linux::prepare_layer_shell_rules(LAYER_NAMESPACE);

    let init_w = initial_w.round().max(1.0) as u32;
    let init_h = initial_h.round().max(1.0) as u32;
    let init_margin_top = initial_y.round() as i32;
    let empty_region = Region::new(&compositor).ok();

    let mut wl_state = WaylandState {
        registry_state: RegistryState::new(&globals),
        compositor,
        layer_shell,
        shm_state,
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        layer: None,
        empty_region,
        pointer: None,
        keyboard: None,
        configured: false,
        ever_configured: false,
        surface_closed: false,
        needs_recreate: false,
        needs_redraw: true,
        scale_factor: 1,
        logical_width: init_w,
        logical_height: init_h,
        pointer_inside: false,
        last_pointer_pos: Pos2::ZERO,
        pending_events: Vec::new(),
    };
    wl_state.create_layer_surface(
        &qh,
        init_w,
        init_h,
        init_margin_top,
        !config.auto_hide_when_idle,
    );

    let startup_deadline = Instant::now() + Duration::from_secs(2);
    while !wl_state.configured && Instant::now() < startup_deadline {
        event_queue
            .blocking_dispatch(&mut wl_state)
            .map_err(|e| format!("Wayland initial configure failed: {e}"))?;
        if wl_state.surface_closed {
            // If compositor had no outputs ready yet, wait cleanly in the main loop;
            // only abort before entering if outputs were present and still rejected immediately.
            break;
        }
    }
    if wl_state.surface_closed && wl_state.output_state.outputs().next().is_some() {
        return Err("wlr-layer-shell surface closed before first configure".to_owned());
    }

    // 512 KB initial SHM pool covers the idle/compact pill without GPU driver overhead.
    let mut pool = SlotPool::new(512 * 1024, &wl_state.shm_state)
        .map_err(|e| format!("wl_shm SlotPool init failed: {e}"))?;
    let mut soft_canvas = SoftCanvas::new();
    let mut active_buffer: Option<Buffer> = None;

    let egui_ctx = egui::Context::default();
    ui::apply_configured_fonts(&egui_ctx, &config.font_name);

    let mut event_loop: EventLoop<WaylandState> =
        EventLoop::try_new().map_err(|e| format!("calloop init failed: {e}"))?;
    let loop_handle = event_loop.handle();

    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle.clone())
        .map_err(|e| format!("WaylandSource insert failed: {e}"))?;

    let (ping, ping_source) = make_ping().map_err(|e| format!("calloop ping failed: {e}"))?;
    loop_handle
        .insert_source(ping_source, |_, _, state| {
            state.needs_redraw = true;
        })
        .map_err(|e| format!("Ping source insert failed: {e}"))?;

    let (event_tx, event_rx) = mpsc::channel();
    let (lyrics_req_tx, lyrics_req_rx) = mpsc::channel();
    let (media_cmd_tx, media_cmd_rx) = mpsc::channel();
    let (calendar_cmd_tx, calendar_cmd_rx) = mpsc::channel();
    let (sysmon_cmd_tx, sysmon_cmd_rx) = mpsc::channel();

    let wake_ping = ping.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || wake_ping.ping());

    platform::spawn_listener(event_tx.clone(), wake.clone());
    platform::spawn_media_listener(event_tx.clone(), media_cmd_rx, wake.clone());
    platform::spawn_fullscreen_watcher(event_tx.clone(), wake.clone());
    lyrics::spawn_lyrics_worker(lyrics_req_rx, event_tx.clone(), wake.clone());
    calendar::spawn_calendar_worker(
        config.google_calendar_ics_url.clone(),
        calendar_cmd_rx,
        event_tx.clone(),
        wake.clone(),
    );
    crate::sysmon::spawn_sysmon_worker(
        sysmon_cmd_rx,
        config.taskbar_monitor_enabled,
        config.taskbar_interval_ms,
        config.taskbar_show_disk,
        config.taskbar_show_network,
        event_tx,
        wake,
    );

    let (settings_tx, settings_rx) = mpsc::channel();
    let ipc_sock_path = spawn_settings_ipc_listener(settings_tx, ping);
    let mut cleanup = SocketCleanup(ipc_sock_path, None);

    let mut app = IslandApp::new(
        config,
        AppChannels {
            event_rx,
            lyrics_req_tx,
            media_cmd_tx,
            calendar_cmd_tx,
            ui_ctrl_rx: None,
            daemon_tx: None,
            sysmon_cmd_tx: Some(sysmon_cmd_tx),
        },
        tray,
        demo_mode,
    );

    let start_time = Instant::now();
    let mut committed_w = init_w;
    let mut committed_h = init_h;
    let mut committed_margin_top = init_margin_top;
    let mut committed_visible = wl_state.layer.is_some();
    let mut committed_scale = wl_state.scale_factor.max(1);
    let mut hidden_frame_committed = false;
    let mut fallback_reason: Option<String> = None;
    let mut last_draw = Instant::now();
    let mut last_recreate_attempt = Instant::now() - Duration::from_secs(5);
    let mut recreate_backoff = Duration::from_millis(120);

    loop {
        while let Ok(cmd) = settings_rx.try_recv() {
            match cmd {
                SettingsIpcCmd::ConfigUpdated => {
                    app.apply_external_config(Config::load(), &egui_ctx);
                }
                SettingsIpcCmd::TodosUpdated => {
                    app.reload_todos(&egui_ctx);
                }
                SettingsIpcCmd::TestNotification => {
                    app.trigger_test_notification(&egui_ctx);
                }
                SettingsIpcCmd::CalendarSetUrl(url) => {
                    app.send_calendar_command(CalendarCommand::SetUrl(url));
                }
                SettingsIpcCmd::CalendarRefresh => {
                    app.send_calendar_command(CalendarCommand::Refresh);
                }
            }
            wl_state.needs_redraw = true;
        }

        let step = app.step_logic(
            &egui_ctx,
            OverlayBackendMode::WaylandLayerShell,
            wl_state.pointer_inside,
        );

        if step.want_exit {
            break;
        }
        if step.open_settings_requested {
            launch_or_focus_settings(&mut cleanup.1, cleanup.0.as_deref());
        }

        let want_w = if step.visible {
            step.size.x.round().max(1.0) as u32
        } else {
            1
        };
        let want_h = if step.visible {
            step.size.y.round().max(1.0) as u32
        } else {
            1
        };
        let want_margin_top = step.top_y.round() as i32;
        let cur_scale = wl_state.scale_factor.max(1);

        // Recreate layer surface cleanly after Hyprland DPMS sleep, monitor hotplug, or config reload.
        if wl_state.layer.is_none() || wl_state.surface_closed || wl_state.needs_recreate {
            active_buffer = None;
            hidden_frame_committed = false;
            if wl_state.output_state.outputs().next().is_some()
                && last_recreate_attempt.elapsed() >= recreate_backoff
            {
                last_recreate_attempt = Instant::now();
                recreate_backoff = (recreate_backoff * 2).min(Duration::from_secs(2));
                linux::prepare_layer_shell_rules(LAYER_NAMESPACE);
                wl_state.create_layer_surface(&qh, want_w, want_h, want_margin_top, step.visible);
                committed_w = want_w;
                committed_h = want_h;
                committed_margin_top = want_margin_top;
                committed_visible = step.visible;
                committed_scale = 1;
                let _ = conn.flush();
            }
            if event_loop
                .dispatch(Some(Duration::from_millis(60)), &mut wl_state)
                .is_err()
            {
                if !wl_state.ever_configured {
                    fallback_reason = Some("Wayland event loop dispatch failed".to_owned());
                }
                break;
            }
            continue;
        }

        if !wl_state.configured {
            if event_loop
                .dispatch(Some(Duration::from_millis(20)), &mut wl_state)
                .is_err()
            {
                if !wl_state.ever_configured {
                    fallback_reason = Some("Wayland initial configure dispatch failed".to_owned());
                }
                break;
            }
            continue;
        }

        // Surface has been healthy and configured; reset recreation backoff.
        if last_recreate_attempt.elapsed() > Duration::from_secs(2) {
            recreate_backoff = Duration::from_millis(120);
        }

        let Some(layer) = wl_state.layer.as_ref() else {
            continue;
        };
        let wl_surface = layer.wl_surface();

        if !step.visible {
            let need_hidden_commit = !hidden_frame_committed
                || committed_visible
                || want_margin_top != committed_margin_top;
            if need_hidden_commit {
                if committed_visible && let Some(region) = &wl_state.empty_region {
                    wl_surface.set_input_region(Some(region.wl_region()));
                    wl_state.pointer_inside = false;
                }
                if committed_w != 1 || committed_h != 1 {
                    layer.set_size(1, 1);
                }
                if want_margin_top != committed_margin_top {
                    layer.set_margin(want_margin_top, 0, 0, 0);
                }
                if committed_scale != 1 {
                    wl_surface.set_buffer_scale(1);
                }
                if let Ok((buffer, bytes)) = pool.create_buffer(1, 1, 4, wl_shm::Format::Argb8888) {
                    bytes[..4].fill(0);
                    let _ = buffer.attach_to(wl_surface);
                    wl_surface.damage_buffer(0, 0, 1, 1);
                    wl_surface.commit();
                    active_buffer = Some(buffer);
                } else {
                    wl_surface.commit();
                }
                let _ = conn.flush();
                committed_w = 1;
                committed_h = 1;
                committed_margin_top = want_margin_top;
                committed_visible = false;
                committed_scale = 1;
                hidden_frame_committed = true;
                wl_state.needs_redraw = false;
                wl_state.pending_events.clear();
            }

            if event_loop
                .dispatch(Some(Duration::from_millis(80)), &mut wl_state)
                .is_err()
            {
                if !wl_state.ever_configured {
                    fallback_reason = Some("Wayland event loop dispatch failed".to_owned());
                }
                break;
            }
            continue;
        }

        hidden_frame_committed = false;
        let state_changed = !committed_visible
            || want_w != committed_w
            || want_h != committed_h
            || want_margin_top != committed_margin_top
            || cur_scale != committed_scale;

        if !state_changed
            && !should_draw(
                wl_state.needs_redraw,
                step.repaint_after,
                last_draw.elapsed(),
            )
        {
            if event_loop
                .dispatch(Some(Duration::from_millis(20)), &mut wl_state)
                .is_err()
            {
                if !wl_state.ever_configured {
                    fallback_reason = Some("Wayland event loop dispatch failed".to_owned());
                }
                break;
            }
            continue;
        }
        last_draw = Instant::now();

        let phys_w_px = (want_w * cur_scale as u32).max(1);
        let phys_h_px = (want_h * cur_scale as u32).max(1);
        let ppp = cur_scale as f32;

        let raw_input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(want_w as f32, want_h as f32),
            )),
            time: Some(start_time.elapsed().as_secs_f64()),
            events: std::mem::take(&mut wl_state.pending_events),
            ..Default::default()
        };

        egui_ctx.set_pixels_per_point(ppp);
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            app.render_island_ui(ui, false);
        });

        let clipped_primitives = egui_ctx.tessellate(full_output.shapes, ppp);
        soft_canvas.update_textures_before_paint(&mut full_output.textures_delta);

        let stride = (phys_w_px * 4) as i32;
        if let Ok((buffer, bytes)) = pool.create_buffer(
            phys_w_px as i32,
            phys_h_px as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) {
            soft_canvas.rasterize(phys_w_px, phys_h_px, ppp, &clipped_primitives, bytes, true);

            if !committed_visible {
                wl_surface.set_input_region(None);
            }
            if want_w != committed_w || want_h != committed_h {
                layer.set_size(want_w, want_h);
            }
            if want_margin_top != committed_margin_top {
                layer.set_margin(want_margin_top, 0, 0, 0);
            }
            if cur_scale != committed_scale {
                wl_surface.set_buffer_scale(cur_scale);
            }
            let _ = buffer.attach_to(wl_surface);
            wl_surface.damage_buffer(0, 0, phys_w_px as i32, phys_h_px as i32);
            wl_surface.commit();
            active_buffer = Some(buffer);

            committed_w = want_w;
            committed_h = want_h;
            committed_margin_top = want_margin_top;
            committed_visible = true;
            committed_scale = cur_scale;
        }
        soft_canvas.free_textures_after_paint(full_output.textures_delta);

        let _ = conn.flush();
        wl_state.needs_redraw = false;

        let wait = if step.repaint_after.is_zero() {
            Duration::from_millis(8)
        } else {
            step.repaint_after.min(Duration::from_millis(80))
        };
        if event_loop.dispatch(Some(wait), &mut wl_state).is_err() {
            if !wl_state.ever_configured {
                fallback_reason = Some("Wayland event loop dispatch failed".to_owned());
            }
            break;
        }
    }

    drop(active_buffer);
    match fallback_reason {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_redraw_is_skipped_until_something_changes() {
        let interval = Duration::from_millis(80);
        assert!(should_draw(true, interval, Duration::ZERO));
        assert!(should_draw(false, Duration::ZERO, Duration::ZERO));
        assert!(!should_draw(false, interval, Duration::from_millis(20)));
        assert!(should_draw(false, interval, interval));
    }
}
