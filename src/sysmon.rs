use std::sync::{Arc, mpsc};
use std::time::Duration;

use crate::battery::{BatteryInfo, read_battery};
use crate::platform::PlatformEvent;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemMetrics {
    pub cpu_pct: f32,
    pub ram_pct: f32,
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub disk_used_pct: Option<f32>,
    pub disk_activity_pct: Option<f32>,
    pub net_down_bps: u64,
    pub net_up_bps: u64,
    pub battery: Option<BatteryInfo>,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum SysmonCommand {
    SetConfig {
        enabled: bool,
        interval_ms: u64,
        collect_disk: bool,
        collect_net: bool,
    },
}

pub fn format_speed_bps(bps: u64) -> String {
    if bps >= 1_048_576 {
        format!("{:.1}M", bps as f64 / 1_048_576.0)
    } else if bps >= 1024 {
        format!("{}K", bps / 1024)
    } else {
        format!("{bps}B")
    }
}

#[allow(dead_code)]
pub fn compute_cpu_delta(
    prev: (u64, u64, u64),
    curr: (u64, u64, u64),
) -> f32 {
    let d_idle = curr.0.saturating_sub(prev.0);
    let d_kernel = curr.1.saturating_sub(prev.1);
    let d_user = curr.2.saturating_sub(prev.2);
    let d_total = d_kernel.saturating_add(d_user);
    if d_total == 0 {
        return 0.0;
    }
    let busy = d_total.saturating_sub(d_idle);
    ((busy as f64 / d_total as f64) * 100.0).clamp(0.0, 100.0) as f32
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
        PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
    };
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows::Win32::System::Threading::GetSystemTimes;
    use windows::core::w;

    use super::{SystemMetrics, compute_cpu_delta};
    use crate::battery::read_battery;

    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

    fn ft_to_u64(ft: FILETIME) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
    }

    fn read_cpu_times() -> Option<(u64, u64, u64)> {
        let mut idle = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
        Some((ft_to_u64(idle), ft_to_u64(kernel), ft_to_u64(user)))
    }

    fn read_ram() -> (f32, f32, f32) {
        let mut mem = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if unsafe { GlobalMemoryStatusEx(&mut mem) }.is_err() {
            return (0.0, 0.0, 0.0);
        }
        const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
        let total = mem.ullTotalPhys as f64 / GIB;
        let avail = mem.ullAvailPhys as f64 / GIB;
        let used = (total - avail).max(0.0);
        (mem.dwMemoryLoad as f32, used as f32, total as f32)
    }

    fn read_system_drive_used_pct() -> Option<f32> {
        let mut free_avail = 0u64;
        let mut total_bytes = 0u64;
        let mut total_free = 0u64;
        unsafe {
            GetDiskFreeSpaceExW(
                w!("C:\\"),
                Some(&mut free_avail),
                Some(&mut total_bytes),
                Some(&mut total_free),
            )
        }
        .ok()?;
        if total_bytes == 0 {
            return None;
        }
        let used = total_bytes.saturating_sub(total_free);
        Some(((used as f64 / total_bytes as f64) * 100.0).clamp(0.0, 100.0) as f32)
    }

    fn read_net_totals() -> Option<(u64, u64)> {
        let mut table_ptr: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if unsafe { GetIfTable2(&mut table_ptr) }.is_err() || table_ptr.is_null() {
            return None;
        }
        let mut rx = 0u64;
        let mut tx = 0u64;
        unsafe {
            let num = (*table_ptr).NumEntries as usize;
            let rows: &[MIB_IF_ROW2] = std::slice::from_raw_parts((*table_ptr).Table.as_ptr(), num);
            for row in rows {
                if row.Type == IF_TYPE_SOFTWARE_LOOPBACK || row.OperStatus != IfOperStatusUp {
                    continue;
                }
                rx = rx.saturating_add(row.InOctets);
                tx = tx.saturating_add(row.OutOctets);
            }
            FreeMibTable(table_ptr as *const c_void);
        }
        Some((rx, tx))
    }

    struct PdhDisk {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
        primed: bool,
    }

    impl PdhDisk {
        fn new() -> Option<Self> {
            let mut query = PDH_HQUERY::default();
            if unsafe { PdhOpenQueryW(None, 0, &mut query) } != 0 {
                return None;
            }
            let mut counter = PDH_HCOUNTER::default();
            let status = unsafe {
                PdhAddEnglishCounterW(
                    query,
                    w!("\\PhysicalDisk(_Total)\\% Disk Time"),
                    0,
                    &mut counter,
                )
            };
            if status != 0 {
                unsafe { PdhCloseQuery(query) };
                return None;
            }
            let _ = unsafe { PdhCollectQueryData(query) };
            Some(Self {
                query,
                counter,
                primed: true,
            })
        }

        fn sample(&mut self) -> Option<f32> {
            if unsafe { PdhCollectQueryData(self.query) } != 0 {
                return None;
            }
            if !self.primed {
                self.primed = true;
                return None;
            }
            let mut val = PDH_FMT_COUNTERVALUE::default();
            if unsafe { PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, None, &mut val) }
                != 0
            {
                return None;
            }
            let pct = unsafe { val.Anonymous.doubleValue };
            if pct.is_finite() {
                Some((pct as f32).clamp(0.0, 100.0))
            } else {
                None
            }
        }
    }

    impl Drop for PdhDisk {
        fn drop(&mut self) {
            if !self.query.is_invalid() {
                unsafe { PdhCloseQuery(self.query) };
            }
        }
    }

    pub struct Sampler {
        prev_cpu: Option<(u64, u64, u64)>,
        prev_net: Option<(u64, u64, Instant)>,
        last_disk_cap: Option<(f32, Instant)>,
        pdh_disk: Option<PdhDisk>,
    }

    impl Sampler {
        pub fn new() -> Self {
            Self {
                prev_cpu: read_cpu_times(),
                prev_net: None,
                last_disk_cap: None,
                pdh_disk: None,
            }
        }

        pub fn sample(&mut self, collect_disk: bool, collect_net: bool) -> SystemMetrics {
            let cpu_pct = if let Some(curr) = read_cpu_times() {
                let pct = self
                    .prev_cpu
                    .map(|prev| compute_cpu_delta(prev, curr))
                    .unwrap_or(0.0);
                self.prev_cpu = Some(curr);
                pct
            } else {
                0.0
            };

            let (ram_pct, ram_used_gb, ram_total_gb) = read_ram();

            let (disk_used_pct, disk_activity_pct) = if collect_disk {
                let now = Instant::now();
                let cap = match self.last_disk_cap {
                    Some((v, at)) if now.duration_since(at) < Duration::from_secs(10) => Some(v),
                    _ => {
                        let v = read_system_drive_used_pct();
                        if let Some(pct) = v {
                            self.last_disk_cap = Some((pct, now));
                        }
                        v
                    }
                };
                if self.pdh_disk.is_none() {
                    self.pdh_disk = PdhDisk::new();
                }
                let act = self.pdh_disk.as_mut().and_then(|p| p.sample());
                (cap, act)
            } else {
                (None, None)
            };

            let (net_down_bps, net_up_bps) = if collect_net {
                let now = Instant::now();
                if let Some((rx, tx)) = read_net_totals() {
                    let speeds = if let Some((prx, ptx, pat)) = self.prev_net {
                        let dt = now.duration_since(pat).as_secs_f64().max(0.05);
                        let down = (rx.saturating_sub(prx) as f64 / dt) as u64;
                        let up = (tx.saturating_sub(ptx) as f64 / dt) as u64;
                        (down, up)
                    } else {
                        (0, 0)
                    };
                    self.prev_net = Some((rx, tx, now));
                    speeds
                } else {
                    (0, 0)
                }
            } else {
                self.prev_net = None;
                (0, 0)
            };

            SystemMetrics {
                cpu_pct,
                ram_pct,
                ram_used_gb,
                ram_total_gb,
                disk_used_pct,
                disk_activity_pct,
                net_down_bps,
                net_up_bps,
                battery: read_battery(),
            }
        }
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
mod imp {
    use super::SystemMetrics;
    use crate::battery::read_battery;

    pub struct Sampler;
    impl Sampler {
        pub fn new() -> Self {
            Self
        }
        pub fn sample(&mut self, _collect_disk: bool, _collect_net: bool) -> SystemMetrics {
            SystemMetrics {
                battery: read_battery(),
                ..Default::default()
            }
        }
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn spawn_sysmon_worker(
    cmd_rx: mpsc::Receiver<SysmonCommand>,
    initial_enabled: bool,
    initial_interval_ms: u64,
    initial_disk: bool,
    initial_net: bool,
    event_tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("sysmon-worker".to_owned())
        .spawn(move || {
            let mut enabled = initial_enabled;
            let mut interval_ms = initial_interval_ms.clamp(500, 5000);
            let mut collect_disk = initial_disk;
            let mut collect_net = initial_net;
            let mut sampler = imp::Sampler::new();

            loop {
                if enabled {
                    let metrics = sampler.sample(collect_disk, collect_net);
                    if event_tx.send(PlatformEvent::SystemMetrics(metrics)).is_err() {
                        break;
                    }
                    on_wake();
                } else {
                    // Still poll battery infrequently when taskbar monitor is off so low-battery alerts work.
                    let metrics = SystemMetrics {
                        battery: read_battery(),
                        ..Default::default()
                    };
                    if event_tx.send(PlatformEvent::SystemMetrics(metrics)).is_err() {
                        break;
                    }
                    on_wake();
                }

                let wait = if enabled {
                    Duration::from_millis(interval_ms)
                } else {
                    Duration::from_secs(10)
                };

                match cmd_rx.recv_timeout(wait) {
                    Ok(SysmonCommand::SetConfig {
                        enabled: e,
                        interval_ms: iv,
                        collect_disk: d,
                        collect_net: n,
                    }) => {
                        enabled = e;
                        interval_ms = iv.clamp(500, 5000);
                        collect_disk = d;
                        collect_net = n;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("spawn sysmon worker thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_delta_computes_percentage() {
        assert_eq!(compute_cpu_delta((100, 200, 100), (150, 250, 150)), 50.0);
        assert_eq!(compute_cpu_delta((100, 100, 100), (100, 100, 100)), 0.0);
    }

    #[test]
    fn format_speed_scales_units() {
        assert_eq!(format_speed_bps(512), "512B");
        assert_eq!(format_speed_bps(15 * 1024), "15K");
        assert_eq!(format_speed_bps(2_621_440), "2.5M");
    }
}
