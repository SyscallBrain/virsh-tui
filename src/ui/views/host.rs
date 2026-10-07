//! Host monitor (screen 02, PLAN.md section 9.4).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::theme::Theme;
use crate::ui::widgets::charts::{self, BrailleMode};

/// One running-domain row.
#[derive(Debug, Clone)]
pub struct HostDomainRow {
    pub name: String,
    pub vcpus: u32,
    pub cpu_frac: f64,
    pub mem_label: String,
    pub mem_frac: f64,
    pub rd: String,
    pub wr: String,
    pub rx: String,
    pub tx: String,
    pub seed: u32,
    /// Live CPU history (0–1); `None` uses the mockup series (demo).
    pub cpu_hist: Option<Vec<f64>>,
    /// Live memory history (0–1); `None` uses the mockup series (demo).
    pub mem_hist: Option<Vec<f64>>,
}

/// Pool gauge row.
#[derive(Debug, Clone)]
pub struct HostPoolRow {
    pub name: String,
    pub frac: f64,
    pub value: String,
}

/// Network row.
#[derive(Debug, Clone)]
pub struct HostNetRow {
    pub name: String,
    pub mode: String,
    pub active: bool,
    pub seed: u32,
    /// Bridge device (traffic source in live mode).
    pub bridge: String,
    /// Live traffic history (0–1); `None` uses the mockup series.
    pub spark: Option<Vec<f64>>,
}

/// Host screen state.
#[derive(Debug, Clone, Default)]
pub struct HostState {
    pub cpu_model: String,
    pub cpu_frac: f64,
    pub ghz: f64,
    pub temp_c: Option<f64>,
    pub load: (f64, f64, f64),
    pub threads: Vec<f64>,
    pub threads_total: u32,
    pub mem_total_gib: f64,
    pub mem_used_gib: f64,
    pub mem_free_gib: f64,
    pub rss_by_domain: Vec<(String, f64)>,
    pub hugepages: String,
    pub ksm: String,
    pub swap: String,
    pub domains: Vec<HostDomainRow>,
    pub vcpu_total: u32,
    pub pools: Vec<HostPoolRow>,
    pub pools_active: String,
    pub nets: Vec<HostNetRow>,
    pub nets_active: String,
    pub system: Vec<(String, String)>,
    /// Live host CPU history (0–1); `None` uses the mockup series (demo).
    pub cpu_hist: Option<Vec<f64>>,
    pub live: bool,
    /// Selected row of the running-domains table.
    pub selected: usize,
    pub message: String,
    pub uri: String,
    pub connected: bool,
}

impl HostState {
    /// Live state from local /proc + hwmon (local URIs only).
    pub async fn live_local() -> Self {
        use crate::backend::virsh::host_local as hl;
        let stat1 = std::fs::read_to_string("/proc/stat").unwrap_or_default();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let stat2 = std::fs::read_to_string("/proc/stat").unwrap_or_default();
        let a = hl::parse_proc_stat(&stat1);
        let b = hl::parse_proc_stat(&stat2);
        let mut threads: Vec<f64> = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| hl::cpu_busy_fraction(*x, *y))
            .collect();
        if threads.is_empty() {
            threads = vec![0.0];
        }
        let threads_total = threads.len() as u32;
        let cpu_frac = threads.iter().sum::<f64>() / threads.len().max(1) as f64;
        let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let mem = hl::parse_meminfo(&meminfo);
        let loadavg = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
        let load = hl::parse_loadavg(&loadavg);
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let temps = read_hwmon_temps();
        Self {
            cpu_model: short_model(&hl::parse_cpu_model(&cpuinfo)),
            cpu_frac,
            ghz: hl::parse_cpu_ghz(&cpuinfo),
            temp_c: hl::max_temp_c(&temps.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
            load,
            threads,
            threads_total,
            mem_total_gib: mem.total_kib as f64 / 1024.0 / 1024.0,
            mem_used_gib: (mem.total_kib.saturating_sub(mem.available_kib)) as f64 / 1024.0 / 1024.0,
            mem_free_gib: mem.available_kib as f64 / 1024.0 / 1024.0,
            rss_by_domain: vec![],
            hugepages: format!("{}/{}", mem.hugepages_free, mem.hugepages_total),
            ksm: read_ksm_gib(),
            swap: format!(
                "{:.1}/{:.1}G",
                (mem.swap_total_kib.saturating_sub(mem.swap_free_kib)) as f64 / 1024.0 / 1024.0,
                mem.swap_total_kib as f64 / 1024.0 / 1024.0
            ),
            domains: vec![],
            vcpu_total: 0,
            pools: vec![],
            pools_active: String::from("—"),
            nets: vec![],
            nets_active: String::from("—"),
            system: vec![("uptime   ".to_string(), read_uptime())],
            cpu_hist: Some(Vec::new()),
            live: true,
            selected: 0,
            message: String::from("-- sampling virsh domstats every 1000 ms --"),
            uri: String::from("qemu:///system"),
            connected: true,
        }
    }

    /// Refresh from the shared live metrics (called every tick while visible).
    pub fn apply_live(&mut self, dash: &crate::ui::views::dashboard::DashboardState) {
        use crate::metrics::store::{fmt_bits_rate, fmt_bytes_rate, normalize};
        let m = &dash.metrics;
        let host_hist: Vec<f64> = m
            .host
            .cpu
            .values()
            .iter()
            .map(|v| f64::from(*v) / 100.0)
            .collect();
        if let Some(last) = host_hist.last() {
            self.cpu_frac = *last;
        }
        self.cpu_hist = Some(host_hist);
        if !m.host.threads.is_empty() {
            self.threads.clone_from(&m.host.threads);
            self.threads_total = self.threads.len() as u32;
        }
        let mem = crate::backend::virsh::host_local::parse_meminfo(
            &std::fs::read_to_string("/proc/meminfo").unwrap_or_default(),
        );
        if mem.total_kib > 0 {
            let gib = |k: u64| (k as f64 / 1024.0 / 1024.0 * 10.0).round() / 10.0;
            self.mem_total_gib = gib(mem.total_kib);
            self.mem_used_gib = gib(mem.total_kib.saturating_sub(mem.available_kib));
            self.mem_free_gib = gib(mem.available_kib);
        }
        self.rss_by_domain = m
            .rss_by_domain()
            .into_iter()
            .map(|(n, kib)| (n, kib as f64 / 1024.0 / 1024.0))
            .collect();
        let mut rows: Vec<HostDomainRow> = dash
            .rows
            .iter()
            .filter(|r| r.state == crate::model::DomainState::Running)
            .map(|r| {
                let series = m.domain(&r.name);
                let now = series.map(|s| s.now.clone()).unwrap_or_default();
                let max = now.mem_max_kib.max(1) as f32;
                HostDomainRow {
                    name: r.name.clone(),
                    vcpus: r.vcpus,
                    cpu_frac: now.cpu_pct / 100.0,
                    mem_label: format!("{:.1}G", now.mem_used_kib as f64 / 1024.0 / 1024.0),
                    mem_frac: f64::from(now.mem_used_kib as f32 / max),
                    rd: fmt_bytes_rate(now.rd_bps).replace("/s", ""),
                    wr: fmt_bytes_rate(now.wr_bps).replace("/s", ""),
                    rx: fmt_bits_rate(now.rx_bps).replace("/s", ""),
                    tx: fmt_bits_rate(now.tx_bps).replace("/s", ""),
                    seed: 0,
                    cpu_hist: Some(
                        series
                            .map(|s| s.cpu.values().iter().map(|v| f64::from(*v) / 100.0).collect())
                            .unwrap_or_default(),
                    ),
                    mem_hist: Some(
                        series
                            .map(|s| normalize(&s.mem.values(), max))
                            .unwrap_or_default(),
                    ),
                }
            })
            .collect();
        rows.sort_by(|a, b| b.cpu_frac.total_cmp(&a.cpu_frac).then(a.name.cmp(&b.name)));
        // Keep the highlighted domain when the CPU order changes.
        let keep = self.domains.get(self.selected).map(|d| d.name.clone());
        if let Some(i) = keep.and_then(|k| rows.iter().position(|r| r.name == k)) {
            self.selected = i;
        }
        for n in &mut self.nets {
            n.spark = m.iface(&n.bridge).map(|s| {
                let sum: Vec<f32> =
                    s.rx.values()
                        .iter()
                        .zip(s.tx.values())
                        .map(|(a, b)| a + b)
                        .collect();
                normalize(&sum, 125_000.0)
            });
        }
        self.vcpu_total = rows.iter().map(|r| r.vcpus).sum();
        self.domains = rows;
        self.uri.clone_from(&dash.uri);
        self.connected = dash.connected;
    }

    /// Selected running domain.
    pub fn selected_name(&self) -> Option<&str> {
        self.domains.get(self.selected).map(|d| d.name.as_str())
    }

    /// Move the table selection.
    pub fn move_selection(&mut self, delta: i32) {
        let n = self.domains.len() as i32;
        if n > 0 {
            self.selected = (self.selected as i32 + delta).clamp(0, n - 1) as usize;
        }
    }

    /// Demo state matching docs/mockups/Host.dc.html renderVals().
    pub fn demo() -> Self {
        let mut rng = charts::Rng::new(99);
        let mut threads = Vec::new();
        for i in 0..32 {
            let base = if i % 4 == 0 { 0.7 } else { 0.25 };
            let v = (base + (rng.next_f64() - 0.5) * 0.6).clamp(0.03, 0.98);
            threads.push(v);
        }
        let raw = [
            (
                "win11-gaming",
                12u32,
                0.87,
                "16.4G",
                0.9,
                "84.1M",
                "22.0M",
                "312M",
                "18.4M",
                3u32,
            ),
            (
                "k8s-worker-02",
                4,
                0.61,
                "0.9G",
                0.3,
                "41.6M",
                "12.1M",
                "22.0M",
                "4.1M",
                7,
            ),
            (
                "k8s-worker-01",
                4,
                0.55,
                "3.2G",
                0.8,
                "6.2M",
                "9.8M",
                "18.7M",
                "6.3M",
                11,
            ),
            (
                "arch-dev", 8, 0.42, "9.8G", 0.78, "12.4M", "3.1M", "6.0M", "0.8M", 13,
            ),
            (
                "k8s-cp-01",
                4,
                0.23,
                "2.4G",
                0.64,
                "1.1M",
                "4.4M",
                "9.3M",
                "9.0M",
                17,
            ),
            (
                "truenas-scale",
                4,
                0.12,
                "6.1G",
                0.88,
                "0.4M",
                "61.2M",
                "48.0M",
                "2.2M",
                19,
            ),
            ("haos", 2, 0.08, "1.1G", 0.7, "0.1M", "0.3M", "0.2M", "0.1M", 23),
            (
                "openbsd-fw",
                2,
                0.04,
                "0.4G",
                0.4,
                "0.0M",
                "0.1M",
                "54.2M",
                "53.9M",
                29,
            ),
            (
                "alpine-edge",
                1,
                0.02,
                "0.2G",
                0.6,
                "0.0M",
                "0.0M",
                "0.1M",
                "0.0M",
                31,
            ),
        ];
        let domains = raw
            .iter()
            .map(
                |(name, vcpu, cpu, mem, memf, rd, wr, rx, tx, seed)| HostDomainRow {
                    name: name.to_string(),
                    vcpus: *vcpu,
                    cpu_frac: *cpu,
                    mem_label: mem.to_string(),
                    mem_frac: *memf,
                    rd: rd.to_string(),
                    wr: wr.to_string(),
                    rx: rx.to_string(),
                    tx: tx.to_string(),
                    seed: *seed,
                    cpu_hist: None,
                    mem_hist: None,
                },
            )
            .collect();
        Self {
            cpu_model: String::from("AMD Ryzen 9 7950X"),
            cpu_frac: 0.38,
            ghz: 5.2,
            temp_c: Some(61.0),
            load: (9.42, 8.71, 7.90),
            threads,
            threads_total: 32,
            mem_total_gib: 64.0,
            mem_used_gib: 41.2,
            mem_free_gib: 22.8,
            rss_by_domain: vec![
                ("win11-gaming".to_string(), 16.4),
                ("arch-dev".to_string(), 9.8),
                ("truenas-scale".to_string(), 6.1),
                ("k8s-worker-01".to_string(), 3.2),
                ("k8s-cp-01".to_string(), 2.4),
                ("haos".to_string(), 1.1),
                ("k8s-worker-02".to_string(), 0.9),
                ("other (5)".to_string(), 1.3),
            ],
            hugepages: String::from("8/16"),
            ksm: String::from("2.1G"),
            swap: String::from("0.4/8G"),
            domains,
            vcpu_total: 47,
            pools: vec![
                HostPoolRow {
                    name: String::from("default"),
                    frac: 0.66,
                    value: String::from("612/931G"),
                },
                HostPoolRow {
                    name: String::from("nvme-fast"),
                    frac: 0.67,
                    value: String::from("1.2/1.8T"),
                },
                HostPoolRow {
                    name: String::from("isos"),
                    frac: 0.31,
                    value: String::from("62/200G"),
                },
                HostPoolRow {
                    name: String::from("nfs-backup"),
                    frac: 0.82,
                    value: String::from("6.6/8.0T"),
                },
            ],
            pools_active: String::from("4/4 active"),
            nets: vec![
                HostNetRow {
                    name: String::from("default"),
                    mode: String::from("nat"),
                    active: true,
                    seed: 41,
                    bridge: String::new(),
                    spark: None,
                },
                HostNetRow {
                    name: String::from("lab-isolated"),
                    mode: String::from("isolated"),
                    active: true,
                    seed: 43,
                    bridge: String::new(),
                    spark: None,
                },
                HostNetRow {
                    name: String::from("br0"),
                    mode: String::from("bridge"),
                    active: true,
                    seed: 47,
                    bridge: String::new(),
                    spark: None,
                },
                HostNetRow {
                    name: String::from("ipv6-routed"),
                    mode: String::from("route"),
                    active: false,
                    seed: 0,
                    bridge: String::new(),
                    spark: None,
                },
            ],
            nets_active: String::from("3/4 active"),
            system: vec![
                ("kernel   ".to_string(), "6.17.2-arch1-1".to_string()),
                ("libvirt  ".to_string(), "12.8.0 · QEMU 10.1.0".to_string()),
                (
                    "kvm      ".to_string(),
                    "✓ amd-v · nested ✓ · iommu ✓".to_string(),
                ),
                ("numa     ".to_string(), "1 node · 16c/32t · SEV ✗".to_string()),
                ("uptime   ".to_string(), "47d 11h 03m".to_string()),
            ],
            cpu_hist: None,
            live: false,
            selected: 0,
            message: String::from("-- sampling virConnectGetAllDomainStats every 1000 ms --"),
            uri: String::from("qemu:///system"),
            connected: true,
        }
    }
}

/// Render the full host screen.
pub fn render(frame: &mut Frame, theme: &Theme, state: &HostState) {
    let area = frame.area();
    if crate::ui::layout::breakpoint(area.width, area.height) == crate::ui::layout::Breakpoint::TooSmall {
        crate::ui::layout::render_too_small(frame, theme);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    render_topbar(frame, theme, rows[0], &state.uri, state.connected);
    render_body(frame, theme, state, rows[1]);
    render_statusbar(frame, theme, rows[2]);
    render_message(frame, theme, state, rows[3]);
}

fn render_topbar(frame: &mut Frame, theme: &Theme, area: Rect, uri: &str, connected: bool) {
    crate::ui::chrome::topbar(frame, theme, area, Some(1), uri, connected, "14:32:07");
}

fn panel_block(theme: &Theme, focused: bool, title: &str, right: Option<String>) -> Block<'static> {
    crate::ui::widgets::panel::block(
        theme,
        focused,
        title,
        right.map(|r| crate::ui::widgets::panel::right(r, theme)),
        None,
        None,
    )
}

fn level_color(theme: &Theme, v: f64) -> ratatui::style::Color {
    let t = &theme.tokens;
    if v < 0.5 {
        t.green
    } else if v < 0.8 {
        t.yellow
    } else {
        t.red
    }
}

fn render_body(frame: &mut Frame, theme: &Theme, state: &HostState, area: Rect) {
    let body = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), area.height);
    if body.width < 160 {
        // Compact: stack row A vertically (meters omitted), then domains, system.
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(9),
                Constraint::Length(8),
                Constraint::Min(0),
                Constraint::Length(7),
            ])
            .split(body);
        render_cpu(frame, theme, state, rows[0], false);
        render_memory(frame, theme, state, rows[1]);
        render_domains(frame, theme, state, rows[2]);
        render_system(frame, theme, state, rows[3]);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        // Row A is 13, not 12: 6 braille + per-thread + 3 meter + index + 2 borders.
        .constraints([Constraint::Length(13), Constraint::Min(0), Constraint::Length(7)])
        .split(body);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(59), Constraint::Percentage(41)])
        .spacing(1)
        .split(rows[0]);
    render_cpu(frame, theme, state, top[0], true);
    render_memory(frame, theme, state, top[1]);
    render_domains(frame, theme, state, rows[1]);
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .spacing(1)
        .split(rows[2]);
    render_system(frame, theme, state, bottom[0]);
    render_pools(frame, theme, state, bottom[1]);
    render_networks(frame, theme, state, bottom[2]);
}

fn render_cpu(frame: &mut Frame, theme: &Theme, state: &HostState, area: Rect, meters: bool) {
    let t = &theme.tokens;
    let data = match &state.cpu_hist {
        Some(h) => crate::metrics::store::padded(h, crate::metrics::store::window_len()),
        None => charts::series(5, 120, 0.38, 0.2, 0.08),
    };
    let inner_w = (area.width.saturating_sub(4).max(10) - 5).max(10) as usize;
    let rows = charts::area(theme.graphs, &data, inner_w.min(92), 6, BrailleMode::Fill);
    let colors = theme.chart_rows(&[t.red, t.magenta, t.blue, t.blue, t.cyan, t.teal]);
    let labels = ["100%", "", "", " 50%", "", "  0%"];
    let mut lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            Line::from(vec![
                Span::styled(format!("{:<5}", labels[i.min(5)]), theme.dim()),
                Span::styled(r.clone(), Style::default().fg(colors[i.min(5)]).bg(t.bg)),
            ])
        })
        .collect();
    lines.push(Line::from(Span::styled(
        format!("per-thread {}", "─".repeat(inner_w)),
        theme.dim(),
    )));
    if meters && !state.threads.is_empty() {
        render_meters(theme, state, inner_w, &mut lines);
    } else if meters {
        lines.push(Line::from(Span::styled(
            "     per-thread sampling is off (Settings › Monitoring)",
            theme.dim(),
        )));
    }
    let temp = state
        .temp_c
        .map(|c| format!("{c:.0}°C"))
        .unwrap_or_else(|| String::from("—"));
    let right = format!(
        "{}% · {:.1} GHz · {} · load {:.2} {:.2} {:.2}",
        (state.cpu_frac * 100.0).round() as u32,
        state.ghz,
        temp,
        state.load.0,
        state.load.1,
        state.load.2
    );
    let title = format!("CPU · {}", state.cpu_model);
    let block = panel_block(theme, false, &title, Some(right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

/// Thread meters: busiest N that fit + "+K". 3 rows (red/blue/cyan) + index row.
fn render_meters(theme: &Theme, state: &HostState, inner_w: usize, lines: &mut Vec<Line>) {
    let t = &theme.tokens;
    let per_thread = 3;
    let mut order: Vec<usize> = (0..state.threads.len()).collect();
    order.sort_by(|&a, &b| state.threads[b].total_cmp(&state.threads[a]));
    let fit = (inner_w / per_thread).min(state.threads.len()).max(1);
    let shown: Vec<usize> = order.iter().take(fit).copied().collect();
    let row_colors = [t.red, t.blue, t.cyan];
    for (depth, &color) in row_colors.iter().enumerate() {
        let mut line = String::new();
        for (k, &idx) in shown.iter().enumerate() {
            if k > 0 {
                line.push(' ');
            }
            line.push_str(
                charts::spark_rows(&[state.threads[idx], state.threads[idx]], 2, 3)[depth].as_str(),
            );
        }
        if depth == 2 && fit < state.threads.len() {
            line.push_str(&format!(" +{}", state.threads.len() - fit));
        }
        lines.push(Line::from(vec![
            Span::raw("     "),
            Span::styled(line, Style::default().fg(color).bg(t.bg)),
        ]));
    }
    let mut index_line = String::from("     ");
    for (k, &idx) in shown.iter().enumerate() {
        if k > 0 {
            index_line.push(' ');
        }
        index_line.push_str(&format!("{idx:02}"));
    }
    lines.push(Line::from(Span::styled(index_line, theme.dim())));
}

fn render_memory(frame: &mut Frame, theme: &Theme, state: &HostState, area: Rect) {
    let t = &theme.tokens;
    let inner_w = area.width.saturating_sub(4) as usize;
    let total: f64 = state.rss_by_domain.iter().map(|(_, v)| v).sum();
    let palette = [
        t.orange, t.blue, t.magenta, t.cyan, t.green, t.yellow, t.teal, t.red, t.fg2,
    ];
    let mut segs: Vec<Span> = Vec::new();
    for (i, (_, v)) in state.rss_by_domain.iter().enumerate() {
        let w = ((v / total.max(0.01) * inner_w as f64).round() as usize).max(1);
        segs.push(Span::styled(
            "█".repeat(w),
            Style::default().fg(palette[i % palette.len()]).bg(t.bg),
        ));
    }
    let used: usize = segs.iter().map(|s| s.content.chars().count()).sum();
    if used < inner_w {
        segs.push(Span::styled(
            "█".repeat(inner_w - used),
            Style::default().fg(t.hl).bg(t.bg),
        ));
    }
    let mut lines = vec![
        Line::from(Span::styled("host RSS by domain", theme.dim())),
        Line::from(segs),
    ];
    for (i, (name, gib)) in state.rss_by_domain.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled("■ ", Style::default().fg(palette[i % palette.len()]).bg(t.bg)),
            Span::styled(format!("{name:15}"), theme.secondary()),
            Span::styled(format!("{gib:>6.1}G"), theme.dim()),
        ]));
    }
    lines.push(Line::from(vec![Span::styled(
        format!(
            "hugepages 1G {} · KSM {} · swap {}",
            state.hugepages, state.ksm, state.swap
        ),
        theme.dim(),
    )]));
    let right = format!("{} used · {} free", state.mem_used_gib, state.mem_free_gib);
    let title = format!("Memory · {} GiB", state.mem_total_gib as u32);
    let block = panel_block(theme, false, &title, Some(right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_domains(frame: &mut Frame, theme: &Theme, state: &HostState, area: Rect) {
    let t = &theme.tokens;
    let mut lines = vec![Line::from(Span::styled(
        "  NAME             vCPU  CPU ▾  CPU · 60s                         MEM        MEM · 60s        DISK  R / W            NET  ↓ / ↑",
        Style::default()
            .fg(t.comment)
            .bg(t.bg)
            .add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(t.hl).bg(t.bg),
    )));
    for (i, d) in state.domains.iter().enumerate() {
        let selected = i == state.selected.min(state.domains.len().saturating_sub(1));
        let bg = if selected { t.sel } else { t.bg };
        let marker = if selected { "▌" } else { " " };
        let cpu = match &d.cpu_hist {
            Some(h) => charts::spark(&crate::metrics::store::padded(h, 32), 32),
            None => charts::spark(&charts::series(d.seed, 60, d.cpu_frac, 0.25, 0.15), 32),
        };
        let mem = match &d.mem_hist {
            Some(h) => charts::spark(&crate::metrics::store::padded(h, 18), 18),
            None => charts::spark(&charts::series(d.seed + 1, 40, d.mem_frac, 0.05, 0.3), 18),
        };
        lines.push(
            Line::from(vec![
                Span::styled(marker, Style::default().fg(t.blue).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:16}", d.name), Style::default().fg(t.fg).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:>4}", d.vcpus), Style::default().fg(t.fg2).bg(bg)),
                Span::raw(" "),
                Span::styled(
                    format!("{:>4}", format!("{}%", (d.cpu_frac * 100.0).round() as u32)),
                    Style::default().fg(level_color(theme, d.cpu_frac)).bg(bg),
                ),
                Span::raw(" "),
                Span::styled(cpu, Style::default().fg(level_color(theme, d.cpu_frac)).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:>6}", d.mem_label), Style::default().fg(t.fg2).bg(bg)),
                Span::raw(" "),
                Span::styled(mem, Style::default().fg(t.magenta).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:>6}", d.rd), Style::default().fg(t.green).bg(bg)),
                Span::styled(" / ", theme.dim()),
                Span::styled(format!("{:>6}", d.wr), Style::default().fg(t.orange).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:>6}/s", d.rx), Style::default().fg(t.cyan).bg(bg)),
                Span::styled(" / ", theme.dim()),
                Span::styled(format!("{:>6}/s", d.tx), Style::default().fg(t.yellow).bg(bg)),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    let overcommit = state.vcpu_total as f64 / state.threads_total.max(1) as f64;
    let right = format!(
        "{} running · {} vCPU on {} threads · overcommit {:.2}×",
        state.domains.len(),
        state.vcpu_total,
        state.threads_total,
        overcommit
    );
    let block = panel_block(theme, true, "Running domains", Some(right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_system(frame: &mut Frame, theme: &Theme, state: &HostState, area: Rect) {
    let lines: Vec<Line> = state
        .system
        .iter()
        .map(|(k, v)| Line::from(vec![Span::styled(k.clone(), theme.dim()), Span::raw(v.clone())]))
        .collect();
    let block = panel_block(theme, false, "System", None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_pools(frame: &mut Frame, theme: &Theme, state: &HostState, area: Rect) {
    let t = &theme.tokens;
    let lines: Vec<Line> = state
        .pools
        .iter()
        .map(|p| {
            let (on, off) = charts::lg(p.frac, 14);
            Line::from(vec![
                Span::styled("● ", Style::default().fg(t.green).bg(t.bg)),
                Span::styled(format!("{:10} ", p.name), theme.secondary()),
                Span::styled(on, Style::default().fg(level_color(theme, p.frac)).bg(t.bg)),
                Span::styled(off, Style::default().fg(t.hl).bg(t.bg)),
                Span::styled(format!(" {}", p.value), theme.dim()),
            ])
        })
        .collect();
    let block = panel_block(theme, false, "Storage pools", Some(state.pools_active.clone()));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_networks(frame: &mut Frame, theme: &Theme, state: &HostState, area: Rect) {
    let t = &theme.tokens;
    let lines: Vec<Line> = state
        .nets
        .iter()
        .map(|n| {
            if !n.active {
                return Line::from(vec![
                    Span::styled("○ ", theme.dim()),
                    Span::styled(format!("{:12} {:8} inactive", n.name, n.mode), theme.dim()),
                ]);
            }
            let spark = match &n.spark {
                Some(h) => charts::spark(&crate::metrics::store::padded(h, 16), 16),
                None if state.live => " ".repeat(16),
                None => charts::spark(&charts::series(n.seed, 40, 0.4, 0.4, 0.2), 16),
            };
            Line::from(vec![
                Span::styled("● ", Style::default().fg(t.green).bg(t.bg)),
                Span::styled(format!("{:12} ", n.name), theme.secondary()),
                Span::styled(format!("{:8} ", n.mode), theme.dim()),
                Span::styled(spark, Style::default().fg(t.cyan).bg(t.bg)),
            ])
        })
        .collect();
    let block = panel_block(theme, false, "Networks", Some(state.nets_active.clone()));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_statusbar(frame: &mut Frame, theme: &Theme, area: Rect) {
    let t = &theme.tokens;
    let spans = vec![
        Span::styled(
            " NORMAL ",
            Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" host › running ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::styled(
            // Only keys the Host view handles (sort/interval/window were advertised
            // without bindings).
            "  j/k move  ⏎ open domain  : cmd  ? help",
            theme.dim(),
        ),
    ];
    crate::ui::chrome::statusbar(frame, theme, area, spans, 3);
}

fn render_message(frame: &mut Frame, theme: &Theme, state: &HostState, area: Rect) {
    crate::ui::chrome::message_line(frame, theme, area, &state.message);
}

/// Shorten `AMD Ryzen 9 7950X 16-Core Processor` to `AMD Ryzen 9 7950X`.
pub fn short_model(model: &str) -> String {
    model
        .split_once(" Processor")
        .map(|(s, _)| s.to_string())
        .unwrap_or_else(|| model.to_string())
}

/// Read hwmon temp inputs as strings.
fn read_hwmon_temps() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/sys/class/hwmon") else {
        return out;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let Ok(files) = std::fs::read_dir(&path) else {
            continue;
        };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            if name.starts_with("temp")
                && name.ends_with("_input")
                && let Ok(v) = std::fs::read_to_string(f.path())
            {
                out.push(v);
            }
        }
    }
    out
}

/// KSM sharing in GiB.
fn read_ksm_gib() -> String {
    let pages: f64 = std::fs::read_to_string("/sys/kernel/mm/ksm/pages_sharing")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.0);
    format!("{:.1}G", pages * 4.0 / 1024.0 / 1024.0)
}

/// Uptime as `Nd HHh MMm`.
fn read_uptime() -> String {
    let text = std::fs::read_to_string("/proc/uptime").unwrap_or_default();
    let secs: f64 = text
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let days = (secs / 86400.0) as u32;
    let hours = ((secs % 86400.0) / 3600.0) as u32;
    let mins = ((secs % 3600.0) / 60.0) as u32;
    format!("{days}d {hours:02}h {mins:02}m")
}
