#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod battery;
mod calendar;
mod clock;
mod config;
mod daemon;
mod ipc;
mod lyrics;
mod platform;
mod pomodoro;
mod state;
mod sysmon;
mod todo;
mod tray;
mod ui;

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::process::ExitCode;
use std::sync::{Arc, mpsc};
use std::time::Duration;

#[cfg(not(windows))]
use eframe::egui;

use crate::calendar::CalendarCommand;
#[cfg(any(windows, target_os = "linux"))]
use crate::config::Config;
use crate::ipc::{DaemonSnapshot, DaemonToUi, UiToDaemon, read_msg, write_msg};
use crate::lyrics::{LyricsData, LyricsRequest};
use crate::platform::{MediaCommand, PlatformEvent};
use crate::ui::{AppChannels, SettingsIpcTarget, UiControlEvent};
#[cfg(not(windows))]
use crate::ui::{IslandApp, WINDOW_TITLE};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--spike-notifications") {
        match platform::run_notification_spike() {
            Ok(summary) => {
                println!("SPIKE OK: {summary}");
                return ExitCode::SUCCESS;
            }
            Err(err) => {
                eprintln!("SPIKE ERR: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    if args.iter().any(|a| a == "--settings") && !args.iter().any(|a| a == "--ui") {
        let ipc_target = args
            .windows(2)
            .find(|w| w[0] == "--ipc-addr")
            .map(|w| SettingsIpcTarget::Tcp(w[1].clone()))
            .or({
                #[cfg(unix)]
                {
                    args.windows(2)
                        .find(|w| w[0] == "--ipc-sock")
                        .map(|w| SettingsIpcTarget::Unix(std::path::PathBuf::from(&w[1])))
                }
                #[cfg(not(unix))]
                {
                    None
                }
            });
        return match ui::run_standalone_settings(ipc_target) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Settings UI error: {err}");
                ExitCode::FAILURE
            }
        };
    }

    let demo_mode = args.iter().any(|a| a == "--demo");

    if args.iter().any(|a| a == "--ui") {
        let ipc_addr = args
            .windows(2)
            .find(|w| w[0] == "--ipc-addr")
            .map(|w| w[1].clone());
        return match ipc_addr {
            Some(addr) => run_ui_client(&addr, demo_mode),
            None => {
                eprintln!("Missing --ipc-addr for --ui mode");
                ExitCode::FAILURE
            }
        };
    }

    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        let config = Config::load();
        let tray = crate::tray::TrayController::new(
            config.auto_hide_when_idle,
            config.notch_dock_mode,
        )
        .ok();
        let ear_w = if config.notch_dock_mode {
            (12.0 * config.scale).round() * 2.0
        } else {
            0.0
        };
        let initial_w = config.idle_width * config.scale + ear_w;
        let initial_h = 30.0 * config.scale;
        let initial_y = if config.notch_dock_mode {
            0.0
        } else {
            config.top_offset_px
        };
        if let Ok(()) = platform::wayland_layer::try_run_layer_shell(
            config,
            tray,
            demo_mode,
            initial_w,
            initial_h,
            initial_y,
        ) {
            return ExitCode::SUCCESS;
        }
    }

    #[cfg(windows)]
    {
        let config = Config::load();
        let tray = crate::tray::TrayController::new(
            config.auto_hide_when_idle,
            config.notch_dock_mode,
        )
        .ok();
        if let Ok(()) = platform::windows_overlay::try_run_layered_direct(config, tray, demo_mode) {
            return ExitCode::SUCCESS;
        }
    }

    daemon::run_daemon(demo_mode)
}

fn run_ui_client(ipc_addr: &str, demo_mode: bool) -> ExitCode {
    let stream = match TcpStream::connect(ipc_addr) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("Failed to connect to daemon at {ipc_addr}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

    let Ok(read_clone) = stream.try_clone() else {
        eprintln!("Failed to clone IPC stream");
        return ExitCode::FAILURE;
    };
    let mut reader = BufReader::new(read_clone);
    let mut first_line = String::new();
    if reader.read_line(&mut first_line).is_err() {
        eprintln!("Failed to read initial snapshot from daemon");
        return ExitCode::FAILURE;
    }
    let snapshot: DaemonSnapshot = match read_msg::<DaemonToUi>(&first_line) {
        Ok(DaemonToUi::Snapshot(snap)) => *snap,
        other => {
            eprintln!("Unexpected initial IPC message: {other:?}");
            return ExitCode::FAILURE;
        }
    };
    let _ = reader.get_ref().set_read_timeout(None);

    let config = snapshot.config.clone();

    let (event_tx, event_rx) = mpsc::channel::<PlatformEvent>();
    let (ui_ctrl_tx, ui_ctrl_rx) = mpsc::channel::<UiControlEvent>();
    let (lyrics_req_tx, lyrics_req_rx) = mpsc::channel::<LyricsRequest>();
    let (media_cmd_tx, media_cmd_rx) = mpsc::channel::<MediaCommand>();
    let (calendar_cmd_tx, calendar_cmd_rx) = mpsc::channel::<CalendarCommand>();
    let (daemon_tx, daemon_rx) = mpsc::channel::<UiToDaemon>();

    let reader_handle_and_writer = move |wake: Arc<dyn Fn() + Send + Sync>| {
        let wake_reader = wake;
        let _ = std::thread::Builder::new()
            .name("ui-ipc-reader".to_owned())
            .spawn(move || {
                for line in reader.lines().map_while(Result::ok) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let Ok(msg) = read_msg::<DaemonToUi>(&line) else {
                        continue;
                    };
                    match msg {
                        DaemonToUi::Snapshot(_) => {}
                        DaemonToUi::Notification(item) => {
                            let _ = event_tx.send(PlatformEvent::Notification(item));
                        }
                        DaemonToUi::Removed(id) => {
                            let _ = event_tx.send(PlatformEvent::Removed(id));
                        }
                        DaemonToUi::Status(status) => {
                            let _ = event_tx.send(PlatformEvent::Status(status));
                        }
                        DaemonToUi::Media(track) => {
                            let _ = event_tx.send(PlatformEvent::Media(
                                track.map(platform::MediaTrack::from),
                            ));
                        }
                        DaemonToUi::Fullscreen(ex, win) => {
                            let _ = event_tx.send(PlatformEvent::Fullscreen(ex, win));
                        }
                        DaemonToUi::LyricsLoaded {
                            artist,
                            title,
                            lyrics,
                        } => {
                            let _ = event_tx.send(PlatformEvent::LyricsLoaded {
                                artist,
                                title,
                                lyrics: LyricsData::from(lyrics),
                            });
                        }
                        DaemonToUi::CalendarUpdated(res) => {
                            let _ = event_tx.send(PlatformEvent::CalendarUpdated(res));
                        }
                        DaemonToUi::ConfigUpdated(cfg) => {
                            let _ = ui_ctrl_tx.send(UiControlEvent::ConfigUpdated(Box::new(cfg)));
                        }
                        DaemonToUi::SetPaused(paused) => {
                            let _ = ui_ctrl_tx.send(UiControlEvent::SetPaused(paused));
                        }
                        DaemonToUi::OpenSettings => {
                            let _ = ui_ctrl_tx.send(UiControlEvent::OpenSettings);
                        }
                        DaemonToUi::TestNotification => {
                            let _ = ui_ctrl_tx.send(UiControlEvent::TestNotification);
                        }
                        DaemonToUi::Shutdown => {
                            let _ = ui_ctrl_tx.send(UiControlEvent::Shutdown);
                            wake_reader();
                            break;
                        }
                    }
                    wake_reader();
                }
                let _ = ui_ctrl_tx.send(UiControlEvent::Shutdown);
                wake_reader();
            });

        let mut writer_stream = stream;
        let _ = std::thread::Builder::new()
            .name("ui-ipc-writer".to_owned())
            .spawn(move || {
                loop {
                    let mut sent_any = false;
                    while let Ok(req) = lyrics_req_rx.try_recv() {
                        if write_msg(&mut writer_stream, &UiToDaemon::LyricsRequest(req)).is_err() {
                            return;
                        }
                        sent_any = true;
                    }
                    while let Ok(cmd) = media_cmd_rx.try_recv() {
                        if write_msg(&mut writer_stream, &UiToDaemon::MediaCommand(cmd)).is_err() {
                            return;
                        }
                        sent_any = true;
                    }
                    while let Ok(cmd) = calendar_cmd_rx.try_recv() {
                        if write_msg(&mut writer_stream, &UiToDaemon::CalendarCommand(cmd)).is_err()
                        {
                            return;
                        }
                        sent_any = true;
                    }
                    while let Ok(msg) = daemon_rx.try_recv() {
                        if write_msg(&mut writer_stream, &msg).is_err() {
                            return;
                        }
                        sent_any = true;
                    }
                    if !sent_any {
                        std::thread::sleep(Duration::from_millis(15));
                    }
                }
            });
    };

    #[cfg(windows)]
    {
        let wake_hwnd = Arc::new(std::sync::atomic::AtomicIsize::new(0));
        let wake_ref = wake_hwnd.clone();
        let wake: Arc<dyn Fn() + Send + Sync> =
            Arc::new(move || platform::windows_overlay::post_wake_to_hwnd(&wake_ref));
        reader_handle_and_writer(wake);
        let channels = AppChannels {
            event_rx,
            lyrics_req_tx,
            media_cmd_tx,
            calendar_cmd_tx,
            ui_ctrl_rx: Some(ui_ctrl_rx),
            daemon_tx: Some(daemon_tx),
            sysmon_cmd_tx: None,
        };
        match platform::windows_overlay::run_layered_ipc_client(
            config,
            snapshot,
            channels,
            wake_hwnd,
            demo_mode,
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Fatal Windows layered UI error: {err}");
                ExitCode::FAILURE
            }
        }
    }

    #[cfg(not(windows))]
    {
        let initial_y = if config.notch_dock_mode {
            0.0
        } else {
            config.top_offset_px
        };
        platform::prepare_overlay_rules(WINDOW_TITLE, initial_y);

        let ear_w = if config.notch_dock_mode {
            (12.0 * config.scale).round() * 2.0
        } else {
            0.0
        };
        let win_w = config.idle_width * config.scale + ear_w;
        let win_h = 30.0 * config.scale;
        let win_x = ((platform::primary_monitor_logical_width(1.0) - win_w) * 0.5).max(0.0);

        let options = eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            viewport: egui::ViewportBuilder::default()
                .with_title(WINDOW_TITLE)
                .with_app_id("liebert-dynamic-island")
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_active(false)
                .with_resizable(platform::overlay_resizable())
                .with_minimize_button(false)
                .with_maximize_button(false)
                .with_close_button(false)
                .with_inner_size([win_w, win_h])
                .with_position([win_x, initial_y]),
            ..Default::default()
        };

        let run = eframe::run_native(
            WINDOW_TITLE,
            options,
            Box::new(move |cc| {
                ui::apply_configured_fonts(&cc.egui_ctx, &config.font_name);
                let egui_ctx = cc.egui_ctx.clone();
                let wake: Arc<dyn Fn() + Send + Sync> =
                    Arc::new(move || egui_ctx.request_repaint());
                reader_handle_and_writer(wake);

                let mut app = IslandApp::new(
                    config,
                    AppChannels {
                        event_rx,
                        lyrics_req_tx,
                        media_cmd_tx,
                        calendar_cmd_tx,
                        ui_ctrl_rx: Some(ui_ctrl_rx),
                        daemon_tx: Some(daemon_tx),
                        sysmon_cmd_tx: None,
                    },
                    None,
                    demo_mode,
                );
                app.apply_snapshot(snapshot, &cc.egui_ctx);
                Ok(Box::new(app))
            }),
        );

        match run {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Fatal UI error: {err}");
                ExitCode::FAILURE
            }
        }
    }
}
