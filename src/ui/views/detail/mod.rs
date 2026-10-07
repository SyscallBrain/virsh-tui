//! Domain detail shell (screen 03, PLAN.md section 9.5, P6 tabs).

pub mod hardware;
pub mod snapshots;

use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::model::{DomainConfig, DomainState};
use crate::theme::Theme;
use crate::ui::widgets::charts::{self, BrailleMode};

/// Inner tabs in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailTab {
    #[default]
    Overview,
    Monitor,
    Hardware,
    Snapshots,
    Console,
    Xml,
}

impl DetailTab {
    pub const ALL: [Self; 6] = [
        Self::Overview,
        Self::Monitor,
        Self::Hardware,
        Self::Snapshots,
        Self::Console,
        Self::Xml,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Monitor => "Monitor",
            Self::Hardware => "Hardware",
            Self::Snapshots => "Snapshots",
            Self::Console => "Console",
            Self::Xml => "XML",
        }
    }
}

/// Line-fold state for the XML tab.
#[derive(Debug, Clone, Default)]
pub struct XmlFold {
    folded: HashSet<usize>,
    lines: usize,
}

impl XmlFold {
    /// Create for a document with line count.
    pub fn new(lines: usize) -> Self {
        Self {
            folded: HashSet::new(),
            lines,
        }
    }

    /// Toggle fold at line.
    pub fn toggle(&mut self, line: usize) {
        if !self.folded.remove(&line) {
            self.folded.insert(line);
        }
    }

    /// True when folded.
    pub fn is_folded(&self, line: usize) -> bool {
        self.folded.contains(&line)
    }

    /// Open all (`zR`).
    pub fn open_all(&mut self) {
        self.folded.clear();
    }

    /// Close all top-level folds (`zM`).
    pub fn close_all(&mut self, foldable: &[usize]) {
        self.folded = foldable.iter().copied().collect();
        let _ = self.lines;
    }
}

/// Prefix a failed XML edit with the error as a comment (virsh-edit style).
pub fn edit_error_comment(xml: &str, error: &str) -> String {
    // `--` is not allowed inside an XML comment (libvirt errors often contain flags).
    let comment = error
        .lines()
        .map(|l| format!(" * {}", l.replace("--", "- -")))
        .collect::<Vec<_>>()
        .join("\n");
    // Replace (not stack) the comment from a previous failed attempt.
    let body = match xml.strip_prefix("<!-- virsh-tui edit error:") {
        Some(rest) => rest.split_once("-->\n").map_or(xml, |(_, b)| b),
        None => xml,
    };
    format!("<!-- virsh-tui edit error:\n{comment}\n-->\n{body}")
}

/// Detail screen state.
#[derive(Debug, Clone)]
pub struct DetailState {
    pub domain: String,
    pub state: DomainState,
    pub uptime: String,
    pub tab: DetailTab,
    pub config: DomainConfig,
    pub xml: String,
    pub fold: XmlFold,
    pub xml_cursor: usize,
    pub message: String,
    pub uri: String,
    pub connected: bool,
    pub hw: hardware::HardwareState,
    pub hw_insert: bool,
    pub snaps: snapshots::SnapshotState,
    pub snap_modal: SnapModal,
    /// Live metrics for this domain (refreshed every tick); `None` in demo.
    pub live_series: Option<crate::metrics::store::DomainSeries>,
}

/// Snapshot tab modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapModal {
    #[default]
    None,
    Create,
    Revert,
    Delete,
}

impl DetailState {
    /// Demo arch-dev detail (PLAN.md section 10).
    pub fn demo(domain: &str) -> Self {
        let xml = crate::backend::demo::fixtures::ARCH_DEV_XML
            .replace("<name>arch-dev</name>", &format!("<name>{domain}</name>"));
        let config = crate::backend::virsh::parse_xml::parse_domain_xml(&xml).unwrap_or_default();
        let lines = xml.lines().count();
        Self {
            domain: domain.to_string(),
            state: DomainState::Running,
            uptime: String::from("3d 04h"),
            tab: DetailTab::Overview,
            config,
            xml,
            fold: XmlFold::new(lines),
            xml_cursor: 0,
            message: String::new(),
            uri: String::from("qemu:///system"),
            connected: true,
            hw: hardware::HardwareState::demo(),
            hw_insert: false,
            snaps: snapshots::SnapshotState::demo(),
            snap_modal: SnapModal::None,
            live_series: None,
        }
    }

    /// Switch tabs.
    pub fn next_tab(&mut self) {
        let i = DetailTab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.tab = DetailTab::ALL[(i + 1) % DetailTab::ALL.len()];
    }

    /// Switch tabs.
    pub fn prev_tab(&mut self) {
        let i = DetailTab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.tab = DetailTab::ALL[(i + DetailTab::ALL.len() - 1) % DetailTab::ALL.len()];
    }
}

/// Render the detail screen.
pub fn render(frame: &mut Frame, theme: &Theme, state: &DetailState) {
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
    render_detail(frame, theme, state, rows[1]);
    render_statusbar(frame, theme, state, rows[2]);
    render_message(frame, theme, state, rows[3]);
}

fn render_topbar(frame: &mut Frame, theme: &Theme, area: Rect, uri: &str, connected: bool) {
    crate::ui::chrome::topbar(frame, theme, area, Some(0), uri, connected, "14:32:07");
}

fn panel_block(theme: &Theme, title: &str, right: Option<&str>) -> Block<'static> {
    crate::ui::widgets::panel::block(
        theme,
        false,
        title,
        right.map(|r| crate::ui::widgets::panel::right(r.to_string(), theme)),
        None,
        None,
    )
}

fn render_detail(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    let body = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), area.height);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
        .split(body);
    render_header(frame, theme, state, rows[0]);
    frame.render_widget(Paragraph::new("").style(theme.base()), rows[1]);
    match state.tab {
        DetailTab::Overview => render_overview(frame, theme, state, rows[2]),
        DetailTab::Monitor => render_monitor(frame, theme, state, rows[2]),
        DetailTab::Hardware => {
            hardware::render(frame, theme, &state.hw, &state.domain, state.hw_insert, rows[2])
        }
        DetailTab::Snapshots => {
            snapshots::render(frame, theme, &state.snaps, rows[2]);
            if state.snap_modal != SnapModal::None {
                crate::ui::widgets::modal::dim_background(frame, theme);
            }
            match state.snap_modal {
                SnapModal::Create => snapshots::render_create(frame, theme, rows[2], &state.snaps),
                SnapModal::Revert => snapshots::render_revert(
                    frame,
                    theme,
                    rows[2],
                    &state.domain,
                    state.snaps.selected_name(),
                    &state.snaps,
                ),
                SnapModal::Delete => snapshots::render_delete(
                    frame,
                    theme,
                    rows[2],
                    &state.domain,
                    state.snaps.selected_name(),
                    &state.snaps,
                ),
                SnapModal::None => {}
            }
        }
        DetailTab::Console => render_console(frame, theme, state, rows[2]),
        DetailTab::Xml => render_xml(frame, theme, state, rows[2]),
    }
}

fn render_header(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    let t = &theme.tokens;
    let glyph = state.state.glyph();
    let state_color = match state.state {
        DomainState::Running => t.green,
        DomainState::Paused => t.yellow,
        DomainState::Crashed => t.red,
        _ => t.comment,
    };
    let mut spans = vec![
        Span::styled("domains › ", theme.dim()),
        Span::styled(
            state.domain.clone(),
            Style::default().fg(t.fg).bg(t.bg).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{glyph} {}", state_label(state.state)),
            Style::default().fg(state_color).bg(t.bg),
        ),
        Span::styled(format!("  {}", state.uptime), theme.dim()),
        Span::raw("   "),
    ];
    for tab in DetailTab::ALL {
        if tab == state.tab {
            spans.push(Span::styled(
                format!(" {} ", tab.label()),
                Style::default().fg(t.fg).bg(t.sel).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(format!(" {} ", tab.label()), theme.dim()));
        }
    }
    spans.push(Span::styled("   H/L switch tab  ⌫ back", theme.dim()));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);
}

fn state_label(s: DomainState) -> &'static str {
    match s {
        DomainState::Running => "running",
        DomainState::Paused => "paused",
        DomainState::Crashed => "crashed",
        DomainState::ShutOff => "shut off",
        DomainState::PmSuspended => "pmsuspended",
        DomainState::Other => "other",
    }
}

fn render_overview(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    let c = &state.config;
    let disks = c
        .disks
        .iter()
        .map(|d| format!("{} {}", d.target, d.device))
        .collect::<Vec<_>>()
        .join(" · ");
    let nics = c
        .nics
        .iter()
        .map(|n| format!("{} {}", n.target, n.mac))
        .collect::<Vec<_>>()
        .join(" · ");
    let gfx = c
        .graphics
        .as_ref()
        .map(|g| format!("{} :{}", g.kind.to_uppercase(), g.port))
        .unwrap_or_else(|| String::from("—"));
    let kv = [
        ("OS           ", c.machine.clone()),
        ("Firmware     ", c.firmware.clone()),
        ("vCPU         ", format!("{}", c.vcpus)),
        (
            "Memory       ",
            format!("{:.1} GiB", c.mem_kib as f64 / 1024.0 / 1024.0),
        ),
        (
            "Disks        ",
            if disks.is_empty() {
                String::from("—")
            } else {
                disks
            },
        ),
        (
            "NICs         ",
            if nics.is_empty() {
                String::from("—")
            } else {
                nics
            },
        ),
        ("Graphics     ", gfx),
        ("Autostart    ", String::from("✓ on boot")),
        ("Persistent   ", String::from("yes")),
        ("UUID         ", c.uuid.clone()),
    ];
    let lines: Vec<Line> = kv
        .iter()
        .map(|(k, v)| Line::from(vec![Span::styled(*k, theme.dim()), Span::raw(v.clone())]))
        .collect();
    let block = panel_block(theme, &format!("Overview · {}", state.domain), None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

/// vCPU meters, per-disk and per-interface rates (Monitor tab, bottom row).
fn render_monitor_breakdown(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    use crate::metrics::store::{DiskNow, NicNow, fmt_bits_rate, fmt_bytes_rate};
    let t = &theme.tokens;
    let (vcpus, disks, nics): (Vec<f64>, Vec<DiskNow>, Vec<NicNow>) = match &state.live_series {
        Some(s) => (s.now.vcpu_frac.clone(), s.now.disks.clone(), s.now.nics.clone()),
        None if !crate::ui::chrome::is_live() => (
            vec![0.62, 0.48, 0.35, 0.71, 0.22, 0.40, 0.18, 0.55],
            vec![
                DiskNow {
                    name: "vda".into(),
                    rd_bps: 12.4 * 1048576.0,
                    wr_bps: 3.1 * 1048576.0,
                    iops: 1200.0,
                },
                DiskNow {
                    name: "vdb".into(),
                    rd_bps: 0.2 * 1048576.0,
                    wr_bps: 0.0,
                    iops: 14.0,
                },
            ],
            vec![NicNow {
                name: "vnet3".into(),
                rx_bps: 6_025_000.0,
                tx_bps: 762_500.0,
                drops: 0,
                errs: 0,
            }],
        ),
        None => (Vec::new(), Vec::new(), Vec::new()),
    };
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .spacing(1)
        .split(area);
    // vCPU meters: 3-row vertical bars like the host per-thread meters.
    let mut vl: Vec<Line> = vec![Line::from(""); 4];
    for (i, v) in vcpus.iter().enumerate() {
        let rows = charts::spark_rows(&[*v, *v], 2, 3);
        for (r, line) in vl.iter_mut().take(3).enumerate() {
            let c = [t.red, t.blue, t.cyan][r];
            line.spans
                .push(Span::styled(rows[r].clone(), Style::default().fg(c)));
            line.spans.push(Span::raw(" "));
        }
        vl[3].spans.push(Span::styled(format!("{i:>2} "), theme.dim()));
    }
    if vcpus.is_empty() {
        vl = vec![Line::from(Span::styled("no samples yet", theme.dim()))];
    }
    let avg = if vcpus.is_empty() {
        0.0
    } else {
        vcpus.iter().sum::<f64>() / vcpus.len() as f64
    };
    frame.render_widget(
        Paragraph::new(vl)
            .block(panel_block(
                theme,
                "vCPUs",
                Some(&format!("{} · avg {:.0}%", vcpus.len(), avg * 100.0)),
            ))
            .style(theme.base()),
        cols[0],
    );
    let mut dl = vec![Line::from(Span::styled(
        format!("{:<8}{:>12}{:>12}{:>8}", "DISK", "READ", "WRITE", "IOPS"),
        theme.dim().add_modifier(Modifier::BOLD),
    ))];
    for d in &disks {
        dl.push(Line::from(vec![
            Span::raw(format!("{:<8}", d.name)),
            Span::styled(
                format!("{:>12}", fmt_bytes_rate(d.rd_bps)),
                Style::default().fg(t.green),
            ),
            Span::styled(
                format!("{:>12}", fmt_bytes_rate(d.wr_bps)),
                Style::default().fg(t.orange),
            ),
            Span::styled(format!("{:>8.0}", d.iops), theme.secondary()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(dl)
            .block(panel_block(theme, "Disks", None))
            .style(theme.base()),
        cols[1],
    );
    let mut nl = vec![Line::from(Span::styled(
        format!(
            "{:<8}{:>12}{:>12}{:>6}{:>6}",
            "IFACE", "↓ RX", "↑ TX", "DROP", "ERR"
        ),
        theme.dim().add_modifier(Modifier::BOLD),
    ))];
    for n in &nics {
        nl.push(Line::from(vec![
            Span::raw(format!("{:<8}", n.name)),
            Span::styled(
                format!("{:>12}", fmt_bits_rate(n.rx_bps)),
                Style::default().fg(t.cyan),
            ),
            Span::styled(
                format!("{:>12}", fmt_bits_rate(n.tx_bps)),
                Style::default().fg(t.yellow),
            ),
            Span::styled(format!("{:>6}", n.drops), theme.secondary()),
            Span::styled(
                format!("{:>6}", n.errs),
                if n.errs > 0 {
                    Style::default().fg(t.red)
                } else {
                    theme.secondary()
                },
            ),
        ]));
    }
    frame.render_widget(
        Paragraph::new(nl)
            .block(panel_block(theme, "Interfaces", None))
            .style(theme.base()),
        cols[2],
    );
}

fn render_monitor(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    // Charts on top, per-vCPU / disk / NIC breakdown below.
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(7)])
        .split(area);
    render_monitor_breakdown(frame, theme, state, split[1]);
    let area = split[0];
    use crate::metrics::store::{fmt_bits_rate, fmt_bytes_rate, normalize, padded};
    let t = &theme.tokens;
    let grid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .spacing(1)
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(grid[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(grid[1]);
    let w = (left[0].width.saturating_sub(4) as usize).max(10);
    let h = (left[0].height.saturating_sub(2) as usize).max(2);
    let n = w * 2;
    // (cpu, mem, rd, wr, rx, tx) as 0–1 series + titles.
    let (cpu, mem, rd, wr, rx, tx, titles) = match &state.live_series {
        Some(s) => {
            let disk_max = s
                .disk_rd
                .values()
                .into_iter()
                .chain(s.disk_wr.values())
                .fold(1024.0 * 1024.0, f32::max);
            let net_max = s
                .net_rx
                .values()
                .into_iter()
                .chain(s.net_tx.values())
                .fold(125_000.0, f32::max);
            let cpu: Vec<f64> = s.cpu.values().iter().map(|v| f64::from(*v) / 100.0).collect();
            (
                padded(&cpu, crate::metrics::store::window_len()),
                padded(
                    &normalize(&s.mem.values(), s.now.mem_max_kib.max(1) as f32),
                    crate::metrics::store::window_len(),
                ),
                padded(
                    &normalize(&s.disk_rd.values(), disk_max),
                    crate::metrics::store::window_len(),
                ),
                padded(
                    &normalize(&s.disk_wr.values(), disk_max),
                    crate::metrics::store::window_len(),
                ),
                padded(
                    &normalize(&s.net_rx.values(), net_max),
                    crate::metrics::store::window_len(),
                ),
                padded(
                    &normalize(&s.net_tx.values(), net_max),
                    crate::metrics::store::window_len(),
                ),
                [
                    format!("{:.0}% · {} vCPU", s.now.cpu_pct, s.now.vcpus),
                    format!(
                        "{:.1}/{:.1} GiB",
                        s.now.mem_used_kib as f64 / 1048576.0,
                        s.now.mem_max_kib as f64 / 1048576.0
                    ),
                    format!(
                        "R {}  W {}",
                        fmt_bytes_rate(s.now.rd_bps),
                        fmt_bytes_rate(s.now.wr_bps)
                    ),
                    format!(
                        "↓ {}  ↑ {}",
                        fmt_bits_rate(s.now.rx_bps),
                        fmt_bits_rate(s.now.tx_bps)
                    ),
                ],
            )
        }
        None if !crate::ui::chrome::is_live() => (
            charts::series(7, 90, 0.42, 0.22, 0.1),
            charts::series(11, 60, 0.74, 0.06, 0.2),
            charts::series(21, 60, 0.35, 0.5, 0.2),
            charts::series(23, 60, 0.15, 0.3, 0.3),
            charts::series(31, 90, 0.45, 0.35, 0.12),
            charts::series(37, 90, 0.2, 0.25, 0.2),
            [
                String::from("42% · 8 vCPU"),
                String::from("12.4/16 GiB"),
                String::from("R 12.4 MiB/s  W 3.1 MiB/s"),
                String::from("↓ 48.2 Mb/s  ↑ 6.1 Mb/s"),
            ],
        ),
        None => {
            let z = vec![0.0; n];
            let msg = if state.state == DomainState::Running {
                "collecting…"
            } else {
                "not running"
            };
            (
                z.clone(),
                z.clone(),
                z.clone(),
                z.clone(),
                z.clone(),
                z,
                std::array::from_fn(|_| msg.to_string()),
            )
        }
    };
    let colored = |rows: Vec<String>, colors: &[ratatui::style::Color]| -> Vec<Line<'static>> {
        let len = rows.len().max(1);
        rows.into_iter()
            .enumerate()
            .map(|(i, r)| {
                Line::from(Span::styled(
                    r,
                    Style::default().fg(colors[i * colors.len() / len]),
                ))
            })
            .collect()
    };
    let half = (h / 2).max(1);
    let cpu_lines = colored(
        charts::area(theme.graphs, &cpu, w, h, BrailleMode::Fill),
        &theme.chart_rows(&[
            t.red, t.magenta, t.magenta, t.blue, t.blue, t.cyan, t.cyan, t.teal,
        ]),
    );
    let mem_lines = colored(charts::spark_rows(&mem, w, h), &[t.magenta]);
    let mut disk_lines = colored(charts::spark_rows(&rd, w, half), &[t.green]);
    disk_lines.extend(colored(
        charts::spark_rows(&wr, w, h.saturating_sub(half).max(1)),
        &[t.orange],
    ));
    let mut net_lines = colored(
        charts::area(theme.graphs, &rx, w, half, BrailleMode::Fill),
        &[t.cyan],
    );
    net_lines.extend(colored(
        charts::area(
            theme.graphs,
            &tx,
            w,
            h.saturating_sub(half).max(1),
            BrailleMode::Down,
        ),
        &[t.yellow],
    ));
    let [tc, tm, td, tn] = titles;
    for (area, title, right, lines) in [
        (left[0], "CPU", tc, cpu_lines),
        (left[1], "Memory", tm, mem_lines),
        (right[0], "Disk I/O", td, disk_lines),
        (right[1], "Network", tn, net_lines),
    ] {
        let block = panel_block(theme, title, Some(&right));
        frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
    }
}

fn render_console(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    let t = &theme.tokens;
    let lines = vec![
        Line::from(Span::styled(
            "Serial console via `virsh console` (Ctrl-] returns).",
            theme.text(),
        )),
        Line::from(Span::styled(
            "Graphical viewer via virt-viewer (detached).",
            theme.text(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" c serial console ", Style::default().fg(t.fg2).bg(t.hl)),
            Span::raw("  "),
            Span::styled(" v open viewer ", Style::default().fg(t.fg2).bg(t.hl)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Graphics  ", theme.dim()),
            Span::raw("spice://127.0.0.1:5901"),
        ]),
        Line::from(vec![
            Span::styled("Domain    ", theme.dim()),
            Span::raw(state.domain.clone()),
        ]),
    ];
    let block = panel_block(theme, "Console", None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

/// Highlight one XML line (moved to [`crate::xml::highlight`]).
pub use crate::xml::highlight::highlight_line;

fn render_xml(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    for (i, line) in state.xml.lines().enumerate() {
        let mut spans = vec![Span::styled(format!("{:4} ", i + 1), theme.dim())];
        spans.extend(highlight_line(theme, line).spans);
        if state.fold.is_folded(i) {
            spans.push(Span::styled("  …", theme.dim()));
        }
        lines.push(Line::from(spans));
    }
    // Keep the last page full: never scroll past the end.
    let visible = area.height.saturating_sub(2) as usize;
    let top = state.xml_cursor.min(lines.len().saturating_sub(visible));
    let right = format!(
        "{}–{} of {} · j/k scroll · e edit",
        top + 1,
        (top + visible).min(lines.len()),
        lines.len()
    );
    let block = panel_block(theme, &format!("XML · {}", state.domain), Some(&right));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(theme.base())
            .scroll((top as u16, 0)),
        area,
    );
}

fn render_statusbar(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    let t = &theme.tokens;
    let snap_confirm = state.tab == DetailTab::Snapshots && state.snap_modal != SnapModal::None;
    if snap_confirm {
        let spans = vec![
            Span::styled(
                " CONFIRM ",
                Style::default().fg(t.bg2).bg(t.red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} › snapshots › {} ", state.domain, state.snaps.selected_name()),
                Style::default().fg(t.fg2).bg(t.hl),
            ),
            Span::styled(
                " confirm style: y/n · type-name for undefine ",
                Style::default().fg(t.comment).bg(t.hl),
            ),
        ];
        crate::ui::chrome::statusbar(frame, theme, area, spans, 2);
        return;
    }
    let insert = state.hw_insert && state.tab == DetailTab::Hardware;
    let pill = if insert { " INSERT " } else { " NORMAL " };
    let pill_bg = if insert { t.green } else { t.blue };
    let crumb = if insert {
        let dev = state.hw.selected_device();
        format!("{} › hardware › {} › max ", state.domain, dev.name.to_lowercase())
    } else {
        format!(
            " detail › {} › {} ",
            state.domain,
            state.tab.label().to_lowercase()
        )
    };
    let hints = if insert {
        "  ⎋ normal  ⇥/⇧⇥ next/prev field  ctrl-a/x ±1  ␣ toggle  :w apply  :q close"
    } else {
        "  H/L tab  e edit  ⏎ open  ⌫ back  ? help"
    };
    let mut spans = vec![
        Span::styled(
            pill,
            Style::default()
                .fg(t.bg2)
                .bg(pill_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(crumb, Style::default().fg(t.fg2).bg(t.hl)),
        Span::styled(hints, theme.dim()),
    ];
    if state.hw.pending_count() > 0 {
        spans.push(Span::styled(
            " [+] modified ",
            Style::default().fg(t.yellow).bg(t.hl),
        ));
    }
    crate::ui::chrome::statusbar(frame, theme, area, spans, 3);
}

fn render_message(frame: &mut Frame, theme: &Theme, state: &DetailState, area: Rect) {
    let insert = state.hw_insert && state.tab == DetailTab::Hardware;
    let snap_confirm = state.tab == DetailTab::Snapshots && state.snap_modal != SnapModal::None;
    let line = if snap_confirm {
        Line::from(Span::styled("-- awaiting confirmation --", theme.dim()))
    } else if insert {
        Line::from(Span::styled("-- INSERT --", theme.dim()))
    } else {
        crate::ui::chrome::message_line(frame, theme, area, &state.message);
        return;
    };
    frame.render_widget(Paragraph::new(line).style(theme.base()), area);
}
