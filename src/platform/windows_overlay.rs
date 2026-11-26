use std::ffi::c_void;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Event, Modifiers, MouseWheelSource, MouseWheelUnit, PointerButton, Pos2, RawInput, Rect,
    TouchPhase, Vec2,
};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
    GetDeviceCaps, HBITMAP, HDC, HGDIOBJ, LOGPIXELSX, ReleaseDC, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWL_EXSTYLE, GWLP_USERDATA, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, HWND_TOPMOST,
    IDC_ARROW, KillTimer, LoadCursorW, MSG, PM_REMOVE, PeekMessageW, PostMessageW, PostQuitMessage,
    RegisterClassW, SM_CXSCREEN, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SetProcessDPIAware, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    ULW_ALPHA, UpdateLayeredWindow, WHEEL_DELTA, WM_APP, WM_CLOSE, WM_DESTROY, WM_DPICHANGED,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR};

use crate::calendar::{self, CalendarCommand};
use crate::config::Config;
use crate::ipc::DaemonSnapshot;
use crate::lyrics;
use crate::platform::{self, windows as win_platform};
use crate::tray::TrayController;
use crate::ui::soft_render::SoftCanvas;
use crate::ui::{
    self, AppChannels, IslandApp, OverlayBackendMode, SETTINGS_WINDOW_TITLE, WINDOW_TITLE,
};

const WM_APP_WAKE: u32 = WM_APP + 1;
const TIMER_TICK_ID: usize = 1;
const OVERLAY_CLASS_NAME: &str = "LiebertDynamicIslandLayered";

enum SettingsIpcCmd {
    ConfigUpdated,
    TodosUpdated,
    TestNotification,
    CalendarSetUrl(String),
    CalendarRefresh,
}

struct OverlayWndState {
    ppp: f32,
    pending_events: Vec<Event>,
    needs_redraw: bool,
    quit: bool,
}

pub fn post_wake_to_hwnd(wake_hwnd: &AtomicIsize) {
    let raw = wake_hwnd.load(Ordering::Acquire);
    if raw != 0 {
        let hwnd = HWND(raw as *mut c_void);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_WAKE, WPARAM(0), LPARAM(0));
        }
    }
}

fn lparam_to_logical_pos(lparam: LPARAM, ppp: f32) -> Pos2 {
    let raw_x = (lparam.0 & 0xFFFF) as u16 as i16 as f32;
    let raw_y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
    Pos2::new(raw_x / ppp.max(1.0), raw_y / ppp.max(1.0))
}

unsafe extern "system" fn layered_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut OverlayWndState;
    if !ptr.is_null() {
        let state = unsafe { &mut *ptr };
        match msg {
            WM_MOUSEMOVE => {
                state
                    .pending_events
                    .push(Event::PointerMoved(lparam_to_logical_pos(lparam, state.ppp)));
                state.needs_redraw = true;
                return LRESULT(0);
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
                let btn = match msg {
                    WM_LBUTTONDOWN => PointerButton::Primary,
                    WM_RBUTTONDOWN => PointerButton::Secondary,
                    _ => PointerButton::Middle,
                };
                state.pending_events.push(Event::PointerButton {
                    pos: lparam_to_logical_pos(lparam, state.ppp),
                    button: btn,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                });
                state.needs_redraw = true;
                return LRESULT(0);
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
                let btn = match msg {
                    WM_LBUTTONUP => PointerButton::Primary,
                    WM_RBUTTONUP => PointerButton::Secondary,
                    _ => PointerButton::Middle,
                };
                state.pending_events.push(Event::PointerButton {
                    pos: lparam_to_logical_pos(lparam, state.ppp),
                    button: btn,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
                state.needs_redraw = true;
                return LRESULT(0);
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32;
                let dy = (delta / WHEEL_DELTA as f32) * 24.0;
                if dy.abs() > 0.0 {
                    state.pending_events.push(Event::MouseWheel {
                        unit: MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, dy),
                        phase: TouchPhase::Move,
                        source: MouseWheelSource::Unknown,
                        modifiers: Modifiers::NONE,
                    });
                    state.needs_redraw = true;
                }
                return LRESULT(0);
            }
            WM_DPICHANGED => {
                let dpi_x = (wparam.0 & 0xFFFF) as u16 as f32;
                if dpi_x > 0.0 {
                    state.ppp = (dpi_x / 96.0).max(1.0);
                    state.needs_redraw = true;
                }
                return LRESULT(0);
            }
            WM_APP_WAKE => {
                state.needs_redraw = true;
                return LRESULT(0);
            }
            WM_TIMER => {
                return LRESULT(0);
            }
            WM_CLOSE => {
                // Ignore Alt+F4 / external close requests; exit goes through tray Exit / shutdown.
                return LRESULT(0);
            }
            WM_DESTROY => {
                state.quit = true;
                unsafe { PostQuitMessage(0) };
                return LRESULT(0);
            }
            _ => {}
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

struct LayeredDib {
    mem_dc: HDC,
    bmp: HBITMAP,
    old_bmp: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
}

impl LayeredDib {
    fn new(screen_dc: HDC) -> Result<Self, String> {
        let mem_dc = unsafe { CreateCompatibleDC(Some(screen_dc)) };
        if mem_dc.is_invalid() {
            return Err("CreateCompatibleDC failed".to_owned());
        }
        Ok(Self {
            mem_dc,
            bmp: HBITMAP::default(),
            old_bmp: HGDIOBJ::default(),
            bits: std::ptr::null_mut(),
            width: 0,
            height: 0,
        })
    }

    fn ensure_size(&mut self, screen_dc: HDC, w: u32, h: u32) -> Result<&mut [u8], String> {
        let w = w.max(1);
        let h = h.max(1);
        if self.width != w || self.height != h || self.bmp.is_invalid() || self.bits.is_null() {
            if !self.old_bmp.is_invalid() {
                unsafe {
                    SelectObject(self.mem_dc, self.old_bmp);
                }
                self.old_bmp = HGDIOBJ::default();
            }
            if !self.bmp.is_invalid() {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(self.bmp.0));
                }
                self.bmp = HBITMAP::default();
            }
            self.bits = std::ptr::null_mut();

            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                // Negative height selects top-down DIB rows matching SoftCanvas scanlines.
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut raw_bits: *mut c_void = std::ptr::null_mut();
            let bmp = unsafe {
                CreateDIBSection(
                    Some(screen_dc),
                    &bmi,
                    DIB_RGB_COLORS,
                    &mut raw_bits,
                    None,
                    0,
                )
            }
            .map_err(|e| format!("CreateDIBSection failed: {e}"))?;
            if bmp.is_invalid() || raw_bits.is_null() {
                return Err("CreateDIBSection returned null buffer".to_owned());
            }
            self.old_bmp = unsafe { SelectObject(self.mem_dc, HGDIOBJ(bmp.0)) };
            self.bmp = bmp;
            self.bits = raw_bits as *mut u8;
            self.width = w;
            self.height = h;
        }
        let byte_len = (w as usize) * (h as usize) * 4;
        Ok(unsafe { std::slice::from_raw_parts_mut(self.bits, byte_len) })
    }
}

impl Drop for LayeredDib {
    fn drop(&mut self) {
        if !self.mem_dc.is_invalid() && !self.old_bmp.is_invalid() {
            unsafe {
                SelectObject(self.mem_dc, self.old_bmp);
            }
        }
        if !self.bmp.is_invalid() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            }
        }
        if !self.mem_dc.is_invalid() {
            unsafe {
                let _ = DeleteDC(self.mem_dc);
            }
        }
    }
}

struct WindowCleanup {
    hwnd: HWND,
    wake_hwnd: Arc<AtomicIsize>,
    settings_child: Option<Child>,
    state_ptr: *mut OverlayWndState,
}

impl Drop for WindowCleanup {
    fn drop(&mut self) {
        self.wake_hwnd.store(0, Ordering::Release);
        if let Some(mut child) = self.settings_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_TICK_ID);
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
            if !self.state_ptr.is_null() {
                drop(Box::from_raw(self.state_ptr));
            }
        }
    }
}

fn measure_system_ppp() -> f32 {
    let hdc = unsafe { GetDC(None) };
    if hdc.is_invalid() {
        return 1.0;
    }
    let dpi = unsafe { GetDeviceCaps(Some(hdc), LOGPIXELSX) };
    unsafe { ReleaseDC(None, hdc) };
    if dpi > 0 {
        (dpi as f32 / 96.0).max(1.0)
    } else {
        1.0
    }
}

fn set_window_passthrough(hwnd: HWND, passthrough: bool) {
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let desired = if passthrough {
        ex | (WS_EX_TRANSPARENT.0 as isize)
    } else {
        ex & !(WS_EX_TRANSPARENT.0 as isize)
    };
    if desired != ex {
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
        }
    }
}

fn spawn_settings_tcp_listener(
    tx: mpsc::Sender<SettingsIpcCmd>,
    wake_hwnd: Arc<AtomicIsize>,
) -> Option<String> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let addr = listener.local_addr().ok()?.to_string();
    std::thread::Builder::new()
        .name("win-settings-ipc".to_owned())
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
                        post_wake_to_hwnd(&wake_hwnd);
                    }
                }
            }
        })
        .ok()?;
    Some(addr)
}

fn launch_or_focus_settings(child: &mut Option<Child>, ipc_addr: Option<&str>) {
    if let Some(proc) = child.as_mut() {
        match proc.try_wait() {
            Ok(None) => {
                win_platform::bring_window_to_front(SETTINGS_WINDOW_TITLE);
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
    if let Some(addr) = ipc_addr {
        cmd.arg("--ipc-addr").arg(addr);
    }
    if let Ok(spawned) = cmd.spawn() {
        *child = Some(spawned);
    }
}

pub fn try_run_layered_direct(
    config: Config,
    tray: Option<TrayController>,
    demo_mode: bool,
) -> Result<(), String> {
    let wake_hwnd = Arc::new(AtomicIsize::new(0));
    let wake_ref = wake_hwnd.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || post_wake_to_hwnd(&wake_ref));

    let (event_tx, event_rx) = mpsc::channel();
    let (lyrics_req_tx, lyrics_req_rx) = mpsc::channel();
    let (media_cmd_tx, media_cmd_rx) = mpsc::channel();
    let (calendar_cmd_tx, calendar_cmd_rx) = mpsc::channel();

    platform::spawn_listener(event_tx.clone(), wake.clone());
    platform::spawn_media_listener(event_tx.clone(), media_cmd_rx, wake.clone());
    platform::spawn_fullscreen_watcher(event_tx.clone(), wake.clone());
    lyrics::spawn_lyrics_worker(lyrics_req_rx, event_tx.clone(), wake.clone());
    calendar::spawn_calendar_worker(
        config.google_calendar_ics_url.clone(),
        calendar_cmd_rx,
        event_tx,
        wake,
    );

    let app = IslandApp::new(
        config.clone(),
        AppChannels {
            event_rx,
            lyrics_req_tx,
            media_cmd_tx,
            calendar_cmd_tx,
            ui_ctrl_rx: None,
            daemon_tx: None,
        },
        tray,
        demo_mode,
    );

    run_layered_loop(app, &config, wake_hwnd, None)
}

pub fn run_layered_ipc_client(
    config: Config,
    snapshot: DaemonSnapshot,
    channels: AppChannels,
    wake_hwnd: Arc<AtomicIsize>,
    demo_mode: bool,
) -> Result<(), String> {
    let app = IslandApp::new(config.clone(), channels, None, demo_mode);
    run_layered_loop(app, &config, wake_hwnd, Some(snapshot))
}

fn should_draw(needs_redraw: bool, repaint_after: Duration, since_last_draw: Duration) -> bool {
    needs_redraw || repaint_after.is_zero() || since_last_draw >= repaint_after
}

fn run_layered_loop(
    mut app: IslandApp,
    initial_config: &Config,
    wake_hwnd: Arc<AtomicIsize>,
    initial_snapshot: Option<DaemonSnapshot>,
) -> Result<(), String> {
    unsafe {
        let _ = SetProcessDPIAware();
    }
    let initial_ppp = measure_system_ppp();

    let class_name = HSTRING::from(OVERLAY_CLASS_NAME);
    let title_name = HSTRING::from(WINDOW_TITLE);
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(layered_wnd_proc),
        hInstance: HINSTANCE::default(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassW(&wc);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            &class_name,
            &title_name,
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            None,
            None,
        )
    }
    .map_err(|e| format!("CreateWindowExW layered overlay failed: {e}"))?;
    if hwnd.is_invalid() {
        return Err("CreateWindowExW returned invalid HWND".to_owned());
    }

    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &DWMWCP_DONOTROUND as *const _ as *const c_void,
            std::mem::size_of_val(&DWMWCP_DONOTROUND) as u32,
        );
    }

    let state_box = Box::new(OverlayWndState {
        ppp: initial_ppp,
        pending_events: Vec::new(),
        needs_redraw: true,
        quit: false,
    });
    let state_ptr = Box::into_raw(state_box);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
    }
    wake_hwnd.store(hwnd.0 as isize, Ordering::Release);

    let (settings_tx, settings_rx) = mpsc::channel();
    let settings_ipc_addr = spawn_settings_tcp_listener(settings_tx, wake_hwnd.clone());
    let mut cleanup = WindowCleanup {
        hwnd,
        wake_hwnd,
        settings_child: None,
        state_ptr,
    };

    let screen_dc = unsafe { GetDC(None) };
    if screen_dc.is_invalid() {
        return Err("GetDC(None) failed".to_owned());
    }
    let mut dib = match LayeredDib::new(screen_dc) {
        Ok(d) => d,
        Err(err) => {
            unsafe { ReleaseDC(None, screen_dc) };
            return Err(err);
        }
    };
    unsafe { ReleaseDC(None, screen_dc) };

    let egui_ctx = egui::Context::default();
    ui::apply_configured_fonts(&egui_ctx, &initial_config.font_name);
    if let Some(snap) = initial_snapshot {
        app.apply_snapshot(snap, &egui_ctx);
    }

    let mut soft_canvas = SoftCanvas::new();
    let start_time = Instant::now();
    let mut last_draw = Instant::now();
    let mut committed_visible = false;
    let mut committed_w = 0u32;
    let mut committed_h = 0u32;
    let mut committed_x = i32::MIN;
    let mut committed_y = i32::MIN;
    let mut hidden_frame_committed = false;
    let mut shown_window = false;
    let mut active_timer_ms = 0u32;
    let mut last_hovered = false;

    loop {
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT {
                unsafe { (&mut *state_ptr).quit = true };
                break;
            }
            let _ = unsafe { TranslateMessage(&msg) };
            unsafe { DispatchMessageW(&msg) };
        }
        let wnd_state = unsafe { &mut *state_ptr };
        if wnd_state.quit {
            break;
        }

        while let Ok(cmd) = settings_rx.try_recv() {
            match cmd {
                SettingsIpcCmd::ConfigUpdated => {
                    let cfg = Config::load();
                    app.apply_external_config(cfg.clone(), &egui_ctx);
                    app.send_config_to_daemon(cfg);
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
            wnd_state.needs_redraw = true;
        }

        let step = app.step_logic(&egui_ctx, OverlayBackendMode::WindowsLayered, false);
        if step.want_exit {
            break;
        }
        if step.open_settings_requested {
            launch_or_focus_settings(&mut cleanup.settings_child, settings_ipc_addr.as_deref());
        }

        let ppp = wnd_state.ppp.max(1.0);
        let screen_w_px = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1);
        let want_w_log = if step.visible {
            step.size.x.round().max(1.0)
        } else {
            1.0
        };
        let want_h_log = if step.visible {
            step.size.y.round().max(1.0)
        } else {
            1.0
        };
        let phys_w = (want_w_log * ppp).round().max(1.0) as u32;
        let phys_h = (want_h_log * ppp).round().max(1.0) as u32;
        let win_x_px = ((screen_w_px - phys_w as i32) / 2).max(0);
        let win_y_px = (step.top_y * ppp).round() as i32;

        if !step.visible {
            if !hidden_frame_committed || committed_visible {
                set_window_passthrough(hwnd, true);
                let screen_dc = unsafe { GetDC(None) };
                if !screen_dc.is_invalid() {
                    if let Ok(pixels) = dib.ensure_size(screen_dc, 1, 1) {
                        pixels[..4].fill(0);
                        let pt_dst = POINT { x: win_x_px, y: win_y_px };
                        let sz = SIZE { cx: 1, cy: 1 };
                        let pt_src = POINT { x: 0, y: 0 };
                        let blend = BLENDFUNCTION {
                            BlendOp: AC_SRC_OVER as u8,
                            BlendFlags: 0,
                            SourceConstantAlpha: 255,
                            AlphaFormat: AC_SRC_ALPHA as u8,
                        };
                        let _ = unsafe {
                            UpdateLayeredWindow(
                                hwnd,
                                Some(screen_dc),
                                Some(&pt_dst),
                                Some(&sz),
                                Some(dib.mem_dc),
                                Some(&pt_src),
                                COLORREF(0),
                                Some(&blend),
                                ULW_ALPHA,
                            )
                        };
                    }
                    unsafe { ReleaseDC(None, screen_dc) };
                }
                committed_visible = false;
                committed_w = 1;
                committed_h = 1;
                committed_x = win_x_px;
                committed_y = win_y_px;
                hidden_frame_committed = true;
                wnd_state.needs_redraw = false;
                wnd_state.pending_events.clear();
            }
            last_hovered = false;
        } else {
            hidden_frame_committed = false;
            if !committed_visible {
                set_window_passthrough(hwnd, false);
            }

            let over_pill = platform::is_cursor_in_screen_rect(
                win_x_px - 6,
                (win_y_px - 6).max(0),
                win_x_px + phys_w as i32 + 6,
                win_y_px + phys_h as i32 + 6,
            )
            .unwrap_or(false);
            if last_hovered && !over_pill {
                wnd_state.pending_events.push(Event::PointerGone);
                wnd_state.needs_redraw = true;
            }
            last_hovered = over_pill;

            let geometry_changed = !committed_visible
                || phys_w != committed_w
                || phys_h != committed_h
                || win_x_px != committed_x
                || win_y_px != committed_y;

            if geometry_changed
                || should_draw(wnd_state.needs_redraw, step.repaint_after, last_draw.elapsed())
            {
                last_draw = Instant::now();
                let raw_input = RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(want_w_log, want_h_log),
                    )),
                    time: Some(start_time.elapsed().as_secs_f64()),
                    events: std::mem::take(&mut wnd_state.pending_events),
                    ..Default::default()
                };

                egui_ctx.set_pixels_per_point(ppp);
                let full_output = egui_ctx.run_ui(raw_input, |ui| {
                    app.render_island_ui(ui, false);
                });
                let clipped_primitives = egui_ctx.tessellate(full_output.shapes, ppp);
                soft_canvas.update_textures_before_paint(&full_output.textures_delta);

                let screen_dc = unsafe { GetDC(None) };
                if !screen_dc.is_invalid() {
                    if let Ok(pixels) = dib.ensure_size(screen_dc, phys_w, phys_h) {
                        soft_canvas.rasterize(
                            phys_w,
                            phys_h,
                            ppp,
                            &clipped_primitives,
                            pixels,
                            true,
                        );
                        let pt_dst = POINT {
                            x: win_x_px,
                            y: win_y_px,
                        };
                        let sz = SIZE {
                            cx: phys_w as i32,
                            cy: phys_h as i32,
                        };
                        let pt_src = POINT { x: 0, y: 0 };
                        let blend = BLENDFUNCTION {
                            BlendOp: AC_SRC_OVER as u8,
                            BlendFlags: 0,
                            SourceConstantAlpha: 255,
                            AlphaFormat: AC_SRC_ALPHA as u8,
                        };
                        let _ = unsafe {
                            UpdateLayeredWindow(
                                hwnd,
                                Some(screen_dc),
                                Some(&pt_dst),
                                Some(&sz),
                                Some(dib.mem_dc),
                                Some(&pt_src),
                                COLORREF(0),
                                Some(&blend),
                                ULW_ALPHA,
                            )
                        };
                        if !shown_window {
                            shown_window = true;
                            unsafe {
                                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                            }
                        }
                        if !committed_visible {
                            unsafe {
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
                        committed_visible = true;
                        committed_w = phys_w;
                        committed_h = phys_h;
                        committed_x = win_x_px;
                        committed_y = win_y_px;
                    }
                    unsafe { ReleaseDC(None, screen_dc) };
                }
                soft_canvas.free_textures_after_paint(full_output.textures_delta);
                wnd_state.needs_redraw = false;
            }
        }

        let desired_timer_ms = if step.repaint_after.is_zero() {
            8
        } else {
            (step.repaint_after.as_millis() as u32).clamp(16, 120)
        };
        if desired_timer_ms != active_timer_ms {
            active_timer_ms = desired_timer_ms;
            unsafe {
                let _ = SetTimer(Some(hwnd), TIMER_TICK_ID, active_timer_ms, None);
            }
        }

        if unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            if msg.message == WM_QUIT {
                break;
            }
            let _ = unsafe { TranslateMessage(&msg) };
            unsafe { DispatchMessageW(&msg) };
        } else {
            break;
        }
    }

    Ok(())
}
