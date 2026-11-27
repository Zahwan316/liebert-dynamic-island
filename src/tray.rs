use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::state::ListenerStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    OpenSettings,
    ToggleDockMode,
    TestNotification,
    TogglePause,
    ToggleAutoHide,
    Exit,
}

pub struct TrayController {
    _tray: TrayIcon,
    status_item: MenuItem,
    settings_item: MenuItem,
    dock_mode_item: MenuItem,
    test_notif_item: MenuItem,
    pause_item: MenuItem,
    auto_hide_item: CheckMenuItem,
    exit_item: MenuItem,
}

impl TrayController {
    pub fn new(auto_hide: bool, notch_dock_mode: bool) -> Result<Self, String> {
        let menu = Menu::new();
        let status_item = MenuItem::new("Status: Starting...", false, None);
        let settings_item = MenuItem::new("Settings...", true, None);
        let dock_mode_item = MenuItem::new(dock_mode_label(notch_dock_mode), true, None);
        let test_notif_item = MenuItem::new("Test Notification Popup", true, None);
        let pause_item = MenuItem::new("Pause Notifications", true, None);
        let auto_hide_item = CheckMenuItem::new("Auto-hide when idle", true, auto_hide, None);
        let exit_item = MenuItem::new("Exit", true, None);

        menu.append_items(&[
            &status_item,
            &PredefinedMenuItem::separator(),
            &settings_item,
            &dock_mode_item,
            &test_notif_item,
            &pause_item,
            &auto_hide_item,
            &PredefinedMenuItem::separator(),
            &exit_item,
        ])
        .map_err(|e| format!("Failed to build tray menu: {e}"))?;

        let icon = build_pill_icon()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_tooltip("Liebert Dynamic Island")
            .build()
            .map_err(|e| format!("Failed to create tray icon: {e}"))?;

        Ok(Self {
            _tray: tray,
            status_item,
            settings_item,
            dock_mode_item,
            test_notif_item,
            pause_item,
            auto_hide_item,
            exit_item,
        })
    }

    pub fn poll_actions(&self) -> Vec<TrayAction> {
        let mut actions = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == *self.settings_item.id() {
                actions.push(TrayAction::OpenSettings);
            } else if event.id == *self.dock_mode_item.id() {
                actions.push(TrayAction::ToggleDockMode);
            } else if event.id == *self.test_notif_item.id() {
                actions.push(TrayAction::TestNotification);
            } else if event.id == *self.pause_item.id() {
                actions.push(TrayAction::TogglePause);
            } else if event.id == *self.auto_hide_item.id() {
                actions.push(TrayAction::ToggleAutoHide);
            } else if event.id == *self.exit_item.id() {
                actions.push(TrayAction::Exit);
            }
        }
        actions
    }

    pub fn is_auto_hide_checked(&self) -> bool {
        self.auto_hide_item.is_checked()
    }

    pub fn set_auto_hide_checked(&self, checked: bool) {
        self.auto_hide_item.set_checked(checked);
    }

    pub fn update_dock_mode(&self, notch_dock_mode: bool) {
        self.dock_mode_item
            .set_text(dock_mode_label(notch_dock_mode));
    }

    pub fn update_status(&self, status: &ListenerStatus, paused: bool) {
        let label = match status {
            ListenerStatus::Starting => "Status: Starting...",
            ListenerStatus::Listening => "Status: Listening",
            ListenerStatus::Paused => "Status: Paused",
            ListenerStatus::AccessDenied(_) => "Status: Permission Needed",
            ListenerStatus::Error(_) => "Status: Listener Error",
        };
        self.status_item.set_text(label);
        self.pause_item.set_text(if paused {
            "Resume Notifications"
        } else {
            "Pause Notifications"
        });
    }
}

fn dock_mode_label(notch_dock_mode: bool) -> &'static str {
    if notch_dock_mode {
        "Switch to Floating Island"
    } else {
        "Switch to Top Notch"
    }
}

fn build_pill_icon() -> Result<Icon, String> {
    const SIZE: u32 = 16;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = ((y * SIZE + x) * 4) as usize;
            let in_pill = (4..=11).contains(&y) && (1..=14).contains(&x);
            if in_pill {
                rgba[idx] = 24;
                rgba[idx + 1] = 24;
                rgba[idx + 2] = 28;
                rgba[idx + 3] = 255;
            }
            if (6..=9).contains(&y) && (4..=7).contains(&x) {
                rgba[idx] = 96;
                rgba[idx + 1] = 195;
                rgba[idx + 2] = 255;
                rgba[idx + 3] = 255;
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| format!("Invalid tray icon RGBA: {e}"))
}
