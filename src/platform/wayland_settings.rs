//! Native Wayland pure-CPU Settings window rendered via `wl_shm` + [`SoftCanvas`].
//!
//! Replaces `eframe`/`wgpu`/`glow` for the standalone `--settings` process on
//! Wayland so GPU/EGL/LLVM driver shared libraries are never loaded, keeping the
//! Settings process footprint well below the 10 MB RAM budget.

use crate::ui::soft_render::SoftCanvas;
use crate::ui::{SETTINGS_WINDOW_TITLE, StandaloneSettingsApp};
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, LoopHandle, RegistrationToken};
use eframe::egui::{
    self, Event, Key, Modifiers, MouseWheelSource, MouseWheelUnit, PointerButton, Pos2, RawInput,
    Rect, TouchPhase, Vec2, ViewportId, ViewportInfo,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    reexports::calloop_wayland_source::WaylandSource,
    reexports::client::{
        Connection, Dispatch, QueueHandle,
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers as WlModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{DecorationMode, Window, WindowConfigure, WindowDecorations, WindowHandler},
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const INITIAL_WIDTH_PT: u32 = 900;
const INITIAL_HEIGHT_PT: u32 = 620;
const MIN_WIDTH_PT: u32 = 540;
const MIN_HEIGHT_PT: u32 = 440;
const CSD_BAR_HEIGHT_PT: f32 = 30.0;
const MAX_SHM_TEXTURE_SIDE: usize = 1024;

/// Runs the standalone Settings window directly on Wayland via CPU rasterization.
pub fn run_wayland_settings(app: StandaloneSettingsApp) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("Wayland connect failed: {e}"))?;
    let (globals, event_queue) =
        registry_queue_init(&conn).map_err(|e| format!("Wayland registry init failed: {e}"))?;
    let qh = event_queue.handle();

    let compositor_state =
        CompositorState::bind(&globals, &qh).map_err(|e| format!("wl_compositor missing: {e}"))?;
    let xdg_shell =
        XdgShell::bind(&globals, &qh).map_err(|e| format!("xdg_wm_base missing: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("wl_shm missing: {e}"))?;

    let surface = compositor_state.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title(SETTINGS_WINDOW_TITLE);
    window.set_app_id("liebert-dynamic-island-settings");
    window.set_min_size(Some((MIN_WIDTH_PT, MIN_HEIGHT_PT)));
    window.commit();

    let egui_ctx = egui::Context::default();
    crate::ui::apply_configured_fonts_light(&egui_ctx, &app.config.font_name);

    let mut event_loop: EventLoop<WaylandSettingsState> =
        EventLoop::try_new().map_err(|e| format!("calloop init failed: {e}"))?;
    let loop_handle = event_loop.handle();

    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle.clone())
        .map_err(|e| format!("WaylandSource insert failed: {e}"))?;

    let mut state = WaylandSettingsState {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        _xdg_shell: xdg_shell,
        qh,
        window,
        pool: None,
        soft: SoftCanvas::new(),
        egui_ctx,
        app,
        loop_handle,
        configured: false,
        exit: false,
        needs_redraw: true,
        frame_pending: false,
        repaint_timer: None,
        width_pt: INITIAL_WIDTH_PT,
        height_pt: INITIAL_HEIGHT_PT,
        scale_factor: 1,
        decoration_mode: DecorationMode::Server,
        pointer: None,
        keyboard: None,
        active_seat: None,
        pointer_pos: None,
        modifiers: Modifiers::NONE,
        egui_events: Vec::with_capacity(16),
        start_time: Instant::now(),
        last_csd_hover: false,
    };

    while !state.exit {
        if let Err(err) = event_loop.dispatch(Some(Duration::from_millis(250)), &mut state) {
            return Err(format!("Wayland settings loop error: {err}"));
        }
        if state.configured && state.needs_redraw && !state.frame_pending && !state.exit {
            state.render_frame();
        }
        let _ = conn.flush();
    }

    Ok(())
}

struct WaylandSettingsState {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    _xdg_shell: XdgShell,
    qh: QueueHandle<Self>,
    window: Window,
    pool: Option<SlotPool>,
    soft: SoftCanvas,
    egui_ctx: egui::Context,
    app: StandaloneSettingsApp,
    loop_handle: LoopHandle<'static, Self>,
    configured: bool,
    exit: bool,
    needs_redraw: bool,
    frame_pending: bool,
    repaint_timer: Option<RegistrationToken>,
    width_pt: u32,
    height_pt: u32,
    scale_factor: i32,
    decoration_mode: DecorationMode,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    active_seat: Option<wl_seat::WlSeat>,
    pointer_pos: Option<Pos2>,
    modifiers: Modifiers,
    egui_events: Vec<Event>,
    start_time: Instant,
    last_csd_hover: bool,
}

impl WaylandSettingsState {
    fn schedule_repaint(&mut self, delay: Duration) {
        if let Some(tok) = self.repaint_timer.take() {
            self.loop_handle.remove(tok);
        }
        if delay.is_zero() {
            self.needs_redraw = true;
            return;
        }
        let timer = Timer::from_duration(delay);
        if let Ok(tok) =
            self.loop_handle
                .insert_source(timer, |_, _, state: &mut WaylandSettingsState| {
                    state.repaint_timer = None;
                    state.needs_redraw = true;
                    TimeoutAction::Drop
                })
        {
            self.repaint_timer = Some(tok);
        }
    }

    fn render_frame(&mut self) {
        let scale = self.scale_factor.max(1) as f32;
        let width_pt = self.width_pt.max(MIN_WIDTH_PT);
        let height_pt = self.height_pt.max(MIN_HEIGHT_PT);
        let width_px = ((width_pt as f32 * scale).round() as u32).max(1);
        let height_px = ((height_pt as f32 * scale).round() as u32).max(1);

        let screen_rect =
            Rect::from_min_size(Pos2::ZERO, Vec2::new(width_pt as f32, height_pt as f32));
        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(
            ViewportId::ROOT,
            ViewportInfo {
                native_pixels_per_point: Some(scale),
                ..Default::default()
            },
        );
        let raw_input = RawInput {
            screen_rect: Some(screen_rect),
            time: Some(self.start_time.elapsed().as_secs_f64()),
            events: std::mem::take(&mut self.egui_events),
            viewports,
            max_texture_side: Some(MAX_SHM_TEXTURE_SIDE),
            ..Default::default()
        };

        let is_csd = self.decoration_mode == DecorationMode::Client;
        let mut csd_close_clicked = false;
        let app = &mut self.app;
        let full_output = self.egui_ctx.run_ui(raw_input, |ui| {
            if is_csd {
                egui::Panel::top("wayland_csd_bar")
                    .exact_size(CSD_BAR_HEIGHT_PT)
                    .show_separator_line(false)
                    .frame(
                        egui::Frame::NONE
                            .fill(egui::Color32::from_rgb(18, 20, 26))
                            .inner_margin(egui::Margin::symmetric(12, 4)),
                    )
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.label(
                                egui::RichText::new(SETTINGS_WINDOW_TITLE)
                                    .size(12.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(208, 214, 224)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let close_btn = egui::Button::new(
                                        egui::RichText::new("X")
                                            .size(11.5)
                                            .strong()
                                            .color(egui::Color32::from_rgb(235, 238, 245)),
                                    )
                                    .fill(egui::Color32::from_rgb(56, 32, 38))
                                    .corner_radius(6.0);
                                    if ui.add_sized([26.0, 20.0], close_btn).clicked() {
                                        csd_close_clicked = true;
                                    }
                                },
                            );
                        });
                    });
            }
            app.render_ui(ui);
        });

        if csd_close_clicked {
            self.exit = true;
            return;
        }

        for cmd in &full_output.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = cmd {
                copy_to_wayland_clipboard(text);
            }
        }

        let mut textures_delta = full_output.textures_delta;
        self.soft.update_textures_before_paint(&mut textures_delta);
        let primitives = self.egui_ctx.tessellate(full_output.shapes, scale);

        let stride = (width_px as usize) * 4;
        let byte_len = stride * (height_px as usize);
        let target_pool_bytes = byte_len.saturating_mul(2).max(4096);

        let pool = match self.pool.as_mut() {
            Some(pool) => {
                // Reallocate down if the window shrank significantly so RAM returns to OS.
                if pool.len() > target_pool_bytes.saturating_mul(2)
                    && let Ok(shrunk) = SlotPool::new(target_pool_bytes, &self.shm)
                {
                    *pool = shrunk;
                }
                pool
            }
            None => match SlotPool::new(target_pool_bytes, &self.shm) {
                Ok(pool) => self.pool.insert(pool),
                Err(_) => return,
            },
        };

        if let Ok((buffer, canvas)) = pool.create_buffer(
            width_px as i32,
            height_px as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
        ) {
            self.soft
                .rasterize(width_px, height_px, scale, &primitives, canvas, true);

            let surface = self.window.wl_surface();
            surface.set_buffer_scale(self.scale_factor.max(1));
            let _ = buffer.attach_to(surface);
            surface.damage_buffer(0, 0, width_px as i32, height_px as i32);
            surface.frame(&self.qh, surface.clone());
            surface.commit();

            self.frame_pending = true;
            self.needs_redraw = false;
        }

        self.soft.free_textures_after_paint(textures_delta);

        if let Some(vp) = full_output.viewport_output.get(&ViewportId::ROOT) {
            let delay = vp.repaint_delay;
            if delay.is_zero() {
                self.needs_redraw = true;
            } else if delay < Duration::from_secs(30) {
                self.schedule_repaint(delay);
            }
        }
    }

    fn push_key_event(&mut self, event: KeyEvent, pressed: bool, repeat: bool) {
        if pressed && self.modifiers.ctrl {
            if event.keysym == Keysym::c || event.keysym == Keysym::C {
                self.egui_events.push(Event::Copy);
                self.needs_redraw = true;
                return;
            }
            if event.keysym == Keysym::x || event.keysym == Keysym::X {
                self.egui_events.push(Event::Cut);
                self.needs_redraw = true;
                return;
            }
            if event.keysym == Keysym::v || event.keysym == Keysym::V {
                if let Some(clip) = paste_from_wayland_clipboard() {
                    self.egui_events.push(Event::Paste(clip));
                    self.needs_redraw = true;
                }
                return;
            }
        }

        if let Some(key) = map_keysym(event.keysym) {
            self.egui_events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat,
                modifiers: self.modifiers,
            });
            self.needs_redraw = true;
        }

        if pressed
            && !self.modifiers.ctrl
            && !self.modifiers.alt
            && !self.modifiers.mac_cmd
            && let Some(text) = event.utf8
            && !text.is_empty()
            && text.chars().all(|c| !c.is_control())
        {
            self.egui_events.push(Event::Text(text));
            self.needs_redraw = true;
        }
    }
}

impl CompositorHandler for WaylandSettingsState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        let clamped = new_factor.max(1);
        if clamped != self.scale_factor {
            self.scale_factor = clamped;
            self.needs_redraw = true;
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
        self.frame_pending = false;
        if self.needs_redraw && !self.exit {
            self.render_frame();
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if let Some(info) = self.output_state.info(output) {
            let sf = info.scale_factor.max(1);
            if sf != self.scale_factor {
                self.scale_factor = sf;
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

impl OutputHandler for WaylandSettingsState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl ShmHandler for WaylandSettingsState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl WindowHandler for WaylandSettingsState {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        self.configured = true;
        self.decoration_mode = configure.decoration_mode;
        if let Some(w) = configure.new_size.0 {
            self.width_pt = w.get().max(MIN_WIDTH_PT);
        }
        if let Some(h) = configure.new_size.1 {
            self.height_pt = h.get().max(MIN_HEIGHT_PT);
        }
        self.needs_redraw = true;
        if !self.frame_pending {
            self.render_frame();
        }
    }
}

impl SeatHandler for WaylandSettingsState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.active_seat.is_none() {
            self.active_seat = Some(seat);
        }
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if self.active_seat.is_none() {
            self.active_seat = Some(seat.clone());
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let lh = self.loop_handle.clone();
            self.keyboard = self
                .seat_state
                .get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    lh,
                    Box::new(|state, _kbd, event| {
                        state.push_key_event(event, true, true);
                    }),
                )
                .ok();
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(p) = self.pointer.take()
        {
            p.release();
        }
        if capability == Capability::Keyboard
            && let Some(k) = self.keyboard.take()
        {
            k.release();
        }
    }

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {
    }
}

impl PointerHandler for WaylandSettingsState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let pos = Pos2::new(event.position.0 as f32, event.position.1 as f32);
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.pointer_pos = Some(pos);
                    self.egui_events.push(Event::PointerMoved(pos));
                    let in_csd = self.decoration_mode == DecorationMode::Client
                        && pos.y <= CSD_BAR_HEIGHT_PT;
                    if in_csd != self.last_csd_hover {
                        self.last_csd_hover = in_csd;
                    }
                    self.needs_redraw = true;
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_pos = None;
                    self.egui_events.push(Event::PointerGone);
                    self.needs_redraw = true;
                }
                PointerEventKind::Press { button, serial, .. } => {
                    self.pointer_pos = Some(pos);
                    if self.decoration_mode == DecorationMode::Client
                        && button == 0x110
                        && pos.y <= CSD_BAR_HEIGHT_PT
                        && pos.x < (self.width_pt as f32 - 44.0)
                    {
                        if let Some(seat) = &self.active_seat {
                            self.window.move_(seat, serial);
                        }
                        continue;
                    }
                    if let Some(btn) = map_pointer_button(button) {
                        self.egui_events.push(Event::PointerButton {
                            pos,
                            button: btn,
                            pressed: true,
                            modifiers: self.modifiers,
                        });
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    self.pointer_pos = Some(pos);
                    if let Some(btn) = map_pointer_button(button) {
                        self.egui_events.push(Event::PointerButton {
                            pos,
                            button: btn,
                            pressed: false,
                            modifiers: self.modifiers,
                        });
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    let dx = if horizontal.discrete != 0 {
                        -(horizontal.discrete as f32) * 32.0
                    } else {
                        -(horizontal.absolute as f32)
                    };
                    let dy = if vertical.discrete != 0 {
                        -(vertical.discrete as f32) * 32.0
                    } else {
                        -(vertical.absolute as f32)
                    };
                    if dx.abs() > 0.01 || dy.abs() > 0.01 {
                        self.egui_events.push(Event::MouseWheel {
                            unit: MouseWheelUnit::Point,
                            delta: Vec2::new(dx, dy),
                            phase: TouchPhase::Move,
                            source: MouseWheelSource::Unknown,
                            modifiers: self.modifiers,
                        });
                        self.needs_redraw = true;
                    }
                }
            }
        }
    }
}

impl KeyboardHandler for WaylandSettingsState {
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
        self.egui_events.push(Event::WindowFocused(true));
        self.needs_redraw = true;
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        self.egui_events.push(Event::WindowFocused(false));
        self.needs_redraw = true;
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        self.push_key_event(event, true, false);
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        self.push_key_event(event, false, false);
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: WlModifiers,
        _layout: u32,
    ) {
        self.modifiers = Modifiers {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            mac_cmd: false,
            command: modifiers.ctrl,
        };
    }
}

impl ProvidesRegistryState for WaylandSettingsState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(WaylandSettingsState);
delegate_output!(WaylandSettingsState);
delegate_shm!(WaylandSettingsState);
delegate_seat!(WaylandSettingsState);
delegate_pointer!(WaylandSettingsState);
delegate_keyboard!(WaylandSettingsState);
delegate_xdg_shell!(WaylandSettingsState);
delegate_xdg_window!(WaylandSettingsState);
delegate_registry!(WaylandSettingsState);

fn map_pointer_button(button: u32) -> Option<PointerButton> {
    match button {
        0x110 => Some(PointerButton::Primary),
        0x111 => Some(PointerButton::Secondary),
        0x112 => Some(PointerButton::Middle),
        0x115 => Some(PointerButton::Extra1),
        0x116 => Some(PointerButton::Extra2),
        _ => None,
    }
}

fn map_keysym(keysym: Keysym) -> Option<Key> {
    match keysym {
        Keysym::BackSpace => Some(Key::Backspace),
        Keysym::Tab | Keysym::ISO_Left_Tab => Some(Key::Tab),
        Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
        Keysym::Escape => Some(Key::Escape),
        Keysym::Delete => Some(Key::Delete),
        Keysym::Insert => Some(Key::Insert),
        Keysym::Home => Some(Key::Home),
        Keysym::End => Some(Key::End),
        Keysym::Page_Up => Some(Key::PageUp),
        Keysym::Page_Down => Some(Key::PageDown),
        Keysym::Left => Some(Key::ArrowLeft),
        Keysym::Right => Some(Key::ArrowRight),
        Keysym::Up => Some(Key::ArrowUp),
        Keysym::Down => Some(Key::ArrowDown),
        Keysym::a | Keysym::A => Some(Key::A),
        Keysym::c | Keysym::C => Some(Key::C),
        Keysym::v | Keysym::V => Some(Key::V),
        Keysym::x | Keysym::X => Some(Key::X),
        Keysym::z | Keysym::Z => Some(Key::Z),
        Keysym::y | Keysym::Y => Some(Key::Y),
        _ => None,
    }
}

fn copy_to_wayland_clipboard(text: &str) {
    if let Ok(mut child) = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

fn paste_from_wayland_clipboard() -> Option<String> {
    let out = Command::new("wl-paste")
        .arg("--no-newline")
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

impl Dispatch<wl_surface::WlSurface, ()> for WaylandSettingsState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_surface::WlSurface,
        _event: wl_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}
