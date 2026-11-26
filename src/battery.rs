#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum BatteryStatus {
    Charging,
    Discharging,
    Full,
    PluggedNotCharging,
}

impl BatteryStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Charging => "Charging",
            Self::Discharging => "Discharging",
            Self::Full => "Fully Charged",
            Self::PluggedNotCharging => "Plugged In (Not Charging)",
        }
    }

    pub fn short_symbol(self) -> &'static str {
        match self {
            Self::Charging => "⚡",
            Self::Discharging => "🔋",
            Self::Full => "🔌",
            Self::PluggedNotCharging => "🔌",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryInfo {
    pub percent: u8,
    pub status: BatteryStatus,
    pub remaining_secs: Option<u32>,
}

/// Reads laptop battery status via Win32 `GetSystemPowerStatus`.
/// Returns `None` on desktops without a system battery or on non-Windows targets.
pub fn read_battery() -> Option<BatteryInfo> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        let mut sps = SYSTEM_POWER_STATUS::default();
        if unsafe { GetSystemPowerStatus(&mut sps) }.is_err() {
            return None;
        }
        // BatteryFlag 128 = No system battery; BatteryLifePercent 255 = Unknown.
        if (sps.BatteryFlag & 128) != 0 || sps.BatteryLifePercent == 255 {
            return None;
        }
        let percent = sps.BatteryLifePercent.min(100);
        let status = if (sps.BatteryFlag & 8) != 0 {
            BatteryStatus::Charging
        } else if sps.ACLineStatus == 1 && percent >= 99 {
            BatteryStatus::Full
        } else if sps.ACLineStatus == 1 {
            BatteryStatus::PluggedNotCharging
        } else {
            BatteryStatus::Discharging
        };
        let remaining_secs = if status == BatteryStatus::Discharging && sps.BatteryLifeTime != u32::MAX
        {
            Some(sps.BatteryLifeTime)
        } else {
            None
        };
        Some(BatteryInfo {
            percent,
            status,
            remaining_secs,
        })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

pub fn format_remaining(secs: u32) -> String {
    let mins = secs / 60;
    let h = mins / 60;
    let m = mins % 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// Tracks low-battery notification state with hysteresis so alerts fire once per discharge cycle.
#[derive(Debug, Default)]
pub struct BatteryAlertTracker {
    warned: bool,
    critical_warned: bool,
}

impl BatteryAlertTracker {
    pub fn check(
        &mut self,
        info: Option<BatteryInfo>,
        enabled: bool,
        warn_pct: u8,
        crit_pct: u8,
    ) -> Option<(&'static str, String)> {
        let bat = info?;
        if bat.status != BatteryStatus::Discharging || bat.percent > warn_pct.saturating_add(2) {
            self.warned = false;
            self.critical_warned = false;
            return None;
        }
        if !enabled {
            return None;
        }
        if bat.percent <= crit_pct && !self.critical_warned {
            self.critical_warned = true;
            self.warned = true;
            return Some((
                "Critical Battery",
                format!("Battery at {}% — plug in charger immediately.", bat.percent),
            ));
        }
        if bat.percent <= warn_pct && !self.warned {
            self.warned = true;
            return Some((
                "Low Battery",
                format!("Battery at {}% — consider plugging in.", bat.percent),
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_remaining_formats_hours_and_minutes() {
        assert_eq!(format_remaining(45 * 60), "45m");
        assert_eq!(format_remaining(135 * 60), "2h 15m");
    }

    #[test]
    fn alert_tracker_fires_once_and_resets_on_charge() {
        let mut tracker = BatteryAlertTracker::default();
        let bat25 = Some(BatteryInfo {
            percent: 25,
            status: BatteryStatus::Discharging,
            remaining_secs: None,
        });
        assert!(tracker.check(bat25, true, 20, 10).is_none());

        let bat19 = Some(BatteryInfo {
            percent: 19,
            status: BatteryStatus::Discharging,
            remaining_secs: None,
        });
        assert_eq!(tracker.check(bat19, true, 20, 10).unwrap().0, "Low Battery");
        assert!(tracker.check(bat19, true, 20, 10).is_none());

        let bat9 = Some(BatteryInfo {
            percent: 9,
            status: BatteryStatus::Discharging,
            remaining_secs: None,
        });
        assert_eq!(
            tracker.check(bat9, true, 20, 10).unwrap().0,
            "Critical Battery"
        );
        assert!(tracker.check(bat9, true, 20, 10).is_none());

        let charging = Some(BatteryInfo {
            percent: 9,
            status: BatteryStatus::Charging,
            remaining_secs: None,
        });
        assert!(tracker.check(charging, true, 20, 10).is_none());
        assert_eq!(
            tracker.check(bat19, true, 20, 10).unwrap().0,
            "Low Battery"
        );
    }
}
