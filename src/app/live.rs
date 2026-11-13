//! Live data: initial view fills and applying poller messages.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Build live host state: local /proc data plus virsh pools/networks/domains.
pub(super) async fn fill_live_host(uri: &str) -> ui::views::host::HostState {
    use crate::backend::{Backend, virsh::VirshBackend};
    let mut host = if is_local_uri(uri) {
        ui::views::host::HostState::live_local().await
    } else {
        // Remote: /proc is the wrong machine; libvirt reports the basics.
        let nodeinfo = crate::backend::virsh::exec::run(uri, &["nodeinfo"])
            .await
            .unwrap_or_default();
        let memstats = crate::backend::virsh::exec::run(uri, &["nodememstats"])
            .await
            .unwrap_or_default();
        let (model, threads, total_kib, available_kib) =
            crate::backend::virsh::host_local::parse_node_facts(&nodeinfo, &memstats);
        let gib = |kib: u64| kib as f64 / 1024.0 / 1024.0;
        ui::views::host::HostState {
            live: true,
            cpu_hist: Some(Vec::new()),
            cpu_model: if model.is_empty() {
                String::from("unavailable")
            } else {
                model
            },
            threads_total: threads,
            mem_total_gib: gib(total_kib),
            mem_free_gib: gib(available_kib),
            mem_used_gib: gib(total_kib.saturating_sub(available_kib)),
            message: String::from("Remote host: per-thread CPU metrics are unavailable"),
            ..Default::default()
        }
    };
    host.uri = uri.to_string();
    host.connected = true;
    host.system = host_system_facts(uri).await;
    let backend = VirshBackend::new(uri);
    if let Ok(pools) = backend.pools().await {
        host.pools_active = format!(
            "{}/{} active",
            pools.iter().filter(|p| p.state == "running").count(),
            pools.len()
        );
        host.pools = pools
            .into_iter()
            .map(|p| {
                let frac = if p.capacity_mib == 0 {
                    0.0
                } else {
                    p.allocation_mib as f64 / p.capacity_mib as f64
                };
                ui::views::host::HostPoolRow {
                    name: p.name,
                    frac,
                    value: format!("{}/{}G", p.allocation_mib / 1024, p.capacity_mib / 1024),
                }
            })
            .collect();
    }
    if let Ok(nets) = backend.networks().await {
        host.nets_active = format!(
            "{}/{} active",
            nets.iter().filter(|n| n.state == "active").count(),
            nets.len()
        );
        let mut rows = Vec::new();
        for n in nets {
            let cfg = crate::backend::virsh::exec::run(uri, &["net-dumpxml", &n.name])
                .await
                .ok()
                .and_then(|x| crate::backend::virsh::parse_xml::parse_network_xml(&x).ok())
                .unwrap_or_default();
            rows.push(ui::views::host::HostNetRow {
                name: n.name,
                mode: cfg.forward_mode,
                active: n.state == "active",
                seed: 41,
                bridge: cfg.bridge,
                spark: Some(Vec::new()),
            });
        }
        host.nets = rows;
    }
    if let Ok(domains) = backend.list_domains().await {
        let running: Vec<_> = domains
            .iter()
            .filter(|d| d.state == crate::model::DomainState::Running)
            .collect();
        host.vcpu_total = running.iter().map(|d| d.vcpus).sum();
        host.domains = running
            .into_iter()
            .map(|d| ui::views::host::HostDomainRow {
                name: d.name.clone(),
                vcpus: d.vcpus,
                cpu_frac: 0.0,
                mem_label: format!("{:.1}G", d.mem_kib as f64 / 1024.0 / 1024.0),
                mem_frac: 0.0,
                rd: String::from("0.0M"),
                wr: String::from("0.0M"),
                rx: String::from("0.0M"),
                tx: String::from("0.0M"),
                seed: 7,
                cpu_hist: Some(Vec::new()),
                mem_hist: Some(Vec::new()),
            })
            .collect();
    }
    host
}

/// Apply one poller message to the live state (cheap, synchronous).
pub(super) fn apply_live_msg(uri: &str, dash: &mut ui::views::dashboard::DashboardState, msg: LiveMsg) {
    match msg {
        LiveMsg::Sample {
            summaries,
            stats,
            proc_stat,
            uptimes,
            ifaces,
            at,
        } => {
            // Keep the cursor on the same domain: rows are re-sorted below
            // (CPU, uptime) after the summaries replaced them.
            let selected = dash.selected().name.clone();
            dash.metrics.ingest_ifaces(&ifaces, at);
            dash.apply_summaries(&summaries);
            for r in dash.rows.iter_mut() {
                match uptimes.get(&r.name) {
                    Some(&secs) => {
                        r.uptime_secs = Some(secs);
                        r.uptime = crate::backend::virsh::host_local::fmt_uptime(secs);
                    }
                    None if r.state != crate::model::DomainState::Running => {
                        r.uptime_secs = None;
                        r.uptime = String::from("—");
                    }
                    None => {}
                }
            }
            dash.metrics.ingest_domains(&stats, at);
            if let Some(stat) = proc_stat {
                dash.metrics.ingest_proc_stat(&stat);
            }
            dash.apply_metrics();
            dash.select_name(&selected);
            dash.check_shutdowns();
            dash.uri = uri.to_string();
            if !dash.connected || dash.message == "loading…" {
                dash.message.clear();
            }
            dash.connected = true;
            if is_local_uri(uri) {
                update_host_gauges(dash);
            }
        }
        LiveMsg::Failed(e) => {
            dash.connected = false;
            dash.message = format!("✗ lost connection to {uri} — retrying… ({e})");
        }
        LiveMsg::Pools(pools) => {
            let frac = |used: u64, total: u64| {
                if total == 0 {
                    0.0
                } else {
                    used as f64 / total as f64
                }
            };
            let mut list: Vec<_> = pools.into_iter().filter(|p| p.capacity_mib > 0).collect();
            list.sort_by_key(|a| std::cmp::Reverse(a.capacity_mib));
            dash.host_pools = list
                .iter()
                .take(2)
                .map(|p| {
                    (
                        format!("pool:{}", p.name),
                        frac(p.allocation_mib, p.capacity_mib),
                        format!("{}/{}G", p.allocation_mib / 1024, p.capacity_mib / 1024),
                    )
                })
                .collect();
        }
        LiveMsg::Config(name, cfg) => {
            dash.configs.insert(name, cfg);
        }
        LiveMsg::Ip(name, ip) => {
            dash.ips.insert(name, ip);
        }
    }
}

/// Host panel gauges from /proc plus the cached pool gauges.
pub(super) fn update_host_gauges(dash: &mut ui::views::dashboard::DashboardState) {
    use crate::backend::virsh::host_local as hl;
    let mem = hl::parse_meminfo(&std::fs::read_to_string("/proc/meminfo").unwrap_or_default());
    let gib = |kib: u64| kib as f64 / 1024.0 / 1024.0;
    let frac = |used: u64, total: u64| {
        if total == 0 {
            0.0
        } else {
            used as f64 / total as f64
        }
    };
    let cpu = f64::from(dash.metrics.host.cpu.last().unwrap_or(0.0)) / 100.0;
    let used = mem.total_kib.saturating_sub(mem.available_kib);
    let swap_used = mem.swap_total_kib.saturating_sub(mem.swap_free_kib);
    if dash.host_title.is_empty() && !dash.metrics.host.threads.is_empty() {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        dash.host_title = format!(
            "{} · {}t · {:.0} GiB",
            ui::views::host::short_model(&hl::parse_cpu_model(&cpuinfo)),
            dash.metrics.host.threads.len(),
            gib(mem.total_kib)
        );
    }
    let huge_used = mem.hugepages_total.saturating_sub(mem.hugepages_free);
    let mut g = vec![
        (String::from("CPU"), cpu, format!("{:.0}%", cpu * 100.0)),
        (
            String::from("RAM"),
            frac(used, mem.total_kib),
            format!("{:.1}/{:.0}G", gib(used), gib(mem.total_kib)),
        ),
        (
            String::from("Swap"),
            frac(swap_used, mem.swap_total_kib),
            format!("{:.1}/{:.0}G", gib(swap_used), gib(mem.swap_total_kib)),
        ),
        (
            String::from("Huge"),
            frac(huge_used, mem.hugepages_total),
            format!("{huge_used}/{}", mem.hugepages_total),
        ),
    ];
    // Row-major across two columns: CPU, Pool1, RAM, Pool2, Swap, Huge.
    let mut it = dash.host_pools.clone().into_iter();
    if let Some(p) = it.next() {
        g.insert(1, p);
    }
    if let Some(p) = it.next() {
        g.insert(3, p);
    }
    dash.host_gauges = g;
}

/// A heavy view state loaded in the background, tagged with its URI.
pub(super) enum Loaded {
    Host(String, u64, u64, Box<ui::views::host::HostState>),
    Nets(String, u64, u64, Box<ui::views::networks::NetworksState>),
    Store(String, u64, u64, Box<ui::views::storage::StorageState>),
    Failed(View, String, u64, u64),
}

/// Load Host/Networks/Storage without blocking the UI (dozens of virsh calls).
pub(super) fn spawn_load(
    uri: &str,
    view: View,
    epoch: u64,
    serial: u64,
    tx: &tokio::sync::mpsc::UnboundedSender<Loaded>,
) -> Option<tokio::task::JoinHandle<()>> {
    if !matches!(view, View::Host | View::Networks | View::Storage) {
        return None;
    }
    let uri = uri.to_string();
    let child_uri = uri.clone();
    let load = async move {
        match view {
            View::Host => Loaded::Host(
                child_uri.clone(),
                epoch,
                serial,
                Box::new(fill_live_host(&child_uri).await),
            ),
            View::Networks => Loaded::Nets(
                child_uri.clone(),
                epoch,
                serial,
                Box::new(fill_live_nets(&child_uri).await),
            ),
            _ => Loaded::Store(
                child_uri.clone(),
                epoch,
                serial,
                Box::new(fill_live_store(&child_uri).await),
            ),
        }
    };
    Some(supervise_view_load(view, uri, epoch, serial, tx.clone(), load))
}

pub(super) fn supervise_view_load(
    view: View,
    uri: String,
    epoch: u64,
    serial: u64,
    tx: tokio::sync::mpsc::UnboundedSender<Loaded>,
    load: impl std::future::Future<Output = Loaded> + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(load);
        let msg = match tasks.join_next().await {
            Some(Ok(loaded)) => loaded,
            _ => Loaded::Failed(view, uri, epoch, serial),
        };
        let _ = tx.send(msg);
    })
}

/// Build live networks state: definitions, host interfaces, and the
/// selected network's leases/attachments.
pub(super) async fn fill_live_nets(uri: &str) -> ui::views::networks::NetworksState {
    use crate::backend::{Backend, virsh::VirshBackend};
    use crate::ui::views::networks::{NetRow, NetworksState};
    let backend = VirshBackend::new(uri);
    let mut st = NetworksState::demo();
    st.uri = uri.to_string();
    st.connected = true;
    st.networks.clear();
    st.leases.clear();
    st.attached.clear();
    st.live = true;
    st.traffic = None;
    st.message.clear();
    st.ifaces = if is_local_uri(uri) {
        read_ifaces().await
    } else {
        Vec::new()
    };
    match backend.networks().await {
        Ok(nets) => {
            for n in &nets {
                let cfg = crate::backend::virsh::exec::run(uri, &["net-dumpxml", &n.name])
                    .await
                    .ok()
                    .and_then(|x| crate::backend::virsh::parse_xml::parse_network_xml(&x).ok())
                    .unwrap_or_default();
                st.networks.push(NetRow {
                    name: n.name.clone(),
                    mode: cfg.forward_mode.clone(),
                    bridge: if cfg.bridge.is_empty() {
                        String::from("—")
                    } else {
                        cfg.bridge.clone()
                    },
                    subnet: cfg.ipv4.as_deref().map_or_else(|| String::from("—"), network_of),
                    active: n.state == "active",
                    autostart: n.autostart,
                    cfg,
                });
            }
        }
        Err(e) => st.message = format!("✗ {}", first_line(&e.to_string())),
    }
    st.selected = 0;
    st.domain_ifaces = Some(domain_interfaces(uri).await);
    load_net_selection(uri, &mut st).await;
    st
}

/// `domiflist` of every domain (bounded concurrency), read once per load.
async fn domain_interfaces(uri: &str) -> Vec<ui::views::networks::DomainIface> {
    use crate::backend::{Backend, virsh::VirshBackend};
    let Ok(domains) = VirshBackend::new(uri).list_domains().await else {
        return vec![];
    };
    let outputs = run_many(
        uri,
        domains
            .iter()
            .map(|d| vec![String::from("domiflist"), d.name.clone()])
            .collect(),
    )
    .await;
    let mut out = Vec::new();
    for (d, result) in domains.iter().zip(outputs) {
        let Ok(text) = result else { continue };
        for (iface, kind, source, mac) in parse_domiflist(&text) {
            out.push((d.name.clone(), iface, kind, source, mac));
        }
    }
    out
}

/// Leases (with static hosts and owning domains) and attached interfaces of
/// the selected network.
pub(super) async fn load_net_selection(uri: &str, st: &mut ui::views::networks::NetworksState) {
    use crate::backend::{Backend, virsh::VirshBackend};
    use crate::ui::views::networks::LeaseRow;
    st.leases.clear();
    st.attached.clear();
    st.lease_selected = 0;
    let Some(net) = st.networks.get(st.selected).cloned() else {
        return;
    };
    // MAC → domain and the interfaces plugged into this network, from the
    // per-load cache (no `domiflist` per domain on every selection change).
    let mut by_mac: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let backend = VirshBackend::new(uri);
    if st.domain_ifaces.is_none() {
        st.domain_ifaces = Some(domain_interfaces(uri).await);
    }
    for (domain, iface, kind, source, mac) in st.domain_ifaces.iter().flatten() {
        by_mac.insert(mac.to_lowercase(), domain.clone());
        let on_net =
            (kind == "network" && *source == net.name) || (kind == "bridge" && *source == net.cfg.bridge);
        if on_net && iface != "-" {
            st.attached.push((iface.clone(), domain.clone()));
        }
    }
    let statics = &net.cfg.static_hosts;
    if net.active
        && let Ok(leases) = backend.dhcp_leases(&net.name).await
    {
        for l in leases {
            let statik = statics.iter().any(|(mac, _, _)| mac.eq_ignore_ascii_case(&l.mac));
            st.leases.push(LeaseRow {
                domain: by_mac.get(&l.mac.to_lowercase()).cloned().unwrap_or_default(),
                hostname: l.hostname.clone().unwrap_or_else(|| String::from("—")),
                expires: if statik {
                    String::from("static")
                } else {
                    l.expiry.chars().take(16).collect()
                },
                mac: l.mac,
                ip: l.ip,
                statik,
            });
        }
    }
    // Static hosts without a current lease are still listed.
    for (mac, name, ip) in statics {
        if !st.leases.iter().any(|l| l.mac.eq_ignore_ascii_case(mac)) {
            st.leases.push(LeaseRow {
                mac: mac.clone(),
                ip: ip.clone(),
                hostname: if name.is_empty() {
                    String::from("—")
                } else {
                    name.clone()
                },
                expires: String::from("static"),
                statik: true,
                domain: by_mac.get(&mac.to_lowercase()).cloned().unwrap_or_default(),
            });
        }
    }
    st.leases
        .sort_by(|a, b| b.statik.cmp(&a.statik).then(a.ip.cmp(&b.ip)));
    if st.message.is_empty() || st.message.starts_with("── ") {
        st.message = format!("── virsh net-dhcp-leases {}", net.name);
    }
}

/// `virsh domiflist` rows: (interface, type, source, mac).
pub(super) fn parse_domiflist(out: &str) -> Vec<(String, String, String, String)> {
    crate::backend::virsh::parse_list::table_rows(out)
        .into_iter()
        .filter(|c| c.len() >= 5)
        .map(|c| (c[0].clone(), c[1].clone(), c[2].clone(), c[4].clone()))
        .collect()
}

/// Network address of a host CIDR (`192.168.122.1/24` → `192.168.122.0/24`).
pub(super) fn network_of(cidr: &str) -> String {
    let Some((addr, prefix)) = cidr.split_once('/') else {
        return cidr.to_string();
    };
    match (addr.parse::<std::net::Ipv4Addr>(), prefix.parse::<u32>()) {
        (Ok(ip), Ok(p)) if p <= 32 => {
            let mask = if p == 0 { 0 } else { u32::MAX << (32 - p) };
            format!("{}/{p}", std::net::Ipv4Addr::from(u32::from(ip) & mask))
        }
        _ => cidr.to_string(),
    }
}

/// IPv4 addresses per interface from `ip -j -4 addr`.
async fn iface_ipv4s() -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let step = crate::command::plan::CommandStep::new("ip", vec!["-j", "-4", "addr"]);
    let (_sender, mut control) = tokio::sync::watch::channel(false);
    let Ok(o) = crate::command::process::run_step("", &step, &mut control).await else {
        return out;
    };
    if !o.status.success() {
        return out;
    }
    let Ok(bytes) = o.structured_stdout() else {
        return out;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return out;
    };
    for iface in v.as_array().into_iter().flatten() {
        let name = iface["ifname"].as_str().unwrap_or_default();
        if let Some(a) = iface["addr_info"].as_array().and_then(|a| a.first()) {
            out.insert(
                name.to_string(),
                format!(
                    "{}/{}",
                    a["local"].as_str().unwrap_or("?"),
                    a["prefixlen"].as_u64().unwrap_or(0)
                ),
            );
        }
    }
    out
}

/// Read host interfaces from /sys/class/net.
pub(super) async fn read_ifaces() -> Vec<crate::ui::views::networks::IfaceRow> {
    let ips = iface_ipv4s().await;
    use crate::ui::views::networks::IfaceRow;
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/sys/class/net") else {
        return out;
    };
    let mut names: Vec<String> = dir
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "lo")
        .collect();
    names.sort();
    for name in names {
        let base = format!("/sys/class/net/{name}");
        let oper = std::fs::read_to_string(format!("{base}/operstate"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let speed = std::fs::read_to_string(format!("{base}/speed"))
            .ok()
            .map(|s| {
                let mb: u64 = s.trim().parse().unwrap_or(0);
                if mb >= 1000 {
                    format!("{:.1}G", mb as f64 / 1000.0)
                } else if mb > 0 {
                    format!("{mb}M")
                } else {
                    String::from("—")
                }
            })
            .unwrap_or_else(|| String::from("—"));
        let mac = std::fs::read_to_string(format!("{base}/address"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let kind = if std::fs::metadata(format!("{base}/bridge")).is_ok() {
            "bridge"
        } else if std::fs::metadata(format!("{base}/wireless")).is_ok() {
            "wireless"
        } else {
            "ethernet"
        };
        out.push(IfaceRow {
            name: name.clone(),
            kind: kind.to_string(),
            speed,
            // IPv4 when configured, otherwise the MAC; "down" for links that are down.
            addr: ips.get(name.as_str()).cloned().unwrap_or(if oper == "up" {
                mac
            } else {
                String::from("down")
            }),
            up: oper == "up",
        });
    }
    out
}

/// Bounded concurrency for per-object virsh reads (N domains, N volumes).
const READ_CONCURRENCY: usize = 6;

/// Run `virsh` reads with bounded concurrency, keeping the input order.
pub(super) async fn run_many(uri: &str, commands: Vec<Vec<String>>) -> Vec<color_eyre::Result<String>> {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(READ_CONCURRENCY));
    let mut set = tokio::task::JoinSet::new();
    let count = commands.len();
    for (i, argv) in commands.into_iter().enumerate() {
        let uri = uri.to_string();
        let semaphore = semaphore.clone();
        set.spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            let args: Vec<&str> = argv.iter().map(String::as_str).collect();
            (i, crate::backend::virsh::exec::run(&uri, &args).await)
        });
    }
    let mut out: Vec<Option<color_eyre::Result<String>>> = (0..count).map(|_| None).collect();
    while let Some(joined) = set.join_next().await {
        if let Ok((i, result)) = joined {
            out[i] = Some(result);
        }
    }
    out.into_iter()
        .map(|r| r.unwrap_or_else(|| Err(color_eyre::eyre::eyre!("read task failed"))))
        .collect()
}

/// Build live storage state: pools, every domain's disk references (inactive
/// and live XML), the volumes of every active pool with backing references
/// counted across pools, and the ISO library.
pub(super) async fn fill_live_store(uri: &str) -> ui::views::storage::StorageState {
    use crate::backend::{Backend, virsh::VirshBackend};
    use crate::ui::views::storage::{IsoEntry, PoolRow, StorageState, VolumeRow};
    let backend = VirshBackend::new(uri);
    let mut st = StorageState::demo();
    st.uri = uri.to_string();
    st.connected = true;
    st.pools.clear();
    st.volumes.clear();
    st.isos.clear();
    st.message.clear();
    st.attachments.clear();
    st.pool_volumes.clear();
    st.incomplete = false;
    match backend.list_domains().await {
        Ok(domains) => {
            let mut commands = Vec::new();
            let mut owners = Vec::new();
            for d in &domains {
                commands.push(vec![
                    String::from("dumpxml"),
                    String::from("--inactive"),
                    d.name.clone(),
                ]);
                owners.push(d.name.clone());
                if matches!(
                    d.state,
                    crate::model::DomainState::Running | crate::model::DomainState::Paused
                ) {
                    commands.push(vec![String::from("dumpxml"), d.name.clone()]);
                    owners.push(d.name.clone());
                }
            }
            for (owner, result) in owners.into_iter().zip(run_many(uri, commands).await) {
                match result {
                    Ok(xml) => {
                        for key in disk_sources(&xml) {
                            if !st.attachments.iter().any(|(d, k)| *d == owner && *k == key) {
                                st.attachments.push((owner.clone(), key));
                            }
                        }
                    }
                    Err(_) => st.incomplete = true,
                }
            }
        }
        Err(_) => st.incomplete = true,
    }
    let pools = match backend.pools().await {
        Ok(pools) => pools,
        Err(e) => {
            st.message = format!("✗ {}", first_line(&e.to_string()));
            return st;
        }
    };
    let pool_xml = run_many(
        uri,
        pools
            .iter()
            .map(|p| vec![String::from("pool-dumpxml"), p.name.clone()])
            .collect(),
    )
    .await;
    // (pool, name, path, format, capacity, allocation, backing)
    type VolumeFacts = (String, String, String, String, u64, u64, Option<String>);
    let mut volumes: Vec<VolumeFacts> = Vec::new();
    for (p, xml) in pools.iter().zip(pool_xml) {
        let xml = xml.unwrap_or_default();
        let frac = if p.capacity_mib == 0 {
            0.0
        } else {
            p.allocation_mib as f64 / p.capacity_mib as f64
        };
        st.pools.push(PoolRow {
            name: p.name.clone(),
            kind: pool_kind(&xml),
            path: pool_target(&xml),
            frac,
            value: format!(
                "{} / {}",
                human_size(p.allocation_mib as f64, "MiB"),
                human_size(p.capacity_mib as f64, "MiB")
            ),
            active: p.state == "running",
            autostart: p.autostart,
        });
        if p.state != "running" {
            continue;
        }
        let Ok(out) = crate::backend::virsh::exec::run(uri, &["vol-list", "--pool", &p.name]).await else {
            st.incomplete = true;
            continue;
        };
        let listed = parse_vol_list(&out);
        let details = run_many(
            uri,
            listed
                .iter()
                .map(|(name, _)| {
                    vec![
                        String::from("vol-dumpxml"),
                        String::from("--pool"),
                        p.name.clone(),
                        name.clone(),
                    ]
                })
                .collect(),
        )
        .await;
        for ((name, path), xml) in listed.into_iter().zip(details) {
            let parsed = xml
                .ok()
                .and_then(|xml| crate::backend::virsh::parse_xml::parse_volume_xml(&xml).ok());
            if parsed.is_none() {
                st.incomplete = true;
            }
            let (format, cap, alloc, backing) = parsed.unwrap_or((String::from("—"), 0, 0, None));
            if name.to_lowercase().ends_with(".iso") {
                st.isos.push(IsoEntry {
                    pool: p.name.clone(),
                    name: name.clone(),
                    path: path.clone(),
                    size: human_size(cap as f64, "B"),
                    date: String::new(),
                });
            }
            volumes.push((p.name.clone(), name, path, format, cap, alloc, backing));
        }
    }
    let backing_count = |path: &str| volumes.iter().filter(|v| v.6.as_deref() == Some(path)).count();
    for (pool, name, path, format, cap, alloc, _) in &volumes {
        let key = ui::views::storage::volume_key(pool, name);
        let mut owners: Vec<&str> = st
            .attachments
            .iter()
            .filter(|(_, k)| k == path || *k == key)
            .map(|(d, _)| d.as_str())
            .collect();
        owners.dedup();
        let children = backing_count(path);
        let used_by = if !owners.is_empty() {
            owners.join(", ")
        } else if children > 0 {
            format!("◆ backing of {children}")
        } else if name.to_lowercase().ends_with(".iso") {
            String::from("iso")
        } else if st.incomplete {
            String::from("? unknown")
        } else {
            String::from("⚠ no domain")
        };
        st.pool_volumes.entry(pool.clone()).or_default().push(VolumeRow {
            name: name.clone(),
            format: format.clone(),
            capacity: human_size(*cap as f64, "B"),
            alloc: human_size(*alloc as f64, "B"),
            path: path.clone(),
            used_by,
        });
    }
    st.pool_selected = 0;
    load_pool_volumes(uri, &mut st).await;
    st
}

/// Volumes of the selected pool, from the per-load cache.
pub(super) async fn load_pool_volumes(_uri: &str, st: &mut ui::views::storage::StorageState) {
    st.volumes.clear();
    st.vol_selected = 0;
    let Some(pool) = st.pools.get(st.pool_selected).cloned() else {
        return;
    };
    if !pool.active {
        return;
    }
    st.volumes = st.pool_volumes.get(&pool.name).cloned().unwrap_or_default();
}

/// Disk references of a domain XML: source paths (file/dev/dir, including
/// backing chains in live XML) and [`ui::views::storage::volume_key`]s for
/// `<source pool= volume=>` disks. Entities are decoded by the XML parser.
pub(super) fn disk_sources(xml: &str) -> Vec<String> {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return vec![];
    };
    let mut out = Vec::new();
    for disk in doc.descendants().filter(|n| n.has_tag_name("disk")) {
        for source in disk.descendants().filter(|n| n.has_tag_name("source")) {
            if let (Some(pool), Some(volume)) = (source.attribute("pool"), source.attribute("volume")) {
                out.push(ui::views::storage::volume_key(pool, volume));
            }
            for attr in ["file", "dev", "dir"] {
                if let Some(path) = source.attribute(attr) {
                    out.push(path.to_string());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Pool target path from pool XML.
pub(super) fn pool_target(xml: &str) -> String {
    for line in xml.lines() {
        if let Some(i) = line.find("<path>") {
            return line[i + 6..].split('<').next().unwrap_or("—").to_string();
        }
    }
    String::from("—")
}

/// Pool type from pool XML root attribute.
pub(super) fn pool_kind(xml: &str) -> String {
    for line in xml.lines() {
        let t = line.trim();
        if t.starts_with("<pool ") {
            for q in ["type='", "type=\""] {
                if let Some(v) = t.split(q).nth(1).and_then(|s| s.split(['\'', '"']).next()) {
                    return v.to_string();
                }
            }
        }
    }
    String::from("dir")
}

/// Parse `vol-list [--details]` rows into (name, path).
pub(super) fn parse_vol_list(text: &str) -> Vec<(String, String)> {
    crate::backend::virsh::parse_list::table_rows(text)
        .into_iter()
        .filter(|c| c.len() >= 2 && !c[0].is_empty())
        .map(|c| (c[0].clone(), c[1].clone()))
        .collect()
}

pub(super) fn human_size(bytes: f64, unit: &str) -> String {
    let b = match unit {
        "K" | "KiB" => bytes * 1024.0,
        "M" | "MiB" => bytes * 1024.0 * 1024.0,
        "G" | "GiB" => bytes * 1024.0 * 1024.0 * 1024.0,
        "T" | "TiB" => bytes * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => bytes,
    };
    if b >= 1024.0_f64.powi(4) {
        format!("{:.1}T", b / 1024.0_f64.powi(4))
    } else if b >= 1024.0_f64.powi(3) {
        format!("{:.1}G", b / 1024.0_f64.powi(3))
    } else if b >= 1024.0_f64.powi(2) {
        format!("{:.1}M", b / 1024.0_f64.powi(2))
    } else {
        format!("{:.0}K", b / 1024.0)
    }
}

/// "System" panel facts: kernel, libvirt/QEMU, KVM/nested/IOMMU, NUMA, uptime.
async fn host_system_facts(uri: &str) -> Vec<(String, String)> {
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default().trim().to_string();
    let version = crate::backend::virsh::exec::run(uri, &["version"])
        .await
        .unwrap_or_default();
    let pick = |prefix: &str| {
        version
            .lines()
            .find_map(|l| l.trim().strip_prefix(prefix))
            .map(|v| v.split_whitespace().last().unwrap_or("").to_string())
            .unwrap_or_else(|| String::from("?"))
    };
    let mut out = vec![(
        String::from("libvirt  "),
        format!(
            "{} · QEMU {}",
            pick("Using library:"),
            pick("Running hypervisor:")
        ),
    )];
    if is_local_uri(uri) {
        let yes = |b: bool| if b { "✓" } else { "✗" };
        let kvm = std::path::Path::new("/dev/kvm").exists();
        let vendor = if std::path::Path::new("/sys/module/kvm_amd").exists() {
            "amd-v"
        } else {
            "vt-x"
        };
        let nested = [
            "/sys/module/kvm_amd/parameters/nested",
            "/sys/module/kvm_intel/parameters/nested",
        ]
        .iter()
        .any(|p| matches!(read(p).as_str(), "1" | "Y"));
        let iommu = std::fs::read_dir("/sys/class/iommu").is_ok_and(|mut d| d.next().is_some());
        let nodes = std::fs::read_dir("/sys/devices/system/node")
            .map(|d| {
                d.flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with("node"))
                    .count()
            })
            .unwrap_or(1);
        let threads = std::fs::read_to_string("/proc/stat")
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
            .count();
        out.insert(0, (String::from("kernel   "), read("/proc/sys/kernel/osrelease")));
        out.push((
            String::from("kvm      "),
            format!(
                "{} {vendor} · nested {} · iommu {}",
                yes(kvm),
                yes(nested),
                yes(iommu)
            ),
        ));
        out.push((
            String::from("numa     "),
            format!(
                "{nodes} node{} · {threads} threads",
                if nodes == 1 { "" } else { "s" }
            ),
        ));
        let up: f64 = read("/proc/uptime")
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        out.push((
            String::from("uptime   "),
            crate::backend::virsh::host_local::fmt_uptime(up as u64),
        ));
    }
    out
}
