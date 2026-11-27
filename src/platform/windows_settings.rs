//! Native Win32 pure-CPU Settings window rendered via top-down GDI DIB (`BitBlt`) + [`SoftCanvas`].
//!
//! Replaces `eframe`/`wgpu`/`glow` for `--settings` on Windows so OpenGL/ANGLE/GPU shader
//! compiler DLLs are not loaded into the Settings process, minimizing RAM usage.

use crate::ui::soft_render::SoftCanvas;
use crate::ui::{SETTINGS_WINDOW_TITLE, StandaloneSettingsApp};
use eframe::egui::{
    self, CursorIcon, Event, Key, Modifiers, MouseWheelSource, MouseWheelUnit, PointerButton, Pos2,
    RawInput, Rect, TouchPhase, Vec2, ViewportId, ViewportInfo,
};
use std::ffi::c_void;
use std::ptr::{copy_nonoverlapping, null_mut};
use std::time::Instant;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, EndPaint, GetDC, GetDeviceCaps, HBITMAP, HDC, HGDIOBJ,
    InvalidateRect, LOGPIXELSX, PAINTSTRUCT, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_BACK,
    VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_INSERT, VK_LEFT, VK_MENU,
    VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, IDC_ARROW, IDC_HAND, IDC_IBEAM, KillTimer,
    LoadCursorW, MINMAXINFO, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW,
    PostQuitMessage, QS_ALLINPUT, RegisterClassExW, SW_SHOW, SetCursor, SetTimer,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_TIMER, WNDCLASSEXW, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::PCWSTR;

const INITIAL_WIDTH_PT: i32 = 900;
const INITIAL_HEIGHT_PT: i32 = 620;
const MIN_WIDTH_PT: i32 = 540;
const MIN_HEIGHT_PT: i32 = 440;
const MAX_DIB_TEXTURE_SIDE: usize = 1024;
const REPAINT_TIMER_ID: usize = 1;
const WM_MOUSELEAVE_MSG: u32 = 0x02A3;
const CF_UNICODETEXT_FMT: u32 = 13;

/// Runs the standalone Settings window on Windows using CPU rasterization into a GDI DIB.
pub fn run_windows_settings(app: StandaloneSettingsApp) -> Result<(), String> {
    let class_name = to_wide("LiebertDynamicIslandSettingsClass");
    let title_wide = to_wide(SETTINGS_WINDOW_TITLE);

    let dpi_scale = unsafe {
        let dc = GetDC(None);
        let dpi = if dc.0.is_null() {
            96
        } else {
            let v = GetDeviceCaps(Some(dc), LOGPIXELSX);
            let _ = ReleaseDC(None, dc);
            v
        };
        (dpi as f32 / 96.0).max(1.0)
    };

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(settings_wnd_proc),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        ..Default::default()
    };

    if unsafe { RegisterClassExW(&wc) } == 0 {
        return Err("RegisterClassExW failed for Settings window".to_owned());
    }

    let egui_ctx = egui::Context::default();
    crate::ui::apply_configured_fonts_light(&egui_ctx, &app.config.font_name);

    let init_w = (INITIAL_WIDTH_PT as f32 * dpi_scale).round() as i32;
    let init_h = (INITIAL_HEIGHT_PT as f32 * dpi_scale).round() as i32;

    let boxed_state = Box::new(WinSettingsState {
        hwnd: HWND::default(),
        soft: SoftCanvas::new(),
        egui_ctx,
        app,
        dib: None,
        dpi_scale,
        width_px: init_w.max(1) as u32,
        height_px: init_h.max(1) as u32,
        tracking_leave: false,
        needs_paint: true,
        closing: false,
        cursor_icon: CursorIcon::Default,
        egui_events: Vec::with_capacity(16),
        start_time: Instant::now(),
        next_repaint: None,
    });
    let state_ptr = Box::into_raw(boxed_state);

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            init_w,
            init_h,
            None,
            None,
            None,
            Some(state_ptr as *const c_void),
        )
    }
    .map_err(|e| {
        unsafe { drop(Box::from_raw(state_ptr)) };
        format!("CreateWindowExW failed: {e}")
    })?;

    unsafe {
        let state = &mut *state_ptr;
        state.hwnd = hwnd;
        let mut rc = RECT::default();
        if GetClientRect(hwnd, &mut rc).is_ok() {
            state.width_px = (rc.right - rc.left).max(1) as u32;
            state.height_px = (rc.bottom - rc.top).max(1) as u32;
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        render_and_present(state);
    }

    let mut msg = MSG::default();
    loop {
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                    drop(Box::from_raw(state_ptr));
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let state = &mut *state_ptr;
            if state.closing {
                continue;
            }
            if state.needs_paint {
                render_and_present(state);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }

            let wait_ms = if let Some(deadline) = state.next_repaint {
                let rem = deadline.saturating_duration_since(Instant::now());
                (rem.as_millis() as u32).clamp(1, 250)
            } else {
                250
            };
            let _ = MsgWaitForMultipleObjects(None, false, wait_ms, QS_ALLINPUT);

            if let Some(deadline) = state.next_repaint
                && Instant::now() >= deadline
            {
                state.needs_paint = true;
            }
        }
    }
}

struct DibSurface {
    mem_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
}

impl DibSurface {
    unsafe fn ensure(&mut self, screen_dc: HDC, width: u32, height: u32) -> bool {
        if self.width == width && self.height == height && !self.bits.is_null() {
            return true;
        }
        unsafe {
            let _ = SelectObject(self.mem_dc, self.old_bitmap);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
        }
        if let Some(next) = unsafe { create_dib(screen_dc, self.mem_dc, width, height) } {
            *self = next;
            true
        } else {
            self.bits = null_mut();
            false
        }
    }
}

impl Drop for DibSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.old_bitmap.0.is_null() {
                let _ = SelectObject(self.mem_dc, self.old_bitmap);
            }
            if !self.bitmap.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            }
            if !self.mem_dc.0.is_null() {
                let _ = DeleteDC(self.mem_dc);
            }
        }
    }
}

unsafe fn create_dib(
    screen_dc: HDC,
    existing_dc: HDC,
    width: u32,
    height: u32,
) -> Option<DibSurface> {
    let mem_dc = if existing_dc.0.is_null() {
        unsafe { CreateCompatibleDC(Some(screen_dc)) }
    } else {
        existing_dc
    };
    if mem_dc.0.is_null() {
        return None;
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // Top-down BGRA matches SoftCanvas bgr_order=true
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut c_void = null_mut();
    let bitmap = match unsafe {
        CreateDIBSection(Some(screen_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
    } {
        Ok(bmp) => bmp,
        Err(_) => {
            if existing_dc.0.is_null() {
                unsafe {
                    let _ = DeleteDC(mem_dc);
                }
            }
            return None;
        }
    };

    let old_bitmap = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };
    Some(DibSurface {
        mem_dc,
        bitmap,
        old_bitmap,
        bits: bits.cast::<u8>(),
        width,
        height,
    })
}

struct WinSettingsState {
    hwnd: HWND,
    soft: SoftCanvas,
    egui_ctx: egui::Context,
    app: StandaloneSettingsApp,
    dib: Option<DibSurface>,
    dpi_scale: f32,
    width_px: u32,
    height_px: u32,
    tracking_leave: bool,
    needs_paint: bool,
    closing: bool,
    cursor_icon: CursorIcon,
    egui_events: Vec<Event>,
    start_time: Instant,
    next_repaint: Option<Instant>,
}

unsafe fn render_and_present(state: &mut WinSettingsState) {
    let width_px = state.width_px.max(1);
    let height_px = state.height_px.max(1);
    let scale = state.dpi_scale.max(1.0);
    let width_pt = width_px as f32 / scale;
    let height_pt = height_px as f32 / scale;

    let mut viewports = egui::ViewportIdMap::default();
    viewports.insert(
        ViewportId::ROOT,
        ViewportInfo {
            native_pixels_per_point: Some(scale),
            ..Default::default()
        },
    );

    let raw_input = RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(width_pt, height_pt),
        )),
        time: Some(state.start_time.elapsed().as_secs_f64()),
        events: std::mem::take(&mut state.egui_events),
        viewports,
        max_texture_side: Some(MAX_DIB_TEXTURE_SIDE),
        ..Default::default()
    };

    let app = &mut state.app;
    let full_output = state.egui_ctx.run_ui(raw_input, |ui| {
        app.render_ui(ui);
    });

    state.cursor_icon = full_output.platform_output.cursor_icon;
    for cmd in &full_output.platform_output.commands {
        if let egui::OutputCommand::CopyText(text) = cmd {
            unsafe { set_win32_clipboard(state.hwnd, text) };
        }
    }

    let mut textures_delta = full_output.textures_delta;
    state.soft.update_textures_before_paint(&mut textures_delta);
    let primitives = state.egui_ctx.tessellate(full_output.shapes, scale);

    let screen_dc = unsafe { windows::Win32::Graphics::Gdi::GetDC(Some(state.hwnd)) };
    if !screen_dc.0.is_null() {
        let ready = match state.dib.as_mut() {
            Some(dib) => unsafe { dib.ensure(screen_dc, width_px, height_px) },
            None => {
                if let Some(created) =
                    unsafe { create_dib(screen_dc, HDC::default(), width_px, height_px) }
                {
                    state.dib = Some(created);
                    true
                } else {
                    false
                }
            }
        };
        if ready
            && let Some(dib) = &state.dib
            && !dib.bits.is_null()
        {
            let len = (width_px as usize) * (height_px as usize) * 4;
            let out = unsafe { std::slice::from_raw_parts_mut(dib.bits, len) };
            state
                .soft
                .rasterize(width_px, height_px, scale, &primitives, out, true);
            let _ = unsafe {
                BitBlt(
                    screen_dc,
                    0,
                    0,
                    width_px as i32,
                    height_px as i32,
                    Some(dib.mem_dc),
                    0,
                    0,
                    SRCCOPY,
                )
            };
        }
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ReleaseDC(Some(state.hwnd), screen_dc);
        }
    }

    state.soft.free_textures_after_paint(textures_delta);
    state.needs_paint = false;

    if let Some(vp) = full_output.viewport_output.get(&ViewportId::ROOT) {
        let delay = vp.repaint_delay;
        if delay.is_zero() {
            state.next_repaint = Some(Instant::now() + std::time::Duration::from_millis(8));
            unsafe {
                SetTimer(Some(state.hwnd), REPAINT_TIMER_ID, 8, None);
            }
        } else if delay < std::time::Duration::from_secs(30) {
            state.next_repaint = Some(Instant::now() + delay);
            let ms = (delay.as_millis() as u32).clamp(10, 5000);
            unsafe {
                SetTimer(Some(state.hwnd), REPAINT_TIMER_ID, ms, None);
            }
        } else {
            state.next_repaint = None;
            unsafe {
                let _ = KillTimer(Some(state.hwnd), REPAINT_TIMER_ID);
            }
        }
    }
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        if !cs.lpCreateParams.is_null() {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            }
        }
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WinSettingsState;
    if ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    let state = unsafe { &mut *ptr };

    match msg {
        WM_GETMINMAXINFO => {
            let mmi = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            let scale = state.dpi_scale.max(1.0);
            mmi.ptMinTrackSize.x = (MIN_WIDTH_PT as f32 * scale).round() as i32;
            mmi.ptMinTrackSize.y = (MIN_HEIGHT_PT as f32 * scale).round() as i32;
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lparam.0 & 0xFFFF) as u32;
            let h = ((lparam.0 >> 16) & 0xFFFF) as u32;
            if w > 0 && h > 0 {
                state.width_px = w;
                state.height_px = h;
                state.needs_paint = true;
                unsafe { render_and_present(state) };
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let new_dpi = (wparam.0 & 0xFFFF) as f32;
            if new_dpi > 0.0 {
                state.dpi_scale = (new_dpi / 96.0).max(1.0);
                state.needs_paint = true;
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            if state.needs_paint {
                unsafe { render_and_present(state) };
            }
            let mut ps = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            if let Some(dib) = &state.dib {
                let _ = unsafe {
                    BitBlt(
                        hdc,
                        0,
                        0,
                        state.width_px as i32,
                        state.height_px as i32,
                        Some(dib.mem_dc),
                        0,
                        0,
                        SRCCOPY,
                    )
                };
            }
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            state.needs_paint = true;
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let hit = (lparam.0 & 0xFFFF) as u32;
            if hit == 1 {
                let idc = match state.cursor_icon {
                    CursorIcon::PointingHand | CursorIcon::Grab | CursorIcon::Grabbing => IDC_HAND,
                    CursorIcon::Text | CursorIcon::VerticalText => IDC_IBEAM,
                    _ => IDC_ARROW,
                };
                if let Ok(cur) = unsafe { LoadCursorW(None, idc) } {
                    unsafe { SetCursor(Some(cur)) };
                    return LRESULT(1);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSEMOVE => {
            if !state.tracking_leave {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                if unsafe { TrackMouseEvent(&mut tme) }.is_ok() {
                    state.tracking_leave = true;
                }
            }
            let pos = lparam_pos(lparam, state.dpi_scale);
            state.egui_events.push(Event::PointerMoved(pos));
            state.needs_paint = true;
            LRESULT(0)
        }
        WM_MOUSELEAVE_MSG => {
            state.tracking_leave = false;
            state.egui_events.push(Event::PointerGone);
            state.needs_paint = true;
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN
        | WM_MBUTTONUP => {
            let (btn, pressed) = match msg {
                WM_LBUTTONDOWN => (PointerButton::Primary, true),
                WM_LBUTTONUP => (PointerButton::Primary, false),
                WM_RBUTTONDOWN => (PointerButton::Secondary, true),
                WM_RBUTTONUP => (PointerButton::Secondary, false),
                WM_MBUTTONDOWN => (PointerButton::Middle, true),
                _ => (PointerButton::Middle, false),
            };
            unsafe {
                if pressed {
                    SetCapture(hwnd);
                } else {
                    let _ = ReleaseCapture();
                }
            }
            let pos = lparam_pos(lparam, state.dpi_scale);
            state.egui_events.push(Event::PointerButton {
                pos,
                button: btn,
                pressed,
                modifiers: current_modifiers(),
            });
            state.needs_paint = true;
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as u16) as i16 as f32 / 120.0 * 32.0;
            state.egui_events.push(Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: Vec2::new(0.0, delta),
                phase: TouchPhase::Move,
                source: MouseWheelSource::Wheel,
                modifiers: current_modifiers(),
            });
            state.needs_paint = true;
            LRESULT(0)
        }
        WM_MOUSEHWHEEL => {
            let delta = -(((wparam.0 >> 16) as u16) as i16 as f32 / 120.0 * 32.0);
            state.egui_events.push(Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: Vec2::new(delta, 0.0),
                phase: TouchPhase::Move,
                source: MouseWheelSource::Wheel,
                modifiers: current_modifiers(),
            });
            state.needs_paint = true;
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
            let pressed = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let repeat = pressed && ((lparam.0 >> 30) & 1) == 1;
            let vk = wparam.0 as u16;
            let mods = current_modifiers();
            if pressed && mods.ctrl {
                if vk == b'C' as u16 {
                    state.egui_events.push(Event::Copy);
                    state.needs_paint = true;
                    return LRESULT(0);
                }
                if vk == b'X' as u16 {
                    state.egui_events.push(Event::Cut);
                    state.needs_paint = true;
                    return LRESULT(0);
                }
                if vk == b'V' as u16 {
                    if let Some(clip) = unsafe { get_win32_clipboard(hwnd) } {
                        state.egui_events.push(Event::Paste(clip));
                        state.needs_paint = true;
                    }
                    return LRESULT(0);
                }
            }
            if let Some(key) = map_vk(vk) {
                state.egui_events.push(Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat,
                    modifiers: mods,
                });
                state.needs_paint = true;
            }
            LRESULT(0)
        }
        WM_CHAR => {
            let ch_u32 = wparam.0 as u32;
            let mods = current_modifiers();
            if !mods.ctrl
                && !mods.alt
                && let Some(ch) = char::from_u32(ch_u32)
                && !ch.is_control()
            {
                state.egui_events.push(Event::Text(ch.to_string()));
                state.needs_paint = true;
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            state.closing = true;
            unsafe {
                let _ = KillTimer(Some(hwnd), REPAINT_TIMER_ID);
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn lparam_pos(lparam: LPARAM, dpi_scale: f32) -> Pos2 {
    let x = ((lparam.0 & 0xFFFF) as u16) as i16 as f32 / dpi_scale.max(1.0);
    let y = (((lparam.0 >> 16) & 0xFFFF) as u16) as i16 as f32 / dpi_scale.max(1.0);
    Pos2::new(x, y)
}

fn current_modifiers() -> Modifiers {
    let down = |vk: i32| -> bool { (unsafe { GetKeyState(vk) } as u16 & 0x8000) != 0 };
    let ctrl = down(VK_CONTROL.0 as i32);
    Modifiers {
        alt: down(VK_MENU.0 as i32),
        ctrl,
        shift: down(VK_SHIFT.0 as i32),
        mac_cmd: false,
        command: ctrl,
    }
}

fn map_vk(vk: u16) -> Option<Key> {
    match vk {
        x if x == VK_BACK.0 => Some(Key::Backspace),
        x if x == VK_TAB.0 => Some(Key::Tab),
        x if x == VK_RETURN.0 => Some(Key::Enter),
        x if x == VK_ESCAPE.0 => Some(Key::Escape),
        x if x == VK_DELETE.0 => Some(Key::Delete),
        x if x == VK_INSERT.0 => Some(Key::Insert),
        x if x == VK_HOME.0 => Some(Key::Home),
        x if x == VK_END.0 => Some(Key::End),
        x if x == VK_PRIOR.0 => Some(Key::PageUp),
        x if x == VK_NEXT.0 => Some(Key::PageDown),
        x if x == VK_LEFT.0 => Some(Key::ArrowLeft),
        x if x == VK_RIGHT.0 => Some(Key::ArrowRight),
        x if x == VK_UP.0 => Some(Key::ArrowUp),
        x if x == VK_DOWN.0 => Some(Key::ArrowDown),
        x if x == b'A' as u16 => Some(Key::A),
        x if x == b'C' as u16 => Some(Key::C),
        x if x == b'V' as u16 => Some(Key::V),
        x if x == b'X' as u16 => Some(Key::X),
        x if x == b'Y' as u16 => Some(Key::Y),
        x if x == b'Z' as u16 => Some(Key::Z),
        _ => None,
    }
}

unsafe fn set_win32_clipboard(hwnd: HWND, text: &str) {
    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    let bytes = utf16.len() * std::mem::size_of::<u16>();
    let Ok(hmem) = (unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }) else {
        return;
    };
    let locked = unsafe { GlobalLock(hmem) };
    if locked.is_null() {
        return;
    }
    unsafe {
        copy_nonoverlapping(utf16.as_ptr(), locked.cast::<u16>(), utf16.len());
        let _ = GlobalUnlock(hmem);
    }
    if unsafe { OpenClipboard(Some(hwnd)) }.is_err() {
        return;
    }
    let _ = unsafe { EmptyClipboard() };
    let _ = unsafe { SetClipboardData(CF_UNICODETEXT_FMT, Some(HANDLE(hmem.0))) };
    let _ = unsafe { CloseClipboard() };
}

unsafe fn get_win32_clipboard(hwnd: HWND) -> Option<String> {
    if unsafe { OpenClipboard(Some(hwnd)) }.is_err() {
        return None;
    }
    let result = (|| {
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT_FMT) }.ok()?;
        let hmem = HGLOBAL(handle.0);
        let ptr = unsafe { GlobalLock(hmem) }.cast::<u16>();
        if ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while unsafe { *ptr.add(len) } != 0 {
            len += 1;
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        let text = String::from_utf16_lossy(slice);
        let _ = unsafe { GlobalUnlock(hmem) };
        Some(text)
    })();
    let _ = unsafe { CloseClipboard() };
    result
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
