//! Live metrics store: turns periodic `virsh domstats --raw` and `/proc/stat`
//! samples into per-domain and host histories.

use std::collections::HashMap;
use std::time::Instant;

use super::ring::Ring;
use super::sampler::{cpu_pct, mem_used_kib, rate};
use crate::backend::RawDomainStats;
use crate::backend::virsh::host_local::{CpuTimes, cpu_busy_fraction, parse_proc_stat};

/// Default chart window: 60 samples (60 s at the default 1 s refresh).
pub const HISTORY: usize = 60;

/// Samples kept per series: enough for the 1 h window.
pub const HISTORY_MAX: usize = 3600;

/// History windows cycled with `t`: (samples, label).
pub const WINDOWS: [(usize, &str); 3] = [(60, "60s"), (300, "5m"), (3600, "1h")];

static WINDOW: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Select the next history window; returns its label.
pub fn cycle_window() -> &'static str {
    let next = (WINDOW.load(std::sync::atomic::Ordering::Relaxed) + 1) % WINDOWS.len();
    WINDOW.store(next, std::sync::atomic::Ordering::Relaxed);
    WINDOWS[next].1
}

/// Samples in the selected window.
pub fn window_len() -> usize {
    WINDOWS[WINDOW.load(std::sync::atomic::Ordering::Relaxed) % WINDOWS.len()].0
}

/// Label of the selected window (`60s`, `5m`, `1h`).
pub fn window_label() -> &'static str {
    WINDOWS[WINDOW.load(std::sync::atomic::Ordering::Relaxed) % WINDOWS.len()].1
}

/// Cumulative counters from one sample, used to compute rates on the next one.
#[derive(Debug, Clone, Default)]
struct Counters {
    cpu_ns: u64,
    rd_bytes: u64,
    wr_bytes: u64,
    reqs: u64,
    rx_bytes: u64,
    tx_bytes: u64,
    /// Per vCPU `vcpu.N.time` (ns).
    vcpu_ns: Vec<u64>,
    /// Per disk: (name, rd bytes, wr bytes, reqs).
    disks: Vec<(String, u64, u64, u64)>,
    /// Per NIC: (name, rx bytes, tx bytes, drops, errs).
    nics: Vec<(String, u64, u64, u64, u64)>,
}

/// Per-disk rates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiskNow {
    pub name: String,
    pub rd_bps: f64,
    pub wr_bps: f64,
    pub iops: f64,
}

/// Per-interface rates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NicNow {
    pub name: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub drops: u64,
    pub errs: u64,
}

/// Latest computed values for one domain.
#[derive(Debug, Clone, Default)]
pub struct DomainNow {
    pub cpu_pct: f64,
    pub vcpus: u32,
    pub mem_used_kib: u64,
    pub mem_max_kib: u64,
    pub rss_kib: u64,
    /// True when `mem_used_kib` comes from guest balloon stats (not just the allocation).
    pub mem_from_guest: bool,
    pub rd_bps: f64,
    pub wr_bps: f64,
    pub iops: f64,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
    pub drops: u64,
    pub errs: u64,
    pub first_disk: String,
    pub first_iface: String,
    /// Busy fraction (0–1) per vCPU.
    pub vcpu_frac: Vec<f64>,
    pub disks: Vec<DiskNow>,
    pub nics: Vec<NicNow>,
}

/// History for one domain. Values are raw units (percent, KiB, bytes/s).
#[derive(Debug, Clone)]
pub struct DomainSeries {
    pub cpu: Ring,
    pub mem: Ring,
    pub disk_rd: Ring,
    pub disk_wr: Ring,
    pub net_rx: Ring,
    pub net_tx: Ring,
    pub now: DomainNow,
}

impl Default for DomainSeries {
    fn default() -> Self {
        Self {
            cpu: Ring::new(HISTORY_MAX),
            mem: Ring::new(HISTORY_MAX),
            disk_rd: Ring::new(HISTORY_MAX),
            disk_wr: Ring::new(HISTORY_MAX),
            net_rx: Ring::new(HISTORY_MAX),
            net_tx: Ring::new(HISTORY_MAX),
            now: DomainNow::default(),
        }
    }
}

/// Host CPU history from `/proc/stat` (local URIs only).
#[derive(Debug, Clone)]
pub struct HostSeries {
    pub cpu: Ring,
    pub threads: Vec<f64>,
    prev: Vec<CpuTimes>,
}

impl Default for HostSeries {
    fn default() -> Self {
        Self {
            cpu: Ring::new(HISTORY_MAX),
            threads: Vec::new(),
            prev: Vec::new(),
        }
    }
}

/// rx/tx history of a host interface (bytes/s).
#[derive(Debug, Clone)]
pub struct IfaceSeries {
    pub rx: Ring,
    pub tx: Ring,
    pub rx_bps: f64,
    pub tx_bps: f64,
    prev: Option<(Instant, u64, u64)>,
}

impl Default for IfaceSeries {
    fn default() -> Self {
        Self {
            rx: Ring::new(HISTORY_MAX),
            tx: Ring::new(HISTORY_MAX),
            rx_bps: 0.0,
            tx_bps: 0.0,
            prev: None,
        }
    }
}

/// All live metrics.
#[derive(Debug, Clone, Default)]
pub struct MetricsStore {
    domains: HashMap<String, DomainSeries>,
    prev: HashMap<String, (Instant, Counters)>,
    pub host: HostSeries,
    ifaces: HashMap<String, IfaceSeries>,
}

fn sum_indexed(s: &RawDomainStats, group: &str, field: &str) -> u64 {
    let count = s.u64(&format!("{group}.count")).unwrap_or(0);
    (0..count)
        .filter_map(|i| s.u64(&format!("{group}.{i}.{field}")))
        .sum()
}

fn counters(s: &RawDomainStats) -> Counters {
    Counters {
        cpu_ns: s.u64("cpu.time").unwrap_or(0),
        rd_bytes: sum_indexed(s, "block", "rd.bytes"),
        wr_bytes: sum_indexed(s, "block", "wr.bytes"),
        reqs: sum_indexed(s, "block", "rd.reqs") + sum_indexed(s, "block", "wr.reqs"),
        rx_bytes: sum_indexed(s, "net", "rx.bytes"),
        tx_bytes: sum_indexed(s, "net", "tx.bytes"),
        vcpu_ns: (0..s.u64("vcpu.maximum").unwrap_or(0))
            .map(|i| s.u64(&format!("vcpu.{i}.time")).unwrap_or(0))
            .collect(),
        disks: (0..s.u64("block.count").unwrap_or(0))
            .map(|i| {
                let k = |f: &str| s.u64(&format!("block.{i}.{f}")).unwrap_or(0);
                (
                    s.values
                        .get(&format!("block.{i}.name"))
                        .cloned()
                        .unwrap_or_default(),
                    k("rd.bytes"),
                    k("wr.bytes"),
                    k("rd.reqs") + k("wr.reqs"),
                )
            })
            .collect(),
        nics: (0..s.u64("net.count").unwrap_or(0))
            .map(|i| {
                let k = |f: &str| s.u64(&format!("net.{i}.{f}")).unwrap_or(0);
                (
                    s.values
                        .get(&format!("net.{i}.name"))
                        .cloned()
                        .unwrap_or_default(),
                    k("rx.bytes"),
                    k("tx.bytes"),
                    k("rx.drop") + k("tx.drop"),
                    k("rx.errs") + k("tx.errs"),
                )
            })
            .collect(),
    }
}

/// Domain states that have a running QEMU (running, blocked, shutting down).
fn is_running(s: &RawDomainStats) -> bool {
    matches!(s.u64("state.state"), Some(1 | 2 | 4))
}

impl MetricsStore {
    /// Ingest one `domstats --raw` sample taken at `now`.
    ///
    /// Only running domains (`state.state=1`) produce samples; histories of
    /// domains that stopped or disappeared are dropped so a restart starts clean.
    pub fn ingest_domains(&mut self, stats: &[RawDomainStats], now: Instant) {
        let running: Vec<&RawDomainStats> = stats.iter().filter(|s| is_running(s)).collect();
        self.domains
            .retain(|name, _| running.iter().any(|s| &s.domain == name));
        self.prev
            .retain(|name, _| running.iter().any(|s| &s.domain == name));
        for s in running {
            let cur = counters(s);
            let series = self.domains.entry(s.domain.clone()).or_default();
            let n = &mut series.now;
            n.vcpus = s.u64("vcpu.current").unwrap_or(1) as u32;
            n.mem_max_kib = s.u64("balloon.current").unwrap_or(0);
            n.rss_kib = s.u64("balloon.rss").unwrap_or(0);
            match (s.u64("balloon.available"), s.u64("balloon.unused")) {
                (Some(avail), Some(unused)) => {
                    n.mem_used_kib = mem_used_kib(avail, unused);
                    n.mem_from_guest = true;
                }
                _ => {
                    // No guest stats: fall back to host RSS (bounded by the allocation).
                    n.mem_used_kib = if n.mem_max_kib > 0 {
                        n.rss_kib.min(n.mem_max_kib)
                    } else {
                        n.rss_kib
                    };
                    n.mem_from_guest = false;
                }
            }
            n.rx_total = cur.rx_bytes;
            n.tx_total = cur.tx_bytes;
            n.drops = sum_indexed(s, "net", "rx.drop") + sum_indexed(s, "net", "tx.drop");
            n.errs = sum_indexed(s, "net", "rx.errs") + sum_indexed(s, "net", "tx.errs");
            n.first_disk = s.values.get("block.0.name").cloned().unwrap_or_default();
            n.first_iface = s.values.get("net.0.name").cloned().unwrap_or_default();
            if let Some((t0, before)) = self.prev.get(&s.domain).cloned().as_ref() {
                let wall = now.duration_since(*t0);
                let secs = wall.as_secs_f64();
                // A counter going backwards means the domain restarted: skip this delta.
                let reset = cur.cpu_ns < before.cpu_ns;
                if secs > 0.0 && !reset {
                    n.cpu_pct = cpu_pct(before.cpu_ns, cur.cpu_ns, wall.as_nanos() as u64, n.vcpus);
                    n.rd_bps = rate(before.rd_bytes, cur.rd_bytes, secs);
                    n.wr_bps = rate(before.wr_bytes, cur.wr_bytes, secs);
                    n.iops = rate(before.reqs, cur.reqs, secs);
                    n.rx_bps = rate(before.rx_bytes, cur.rx_bytes, secs);
                    n.tx_bps = rate(before.tx_bytes, cur.tx_bytes, secs);
                    let wall_ns = wall.as_nanos() as f64;
                    n.vcpu_frac = cur
                        .vcpu_ns
                        .iter()
                        .enumerate()
                        .map(|(i, t)| {
                            let b = before.vcpu_ns.get(i).copied().unwrap_or(*t);
                            (t.saturating_sub(b) as f64 / wall_ns).clamp(0.0, 1.0)
                        })
                        .collect();
                    n.disks = cur
                        .disks
                        .iter()
                        .map(|(name, rd, wr, reqs)| {
                            let b = before.disks.iter().find(|d| d.0 == *name);
                            let (brd, bwr, breq) = b.map_or((*rd, *wr, *reqs), |d| (d.1, d.2, d.3));
                            DiskNow {
                                name: name.clone(),
                                rd_bps: rate(brd, *rd, secs),
                                wr_bps: rate(bwr, *wr, secs),
                                iops: rate(breq, *reqs, secs),
                            }
                        })
                        .collect();
                    n.nics = cur
                        .nics
                        .iter()
                        .map(|(name, rx, tx, drops, errs)| {
                            let b = before.nics.iter().find(|d| d.0 == *name);
                            let (brx, btx) = b.map_or((*rx, *tx), |d| (d.1, d.2));
                            NicNow {
                                name: name.clone(),
                                rx_bps: rate(brx, *rx, secs),
                                tx_bps: rate(btx, *tx, secs),
                                drops: *drops,
                                errs: *errs,
                            }
                        })
                        .collect();
                    series.cpu.push(n.cpu_pct as f32);
                    series.mem.push(n.mem_used_kib as f32);
                    series.disk_rd.push(n.rd_bps as f32);
                    series.disk_wr.push(n.wr_bps as f32);
                    series.net_rx.push(n.rx_bps as f32);
                    series.net_tx.push(n.tx_bps as f32);
                }
            }
            self.prev.insert(s.domain.clone(), (now, cur));
        }
    }

    /// Ingest one `/proc/stat` snapshot (host CPU, per thread).
    pub fn ingest_proc_stat(&mut self, text: &str) {
        let cur = parse_proc_stat(text);
        if cur.is_empty() {
            return;
        }
        if self.host.prev.len() == cur.len() {
            let threads: Vec<f64> = self
                .host
                .prev
                .iter()
                .zip(cur.iter())
                .map(|(a, b)| cpu_busy_fraction(*a, *b))
                .collect();
            let avg = threads.iter().sum::<f64>() / threads.len() as f64;
            self.host.cpu.push((avg * 100.0) as f32);
            self.host.threads = threads;
        }
        self.host.prev = cur;
    }

    /// Ingest cumulative rx/tx byte counters of host interfaces (bridges).
    pub fn ingest_ifaces(&mut self, counters: &HashMap<String, (u64, u64)>, now: Instant) {
        self.ifaces.retain(|k, _| counters.contains_key(k));
        for (name, (rx, tx)) in counters {
            let s = self.ifaces.entry(name.clone()).or_default();
            if let Some((t0, prx, ptx)) = s.prev {
                let secs = now.duration_since(t0).as_secs_f64();
                if secs > 0.0 && *rx >= prx && *tx >= ptx {
                    s.rx_bps = rate(prx, *rx, secs);
                    s.tx_bps = rate(ptx, *tx, secs);
                    s.rx.push(s.rx_bps as f32);
                    s.tx.push(s.tx_bps as f32);
                }
            }
            s.prev = Some((now, *rx, *tx));
        }
    }

    /// History of a host interface.
    pub fn iface(&self, name: &str) -> Option<&IfaceSeries> {
        self.ifaces.get(name)
    }

    /// History for a running domain, if any sample was taken.
    pub fn domain(&self, name: &str) -> Option<&DomainSeries> {
        self.domains.get(name)
    }

    /// Latest CPU fraction (0–1) for a domain.
    pub fn cpu_frac(&self, name: &str) -> f64 {
        self.domain(name).map_or(0.0, |d| d.now.cpu_pct / 100.0)
    }

    /// Sum of host RSS (KiB) per running domain, largest first.
    pub fn rss_by_domain(&self) -> Vec<(String, u64)> {
        let mut v: Vec<(String, u64)> = self
            .domains
            .iter()
            .map(|(n, s)| (n.clone(), s.now.rss_kib))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }
}

/// Scale a series into 0–1 for charts. `floor` is the minimum full-scale value,
/// so near-idle traffic does not fill the chart.
pub fn normalize(values: &[f32], floor: f32) -> Vec<f64> {
    let max = values.iter().copied().fold(floor, f32::max);
    values
        .iter()
        .map(|v| f64::from(*v / max).clamp(0.0, 1.0))
        .collect()
}

/// Left-pad a history with zeros so charts fill from the right ("now") edge.
/// Longer histories are kept whole (the chart resamples them).
pub fn padded(values: &[f64], len: usize) -> Vec<f64> {
    if values.len() >= len {
        return values.to_vec();
    }
    let mut out = vec![0.0; len - values.len()];
    out.extend_from_slice(values);
    out
}

/// Human byte rate (`12.4 MiB/s`).
pub fn fmt_bytes_rate(bps: f64) -> String {
    format!("{}/s", fmt_bytes(bps))
}

/// Human byte size (`18.4 GiB`).
pub fn fmt_bytes(b: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = b.max(0.0);
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Human bit rate from bytes/s (`48.2 Mb/s`).
pub fn fmt_bits_rate(bps: f64) -> String {
    let bits = bps.max(0.0) * 8.0;
    if bits >= 1e9 {
        format!("{:.1} Gb/s", bits / 1e9)
    } else if bits >= 1e6 {
        format!("{:.1} Mb/s", bits / 1e6)
    } else if bits >= 1e3 {
        format!("{:.1} kb/s", bits / 1e3)
    } else {
        format!("{bits:.0} b/s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn stats(name: &str, cpu_ns: u64, rd: u64, rx: u64) -> RawDomainStats {
        let mut s = RawDomainStats {
            domain: name.to_string(),
            values: HashMap::new(),
        };
        for (k, v) in [
            ("state.state", "1".to_string()),
            ("cpu.time", cpu_ns.to_string()),
            ("vcpu.current", "2".to_string()),
            ("balloon.current", "4194304".to_string()),
            ("balloon.available", "4000000".to_string()),
            ("balloon.unused", "1000000".to_string()),
            ("block.count", "1".to_string()),
            ("block.0.name", "vda".to_string()),
            ("block.0.rd.bytes", rd.to_string()),
            ("block.0.wr.bytes", "0".to_string()),
            ("net.count", "1".to_string()),
            ("net.0.name", "vnet0".to_string()),
            ("net.0.rx.bytes", rx.to_string()),
            ("net.0.tx.bytes", "0".to_string()),
        ] {
            s.values.insert(k.to_string(), v);
        }
        s
    }

    #[test]
    fn computes_rates_from_two_samples() {
        let mut m = MetricsStore::default();
        let t0 = Instant::now();
        m.ingest_domains(&[stats("a", 0, 0, 0)], t0);
        assert!(m.domain("a").unwrap().cpu.is_empty(), "first sample has no delta");
        // 1 s later: 1 s of CPU time over 2 vCPUs = 50 %.
        m.ingest_domains(
            &[stats("a", 1_000_000_000, 2048, 1000)],
            t0 + Duration::from_secs(1),
        );
        let d = m.domain("a").unwrap();
        assert!((d.now.cpu_pct - 50.0).abs() < 0.01);
        assert!((d.now.rd_bps - 2048.0).abs() < 0.01);
        assert!((d.now.rx_bps - 1000.0).abs() < 0.01);
        assert_eq!(d.now.mem_used_kib, 3_000_000);
        assert_eq!(d.now.first_disk, "vda");
        assert_eq!(d.cpu.len(), 1);
    }

    #[test]
    fn drops_stopped_domains_and_resets() {
        let mut m = MetricsStore::default();
        let t0 = Instant::now();
        m.ingest_domains(&[stats("a", 5_000, 0, 0)], t0);
        m.ingest_domains(&[], t0 + Duration::from_secs(1));
        assert!(m.domain("a").is_none());
        m.ingest_domains(&[stats("a", 9_000_000_000, 0, 0)], t0 + Duration::from_secs(2));
        m.ingest_domains(&[stats("a", 100, 0, 0)], t0 + Duration::from_secs(3));
        assert!(m.domain("a").unwrap().cpu.is_empty(), "counter reset is skipped");
    }

    #[test]
    fn per_vcpu_disk_and_nic_breakdown() {
        let mut m = MetricsStore::default();
        let t0 = Instant::now();
        let mut a = stats("a", 0, 0, 0);
        a.values.insert("vcpu.maximum".into(), "2".into());
        a.values.insert("vcpu.0.time".into(), "0".into());
        a.values.insert("vcpu.1.time".into(), "0".into());
        m.ingest_domains(std::slice::from_ref(&a), t0);
        let mut b = stats("a", 1_000_000_000, 4096, 2000);
        b.values.insert("vcpu.maximum".into(), "2".into());
        b.values.insert("vcpu.0.time".into(), "750000000".into());
        b.values.insert("vcpu.1.time".into(), "250000000".into());
        b.values.insert("state.state".into(), "2".into()); // blocked still counts as running
        m.ingest_domains(&[b], t0 + Duration::from_secs(1));
        let n = &m.domain("a").unwrap().now;
        assert_eq!(n.vcpu_frac, vec![0.75, 0.25]);
        assert_eq!(n.disks[0].name, "vda");
        assert!((n.disks[0].rd_bps - 4096.0).abs() < 0.01);
        assert_eq!(n.nics[0].name, "vnet0");
        assert!((n.nics[0].rx_bps - 2000.0).abs() < 0.01);
    }

    #[test]
    fn host_cpu_from_proc_stat() {
        let mut m = MetricsStore::default();
        m.ingest_proc_stat("cpu  0 0 0 0\ncpu0 100 0 0 100\ncpu1 0 0 0 200\n");
        m.ingest_proc_stat("cpu  0 0 0 0\ncpu0 200 0 0 100\ncpu1 0 0 0 300\n");
        assert_eq!(m.host.threads, vec![1.0, 0.0]);
        assert_eq!(m.host.cpu.last(), Some(50.0));
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_bits_rate(6_025_000.0), "48.2 Mb/s");
        assert_eq!(fmt_bytes(1536.0), "1.5 KiB");
        assert_eq!(padded(&[1.0], 3), vec![0.0, 0.0, 1.0]);
        assert_eq!(normalize(&[1.0, 2.0], 4.0), vec![0.25, 0.5]);
    }
}
