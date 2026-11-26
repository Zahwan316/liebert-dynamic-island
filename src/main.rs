#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod calendar;
mod clock;
mod config;
mod lyrics;
mod platform;
mod pomodoro;
mod state;
mod todo;
mod tray;
mod ui;

use std::process::ExitCode;
use std::sync::{Arc, mpsc};

use eframe::egui;

use crate::config::Config;
use crate::tray::TrayController;
use crate::ui::{AppChannels, IslandApp, WINDOW_TITLE};

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

    #[cfg(target_os = "linux")]
    if args.iter().any(|a| a == "--settings") {
        let ipc_sock = args
            .windows(2)
            .find(|w| w[0] == "--ipc-sock")
            .map(|w| std::path::PathBuf::from(&w[1]));
        return match ui::run_standalone_settings(ipc_sock) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Settings UI error: {err}");
                ExitCode::FAILURE
            }
        };
    }

    let demo_mode = args.iter().any(|a| a == "--demo");
    let config = Config::load();
    let tray = match TrayController::new(config.auto_hide_when_idle) {
        Ok(t) => Some(t),
        Err(err) => {
            eprintln!("Warning: tray unavailable ({err})");
            None
        }
    };

    let canvas = ui::permanent_canvas_size(&config);
    let initial_x = ((platform::primary_monitor_logical_width(1.0) - canvas.x) * 0.5).max(0.0);
    let initial_w = canvas.x;
    let initial_h = canvas.y;
    let initial_y = if config.notch_dock_mode {
        0.0
    } else {
        config.top_offset_px
    };

    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        match platform::wayland_layer::try_run_layer_shell(
            config.clone(),
            tray,
            demo_mode,
            initial_w,
            initial_h,
            initial_y,
        ) {
            Ok(()) => return ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Warning: Wayland wlr-layer-shell unavailable ({err}), falling back to eframe");
                let fallback_tray = TrayController::new(config.auto_hide_when_idle).ok();
                return run_eframe_overlay(
                    config,
                    fallback_tray,
                    demo_mode,
                    initial_w,
                    initial_h,
                    initial_x,
                    initial_y,
                );
            }
        }
    }

    run_eframe_overlay(
        config,
        tray,
        demo_mode,
        initial_w,
        initial_h,
        initial_x,
        initial_y,
    )
}

fn run_eframe_overlay(
    config: Config,
    tray: Option<TrayController>,
    demo_mode: bool,
    initial_w: f32,
    initial_h: f32,
    initial_x: f32,
    initial_y: f32,
) -> ExitCode {
    let (event_tx, event_rx) = mpsc::channel();
    let (lyrics_req_tx, lyrics_req_rx) = mpsc::channel();
    let (media_cmd_tx, media_cmd_rx) = mpsc::channel();
    let (calendar_cmd_tx, calendar_cmd_rx) = mpsc::channel();

    platform::prepare_overlay_rules(WINDOW_TITLE, initial_y);

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
            .with_inner_size([initial_w, initial_h])
            .with_position([initial_x, 0.0]),
        ..Default::default()
    };

    let run = eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            ui::apply_configured_fonts(&cc.egui_ctx, &config.font_name);
            let egui_ctx = cc.egui_ctx.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || egui_ctx.request_repaint());

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

            Ok(Box::new(IslandApp::new(
                config,
                AppChannels {
                    event_rx,
                    lyrics_req_tx,
                    media_cmd_tx,
                    calendar_cmd_tx,
                },
                tray,
                demo_mode,
            )))
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
