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
    domain_uptime_secs(name).map(fmt_uptime)
}

/// [`domain_uptime`] in seconds.
pub fn domain_uptime_secs(name: &str) -> Option<u64> {
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
    proc_uptime_secs(&stat, host_up, USER_HZ)
}

/// Inverse of [`fmt_uptime`] (`3d 04h`, `5h 12m`, `2m`) for fixtures; `—` → None.
pub fn parse_uptime_label(label: &str) -> Option<u64> {
    let mut total = 0u64;
    let mut any = false;
    for part in label.split_whitespace() {
        let (num, unit) = part.split_at(part.find(|c: char| !c.is_ascii_digit())?);
        let n: u64 = num.parse().ok()?;
        total += n * match unit {
            "d" => 86_400,
            "h" => 3600,
            "m" => 60,
            "s" => 1,
            _ => return None,
        };
        any = true;
    }
    any.then_some(total)
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
        // user nice system idle iowait irq softirq steal: `guest` and
        // `guest_nice` (fields 9-10) are already counted in user/nice.
        let total: u64 = nums.iter().take(8).sum();
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

/// Host facts from `virsh nodeinfo` + `virsh nodememstats` (any URI,
/// remote included): (cpu model, threads, total KiB, available KiB).
pub fn parse_node_facts(nodeinfo: &str, memstats: &str) -> (String, u32, u64, u64) {
    let field = |text: &str, key: &str| {
        text.lines()
            .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == key))
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default()
    };
    let kib = |v: String| {
        v.split_whitespace()
            .next()
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let model = field(nodeinfo, "CPU model");
    let threads = field(nodeinfo, "CPU(s)").parse().unwrap_or(0);
    let total = kib(field(memstats, "total")).max(kib(field(nodeinfo, "Memory size")));
    // Older libvirt has no `available`: free + buffers + cached.
    let available = match kib(field(memstats, "available")) {
        0 => kib(field(memstats, "free")) + kib(field(memstats, "buffers")) + kib(field(memstats, "cached")),
        n => n,
    };
    (model, threads, total, available)
}

/// Whole-host CPU counters from `virsh nodecpustats` (cumulative ns, any URI).
///
/// Unlike `--percent`, which blocks for a second while it samples, the raw
/// counters return at once; the busy fraction comes from two samples, the
/// same way `/proc/stat` is read locally. libvirt already folds nice, irq
/// and softirq into `user`/`system`, and `guest` is part of `user`.
pub fn parse_nodecpustats(text: &str) -> Option<CpuTimes> {
    let mut total = 0u64;
    let mut idle = 0u64;
    let mut seen = false;
    for line in text.lines() {
        let Some((key, val)) = line.split_once(':') else {
            continue;
        };
        let Ok(ns) = val.trim().parse::<u64>() else {
            continue;
        };
        match key.trim() {
            "user" | "system" => total += ns,
            "idle" | "iowait" => {
                total += ns;
                idle += ns;
            }
            _ => continue,
        }
        seen = true;
    }
    (seen && total > 0).then_some(CpuTimes { total, idle })
}

#[cfg(test)]
mod node_tests {
    use super::*;

    #[test]
    fn nodecpustats_counters() {
        let text = include_str!("../../../tests/fixtures/virsh/nodecpustats.txt");
        let t = parse_nodecpustats(text).unwrap();
        assert_eq!(t.idle, 5_463_520_130_000_000 + 10_339_240_000_000);
        assert_eq!(
            t.total,
            305_254_360_000_000 + 81_362_950_000_000 + 5_463_520_130_000_000 + 10_339_240_000_000
        );
    }

    #[test]
    fn nodecpustats_busy_between_samples() {
        let a = parse_nodecpustats("user: 100\nsystem: 0\nidle: 900\niowait: 0\n").unwrap();
        let b = parse_nodecpustats("user: 400\nsystem: 100\nidle: 1500\niowait: 0\n").unwrap();
        assert!((cpu_busy_fraction(a, b) - 0.4).abs() < 1e-9);
    }

    #[test]
    fn nodecpustats_rejects_garbage() {
        assert_eq!(parse_nodecpustats(""), None);
        assert_eq!(parse_nodecpustats("error: failed to connect\n"), None);
    }

    #[test]
    fn node_facts_from_fixtures() {
        let (model, threads, total, available) = parse_node_facts(
            include_str!("../../../tests/fixtures/virsh/nodeinfo.txt"),
            include_str!("../../../tests/fixtures/virsh/nodememstats.txt"),
        );
        assert_eq!(
            (model.as_str(), threads, total, available),
            ("x86_64", 6, 8_067_340, 4_726_880)
        );
    }

    #[test]
    fn node_facts_without_available() {
        let (_, _, total, available) = parse_node_facts(
            "",
            "total: 1000 KiB\nfree: 200 KiB\nbuffers: 100 KiB\ncached: 300 KiB\n",
        );
        assert_eq!((total, available), (1000, 600));
    }
}
