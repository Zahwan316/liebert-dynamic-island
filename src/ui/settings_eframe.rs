#![cfg(not(windows))]

use eframe::egui::{self, ViewportBuilder};

use crate::ui::icons::SETTINGS_WINDOW_TITLE;
use crate::ui::settings::{SettingsIpcTarget, StandaloneSettingsApp};

struct EframeSettingsApp(StandaloneSettingsApp);

impl eframe::App for EframeSettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.render_ui(ui);
    }
}

/// Minimal X11 standalone Settings host. Wayland and Windows use their own
/// native, GPU-free window backends, so `eframe` is only linked on this path.
pub fn run_x11_settings(ipc_target: Option<SettingsIpcTarget>) -> Result<(), String> {
    let app = StandaloneSettingsApp::new(ipc_target);
    let initial_font = app.config.font_name.clone();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: ViewportBuilder::default()
            .with_title(SETTINGS_WINDOW_TITLE)
            .with_app_id("liebert-dynamic-island-settings")
            .with_inner_size([760.0, 520.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        SETTINGS_WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            crate::ui::apply_configured_fonts_light(&cc.egui_ctx, &initial_font);
            Ok(Box::new(EframeSettingsApp(app)))
        }),
    )
    .map_err(|e| e.to_string())
}
