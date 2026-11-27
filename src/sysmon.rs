use std::sync::{Arc, mpsc};
use std::time::Duration;

use crate::battery::BatteryInfo;
use crate::platform::PlatformEvent;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct GpuSample {
    pub temp_c: Option<f32>,
    pub util_pct: Option<f32>,
    pub vram_used_mb: Option<f32>,
    pub vram_total_mb: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemMetrics {
    pub cpu_pct: f32,
    pub cpu_temp_c: Option<f32>,
    pub gpu_pct: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub gpu_vram_used_mb: Option<f32>,
    pub gpu_vram_total_mb: Option<f32>,
    pub ram_pct: f32,
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub disk_used_pct: Option<f32>,
    pub disk_used_gb: Option<f32>,
    pub disk_total_gb: Option<f32>,
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
pub fn format_vram_mb(used_mb: f32, total_mb: Option<f32>) -> String {
    let used_clean = used_mb.max(0.0);
    if let Some(total) = total_mb
        && total > 0.0
    {
        if total >= 1024.0 {
            return format!("{:.1}/{:.1}G", used_clean / 1024.0, total / 1024.0);
        }
        return format!("{:.0}/{:.0}M", used_clean, total);
    }
    if used_clean >= 1024.0 {
        format!("{:.1}G", used_clean / 1024.0)
    } else {
        format!("{used_clean:.0}M")
    }
}

#[allow(dead_code)]
pub fn compute_cpu_delta(prev: (u64, u64, u64), curr: (u64, u64, u64)) -> f32 {
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

/// Parses `nvidia-smi --query-gpu=temperature.gpu,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits`.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
pub(crate) fn parse_nvidia_smi_csv(text: &str) -> Option<GpuSample> {
    let mut best: Option<(f32, GpuSample)> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols: Vec<&str> = trimmed.split(',').map(str::trim).collect();
        if cols.len() < 4 {
            continue;
        }
        let parse_opt = |s: &str| -> Option<f32> {
            let v = s.parse::<f32>().ok()?;
            v.is_finite().then_some(v)
        };
        let temp_c = parse_opt(cols[0]).filter(|v| (5.0..=125.0).contains(v));
        let util_pct = parse_opt(cols[1]).map(|v| v.clamp(0.0, 100.0));
        let vram_used_mb = parse_opt(cols[2]).filter(|v| *v >= 0.0);
        let vram_total_mb = parse_opt(cols[3]).filter(|v| *v > 0.0);
        if temp_c.is_none()
            && util_pct.is_none()
            && vram_used_mb.is_none()
            && vram_total_mb.is_none()
        {
            continue;
        }
        let score = util_pct.unwrap_or(0.0) + vram_used_mb.unwrap_or(0.0) * 0.001;
        let sample = GpuSample {
            temp_c,
            util_pct,
            vram_used_mb,
            vram_total_mb,
        };
        if best.is_none_or(|(b_score, _)| score > b_score) {
            best = Some((score, sample));
        }
    }
    best.map(|(_, sample)| sample)
}

#[cfg(windows)]
mod imp {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    use windows::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::Performance::{
        PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_FMT_LARGE,
        PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
        PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
    };
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows::Win32::System::Threading::GetSystemTimes;
    use windows::core::w;

    use super::{GpuSample, SystemMetrics, compute_cpu_delta, parse_nvidia_smi_csv};
    use crate::battery::read_battery;

    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
    const CREATE_NO_WINDOW_FLAG: u32 = 0x0800_0000;
    const PDH_MORE_DATA_ERR: u32 = 0x8000_07D2;
    const PDH_CSTATUS_VALID_MAX: u32 = 1;
    const DXGI_ADAPTER_FLAG_SOFTWARE_BIT: u32 = 2;

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

    fn read_system_drive_usage() -> Option<(f32, f32, f32)> {
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
        const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
        let used = total_bytes.saturating_sub(total_free);
        let pct = ((used as f64 / total_bytes as f64) * 100.0).clamp(0.0, 100.0) as f32;
        Some((
            pct,
            (used as f64 / GIB) as f32,
            (total_bytes as f64 / GIB) as f32,
        ))
    }

    fn read_dxgi_vram_total_mb() -> Option<f32> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
        let mut best_mb: Option<f32> = None;
        let mut idx = 0u32;
        while let Ok(adapter) = unsafe { factory.EnumAdapters1(idx) } {
            idx += 1;
            let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
                continue;
            };
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE_BIT) != 0 {
                continue;
            }
            let bytes = if desc.DedicatedVideoMemory > 0 {
                desc.DedicatedVideoMemory
            } else {
                desc.DedicatedSystemMemory
            };
            if bytes > 0 {
                let mb = (bytes as f64 / (1024.0 * 1024.0)) as f32;
                if best_mb.is_none_or(|b| mb > b) {
                    best_mb = Some(mb);
                }
            }
        }
        best_mb
    }

    fn find_nvidia_smi_exe() -> Option<PathBuf> {
        static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                let candidates = [
                    r"C:\Windows\System32\nvidia-smi.exe",
                    r"C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe",
                    "nvidia-smi.exe",
                ];
                for c in candidates {
                    if let Ok(out) = std::process::Command::new(c)
                        .args([
                            "--query-gpu=temperature.gpu",
                            "--format=csv,noheader,nounits",
                        ])
                        .creation_flags(CREATE_NO_WINDOW_FLAG)
                        .output()
                        && out.status.success()
                    {
                        return Some(PathBuf::from(c));
                    }
                }
                None
            })
            .clone()
    }

    fn read_nvidia_smi() -> Option<GpuSample> {
        let exe = find_nvidia_smi_exe()?;
        let out = std::process::Command::new(exe)
            .args([
                "--query-gpu=temperature.gpu,utilization.gpu,memory.used,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .creation_flags(CREATE_NO_WINDOW_FLAG)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_nvidia_smi_csv(&String::from_utf8_lossy(&out.stdout))
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

    struct PdhCounter {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
        primed: bool,
    }

    impl PdhCounter {
        fn new(path: windows::core::PCWSTR) -> Option<Self> {
            let mut query = PDH_HQUERY::default();
            if unsafe { PdhOpenQueryW(None, 0, &mut query) } != 0 {
                return None;
            }
            let mut counter = PDH_HCOUNTER::default();
            let status = unsafe { PdhAddEnglishCounterW(query, path, 0, &mut counter) };
            if status != 0 {
                unsafe { PdhCloseQuery(query) };
                return None;
            }
            let _ = unsafe { PdhCollectQueryData(query) };
            Some(Self {
                query,
                counter,
                primed: false,
            })
        }

        fn sample_scalar_double(&mut self) -> Option<f32> {
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
                || val.CStatus > PDH_CSTATUS_VALID_MAX
            {
                return None;
            }
            let pct = unsafe { val.Anonymous.doubleValue };
            pct.is_finite().then_some((pct as f32).clamp(0.0, 100.0))
        }

        fn sample_array<F>(&mut self, fmt: PDH_FMT, mut collect_fn: F) -> bool
        where
            F: FnMut(&str, PDH_FMT_COUNTERVALUE),
        {
            if unsafe { PdhCollectQueryData(self.query) } != 0 {
                return false;
            }
            if !self.primed {
                self.primed = true;
                return false;
            }
            let mut buf_size = 0u32;
            let mut count = 0u32;
            let status = unsafe {
                PdhGetFormattedCounterArrayW(self.counter, fmt, &mut buf_size, &mut count, None)
            };
            if status != PDH_MORE_DATA_ERR || buf_size == 0 || count == 0 {
                return false;
            }
            let elem_sz = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>().max(1);
            let cap = (buf_size as usize).div_ceil(elem_sz);
            let mut items = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); cap];
            let status2 = unsafe {
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    fmt,
                    &mut buf_size,
                    &mut count,
                    Some(items.as_mut_ptr()),
                )
            };
            if status2 != 0 {
                return false;
            }
            let valid_len = (count as usize).min(items.len());
            for item in &items[..valid_len] {
                if item.FmtValue.CStatus > PDH_CSTATUS_VALID_MAX {
                    continue;
                }
                let name = if item.szName.0.is_null() {
                    String::new()
                } else {
                    unsafe { item.szName.to_string() }.unwrap_or_default()
                };
                collect_fn(&name, item.FmtValue);
            }
            true
        }
    }

    impl Drop for PdhCounter {
        fn drop(&mut self) {
            if !self.query.is_invalid() {
                unsafe { PdhCloseQuery(self.query) };
            }
        }
    }

    fn extract_luid_key(instance_name: &str) -> &str {
        if let Some(pos) = instance_name.find("luid_") {
            let tail = &instance_name[pos..];
            if let Some(end) = tail.find("_eng_") {
                return &tail[..end];
            }
            return tail;
        }
        "default"
    }

    pub struct Sampler {
        prev_cpu: Option<(u64, u64, u64)>,
        prev_net: Option<(u64, u64, Instant)>,
        last_disk_cap: Option<((f32, f32, f32), Instant)>,
        pdh_disk: Option<PdhCounter>,
        pdh_gpu_util: Option<PdhCounter>,
        pdh_gpu_vram: Option<PdhCounter>,
        pdh_thermal: Option<PdhCounter>,
        dxgi_vram_total_mb: Option<f32>,
        nvidia_cache: Option<(GpuSample, Instant)>,
    }

    impl Sampler {
        pub fn new() -> Self {
            Self {
                prev_cpu: read_cpu_times(),
                prev_net: None,
                last_disk_cap: None,
                pdh_disk: None,
                pdh_gpu_util: PdhCounter::new(w!(
                    "\\GPU Engine(*engtype_3D)\\Utilization Percentage"
                )),
                pdh_gpu_vram: PdhCounter::new(w!("\\GPU Adapter Memory(*)\\Dedicated Usage")),
                pdh_thermal: PdhCounter::new(w!(
                    "\\Thermal Zone Information(*)\\High Precision Temperature"
                ))
                .or_else(|| PdhCounter::new(w!("\\Thermal Zone Information(*)\\Temperature"))),
                dxgi_vram_total_mb: read_dxgi_vram_total_mb(),
                nvidia_cache: None,
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

            let now = Instant::now();
            let disk_cap = match self.last_disk_cap {
                Some((v, at)) if now.duration_since(at) < Duration::from_secs(10) => Some(v),
                _ => {
                    let v = read_system_drive_usage();
                    if let Some(cap) = v {
                        self.last_disk_cap = Some((cap, now));
                    }
                    v
                }
            };
            let (disk_used_pct, disk_used_gb, disk_total_gb) = match disk_cap {
                Some((p, u, t)) => (Some(p), Some(u), Some(t)),
                None => (None, None, None),
            };

            let disk_activity_pct = if collect_disk {
                if self.pdh_disk.is_none() {
                    self.pdh_disk = PdhCounter::new(w!("\\PhysicalDisk(_Total)\\% Disk Time"));
                }
                self.pdh_disk
                    .as_mut()
                    .and_then(|p| p.sample_scalar_double())
            } else {
                None
            };

            let (net_down_bps, net_up_bps) = if collect_net {
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

            let mut cpu_temp_c: Option<f32> = None;
            if let Some(therm) = self.pdh_thermal.as_mut() {
                let mut best_temp: Option<f32> = None;
                therm.sample_array(PDH_FMT_DOUBLE, |_, val| {
                    let raw = unsafe { val.Anonymous.doubleValue };
                    if !raw.is_finite() {
                        return;
                    }
                    let c = if raw > 2000.0 {
                        (raw / 10.0) - 273.15
                    } else if raw > 200.0 {
                        raw - 273.15
                    } else {
                        raw
                    } as f32;
                    if (5.0..=125.0).contains(&c) && best_temp.is_none_or(|b| c > b) {
                        best_temp = Some(c);
                    }
                });
                cpu_temp_c = best_temp;
            }

            let mut gpu_pct: Option<f32> = None;
            if let Some(gpu_counter) = self.pdh_gpu_util.as_mut() {
                let mut by_luid: HashMap<String, f64> = HashMap::new();
                if gpu_counter.sample_array(PDH_FMT_DOUBLE, |name, val| {
                    let v = unsafe { val.Anonymous.doubleValue };
                    if v.is_finite() && v >= 0.0 {
                        *by_luid
                            .entry(extract_luid_key(name).to_owned())
                            .or_default() += v;
                    }
                }) {
                    gpu_pct = by_luid
                        .into_values()
                        .fold(None, |acc: Option<f64>, v| {
                            Some(acc.map_or(v, |prev| prev.max(v)))
                        })
                        .map(|v| (v as f32).clamp(0.0, 100.0));
                }
            }

            let mut gpu_vram_used_mb: Option<f32> = None;
            if let Some(vram_counter) = self.pdh_gpu_vram.as_mut() {
                let mut max_bytes: Option<i64> = None;
                if vram_counter.sample_array(PDH_FMT_LARGE, |_, val| {
                    let b = unsafe { val.Anonymous.largeValue };
                    if b >= 0 && max_bytes.is_none_or(|m| b > m) {
                        max_bytes = Some(b);
                    }
                }) {
                    gpu_vram_used_mb = max_bytes.map(|b| (b as f64 / (1024.0 * 1024.0)) as f32);
                }
            }

            let mut gpu_vram_total_mb = self.dxgi_vram_total_mb;
            let mut gpu_temp_c: Option<f32> = None;

            let nv = match self.nvidia_cache {
                Some((cached, at)) if now.duration_since(at) < Duration::from_secs(2) => {
                    Some(cached)
                }
                _ => {
                    let fresh = read_nvidia_smi();
                    if let Some(v) = fresh {
                        self.nvidia_cache = Some((v, now));
                    }
                    fresh
                }
            };
            if let Some(nv) = nv {
                gpu_temp_c = nv.temp_c.or(gpu_temp_c);
                gpu_pct = nv.util_pct.or(gpu_pct);
                gpu_vram_used_mb = nv.vram_used_mb.or(gpu_vram_used_mb);
                gpu_vram_total_mb = nv.vram_total_mb.or(gpu_vram_total_mb);
            }

            SystemMetrics {
                cpu_pct,
                cpu_temp_c,
                gpu_pct,
                gpu_temp_c,
                gpu_vram_used_mb,
                gpu_vram_total_mb,
                ram_pct,
                ram_used_gb,
                ram_total_gb,
                disk_used_pct,
                disk_used_gb,
                disk_total_gb,
                disk_activity_pct,
                net_down_bps,
                net_up_bps,
                battery: read_battery(),
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use super::{GpuSample, SystemMetrics, compute_cpu_delta, parse_nvidia_smi_csv};
    use crate::battery::read_battery;

    pub struct Sampler {
        prev_cpu: Option<(u64, u64, u64)>,
        prev_disk: Option<(u64, u64, Instant)>,
        prev_net: Option<(u64, u64, Instant)>,
        disk_capacity: Option<(Instant, f32, f32, f32)>,
        cpu_temp_sensor: Option<(Instant, Option<PathBuf>)>,
        gpu_cache: Option<(Instant, GpuSample)>,
    }

    impl Sampler {
        pub fn new() -> Self {
            Self {
                prev_cpu: None,
                prev_disk: None,
                prev_net: None,
                disk_capacity: None,
                cpu_temp_sensor: None,
                gpu_cache: None,
            }
        }

        pub fn sample(&mut self, collect_disk: bool, collect_net: bool) -> SystemMetrics {
            let mut out = SystemMetrics {
                battery: read_battery(),
                ..Default::default()
            };

            if let Some(curr) = read_cpu_ticks() {
                if let Some(prev) = self.prev_cpu {
                    out.cpu_pct = compute_cpu_delta(prev, curr);
                }
                self.prev_cpu = Some(curr);
            }
            out.cpu_temp_c = self.read_cpu_temp();

            let (total_kb, avail_kb) = read_meminfo().unwrap_or((0, 0));
            if total_kb > 0 {
                let used_kb = total_kb.saturating_sub(avail_kb);
                out.ram_total_gb = total_kb as f32 / 1_048_576.0;
                out.ram_used_gb = used_kb as f32 / 1_048_576.0;
                out.ram_pct = (used_kb as f32 / total_kb as f32 * 100.0).clamp(0.0, 100.0);
            }

            // Always sample storage capacity (10s cached) so System & Home tabs display real usage
            // even when taskbar_show_disk is false.
            let recently_cached = self
                .disk_capacity
                .is_some_and(|(at, ..)| at.elapsed() < Duration::from_secs(10));
            if !recently_cached && let Some((pct, used_gb, total_gb)) = disk_capacity() {
                self.disk_capacity = Some((Instant::now(), pct, used_gb, total_gb));
            }
            if let Some((_, pct, used_gb, total_gb)) = self.disk_capacity {
                out.disk_used_pct = Some(pct);
                out.disk_used_gb = Some(used_gb);
                out.disk_total_gb = Some(total_gb);
            }

            if collect_disk {
                if let Some((read_ticks, write_ticks)) = disk_io_ticks() {
                    let now = Instant::now();
                    if let Some((prev_r, prev_w, prev_at)) = self.prev_disk {
                        let elapsed_ms = now.duration_since(prev_at).as_millis().max(1) as f64;
                        let busy_ms =
                            read_ticks.saturating_sub(prev_r) + write_ticks.saturating_sub(prev_w);
                        out.disk_activity_pct =
                            Some((busy_ms as f64 / elapsed_ms / 10.0).clamp(0.0, 100.0) as f32);
                    }
                    self.prev_disk = Some((read_ticks, write_ticks, now));
                }
            } else {
                self.prev_disk = None;
            }

            if collect_net {
                if let Some((rx, tx)) = net_bytes() {
                    let now = Instant::now();
                    if let Some((prev_rx, prev_tx, prev_at)) = self.prev_net {
                        let elapsed = now.duration_since(prev_at).as_secs_f64().max(0.001);
                        out.net_down_bps = (rx.saturating_sub(prev_rx) as f64 / elapsed) as u64;
                        out.net_up_bps = (tx.saturating_sub(prev_tx) as f64 / elapsed) as u64;
                    }
                    self.prev_net = Some((rx, tx, now));
                }
            } else {
                self.prev_net = None;
            }

            let (gpu_temp, gpu_pct, vram_used, vram_total) = self.read_gpu_metrics();
            out.gpu_temp_c = gpu_temp;
            out.gpu_pct = gpu_pct;
            out.gpu_vram_used_mb = vram_used;
            out.gpu_vram_total_mb = vram_total;

            out
        }

        fn read_cpu_temp(&mut self) -> Option<f32> {
            let need_discover = self
                .cpu_temp_sensor
                .as_ref()
                .is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(30));
            if need_discover {
                let path = discover_cpu_temp_sensor();
                self.cpu_temp_sensor = Some((Instant::now(), path));
            }
            let path = self.cpu_temp_sensor.as_ref()?.1.as_ref()?;
            if let Some(temp) = read_sysfs_temp_c(path) {
                return Some(temp);
            }
            self.cpu_temp_sensor = None;
            None
        }

        fn read_gpu_metrics(&mut self) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
            let (mut temp, mut util, mut vram_used, mut vram_total, has_nvidia_active) =
                read_drm_sysfs_gpu();

            let need_nvidia = has_nvidia_active || (util.is_none() && temp.is_none());
            if need_nvidia {
                let cached = self
                    .gpu_cache
                    .as_ref()
                    .filter(|(at, _)| at.elapsed() < Duration::from_secs(2))
                    .map(|(_, data)| *data);
                let nv = match cached {
                    Some(v) => Some(v),
                    None => {
                        let fresh = read_nvidia_smi();
                        if let Some(v) = fresh {
                            self.gpu_cache = Some((Instant::now(), v));
                        }
                        fresh
                    }
                };
                if let Some(nv) = nv {
                    temp = nv.temp_c.or(temp);
                    util = nv.util_pct.or(util);
                    vram_used = nv.vram_used_mb.or(vram_used);
                    vram_total = nv.vram_total_mb.or(vram_total);
                }
            }
            (temp, util, vram_used, vram_total)
        }
    }

    pub(crate) fn parse_sysfs_millidegrees(raw: &str) -> Option<f32> {
        let val = raw.trim().parse::<f32>().ok()?;
        if !val.is_finite() {
            return None;
        }
        let c = if val > 250.0 { val / 1000.0 } else { val };
        (5.0..=125.0).contains(&c).then_some(c)
    }

    fn read_sysfs_temp_c(path: &Path) -> Option<f32> {
        let raw = fs::read_to_string(path).ok()?;
        parse_sysfs_millidegrees(&raw)
    }

    fn discover_cpu_temp_sensor() -> Option<PathBuf> {
        let mut best: Option<(u8, PathBuf)> = None;
        if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
            for entry in entries.flatten() {
                let dir = entry.path();
                let name = fs::read_to_string(dir.join("name"))
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                let driver_prio = if matches!(
                    name.as_str(),
                    "coretemp" | "k10temp" | "zenpower" | "cpu_thermal" | "cros_ec"
                ) {
                    30
                } else if matches!(name.as_str(), "thinkpad" | "dell_smm" | "asus-ec-sensors")
                    || name.starts_with("nct")
                    || name.starts_with("it87")
                {
                    20
                } else if name == "acpitz" {
                    5
                } else {
                    0
                };
                if driver_prio == 0 {
                    continue;
                }
                for idx in 1..=12u8 {
                    let input = dir.join(format!("temp{idx}_input"));
                    if !input.is_file() || read_sysfs_temp_c(&input).is_none() {
                        continue;
                    }
                    let label = fs::read_to_string(dir.join(format!("temp{idx}_label")))
                        .unwrap_or_default()
                        .trim()
                        .to_ascii_lowercase();
                    let label_bonus = if label.contains("package")
                        || label.contains("tctl")
                        || label.contains("tdie")
                        || label == "cpu"
                    {
                        9
                    } else if label.contains("core 0") || idx == 1 {
                        5
                    } else {
                        1
                    };
                    let score = driver_prio + label_bonus;
                    if best.as_ref().is_none_or(|(b, _)| score > *b) {
                        best = Some((score, input));
                    }
                }
            }
        }
        if let Some((_, p)) = best {
            return Some(p);
        }

        if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
            let mut zbest: Option<(u8, PathBuf)> = None;
            for entry in entries.flatten() {
                let dir = entry.path();
                if !dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n.starts_with("thermal_zone"))
                {
                    continue;
                }
                let ttype = fs::read_to_string(dir.join("type"))
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                let score = if ttype.contains("x86_pkg_temp") {
                    20
                } else if ttype.contains("cpu") || ttype.contains("soc") {
                    15
                } else if ttype.contains("acpitz") {
                    5
                } else {
                    1
                };
                let temp_path = dir.join("temp");
                if temp_path.is_file()
                    && read_sysfs_temp_c(&temp_path).is_some()
                    && zbest.as_ref().is_none_or(|(b, _)| score > *b)
                {
                    zbest = Some((score, temp_path));
                }
            }
            if let Some((_, p)) = zbest {
                return Some(p);
            }
        }
        None
    }

    fn read_drm_sysfs_gpu() -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>, bool) {
        let mut temp: Option<f32> = None;
        let mut util: Option<f32> = None;
        let mut vram_used: Option<f32> = None;
        let mut vram_total: Option<f32> = None;
        let mut nvidia_active = false;

        let Ok(entries) = fs::read_dir("/sys/class/drm") else {
            return (None, None, None, None, false);
        };
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let name_str = fname.to_string_lossy();
            if !name_str.starts_with("card") || name_str.contains('-') {
                continue;
            }
            let dev = entry.path().join("device");
            let vendor = fs::read_to_string(dev.join("vendor")).unwrap_or_default();
            if vendor.trim().eq_ignore_ascii_case("0x10de") {
                let rt = fs::read_to_string(dev.join("power/runtime_status")).unwrap_or_default();
                if rt.trim() != "suspended" {
                    nvidia_active = true;
                }
            }
            if util.is_none()
                && let Ok(raw) = fs::read_to_string(dev.join("gpu_busy_percent"))
                && let Ok(v) = raw.trim().parse::<f32>()
                && v.is_finite()
            {
                util = Some(v.clamp(0.0, 100.0));
            }
            if vram_used.is_none()
                && let Ok(raw) = fs::read_to_string(dev.join("mem_info_vram_used"))
                && let Ok(bytes) = raw.trim().parse::<u64>()
            {
                vram_used = Some((bytes as f64 / 1_048_576.0) as f32);
            }
            if vram_total.is_none()
                && let Ok(raw) = fs::read_to_string(dev.join("mem_info_vram_total"))
                && let Ok(bytes) = raw.trim().parse::<u64>()
                && bytes > 0
            {
                vram_total = Some((bytes as f64 / 1_048_576.0) as f32);
            }
            if temp.is_none()
                && let Ok(hwmons) = fs::read_dir(dev.join("hwmon"))
            {
                for hw in hwmons.flatten() {
                    if let Some(t) = read_sysfs_temp_c(&hw.path().join("temp1_input")) {
                        temp = Some(t);
                        break;
                    }
                }
            }
        }
        (temp, util, vram_used, vram_total, nvidia_active)
    }

    fn find_nvidia_smi() -> Option<PathBuf> {
        static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                for cand in ["/usr/bin/nvidia-smi", "nvidia-smi"] {
                    if cand.starts_with('/') && !Path::new(cand).is_file() {
                        continue;
                    }
                    if let Ok(out) = std::process::Command::new(cand)
                        .args([
                            "--query-gpu=temperature.gpu",
                            "--format=csv,noheader,nounits",
                        ])
                        .output()
                        && out.status.success()
                    {
                        return Some(PathBuf::from(cand));
                    }
                }
                None
            })
            .clone()
    }

    fn read_nvidia_smi() -> Option<GpuSample> {
        let exe = find_nvidia_smi()?;
        let out = std::process::Command::new(exe)
            .args([
                "--query-gpu=temperature.gpu,utilization.gpu,memory.used,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_nvidia_smi_csv(&String::from_utf8_lossy(&out.stdout))
    }

    fn read_cpu_ticks() -> Option<(u64, u64, u64)> {
        let text = fs::read_to_string("/proc/stat").ok()?;
        parse_cpu_line(&text)
    }

    fn parse_cpu_line(text: &str) -> Option<(u64, u64, u64)> {
        let line = text.lines().find(|line| line.starts_with("cpu "))?;
        let vals = line
            .split_whitespace()
            .skip(1)
            .map(|v| v.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        let user = vals.first().copied()? + vals.get(1).copied().unwrap_or(0);
        let idle = vals.get(3).copied()? + vals.get(4).copied().unwrap_or(0);
        let total = vals
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, _)| *i < 8)
            .fold(0u64, |sum, (_, v)| sum.saturating_add(v));
        Some((idle, total - user, user))
    }

    fn read_meminfo() -> Option<(u64, u64)> {
        let text = fs::read_to_string("/proc/meminfo").ok()?;
        parse_meminfo(&text)
    }

    fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
        let mut total = None;
        let mut available = None;
        let mut free = None;
        let mut buffers = None;
        let mut cached = None;
        for line in text.lines() {
            let (key, rest) = line.split_once(':')?;
            let value = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())?;
            match key {
                "MemTotal" => total = Some(value),
                "MemAvailable" => available = Some(value),
                "MemFree" => free = Some(value),
                "Buffers" => buffers = Some(value),
                "Cached" => cached = Some(value),
                _ => {}
            }
        }
        let total = total?;
        let avail =
            available.or_else(|| Some(free? + buffers.unwrap_or(0) + cached.unwrap_or(0)))?;
        Some((total, avail.min(total)))
    }

    fn parse_proc_mounts_fstypes(text: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let Some(_src) = parts.next() else { continue };
            let Some(mount) = parts.next() else { continue };
            let Some(fstype) = parts.next() else { continue };
            let unescaped = mount.replace("\\040", " ");
            map.insert(unescaped, fstype.to_owned());
        }
        map
    }

    fn statvfs_root() -> Option<(f32, f32, f32)> {
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::statvfs(c"/".as_ptr(), &mut st) };
        if rc != 0 || st.f_blocks == 0 || st.f_frsize == 0 {
            return None;
        }
        let bsize = u128::from(st.f_frsize);
        let total_b = u128::from(st.f_blocks) * bsize;
        let free_b = u128::from(st.f_bfree) * bsize;
        let avail_b = u128::from(st.f_bavail) * bsize;
        let used_b = total_b.saturating_sub(free_b);
        let denom = used_b.saturating_add(avail_b).max(1);
        let pct = ((used_b as f64 / denom as f64) * 100.0).clamp(0.0, 100.0) as f32;
        const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
        Some((
            pct,
            (used_b as f64 / GIB) as f32,
            (total_b as f64 / GIB) as f32,
        ))
    }

    /// Aggregate unique backing filesystems via POSIX `df -P -k` + `/proc/mounts` fstype filtering,
    /// deduplicating Btrfs/bind subvolumes and excluding pseudo/loop/AppImage mounts, with a direct
    /// `statvfs("/")` fallback when `df` cannot inspect `/` (e.g. sandboxed environments).
    fn disk_capacity() -> Option<(f32, f32, f32)> {
        let mounts_raw = fs::read_to_string("/proc/mounts").unwrap_or_default();
        let fstypes = parse_proc_mounts_fstypes(&mounts_raw);
        let out = std::process::Command::new("df")
            .args(["-P", "-k"])
            .env("LC_ALL", "C")
            .output()
            .ok();
        if let Some(out) = out
            && out.status.success()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(res) = parse_df_capacity_with_fs(&text, &fstypes) {
                return Some(res);
            }
        }
        statvfs_root()
    }

    pub(crate) fn parse_df_capacity_with_fs(
        text: &str,
        fstypes: &HashMap<String, String>,
    ) -> Option<(f32, f32, f32)> {
        const PSEUDO_FS: &[&str] = &[
            "tmpfs",
            "devtmpfs",
            "proc",
            "sysfs",
            "devpts",
            "cgroup",
            "cgroup2",
            "mqueue",
            "hugetlbfs",
            "securityfs",
            "pstore",
            "debugfs",
            "tracefs",
            "bpf",
            "configfs",
            "fusectl",
            "efivarfs",
            "ramfs",
            "autofs",
            "binfmt_misc",
            "nsfs",
            "squashfs",
            "overlay",
            "iso9660",
            "fuse.AppImage",
            "fuse.portal",
            "fuse.gvfsd-fuse",
        ];

        let mut total_kb = 0_u64;
        let mut used_kb = 0_u64;
        let mut seen_devs: HashSet<&str> = HashSet::new();
        let mut found = false;

        for line in text.lines().skip(1) {
            let cols = line.split_whitespace().collect::<Vec<_>>();
            if cols.len() < 6 {
                continue;
            }
            let fs = cols[0];
            let mount = cols[5..].join(" ");
            if PSEUDO_FS.contains(&fs)
                || fs.starts_with("/dev/loop")
                || fs.starts_with("/dev/zram")
                || fs.ends_with(".AppImage")
                || fs.contains(".AppImage")
                || mount.starts_with("/tmp/.mount_")
                || mount.starts_with("/snap/")
            {
                continue;
            }
            if let Some(fstype) = fstypes.get(&mount)
                && PSEUDO_FS.contains(&fstype.as_str())
            {
                continue;
            }
            let Some(total) = cols[1].parse::<u64>().ok() else {
                continue;
            };
            let Some(used) = cols[2].parse::<u64>().ok() else {
                continue;
            };
            if total == 0 {
                continue;
            }
            if fs.starts_with('/') && !seen_devs.insert(fs) {
                // Btrfs subvolumes (@, @home, @var) or bind mounts share one partition; count once.
                continue;
            }
            total_kb = total_kb.saturating_add(total);
            used_kb = used_kb.saturating_add(used);
            found = true;
        }
        if !found || total_kb == 0 {
            return None;
        }
        let pct = (used_kb as f64 / total_kb as f64 * 100.0).clamp(0.0, 100.0) as f32;
        Some((
            pct,
            (used_kb as f64 / 1_048_576.0) as f32,
            (total_kb as f64 / 1_048_576.0) as f32,
        ))
    }

    #[cfg(test)]
    pub(crate) fn parse_df_capacity(text: &str) -> Option<f32> {
        parse_df_capacity_with_fs(text, &HashMap::new()).map(|(pct, _, _)| pct)
    }

    fn disk_io_ticks() -> Option<(u64, u64)> {
        let text = fs::read_to_string("/proc/diskstats").ok()?;
        let mut reads = 0_u64;
        let mut writes = 0_u64;
        let mut found = false;
        for line in text.lines() {
            let cols = line.split_whitespace().collect::<Vec<_>>();
            let Some(name) = cols.get(2) else { continue };
            if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
                continue;
            }
            let Some(r) = cols.get(6).and_then(|v| v.parse::<u64>().ok()) else {
                continue;
            };
            let Some(w) = cols.get(10).and_then(|v| v.parse::<u64>().ok()) else {
                continue;
            };
            reads = reads.saturating_add(r);
            writes = writes.saturating_add(w);
            found = true;
        }
        found.then_some((reads, writes))
    }

    fn net_bytes() -> Option<(u64, u64)> {
        let text = fs::read_to_string("/proc/net/dev").ok()?;
        let mut rx = 0_u64;
        let mut tx = 0_u64;
        let mut found = false;
        for line in text.lines().skip(2) {
            let Some((name, rest)) = line.split_once(':') else {
                continue;
            };
            if name.trim() == "lo" {
                continue;
            }
            let cols = rest.split_whitespace().collect::<Vec<_>>();
            let Some(r) = cols.first().and_then(|v| v.parse::<u64>().ok()) else {
                continue;
            };
            let Some(t) = cols.get(8).and_then(|v| v.parse::<u64>().ok()) else {
                continue;
            };
            rx = rx.saturating_add(r);
            tx = tx.saturating_add(t);
            found = true;
        }
        found.then_some((rx, tx))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn linux_cpu_line_maps_idle_total_user() {
            let (idle, total_excluding_user, user) =
                parse_cpu_line("cpu  10 2 30 4 5 6 7 8 9 10\n").unwrap();
            assert_eq!(idle, 9);
            assert_eq!(total_excluding_user, 60);
            assert_eq!(user, 12);
            assert_eq!(compute_cpu_delta((9, 60, 12), (21, 70, 18)), 25.0);
        }

        #[test]
        fn linux_meminfo_uses_available_or_fallback() {
            let text = "MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 250 kB\n";
            assert_eq!(parse_meminfo(text), Some((1000, 400)));
        }

        #[test]
        fn linux_sampler_reads_live_proc_metrics() {
            let mut sampler = Sampler::new();
            let metrics = sampler.sample(true, true);
            assert!(metrics.ram_total_gb > 0.0);
            assert!(metrics.ram_pct.is_finite());
            assert!(metrics.cpu_pct.is_finite());
            assert!(metrics.disk_used_pct.is_some());
        }

        #[test]
        fn linux_df_capacity_aggregates_system_mounts_and_skips_pseudo_filesystems() {
            // Shortened POSIX `df -P -k` output: root plus a separate /home mount, plus Btrfs
            // duplicate subvolume on /dev/sda1 and AppImage mount that must not skew totals.
            let text = "Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                        /dev/sda1 100000 40000 60000 40% /\n\
                        /dev/sda1 100000 40000 60000 40% /var/log\n\
                        tmpfs 10000 8000 2000 80% /dev/shm\n\
                        Liebert_Dynamic_Island-0.2.0-x86_64.AppImage 5632 5632 0 100% /tmp/.mount_Lieber\n\
                        /dev/sdb1 200000 100000 100000 50% /home\n";
            let pct = super::parse_df_capacity(text).expect("capacity");
            assert!((pct - 46.666_668).abs() < 0.01, "unexpected {pct}");
        }

        #[test]
        fn parses_sysfs_millidegrees_and_nvidia_smi() {
            assert_eq!(parse_sysfs_millidegrees("54000\n"), Some(54.0));
            assert_eq!(parse_sysfs_millidegrees("42"), Some(42.0));
            assert_eq!(parse_sysfs_millidegrees("0"), None);

            let nv = parse_nvidia_smi_csv("39, 3, 1432, 2048\n").expect("nvidia row");
            assert_eq!(
                nv,
                GpuSample {
                    temp_c: Some(39.0),
                    util_pct: Some(3.0),
                    vram_used_mb: Some(1432.0),
                    vram_total_mb: Some(2048.0),
                }
            );
            assert_eq!(
                parse_nvidia_smi_csv("[N/A], 15, [Not Supported], 4096"),
                Some(GpuSample {
                    temp_c: None,
                    util_pct: Some(15.0),
                    vram_used_mb: None,
                    vram_total_mb: Some(4096.0),
                })
            );
        }
    }
}

#[cfg(all(not(windows), not(target_os = "linux")))]
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
        .stack_size(128 * 1024)
        .spawn(move || {
            let mut enabled = initial_enabled;
            let mut interval_ms = initial_interval_ms.clamp(500, 5000);
            let mut collect_disk = initial_disk;
            let mut collect_net = initial_net;
            let mut sampler = imp::Sampler::new();

            loop {
                let metrics = sampler.sample(enabled && collect_disk, enabled && collect_net);
                if event_tx
                    .send(PlatformEvent::SystemMetrics(metrics))
                    .is_err()
                {
                    break;
                }
                on_wake();

                let wait = if enabled {
                    Duration::from_millis(interval_ms)
                } else {
                    Duration::from_secs(4)
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
    fn format_speed_and_vram_scale_units() {
        assert_eq!(format_speed_bps(512), "512B");
        assert_eq!(format_speed_bps(15 * 1024), "15K");
        assert_eq!(format_speed_bps(2_621_440), "2.5M");
        assert_eq!(format_vram_mb(512.0, Some(1024.0)), "0.5/1.0G");
        assert_eq!(format_vram_mb(420.0, Some(512.0)), "420/512M");
    }

    #[test]
    fn parses_nvidia_csv_rows() {
        let parsed = parse_nvidia_smi_csv("44, 12, 1200, 8192\n51, 68, 3072, 8192\n")
            .expect("picks busiest gpu");
        assert_eq!(
            parsed,
            GpuSample {
                temp_c: Some(51.0),
                util_pct: Some(68.0),
                vram_used_mb: Some(3072.0),
                vram_total_mb: Some(8192.0),
            }
        );
    }
}
