//! Live data: initial view fills and applying poller messages.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Build live host state: local /proc data plus virsh pools/networks/domains.
pub(super) async fn fill_live_host(uri: &str) -> ui::views::host::HostState {
    use crate::backend::{Backend, virsh::VirshBackend};
    let mut host = ui::views::host::HostState::live_local().await;
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
            dash.metrics.ingest_ifaces(&ifaces, at);
            dash.apply_summaries(&summaries);
            for r in dash.rows.iter_mut() {
                if let Some(up) = uptimes.get(&r.name) {
                    r.uptime.clone_from(up);
                }
            }
            dash.metrics.ingest_domains(&stats, at);
            if let Some(stat) = proc_stat {
                dash.metrics.ingest_proc_stat(&stat);
            }
            dash.apply_metrics();
            dash.check_shutdowns();
            dash.uri = uri.to_string();
            if !dash.connected {
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
    Host(String, Box<ui::views::host::HostState>),
    Nets(String, Box<ui::views::networks::NetworksState>),
    Store(String, Box<ui::views::storage::StorageState>),
}

/// Load Host/Networks/Storage without blocking the UI (dozens of virsh calls).
pub(super) fn spawn_load(uri: &str, view: View, tx: &tokio::sync::mpsc::UnboundedSender<Loaded>) {
    if !matches!(view, View::Host | View::Networks | View::Storage) {
        return;
    }
    let uri = uri.to_string();
    let tx = tx.clone();
    tokio::spawn(async move {
        let msg = match view {
            View::Host => Loaded::Host(uri.clone(), Box::new(fill_live_host(&uri).await)),
            View::Networks => Loaded::Nets(uri.clone(), Box::new(fill_live_nets(&uri).await)),
            _ => Loaded::Store(uri.clone(), Box::new(fill_live_store(&uri).await)),
        };
        let _ = tx.send(msg);
    });
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
    load_net_selection(uri, &mut st).await;
    st
}

/// Reload the networks list keeping the selection (after a mutation).
pub(super) async fn reload_nets(uri: &str, st: &mut ui::views::networks::NetworksState) {
    let keep = st.selected_name().to_string();
    let message = std::mem::take(&mut st.message);
    let mut fresh = fill_live_nets(uri).await;
    if let Some(i) = fresh.networks.iter().position(|n| n.name == keep) {
        fresh.selected = i;
        load_net_selection(uri, &mut fresh).await;
    }
    fresh.message = message;
    fresh.focus = st.focus;
    *st = fresh;
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
    // MAC → domain and the interfaces plugged into this network.
    let mut by_mac: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let backend = VirshBackend::new(uri);
    if let Ok(domains) = backend.list_domains().await {
        for d in &domains {
            let Ok(out) = crate::backend::virsh::exec::run(uri, &["domiflist", &d.name]).await else {
                continue;
            };
            for (iface, kind, source, mac) in parse_domiflist(&out) {
                by_mac.insert(mac.to_lowercase(), d.name.clone());
                let on_net = (kind == "network" && source == net.name)
                    || (kind == "bridge" && source == net.cfg.bridge);
                if on_net && iface != "-" {
                    st.attached.push((iface, d.name.clone()));
                }
            }
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
    out.lines()
        .skip(2)
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.len() >= 5).then(|| {
                (
                    c[0].to_string(),
                    c[1].to_string(),
                    c[2].to_string(),
                    c[4].to_string(),
                )
            })
        })
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
    let Ok(o) = tokio::process::Command::new("ip")
        .args(["-j", "-4", "addr"])
        .output()
        .await
    else {
        return out;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) else {
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

/// Build live storage state: pools, every domain's disk sources, the volumes
/// of the selected pool, and the ISO library across all active pools.
pub(super) async fn fill_live_store(uri: &str) -> ui::views::storage::StorageState {
    use crate::backend::{Backend, virsh::VirshBackend};
    use crate::ui::views::storage::{IsoEntry, PoolRow, StorageState};
    let backend = VirshBackend::new(uri);
    let mut st = StorageState::demo();
    st.uri = uri.to_string();
    st.connected = true;
    st.pools.clear();
    st.volumes.clear();
    st.isos.clear();
    st.message.clear();
    st.attachments.clear();
    if let Ok(domains) = backend.list_domains().await {
        for d in &domains {
            if let Ok(xml) = crate::backend::virsh::exec::run(uri, &["dumpxml", "--inactive", &d.name]).await
            {
                for src in disk_sources(&xml) {
                    st.attachments.push((d.name.clone(), src));
                }
            }
        }
    }
    match backend.pools().await {
        Ok(pools) => {
            for p in &pools {
                let xml = crate::backend::virsh::exec::run(uri, &["pool-dumpxml", &p.name])
                    .await
                    .unwrap_or_default();
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
                });
                // ISO library: every *.iso in every active pool.
                if p.state == "running"
                    && let Ok(out) =
                        crate::backend::virsh::exec::run(uri, &["vol-list", "--pool", &p.name, "--details"])
                            .await
                {
                    for (name, _path) in parse_vol_list(&out) {
                        if name.to_lowercase().ends_with(".iso") {
                            st.isos.push(IsoEntry {
                                pool: p.name.clone(),
                                name,
                                size: String::new(),
                                date: String::new(),
                            });
                        }
                    }
                }
            }
        }
        Err(e) => st.message = format!("✗ {}", first_line(&e.to_string())),
    }
    st.pool_selected = 0;
    load_pool_volumes(uri, &mut st).await;
    st
}

/// Reload storage after a mutation, keeping the selected pool and message.
pub(super) async fn reload_store(uri: &str, st: &mut ui::views::storage::StorageState) {
    let keep_pool = st.pools.get(st.pool_selected).map(|p| p.name.clone());
    let keep_vol = st.vol_selected;
    let message = std::mem::take(&mut st.message);
    let focus = st.focus_pools;
    let mut fresh = fill_live_store(uri).await;
    if let Some(i) = keep_pool.and_then(|k| fresh.pools.iter().position(|p| p.name == k))
        && i != fresh.pool_selected
    {
        fresh.pool_selected = i;
        load_pool_volumes(uri, &mut fresh).await;
    }
    fresh.vol_selected = keep_vol.min(fresh.volumes.len().saturating_sub(1));
    fresh.message = message;
    fresh.focus_pools = focus;
    *st = fresh;
}

/// Volumes of the selected pool with format/sizes, backing chains and owners.
pub(super) async fn load_pool_volumes(uri: &str, st: &mut ui::views::storage::StorageState) {
    use crate::ui::views::storage::VolumeRow;
    st.volumes.clear();
    st.vol_selected = 0;
    let Some(pool) = st.pools.get(st.pool_selected).cloned() else {
        return;
    };
    if !pool.active {
        return;
    }
    let Ok(out) =
        crate::backend::virsh::exec::run(uri, &["vol-list", "--pool", &pool.name, "--details"]).await
    else {
        return;
    };
    // First pass: every volume's facts; second pass: backing counts (order independent).
    let mut rows: Vec<(String, String, String, u64, u64, Option<String>)> = Vec::new();
    for (name, path) in parse_vol_list(&out) {
        let xml = crate::backend::virsh::exec::run(uri, &["vol-dumpxml", "--pool", &pool.name, &name])
            .await
            .unwrap_or_default();
        let (format, cap, alloc, backing) = crate::backend::virsh::parse_xml::parse_volume_xml(&xml)
            .unwrap_or((String::from("—"), 0, 0, None));
        rows.push((name, path, format, cap, alloc, backing));
    }
    let backing_of = |path: &str| rows.iter().filter(|r| r.5.as_deref() == Some(path)).count();
    for (name, path, format, cap, alloc, _) in &rows {
        let owners: Vec<&str> = st
            .attachments
            .iter()
            .filter(|(_, src)| src == path)
            .map(|(d, _)| d.as_str())
            .collect();
        let children = backing_of(path);
        let used_by = if !owners.is_empty() {
            owners.join(", ")
        } else if children > 0 {
            format!("◆ backing of {children}")
        } else if name.to_lowercase().ends_with(".iso") {
            String::from("iso")
        } else {
            String::from("⚠ orphan")
        };
        st.volumes.push(VolumeRow {
            name: name.clone(),
            format: format.clone(),
            capacity: human_size(*cap as f64, "B"),
            alloc: human_size(*alloc as f64, "B"),
            path: path.clone(),
            used_by,
        });
    }
}

/// Disk source files from domain XML.
pub(super) fn disk_sources(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in xml.lines() {
        let t = line.trim();
        if t.starts_with("<source") {
            for q in ["file='", "file=\"", "dev='", "dev=\""] {
                if let Some(v) = t.split(q).nth(1).and_then(|s| s.split(['\'', '"']).next()) {
                    out.push(v.to_string());
                    break;
                }
            }
        }
    }
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

/// Parse `vol-list --details` rows into (name, path).
pub(super) fn parse_vol_list(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines().skip(2) {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 2 {
            out.push((cols[0].to_string(), cols[1].to_string()));
        }
    }
    out
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
