//! Local host data: /proc files and `virsh nodeinfo`.

use crate::model::HostInfo;

/// Parse `virsh nodeinfo` into HostInfo with a hostname.
pub fn parse_nodeinfo(text: &str, hostname: &str) -> HostInfo {
    let mut model = String::from("unknown");
    let mut threads = 0u32;
    let mut mem_kib = 0u64;
    for line in text.lines() {
        let mut parts = line.splitn(2, ':');
        let key = parts.next().unwrap_or("").trim();
        let val = parts.next().unwrap_or("").trim();
        match key {
            "CPU model" => model = val.to_string(),
            "CPU(s)" => threads = val.parse().unwrap_or(0),
            "Memory size" => {
                // e.g. `65456164 KiB`
                mem_kib = val
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
            }
            _ => {}
        }
    }
    HostInfo {
        hostname: hostname.to_string(),
        cpu_model: model,
        threads,
        mem_kib,
    }
}

/// Uptime of a running domain on a local URI, from its QEMU pid file
/// (`/run/libvirt/qemu/<name>.pid`) and `/proc/<pid>/stat` start time.
pub fn domain_uptime(name: &str) -> Option<String> {
    if name.contains('/') {
        return None;
    }
    // qemu:///system keeps pid files in /run/libvirt/qemu; qemu:///session in
    // $XDG_RUNTIME_DIR/libvirt/qemu/run.
    let session = std::env::var("XDG_RUNTIME_DIR")
        .map(|d| format!("{d}/libvirt/qemu/run/{name}.pid"))
        .unwrap_or_default();
    let pid = [format!("/run/libvirt/qemu/{name}.pid"), session]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())?;
    let pid: u32 = pid.trim().parse().ok()?;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let host_up: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let secs = proc_uptime_secs(&stat, host_up, USER_HZ)?;
    Some(fmt_uptime(secs))
}

/// Clock ticks per second in `/proc/<pid>/stat` (USER_HZ, fixed at 100 on Linux).
const USER_HZ: f64 = 100.0;

/// Process age in seconds from a `/proc/<pid>/stat` line.
pub fn proc_uptime_secs(stat: &str, host_uptime: f64, ticks: f64) -> Option<u64> {
    // Field 22 (starttime) counted after the `(comm)` field, which may contain spaces.
    let after = &stat[stat.rfind(')')? + 1..];
    let start: f64 = after.split_whitespace().nth(19)?.parse().ok()?;
    Some((host_uptime - start / ticks).max(0.0) as u64)
}

/// `3d 04h`, `5h 12m`, `2m`.
pub fn fmt_uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d {h:02}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

/// Per-CPU counters from one `cpuN` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

/// Parse `/proc/stat`: one entry per `cpuN` thread line (skips aggregate `cpu`).
pub fn parse_proc_stat(text: &str) -> Vec<CpuTimes> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut cols = line.split_whitespace();
        let label = cols.next().unwrap_or("");
        if !label.starts_with("cpu") || label.len() <= 3 {
            continue;
        }
        if !label[3..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let nums: Vec<u64> = cols.filter_map(|c| c.parse().ok()).collect();
        if nums.len() < 4 {
            continue;
        }
        let total: u64 = nums.iter().sum();
        let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
        out.push(CpuTimes { total, idle });
    }
    out
}

/// Busy fraction between two samples.
pub fn cpu_busy_fraction(before: CpuTimes, after: CpuTimes) -> f64 {
    let dt = after.total.saturating_sub(before.total);
    let di = after.idle.saturating_sub(before.idle);
    if dt == 0 {
        0.0
    } else {
        (dt - di) as f64 / dt as f64
    }
}

/// Memory figures from `/proc/meminfo` (all KiB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemInfo {
    pub total_kib: u64,
    pub available_kib: u64,
    pub swap_total_kib: u64,
    pub swap_free_kib: u64,
    pub hugepages_total: u64,
    pub hugepages_free: u64,
    pub hugepage_kib: u64,
}

/// Parse `/proc/meminfo`.
pub fn parse_meminfo(text: &str) -> MemInfo {
    let mut m = MemInfo::default();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        let val: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        match key {
            "MemTotal:" => m.total_kib = val,
            "MemAvailable:" => m.available_kib = val,
            "SwapTotal:" => m.swap_total_kib = val,
            "SwapFree:" => m.swap_free_kib = val,
            "HugePages_Total:" => m.hugepages_total = val,
            "HugePages_Free:" => m.hugepages_free = val,
            "Hugepagesize:" => m.hugepage_kib = val,
            _ => {}
        }
    }
    m
}

/// Parse `/proc/loadavg`: first three averages.
pub fn parse_loadavg(text: &str) -> (f64, f64, f64) {
    let mut it = text.split_whitespace();
    let f = |s: Option<&str>| s.and_then(|v| v.parse().ok()).unwrap_or(0.0);
    (f(it.next()), f(it.next()), f(it.next()))
}

/// CPU model name from `/proc/cpuinfo` (first `model name`).
pub fn parse_cpu_model(text: &str) -> String {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("model name")
            && let Some(name) = rest.split_once(':').map(|(_, v)| v.trim())
        {
            return name.to_string();
        }
    }
    String::from("unknown")
}

/// Max CPU frequency in GHz from `/proc/cpuinfo` (`cpu MHz` max).
pub fn parse_cpu_ghz(text: &str) -> f64 {
    let mut max = 0.0f64;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("cpu MHz")
            && let Some(mhz) = rest.split_once(':').and_then(|(_, v)| v.trim().parse().ok())
        {
            max = max.max(mhz);
        }
    }
    max / 1000.0
}

/// Highest temperature in °C from hwmon inputs (values are millidegrees).
pub fn max_temp_c(inputs: &[&str]) -> Option<f64> {
    inputs
        .iter()
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .map(|m| m / 1000.0)
        .reduce(f64::max)
}

/// Read /proc/meminfo total kB (local URIs only).
pub fn local_mem_total_kib() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod uptime_tests {
    #[test]
    fn parses_stat_with_spaces_in_comm() {
        // starttime (field 22) = 1000 ticks = 10 s after boot; host up 3610 s.
        let stat = "123 (qemu system x) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 1000 20";
        assert_eq!(super::proc_uptime_secs(stat, 3610.0, 100.0), Some(3600));
        assert_eq!(super::fmt_uptime(3600), "1h 00m");
        assert_eq!(super::fmt_uptime(3 * 86_400 + 4 * 3600), "3d 04h");
    }
}

#[cfg(test)]
mod tests {
    use super::parse_nodeinfo;

    #[test]
    fn parses_fixture() {
        let text = std::fs::read_to_string("tests/fixtures/virsh/nodeinfo.txt").unwrap();
        let info = parse_nodeinfo(&text, "forge");
        assert!(info.threads > 0);
        assert!(info.mem_kib > 0);
    }
}
