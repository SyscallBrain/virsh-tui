//! Background live poller.
//!
//! All periodic `virsh` calls run here, off the UI loop, so rendering and key
//! handling never wait on a subprocess. Results arrive as `LiveMsg`s and are
//! applied to the state in one cheap, synchronous step.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::backend::{Backend, RawDomainStats, virsh::VirshBackend};
use crate::model::{DomainConfig, DomainSummary, PoolSummary};

/// Data produced by the poller.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // one message per second; boxing buys nothing
pub enum LiveMsg {
    /// One refresh: domains + raw stats (+ local host data).
    Sample {
        summaries: Vec<DomainSummary>,
        stats: Vec<RawDomainStats>,
        proc_stat: Option<String>,
        uptimes: HashMap<String, String>,
        /// rx/tx byte counters of host bridges (local URIs).
        ifaces: HashMap<String, (u64, u64)>,
        at: Instant,
    },
    /// The refresh failed (connection lost); the poller backs off.
    Failed(String),
    Pools(Vec<PoolSummary>),
    Config(String, DomainConfig),
    Ip(String, String),
}

/// Requests from the UI.
#[derive(Debug)]
pub enum Request {
    /// The selected domain changed (config/IP are fetched for it).
    Selected { name: String, running: bool },
    /// Re-read a domain config (after an edit).
    RefetchConfig(String),
    /// Refresh now (after a mutation) instead of waiting for the next tick.
    Now,
}

/// Handle to a running poller; dropping it stops the task.
pub struct Poller {
    pub rx: mpsc::Receiver<LiveMsg>,
    pub tx: mpsc::UnboundedSender<Request>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// True for URIs that talk to the local hypervisor (`qemu:///system`), where
/// `/proc` and `/run/libvirt` describe the same host.
pub fn is_local_uri(uri: &str) -> bool {
    uri.split_once("://")
        .is_some_and(|(_, rest)| rest.starts_with('/'))
}

/// First IPv4 address (without prefix) from `virsh domifaddr` output.
pub fn parse_domifaddr_ipv4(out: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let cols: Vec<&str> = l.split_whitespace().collect();
        let i = cols.iter().position(|c| *c == "ipv4")?;
        cols.get(i + 1)
            .map(|a| a.split('/').next().unwrap_or(a).to_string())
    })
}

/// Start polling `uri` every `interval_secs`.
///
/// With `balloon_period` (Settings › Monitoring › guest memory stats, opt-in),
/// running guests without balloon statistics get `dommemstat --period N --live`
/// once, so their used memory can be shown.
pub fn spawn(uri: &str, interval_secs: u64, balloon_period: Option<u64>) -> Poller {
    let (out_tx, rx) = mpsc::channel(64);
    let (tx, mut req_rx) = mpsc::unbounded_channel();
    let uri = uri.to_string();
    let task = tokio::spawn(async move {
        let backend = VirshBackend::new(&uri);
        let local = is_local_uri(&uri);
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs.max(1)));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut tick: u64 = 0;
        let mut failures: u32 = 0;
        let mut next_try = Instant::now();
        let mut selected: Option<(String, bool)> = None;
        let mut known: HashSet<String> = HashSet::new();
        let mut ip_due = true;
        let mut ballooned: HashSet<String> = HashSet::new();
        loop {
            let mut sample_now = false;
            tokio::select! {
                _ = ticker.tick() => sample_now = true,
                req = req_rx.recv() => match req {
                    None => break,
                    Some(Request::Selected { name, running }) => {
                        ip_due = selected.as_ref().is_none_or(|(n, _)| *n != name);
                        selected = Some((name, running));
                    }
                    Some(Request::RefetchConfig(name)) => {
                        known.remove(&name);
                    }
                    Some(Request::Now) => sample_now = true,
                },
            }
            if !sample_now || Instant::now() < next_try {
                // Still serve a pending config fetch for a new selection.
                if let Some((name, _)) = &selected
                    && !known.contains(name)
                    && let Some(msg) = fetch_config(&uri, name).await
                {
                    known.insert(name.clone());
                    if out_tx.send(msg).await.is_err() {
                        break;
                    }
                }
                continue;
            }
            tick += 1;
            let msg = match backend.domains_with_stats().await {
                Ok((summaries, stats)) => {
                    failures = 0;
                    if let Some(period) = balloon_period {
                        for s in stats.iter().filter(|s| s.u64("state.state") == Some(1)) {
                            if s.values.contains_key("balloon.available")
                                || !ballooned.insert(s.domain.clone())
                            {
                                continue;
                            }
                            let period = period.to_string();
                            if let Err(e) = crate::backend::virsh::exec::run(
                                &uri,
                                &["dommemstat", &s.domain, "--period", &period, "--live"],
                            )
                            .await
                            {
                                tracing::info!("dommemstat {}: {e}", s.domain);
                            }
                        }
                    }
                    let uptimes = if local {
                        summaries
                            .iter()
                            .filter(|s| s.state == crate::model::DomainState::Running)
                            .filter_map(|s| {
                                crate::backend::virsh::host_local::domain_uptime(&s.name)
                                    .map(|u| (s.name.clone(), u))
                            })
                            .collect()
                    } else {
                        HashMap::new()
                    };
                    LiveMsg::Sample {
                        summaries,
                        stats,
                        proc_stat: if local {
                            std::fs::read_to_string("/proc/stat").ok()
                        } else {
                            None
                        },
                        uptimes,
                        ifaces: if local { bridge_counters() } else { HashMap::new() },
                        at: Instant::now(),
                    }
                }
                Err(e) => {
                    failures += 1;
                    next_try = Instant::now() + Duration::from_secs(super::backoff::delay_secs(failures));
                    LiveMsg::Failed(e.to_string().lines().next().unwrap_or("").to_string())
                }
            };
            let failed = matches!(msg, LiveMsg::Failed(_));
            if out_tx.send(msg).await.is_err() {
                break;
            }
            if failed {
                continue;
            }
            if (tick == 1 || tick.is_multiple_of(30))
                && let Ok(pools) = backend.pools().await
                && out_tx.send(LiveMsg::Pools(pools)).await.is_err()
            {
                break;
            }
            if let Some((name, running)) = selected.clone() {
                if !known.contains(&name)
                    && let Some(msg) = fetch_config(&uri, &name).await
                {
                    known.insert(name.clone());
                    if out_tx.send(msg).await.is_err() {
                        break;
                    }
                }
                if running && (ip_due || tick.is_multiple_of(10)) {
                    ip_due = false;
                    let ip = fetch_ip(&uri, &name).await;
                    if out_tx.send(LiveMsg::Ip(name, ip)).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    Poller { rx, tx, task }
}

/// rx/tx byte counters of every bridge in /sys/class/net (virbrN, br0, …),
/// seen from the guests' side: bridge tx = guests' download (↓).
pub fn bridge_counters() -> HashMap<String, (u64, u64)> {
    let mut out = HashMap::new();
    let Ok(dir) = std::fs::read_dir("/sys/class/net") else {
        return out;
    };
    for e in dir.flatten() {
        let base = e.path();
        if !base.join("bridge").exists() {
            continue;
        }
        let read = |f: &str| {
            std::fs::read_to_string(base.join("statistics").join(f))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
        };
        if let (Some(down), Some(up)) = (read("tx_bytes"), read("rx_bytes")) {
            out.insert(e.file_name().to_string_lossy().into_owned(), (down, up));
        }
    }
    out
}

async fn fetch_config(uri: &str, name: &str) -> Option<LiveMsg> {
    let xml = crate::backend::virsh::exec::run(uri, &["dumpxml", "--inactive", name])
        .await
        .ok()?;
    let cfg = crate::backend::virsh::parse_xml::parse_domain_xml(&xml).ok()?;
    Some(LiveMsg::Config(name.to_string(), cfg))
}

async fn fetch_ip(uri: &str, name: &str) -> String {
    for source in ["lease", "agent"] {
        if let Ok(out) = crate::backend::virsh::exec::run(uri, &["domifaddr", name, "--source", source]).await
            && let Some(ip) = parse_domifaddr_ipv4(&out)
        {
            return ip;
        }
    }
    String::from("—")
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_uris() {
        assert!(super::is_local_uri("qemu:///system"));
        assert!(super::is_local_uri("qemu:///session"));
        assert!(!super::is_local_uri("qemu+ssh://root@host/system"));
    }

    #[test]
    fn domifaddr() {
        let out = " Name       MAC address          Protocol     Address\n-------------------------------------------------------------------------------\n vnet0      52:54:00:a3:1f:7c    ipv4         192.168.122.48/24\n";
        assert_eq!(
            super::parse_domifaddr_ipv4(out).as_deref(),
            Some("192.168.122.48")
        );
    }
}
