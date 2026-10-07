//! Domains dashboard (screen 01, PLAN.md section 9.3).
//!
//! Layout at 174x43: top bar, body (left 101 cols + right), status bar, message line.

use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::backend::demo::fixtures;
use crate::model::DomainState;
use crate::theme::Theme;
use crate::ui::widgets::charts::{self, BrailleMode};

/// State filter chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Chip {
    #[default]
    All,
    Running,
    Paused,
    Crashed,
    Off,
}

impl Chip {
    fn matches(self, state: DomainState) -> bool {
        match self {
            Self::All => true,
            Self::Running => state == DomainState::Running,
            Self::Paused => state == DomainState::Paused,
            Self::Crashed => state == DomainState::Crashed,
            Self::Off => state == DomainState::ShutOff,
        }
    }
}

/// Sort column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    State,
    Name,
    Cpu,
    Mem,
    Uptime,
}

impl Sort {
    fn label(self) -> &'static str {
        match self {
            Self::State => "state › name",
            Self::Name => "name",
            Self::Cpu => "cpu",
            Self::Mem => "mem",
            Self::Uptime => "uptime",
        }
    }
}

/// One dashboard row.
#[derive(Debug, Clone)]
pub struct DomainRow {
    pub name: String,
    pub state: DomainState,
    pub state_label: &'static str,
    pub vcpus: u32,
    pub mem_label: String,
    pub mem_sort_mib: u64,
    pub cpu_frac: f64,
    pub uptime: String,
    pub autostart: bool,
}

/// Dashboard state.
#[derive(Debug, Clone)]
pub struct DashboardState {
    pub rows: Vec<DomainRow>,
    pub selection: usize,
    pub marks: HashSet<String>,
    pub chip: Chip,
    pub sort: Sort,
    pub filter: String,
    pub message: String,
    pub detail_tab: usize,
    pub uri: String,
    pub connected: bool,
    /// Live mode: render real metrics/config instead of the mockup fixtures.
    pub live: bool,
    /// Live metrics (domstats + /proc/stat histories).
    pub metrics: crate::metrics::store::MetricsStore,
    /// Parsed domain configs (inactive XML), fetched lazily per selection.
    pub configs: std::collections::HashMap<String, crate::model::DomainConfig>,
    /// First IP address per domain (`domifaddr`), fetched lazily per selection.
    pub ips: std::collections::HashMap<String, String>,
    /// Most recent libvirt events (newest last) for the Events panel.
    pub recent_events: Vec<crate::model::LibvirtEvent>,
    /// Host panel gauges: (label, fraction, value).
    pub host_gauges: Vec<(String, f64, String)>,
    /// Largest two storage pools as host gauges (refreshed every 30 ticks).
    pub host_pools: Vec<(String, f64, String)>,
    /// Host panel right title (cpu model · threads · memory).
    pub host_title: String,
    pub hostname: String,
    /// Graceful shutdowns being tracked until the guest powers off.
    pub shutdowns: Vec<PendingShutdown>,
    /// VISUAL mode anchor (`V`): rows between it and the cursor are marked.
    pub visual_anchor: Option<usize>,
    /// `C-l`: the app loop clears and repaints the terminal.
    pub redraw_requested: bool,
}

/// A `virsh shutdown` whose outcome is not known yet.
///
/// `virsh shutdown` only *asks* the guest (ACPI power button) and returns
/// immediately; the guest may ignore it (live ISOs, GRUB, UEFI shell, no acpid).
#[derive(Debug, Clone)]
pub struct PendingShutdown {
    pub name: String,
    pub since: std::time::Instant,
}

/// Seconds to wait for a guest to honour a shutdown request.
pub const SHUTDOWN_TIMEOUT_SECS: u64 = 90;

impl DashboardState {
    /// Demo state matching the mockup: arch-dev selected, 3 k8s marks.
    pub fn demo() -> Self {
        let rows = fixtures::DOMAINS
            .iter()
            .map(|(name, state, vcpus, mem, cpu, up, auto, _)| DomainRow {
                name: name.to_string(),
                state: *state,
                state_label: match state {
                    DomainState::Running => "running",
                    DomainState::Paused => "paused",
                    DomainState::Crashed => "crashed",
                    _ => "shut off",
                },
                vcpus: *vcpus,
                mem_label: mem.to_string(),
                mem_sort_mib: mem_mib(mem),
                cpu_frac: *cpu,
                uptime: up.to_string(),
                autostart: *auto,
            })
            .collect();
        let marks: HashSet<String> = ["k8s-cp-01", "k8s-worker-01", "k8s-worker-02"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            rows,
            selection: 1,
            detail_tab: 0,
            marks,
            chip: Chip::All,
            sort: Sort::State,
            filter: String::new(),
            message: String::from("✓ Started k8s-worker-02  ── virsh start k8s-worker-02"),
            uri: String::from("qemu:///system"),
            connected: true,
            live: false,
            metrics: Default::default(),
            configs: Default::default(),
            ips: Default::default(),
            recent_events: Vec::new(),
            host_gauges: Vec::new(),
            host_pools: Vec::new(),
            host_title: String::new(),
            hostname: String::from("forge"),
            shutdowns: Vec::new(),
            visual_anchor: None,
            redraw_requested: false,
        }
    }

    /// Build from live domain summaries.
    pub fn from_summaries(summaries: &[crate::model::DomainSummary]) -> Self {
        let rows = summaries
            .iter()
            .map(|s| DomainRow {
                name: s.name.clone(),
                state: s.state,
                state_label: match s.state {
                    DomainState::Running => "running",
                    DomainState::Paused => "paused",
                    DomainState::Crashed => "crashed",
                    _ => "shut off",
                },
                vcpus: s.vcpus,
                mem_label: format!("{:.1}/{:.1}G", mib_to_gib(s.mem_kib), mib_to_gib(s.max_mem_kib)),
                mem_sort_mib: s.max_mem_kib / 1024,
                cpu_frac: 0.0,
                uptime: String::from("—"),
                autostart: s.autostart,
            })
            .collect();
        Self {
            rows,
            selection: 0,
            detail_tab: 0,
            marks: HashSet::new(),
            chip: Chip::All,
            sort: Sort::State,
            filter: String::new(),
            message: String::new(),
            uri: String::from("qemu:///system"),
            connected: true,
            live: true,
            metrics: Default::default(),
            configs: Default::default(),
            ips: Default::default(),
            recent_events: Vec::new(),
            host_gauges: Vec::new(),
            host_pools: Vec::new(),
            host_title: String::new(),
            hostname: crate::backend::virsh::hostname(),
            shutdowns: Vec::new(),
            visual_anchor: None,
            redraw_requested: false,
        }
    }

    /// Replace the rows with a fresh live list, keeping the selection on the same
    /// domain (by name, within the visible/sorted list) and dropping marks of
    /// domains that no longer exist.
    pub fn apply_summaries(&mut self, summaries: &[crate::model::DomainSummary]) {
        let selected = self.selected().name.clone();
        let fresh = Self::from_summaries(summaries);
        self.rows = fresh.rows;
        let names: HashSet<&str> = self.rows.iter().map(|r| r.name.as_str()).collect();
        self.marks.retain(|m| names.contains(m.as_str()));
        self.select_name(&selected);
    }

    /// Select a domain by name in the visible list (no-op when hidden/absent).
    pub fn select_name(&mut self, name: &str) {
        if let Some(i) = self.visible_rows().iter().position(|r| r.name == name) {
            self.selection = i;
        } else {
            let n = self.visible_rows().len();
            self.selection = self.selection.min(n.saturating_sub(1));
        }
    }

    /// Track a shutdown request until the guest powers off or times out.
    pub fn track_shutdown(&mut self, name: &str) {
        self.shutdowns.retain(|p| p.name != name);
        self.shutdowns.push(PendingShutdown {
            name: name.to_string(),
            since: std::time::Instant::now(),
        });
    }

    /// Resolve tracked shutdowns against the current rows (call after a refresh).
    pub fn check_shutdowns(&mut self) {
        let mut done = Vec::new();
        for p in &self.shutdowns {
            let state = self.rows.iter().find(|r| r.name == p.name).map(|r| r.state);
            match state {
                Some(DomainState::ShutOff) | None => {
                    done.push(p.name.clone());
                    self.message = format!("✓ {} shut off", p.name);
                }
                Some(_) if p.since.elapsed().as_secs() >= SHUTDOWN_TIMEOUT_SECS => {
                    done.push(p.name.clone());
                    self.message = format!(
                        "⚠ {} is still running {SHUTDOWN_TIMEOUT_SECS}s after the ACPI request: the guest ignored it. \
                         Try :shutdown {} --mode agent (needs qemu-ga) or D to force off",
                        p.name, p.name
                    );
                }
                Some(_) => {}
            }
        }
        self.shutdowns.retain(|p| !done.contains(&p.name));
    }

    /// Feed live CPU fractions into the rows (for the CPU% column and sorting).
    pub fn apply_metrics(&mut self) {
        for r in &mut self.rows {
            r.cpu_frac = if r.state == DomainState::Running {
                self.metrics.cpu_frac(&r.name)
            } else {
                0.0
            };
        }
    }
    /// Rows after chip + text filter + sort (selection indexes into this list).
    pub fn visible_rows(&self) -> Vec<&DomainRow> {
        let mut out: Vec<&DomainRow> = self
            .rows
            .iter()
            .filter(|r| self.chip.matches(r.state))
            .filter(|r| self.filter.is_empty() || r.name.to_lowercase().contains(&self.filter.to_lowercase()))
            .collect();
        match self.sort {
            Sort::State => out.sort_by_key(|r| (state_rank(r.state), r.name.clone())),
            Sort::Name => out.sort_by_key(|r| r.name.clone()),
            Sort::Cpu => out.sort_by(|a, b| b.cpu_frac.total_cmp(&a.cpu_frac).then(a.name.cmp(&b.name))),
            Sort::Mem => out.sort_by_key(|r| (u64::MAX - r.mem_sort_mib, r.name.clone())),
            Sort::Uptime => out.sort_by_key(|r| r.uptime.clone()),
        }
        out
    }

    /// Selected domain (defaults to arch-dev).
    pub fn selected(&self) -> &DomainRow {
        static EMPTY: std::sync::LazyLock<DomainRow> = std::sync::LazyLock::new(|| DomainRow {
            name: String::new(),
            state: DomainState::ShutOff,
            state_label: "shut off",
            vcpus: 0,
            mem_label: String::new(),
            mem_sort_mib: 0,
            cpu_frac: 0.0,
            uptime: String::new(),
            autostart: false,
        });
        let visible = self.visible_rows();
        visible
            .get(self.selection.min(visible.len().saturating_sub(1)))
            .copied()
            .unwrap_or(&EMPTY)
    }

    /// Cycle state chip forward.
    pub fn cycle_chip(&mut self) {
        self.chip = match self.chip {
            Chip::All => Chip::Running,
            Chip::Running => Chip::Paused,
            Chip::Paused => Chip::Crashed,
            Chip::Crashed => Chip::Off,
            Chip::Off => Chip::All,
        };
        self.selection = 0;
    }

    /// Cycle sort column.
    pub fn cycle_sort(&mut self) {
        let selected = self.selected().name.clone();
        self.sort = match self.sort {
            Sort::State => Sort::Name,
            Sort::Name => Sort::Cpu,
            Sort::Cpu => Sort::Mem,
            Sort::Mem => Sort::Uptime,
            Sort::Uptime => Sort::State,
        };
        self.select_name(&selected);
    }

    /// Set text filter.
    pub fn set_filter(&mut self, f: &str) {
        self.filter = f.to_string();
        self.selection = 0;
    }

    /// Toggle mark on the selected row.
    pub fn toggle_mark(&mut self) {
        let name = self.selected().name.clone();
        if !self.marks.remove(&name) {
            self.marks.insert(name);
        }
    }

    /// Move selection by delta (clamped).
    pub fn move_selection(&mut self, delta: i32) {
        let n = self.visible_rows().len() as i32;
        if n == 0 {
            return;
        }
        let cur = self.selection as i32;
        self.selection = (cur + delta).clamp(0, n - 1) as usize;
        self.update_visual();
    }

    /// Enter/leave VISUAL mode (`V`). Entering marks the current row.
    pub fn toggle_visual(&mut self) {
        if self.visual_anchor.take().is_none() {
            self.visual_anchor = Some(self.selection);
            self.update_visual();
        }
    }

    /// In VISUAL mode, the marks are exactly the rows between anchor and cursor.
    pub fn update_visual(&mut self) {
        let Some(anchor) = self.visual_anchor else { return };
        let (a, b) = (anchor.min(self.selection), anchor.max(self.selection));
        let names: Vec<String> = self
            .visible_rows()
            .iter()
            .enumerate()
            .filter(|(i, _)| (a..=b).contains(i))
            .map(|(_, r)| r.name.clone())
            .collect();
        self.marks = names.into_iter().collect();
    }

    /// `n`/`N`: next/previous row (wrapping) among the filtered rows.
    pub fn next_match(&mut self, delta: i32) {
        let n = self.visible_rows().len() as i32;
        if n > 0 {
            self.selection = (self.selection as i32 + delta).rem_euclid(n) as usize;
            self.update_visual();
        }
    }

    fn counts(&self) -> (usize, usize, usize, usize) {
        let mut running = 0;
        let mut paused = 0;
        let mut crashed = 0;
        let mut off = 0;
        for r in &self.rows {
            match r.state {
                DomainState::Running => running += 1,
                DomainState::Paused => paused += 1,
                DomainState::Crashed => crashed += 1,
                _ => off += 1,
            }
        }
        (running, paused, crashed, off)
    }
}

fn state_rank(s: DomainState) -> u8 {
    match s {
        DomainState::Running => 0,
        DomainState::Paused => 1,
        DomainState::Crashed => 2,
        DomainState::ShutOff => 3,
        _ => 4,
    }
}

fn mib_to_gib(kib: u64) -> f64 {
    kib as f64 / 1024.0 / 1024.0
}

fn mem_mib(label: &str) -> u64 {
    let max = label.split('/').nth(1).unwrap_or("0G");
    let num: f64 = max.trim_end_matches(['G', 'M', 'T']).parse().unwrap_or(0.0);
    if max.contains('G') {
        (num * 1024.0) as u64
    } else {
        num as u64
    }
}

/// Render the full dashboard screen (chrome + body).
pub fn render(frame: &mut Frame, theme: &Theme, state: &DashboardState) {
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
    render_statusbar(frame, theme, state, rows[2]);
    render_message(frame, theme, state, rows[3]);
}

fn render_topbar(frame: &mut Frame, theme: &Theme, area: Rect, uri: &str, connected: bool) {
    crate::ui::chrome::topbar(frame, theme, area, Some(0), uri, connected, "14:32:07");
}

fn panel_block(theme: &Theme, focused: bool, title: &str, right: Option<&str>) -> Block<'static> {
    crate::ui::widgets::panel::block(
        theme,
        focused,
        title,
        right.map(|r| crate::ui::widgets::panel::right(r.to_string(), theme)),
        None,
        None,
    )
}

fn render_body(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let body = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), area.height);
    // Side by side, the metrics panel keeps its reference width (70) and the
    // domains list gives up its spare columns first (101 down to 86). Below
    // that the panel may shrink to 64; narrower terminals get the tabbed layout.
    const LEFT_MAX: u16 = 101;
    const LEFT_MIN: u16 = 86;
    const RIGHT_PREF: u16 = 70;
    const RIGHT_MIN: u16 = 64;
    if body.width < LEFT_MIN + 1 + RIGHT_MIN {
        render_compact(frame, theme, state, body);
        return;
    }
    let left = body
        .width
        .saturating_sub(1 + RIGHT_PREF)
        .clamp(LEFT_MIN, LEFT_MAX);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(left), Constraint::Min(0)])
        .spacing(1)
        .split(body);
    render_left(frame, theme, state, cols[0]);
    render_right(frame, theme, state, cols[1]);
}

/// Compact layout (100-152 terminal cols): domains table, one tabbed detail panel
/// (`[`/`]` switch between Overview, CPU, Mem, Disk, Net), events, host.
fn render_compact(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(11),
            Constraint::Length(6),
            Constraint::Length(5),
        ])
        .split(area);
    render_domains(frame, theme, state, rows[0]);
    const TABS: [&str; 5] = ["Overview", "CPU", "Mem", "Disk", "Net"];
    let running = state.selected().state == DomainState::Running;
    match state.detail_tab {
        0 => render_overview(frame, theme, state, rows[1]),
        _ if !running => render_not_running(frame, theme, state, rows[1]),
        1 => render_cpu(frame, theme, state, rows[1]),
        2 => render_memory(frame, theme, state, rows[1]),
        3 => render_disk(frame, theme, state, rows[1]),
        _ => render_network(frame, theme, state, rows[1]),
    }
    // Tab strip on the panel's bottom border.
    let mut spans = vec![Span::styled(" ", theme.dim())];
    for (i, tab) in TABS.iter().enumerate() {
        let style = if i == state.detail_tab {
            Style::default()
                .fg(theme.tokens.fg)
                .bg(theme.tokens.sel)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.dim()
        };
        spans.push(Span::styled(format!(" {tab} "), style));
    }
    let strip = Rect::new(
        rows[1].x + 2,
        rows[1].y + rows[1].height.saturating_sub(1),
        rows[1].width.saturating_sub(4),
        1,
    );
    frame.render_widget(Paragraph::new(Line::from(spans)), strip);
    render_events(frame, theme, state, rows[2]);
    render_host(frame, theme, state, rows[3]);
}

fn render_left(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(20), Constraint::Length(6), Constraint::Length(5)])
        .split(area);
    render_domains(frame, theme, state, rows[0]);
    render_events(frame, theme, state, rows[1]);
    render_host(frame, theme, state, rows[2]);
}

fn state_style(theme: &Theme, s: DomainState) -> Style {
    let t = &theme.tokens;
    Style::default()
        .fg(match s {
            DomainState::Running => t.green,
            DomainState::Paused => t.yellow,
            DomainState::Crashed => t.red,
            DomainState::PmSuspended => t.magenta,
            _ => t.comment,
        })
        .bg(t.bg)
}

fn level_style(theme: &Theme, v: f64) -> Style {
    let t = &theme.tokens;
    Style::default()
        .fg(if v < 0.5 {
            t.green
        } else if v < 0.8 {
            t.yellow
        } else {
            t.red
        })
        .bg(t.bg)
}

/// First visible row index so the selection stays on screen with a
/// 2-row scrolloff (vim `scrolloff=2`).
fn scroll_offset(selection: usize, total: usize, height: usize) -> usize {
    if height == 0 || total <= height {
        return 0;
    }
    let off = 2.min(height / 2);
    let max_start = total - height;
    selection.saturating_sub(height - 1 - off).min(max_start)
}

fn render_domains(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let t = &theme.tokens;
    let visible = state.visible_rows();
    let (running, paused, crashed, off) = state.counts();
    let mut lines: Vec<Line> = Vec::new();
    // Filter row: text filter left, state chips right-aligned (mockup).
    let filter = if state.filter.is_empty() {
        Span::styled("/ filter…", theme.dim())
    } else {
        Span::styled(format!("/ {}", state.filter), theme.text())
    };
    let chip = |label: String, glyph: Option<(&'static str, ratatui::style::Color)>, active: bool| {
        let base = chip_style(theme, active);
        let mut v = vec![Span::styled(" ", base)];
        if let Some((g, c)) = glyph {
            v.push(Span::styled(format!("{g} "), base.fg(c)));
        }
        v.push(Span::styled(format!("{label} "), base));
        v
    };
    let mut chips: Vec<Span> = Vec::new();
    for (i, group) in [
        chip(format!("all {}", state.rows.len()), None, state.chip == Chip::All),
        chip(
            format!("running {running}"),
            Some(("●", t.green)),
            state.chip == Chip::Running,
        ),
        chip(
            format!("paused {paused}"),
            Some(("‖", t.yellow)),
            state.chip == Chip::Paused,
        ),
        chip(
            format!("crashed {crashed}"),
            Some(("✗", t.red)),
            state.chip == Chip::Crashed,
        ),
        chip(
            format!("off {off}"),
            Some(("○", t.comment)),
            state.chip == Chip::Off,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            chips.push(Span::raw(" "));
        }
        chips.extend(group);
    }
    let inner_w = area.width.saturating_sub(4) as usize;
    let chips_w: usize = chips.iter().map(|s| s.width()).sum();
    let pad = inner_w.saturating_sub(filter.width() + chips_w);
    let mut first = vec![filter, Span::raw(" ".repeat(pad))];
    first.extend(chips);
    lines.push(Line::from(first));
    let mark = |col: Sort| if state.sort == col { " ▾" } else { "" };
    let header = format!(
        "      {:<17} {:<9} {:<3} {:<10} {:<21} {:<9} A",
        format!("NAME{}", mark(Sort::Name)),
        format!("STATE{}", mark(Sort::State)),
        format!("CPU{}", if state.sort == Sort::Cpu { "▾" } else { "" }),
        format!("MEMORY{}", mark(Sort::Mem)),
        "CPU%",
        format!("UPTIME{}", mark(Sort::Uptime)),
    );
    lines.push(Line::from(Span::styled(
        header,
        Style::default()
            .fg(t.comment)
            .bg(t.bg)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "─".repeat(inner_w),
        Style::default().fg(t.hl).bg(t.bg),
    )));
    let body_h = (area.height as usize).saturating_sub(2 + lines.len());
    let start = scroll_offset(state.selection, visible.len(), body_h);
    for (i, r) in visible.iter().enumerate().skip(start).take(body_h) {
        let selected = i == state.selection;
        let marked = state.marks.contains(&r.name);
        let st = state_style(theme, r.state);
        let bg = if selected { t.sel } else { t.bg };
        let marker = if selected { "▌" } else { " " };
        let mark = if marked { "◆" } else { " " };
        let name: String = r.name.chars().take(17).collect();
        let running = r.state == DomainState::Running;
        let (fill, rest) = charts::hbar(r.cpu_frac, 16);
        let pct = if running {
            format!("{:>5}", format!("{}%", (r.cpu_frac * 100.0).round() as u32))
        } else {
            String::from("  —  ")
        };
        let cpu_fg = if running {
            level_style(theme, r.cpu_frac).fg
        } else {
            Some(t.comment)
        }
        .unwrap_or(t.fg);
        let name_style = Style::default()
            .fg(if running || selected { t.fg } else { t.fg2 })
            .add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        let st_fg = st.fg.unwrap_or(t.fg);
        // The line style carries the row background, so separators and the
        // trailing cells are filled too (no "striped" selection).
        lines.push(
            Line::from(vec![
                Span::styled(marker, Style::default().fg(t.blue)),
                Span::raw(" "),
                Span::styled(mark, Style::default().fg(t.magenta)),
                Span::raw(" "),
                Span::styled(format!("{}", r.state.glyph()), Style::default().fg(st_fg)),
                Span::raw(" "),
                Span::styled(format!("{name:17}"), name_style),
                Span::raw(" "),
                Span::styled(format!("{:9}", r.state_label), Style::default().fg(st_fg)),
                Span::raw(" "),
                Span::styled(format!("{:>3}", r.vcpus), Style::default().fg(t.fg2)),
                Span::raw(" "),
                Span::styled(format!("{:10}", r.mem_label), Style::default().fg(t.fg2)),
                Span::raw(" "),
                Span::styled(
                    if running { fill } else { String::new() },
                    Style::default().fg(cpu_fg),
                ),
                Span::styled(
                    if running { rest } else { " ".repeat(16) },
                    Style::default().fg(t.hl),
                ),
                Span::styled(pct, Style::default().fg(cpu_fg)),
                Span::raw(" "),
                Span::styled(format!("{:9}", r.uptime), Style::default().fg(t.fg2)),
                Span::raw(" "),
                Span::styled(if r.autostart { "✓" } else { "·" }, Style::default().fg(t.green)),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    if visible.is_empty() {
        lines.push(Line::from(Span::styled(
            if state.rows.is_empty() {
                "  no domains"
            } else {
                "  no domains match the filter"
            },
            theme.dim(),
        )));
    }
    let pos = if visible.is_empty() {
        0
    } else {
        state.selection + 1
    };
    let footer_right = if state.marks.is_empty() {
        format!("{pos}/{}", visible.len())
    } else {
        format!("{pos}/{} · {} marked", visible.len(), state.marks.len())
    };
    let footer_left = format!("sort: {}", state.sort.label());
    let block = crate::ui::widgets::panel::block(
        theme,
        true,
        "Domains",
        Some(crate::ui::widgets::panel::right(
            format!("{} total", state.rows.len()),
            theme,
        )),
        Some(&footer_left),
        Some(&footer_right),
    );
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn chip_style(theme: &Theme, active: bool) -> Style {
    if active {
        Style::default().fg(theme.tokens.fg).bg(theme.tokens.sel)
    } else {
        Style::default().fg(theme.tokens.fg2).bg(theme.tokens.hl)
    }
}

/// Glyph + colour for an event (`Started`, `Stopped`, `Crashed`, …).
fn event_look(theme: &Theme, event: &str) -> (&'static str, ratatui::style::Color) {
    let t = &theme.tokens;
    match event {
        "Started" | "Resumed" => ("●", t.green),
        "Suspended" | "PMSuspended" => ("‖", t.yellow),
        "Crashed" => ("✗", t.red),
        "Stopped" | "Shutdown" => ("○", t.comment),
        "Defined" | "Undefined" => ("◆", t.blue),
        _ => ("◆", t.magenta),
    }
}

fn render_events(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let t = &theme.tokens;
    let demo_rows = [
        ("14:31:55", "k8s-worker-02", "started", "booted", "●", t.green),
        (
            "14:29:02",
            "ubuntu-24-ci",
            "crashed",
            "guest-panicked",
            "✗",
            t.red,
        ),
        (
            "14:12:40",
            "nixos-lab",
            "suspended",
            "paused by user",
            "‖",
            t.yellow,
        ),
        (
            "13:58:11",
            "arch-dev",
            "snapshot",
            "created \"pre-kernel-6.17\"",
            "◆",
            t.magenta,
        ),
    ];
    let rows: Vec<(String, String, String, String, &str, ratatui::style::Color)> = if state.live {
        state
            .recent_events
            .iter()
            .rev()
            .take(4)
            .map(|e| {
                let (g, c) = event_look(theme, &e.event);
                (
                    e.timestamp.clone(),
                    e.object.clone(),
                    e.event.to_lowercase(),
                    e.detail.to_lowercase(),
                    g,
                    c,
                )
            })
            .collect()
    } else {
        demo_rows
            .iter()
            .map(|(ts, n, ev, d, g, c)| {
                (
                    ts.to_string(),
                    n.to_string(),
                    ev.to_string(),
                    d.to_string(),
                    *g,
                    *c,
                )
            })
            .collect()
    };
    let mut lines: Vec<Line> = rows
        .iter()
        .map(|(ts, name, ev, detail, g, c)| {
            let name: String = name.chars().take(15).collect();
            Line::from(vec![
                Span::styled(format!("{ts:<10}"), theme.dim()),
                Span::styled(format!("{g} "), Style::default().fg(*c)),
                Span::raw(format!("{name:<16}")),
                Span::styled(format!("{ev:<11}"), Style::default().fg(*c)),
                Span::styled(detail.clone(), theme.dim()),
            ])
        })
        .collect();
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("waiting for events…", theme.dim())));
    }
    let block = panel_block(theme, false, "Events", Some("libvirt event stream"));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_host(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let gauges: Vec<(String, f64, String)> = if state.live {
        state.host_gauges.clone()
    } else {
        fixtures::HOST_GAUGES
            .iter()
            .map(|(l, f, v)| (l.trim_end().to_string(), *f, v.to_string()))
            .collect()
    };
    // Two gauges per line, row-major (CPU | Pool, RAM | NVMe, Swap | Huge).
    // Labels keep their full name (pool names were cut to 5 chars); the bar
    // shrinks to make room.
    let label_of = |l: &str| l.trim_start_matches("pool:").to_string();
    let label_w = gauges
        .iter()
        .map(|(l, _, _)| label_of(l).chars().count())
        .max()
        .unwrap_or(4)
        .clamp(5, 16);
    let inner_w = area.width.saturating_sub(2) as usize;
    // Two cells (label + space + bar + space + 10-char value) and a 3-col gap.
    let bar_w = (inner_w.saturating_sub(3) / 2)
        .saturating_sub(label_w + 1 + 11)
        .clamp(6, 22);
    let cell = |(label, frac, val): &(String, f64, String)| -> Vec<Span<'static>> {
        let label: String = label_of(label).chars().take(label_w).collect();
        let (on, off) = charts::lg(*frac, bar_w);
        vec![
            Span::styled(format!("{label:<w$} ", w = label_w), theme.dim()),
            Span::styled(on, level_style(theme, *frac)),
            Span::styled(off, Style::default().fg(theme.tokens.hl).bg(theme.tokens.bg)),
            Span::styled(format!(" {val:<10}"), theme.secondary()),
        ]
    };
    let mut lines: Vec<Line> = Vec::new();
    for pair in gauges.chunks(2).take(3) {
        let mut spans = cell(&pair[0]);
        if let Some(second) = pair.get(1) {
            spans.push(Span::raw("   "));
            spans.extend(cell(second));
        }
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("collecting…", theme.dim())));
    }
    let (title, right) = if state.live {
        (format!("Host · {}", state.hostname), state.host_title.clone())
    } else {
        (
            String::from("Host · forge"),
            String::from("Ryzen 9 7950X · 32t · 64 GiB"),
        )
    };
    let block = panel_block(theme, false, &title, Some(&right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_right(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),
            Constraint::Length(11),
            Constraint::Length(8),
            Constraint::Min(9),
        ])
        .split(area);
    render_overview(frame, theme, state, rows[0]);
    if state.selected().state != DomainState::Running {
        let rest = Rect::new(
            rows[1].x,
            rows[1].y,
            rows[1].width,
            area.height.saturating_sub(rows[0].height),
        );
        render_not_running(frame, theme, state, rest);
        return;
    }
    render_cpu(frame, theme, state, rows[1]);
    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .spacing(1)
        .split(rows[2]);
    render_memory(frame, theme, state, mid[0]);
    render_disk(frame, theme, state, mid[1]);
    render_network(frame, theme, state, rows[3]);
}

/// Action chips for the selected domain, wrapped to `width` cells.
fn action_chips(theme: &Theme, state: DomainState, width: usize) -> Vec<Line<'static>> {
    let chips: &[(&str, &str)] = match state {
        DomainState::Running => &[
            ("S", "shutdown"),
            ("r", "reboot"),
            ("p", "pause"),
            ("c", "console"),
            ("v", "viewer"),
            ("␣s", "snapshot"),
            ("e", "edit xml"),
        ],
        DomainState::Paused => &[
            ("p", "resume"),
            ("D", "destroy"),
            ("␣s", "snapshot"),
            ("e", "edit xml"),
        ],
        _ => &[
            ("s", "start"),
            ("e", "edit xml"),
            ("␣c", "clone"),
            ("␣r", "rename"),
            ("X", "undefine"),
        ],
    };
    let mut lines = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    let mut w = 0;
    for (k, label) in chips {
        let cw = k.chars().count() + label.chars().count() + 3;
        if w > 0 && w + 1 + cw > width {
            lines.push(Line::from(std::mem::take(&mut cur)));
            w = 0;
        }
        if w > 0 {
            cur.push(Span::raw(" "));
            w += 1;
        }
        cur.push(Span::styled(" ", chip_style(theme, false)));
        cur.push(Span::styled(
            k.to_string(),
            chip_style(theme, false).patch(theme.key()),
        ));
        cur.push(Span::styled(format!(" {label} "), chip_style(theme, false)));
        w += cw;
    }
    if !cur.is_empty() {
        lines.push(Line::from(cur));
    }
    lines
}

fn render_overview(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let sel = state.selected();
    let t = &theme.tokens;
    let st = state_style(theme, sel.state);
    let title_right = Line::from(Span::styled(
        format!("{} {}", sel.state.glyph(), sel.state_label),
        st,
    ));
    let kv: Vec<(&str, String, &str, String)> = if !state.live {
        vec![
            (
                "OS       ",
                "Arch Linux · x86_64".into(),
                "Uptime  ",
                "3d 04h 17m".into(),
            ),
            (
                "Machine  ",
                "pc-q35-10.1 · KVM".into(),
                "IP      ",
                "192.168.122.48".into(),
            ),
            (
                "Firmware ",
                "UEFI · OVMF · SB off".into(),
                "Display ",
                "SPICE :5901".into(),
            ),
            (
                "vCPU     ",
                "8 · host-passthrough".into(),
                "Agent   ",
                "qemu-ga ✓".into(),
            ),
            (
                "Disks    ",
                "vda 120G qcow2 · sda iso".into(),
                "Auto    ",
                "✓ on boot".into(),
            ),
        ]
    } else if let Some(c) = state.configs.get(&sel.name) {
        let disks = c
            .disks
            .iter()
            .map(|d| {
                if d.device == "cdrom" {
                    format!("{} iso", d.target)
                } else {
                    d.target.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let display = c
            .graphics
            .as_ref()
            .map(|g| {
                if g.port.is_empty() || g.port == "-1" {
                    g.kind.to_uppercase()
                } else {
                    format!("{} :{}", g.kind.to_uppercase(), g.port)
                }
            })
            .unwrap_or_else(|| String::from("—"));
        vec![
            (
                "OS       ",
                format!("{} · {}", c.os_label(), c.arch),
                "Uptime  ",
                sel.uptime.clone(),
            ),
            (
                "Machine  ",
                c.machine.clone(),
                "IP      ",
                state
                    .ips
                    .get(&sel.name)
                    .cloned()
                    .unwrap_or_else(|| String::from("—")),
            ),
            ("Firmware ", c.firmware_label().to_string(), "Display ", display),
            (
                "vCPU     ",
                format!("{} · {}", c.current_vcpus, c.cpu_mode),
                "Agent   ",
                if c.has_agent {
                    "qemu-ga channel".into()
                } else {
                    "—".into()
                },
            ),
            (
                "Disks    ",
                if disks.is_empty() { "—".into() } else { disks },
                "Auto    ",
                if sel.autostart {
                    "✓ on boot".into()
                } else {
                    "· off".into()
                },
            ),
        ]
    } else {
        vec![("", String::from("loading configuration…"), "", String::new())]
    };
    let mut lines: Vec<Line> = Vec::new();
    for (k1, v1, k2, v2) in kv {
        let v1: String = v1.chars().take(28).collect();
        lines.push(Line::from(vec![
            Span::styled(k1, theme.dim()),
            Span::raw(format!("{v1:<29}")),
            Span::styled(k2, theme.dim()),
            Span::styled(
                v2,
                if k2.starts_with("IP") {
                    Style::default().fg(t.cyan).bg(t.bg)
                } else {
                    theme.text()
                },
            ),
        ]));
    }
    let chips = action_chips(theme, sel.state, area.width.saturating_sub(4) as usize);
    let free = (area.height as usize).saturating_sub(2 + lines.len());
    if free > chips.len() {
        lines.push(Line::from(""));
    }
    lines.extend(chips);
    let block = crate::ui::widgets::panel::block(theme, false, &sel.name, Some(title_right), None, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

/// Not-running variant: config summary plus a large start hint.
fn render_not_running(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let sel = state.selected();
    let mut lines = vec![
        Line::from(vec![
            Span::styled("State    ", theme.dim()),
            Span::styled(sel.state_label, state_style(theme, sel.state)),
        ]),
        Line::from(vec![
            Span::styled("vCPU     ", theme.dim()),
            Span::raw(format!("{}", sel.vcpus)),
        ]),
        Line::from(vec![
            Span::styled("Memory   ", theme.dim()),
            Span::raw(sel.mem_label.clone()),
        ]),
        Line::from(""),
    ];
    let hint = match sel.state {
        DomainState::Paused => ("p", "  resume"),
        DomainState::PmSuspended => ("s", "  wake up"),
        _ => ("s", "  start"),
    };
    lines.push(Line::from(vec![
        Span::styled(hint.0, theme.key()),
        Span::styled(hint.1, theme.text()),
    ]));
    let title = if sel.state == DomainState::Paused {
        "Paused"
    } else {
        "Not running"
    };
    let block = panel_block(theme, false, title, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

/// Live series for the selected domain, or the mockup series in demo mode.
fn selected_series(state: &DashboardState) -> Option<&crate::metrics::store::DomainSeries> {
    if state.live {
        state.metrics.domain(&state.selected().name)
    } else {
        None
    }
}

fn render_cpu(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    use crate::metrics::store::padded;
    let width = (area.width.saturating_sub(4 + 5) as usize).clamp(10, 60);
    let (data, right) = match selected_series(state) {
        Some(s) => {
            let v: Vec<f64> = s.cpu.values().iter().map(|x| f64::from(*x) / 100.0).collect();
            (
                padded(&v, crate::metrics::store::window_len()),
                format!(
                    "{:.0}% · {} vCPU · {}",
                    s.now.cpu_pct,
                    s.now.vcpus,
                    crate::metrics::store::window_label()
                ),
            )
        }
        None if state.live => (
            vec![0.0; crate::metrics::store::window_len()],
            String::from("collecting…"),
        ),
        None => (
            charts::series(7, 90, 0.42, 0.22, 0.1),
            format!("42% · 8 vCPU · {}", crate::metrics::store::window_label()),
        ),
    };
    let rows = charts::area(theme.graphs, &data, width, 8, BrailleMode::Fill);
    let t = &theme.tokens;
    let colors = theme.chart_rows(&[
        t.red, t.magenta, t.magenta, t.blue, t.blue, t.cyan, t.cyan, t.teal,
    ]);
    let labels = ["100%", "", "", "", " 50%", "", "", "  0%"];
    let mut lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            Line::from(vec![
                Span::styled(format!("{:<5}", labels[i]), theme.dim()),
                Span::styled(r.clone(), Style::default().fg(colors[i]).bg(t.bg)),
            ])
        })
        .collect();
    // X axis follows the history window (`t`): 60s, 5m or 1h.
    let (left, mid) = match crate::metrics::store::window_label() {
        "5m" => ("-5m", "-2.5m"),
        "1h" => ("-1h", "-30m"),
        _ => ("-60s", "-30s"),
    };
    let gap = width.saturating_sub(left.len() + mid.len() + 3) / 2;
    lines.push(Line::from(Span::styled(
        format!(
            "     {left}{}{mid}{}now",
            " ".repeat(gap.saturating_sub(left.len().saturating_sub(4))),
            " ".repeat(width.saturating_sub(left.len() + mid.len() + 3 + gap))
        ),
        theme.dim(),
    )));
    let (pct, rest) = right.split_once(' ').unwrap_or((right.as_str(), ""));
    let right_line = Line::from(vec![
        Span::styled(pct.to_string(), theme.accent().add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {rest}"), theme.dim()),
    ]);
    let block = crate::ui::widgets::panel::block(theme, false, "CPU", Some(right_line), None, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_memory(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    use crate::metrics::store::{normalize, padded};
    let t = &theme.tokens;
    let width = (area.width.saturating_sub(4) as usize).min(28);
    let gib = |kib: u64| kib as f64 / 1024.0 / 1024.0;
    let (frac, data, used, max, foot) = match selected_series(state) {
        Some(s) => {
            let max = s.now.mem_max_kib.max(1);
            let hist: Vec<f32> = s.mem.values();
            (
                s.now.mem_used_kib as f64 / max as f64,
                padded(&normalize(&hist, max as f32), crate::metrics::store::window_len()),
                gib(s.now.mem_used_kib),
                gib(max),
                if s.now.mem_from_guest {
                    format!("rss {:.1}G · guest stats", gib(s.now.rss_kib))
                } else {
                    format!("rss {:.1}G · no guest stats", gib(s.now.rss_kib))
                },
            )
        }
        None if state.live => (
            0.0,
            vec![0.0; crate::metrics::store::window_len()],
            0.0,
            0.0,
            String::from("collecting…"),
        ),
        None => (
            0.78,
            charts::series(11, 60, 0.74, 0.06, 0.2),
            12.4,
            16.0,
            String::from("rss 13.1G · swap-in 0 · balloon"),
        ),
    };
    let (on, off) = charts::lg(frac.clamp(0.0, 1.0), width.saturating_sub(5));
    let mut lines = vec![Line::from(vec![
        Span::styled(on, Style::default().fg(t.magenta).bg(t.bg)),
        Span::styled(off, Style::default().fg(t.hl).bg(t.bg)),
        Span::styled(format!(" {:.0}%", frac * 100.0), theme.secondary()),
    ])];
    for r in charts::spark_rows(&data, width, 3) {
        lines.push(Line::from(Span::styled(
            r,
            Style::default().fg(t.magenta).bg(t.bg),
        )));
    }
    lines.push(Line::from(Span::styled(foot, theme.dim())));
    // Sub-GiB guests read better in MiB (`48/64 MiB` rather than `0.0/0 GiB`).
    let (used_txt, max_txt) = if max > 0.0 && max < 1.0 {
        (
            format!("{:.0}", used * 1024.0),
            format!("/{:.0} MiB", max * 1024.0),
        )
    } else {
        (format!("{used:.1}"), format!("/{max:.0} GiB"))
    };
    let right = Line::from(vec![
        Span::styled(used_txt, Style::default().fg(t.magenta)),
        Span::styled(max_txt, theme.dim()),
    ]);
    let block = crate::ui::widgets::panel::block(theme, false, "Memory", Some(right), None, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_disk(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    use crate::metrics::store::{fmt_bytes_rate, normalize, padded};
    let t = &theme.tokens;
    let width = (area.width.saturating_sub(4) as usize).min(28);
    let (rd, wr, rd_txt, wr_txt, title, iops) = match selected_series(state) {
        Some(s) => {
            // Read and write share one scale (min. full scale 1 MiB/s).
            let both: Vec<f32> = s.disk_rd.values().into_iter().chain(s.disk_wr.values()).collect();
            let max = both.iter().copied().fold(1024.0 * 1024.0, f32::max);
            (
                padded(
                    &normalize(&s.disk_rd.values(), max),
                    crate::metrics::store::window_len(),
                ),
                padded(
                    &normalize(&s.disk_wr.values(), max),
                    crate::metrics::store::window_len(),
                ),
                fmt_bytes_rate(s.now.rd_bps),
                fmt_bytes_rate(s.now.wr_bps),
                format!(
                    "Disk I/O · {}",
                    if s.now.first_disk.is_empty() {
                        "—"
                    } else {
                        &s.now.first_disk
                    }
                ),
                format!("{:.0} IOPS", s.now.iops),
            )
        }
        None if state.live => (
            vec![0.0; crate::metrics::store::window_len()],
            vec![0.0; crate::metrics::store::window_len()],
            String::from("—"),
            String::from("—"),
            String::from("Disk I/O"),
            String::new(),
        ),
        None => (
            charts::series(21, 60, 0.35, 0.5, 0.2),
            charts::series(23, 60, 0.15, 0.3, 0.3),
            String::from("12.4 MiB/s"),
            String::from(" 3.1 MiB/s"),
            String::from("Disk I/O · vda"),
            String::from("1.2k IOPS"),
        ),
    };
    let mut lines = vec![Line::from(vec![
        Span::styled("read  ", Style::default().fg(t.green).bg(t.bg)),
        Span::styled(rd_txt, theme.secondary()),
    ])];
    for r in charts::spark_rows(&rd, width, 2) {
        lines.push(Line::from(Span::styled(r, Style::default().fg(t.green).bg(t.bg))));
    }
    lines.push(Line::from(vec![
        Span::styled("write ", Style::default().fg(t.orange).bg(t.bg)),
        Span::styled(wr_txt, theme.secondary()),
    ]));
    for r in charts::spark_rows(&wr, width, 2) {
        lines.push(Line::from(Span::styled(
            r,
            Style::default().fg(t.orange).bg(t.bg),
        )));
    }
    let block = panel_block(
        theme,
        false,
        &title,
        if iops.is_empty() { None } else { Some(&iops) },
    );
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_network(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    use crate::metrics::store::{fmt_bits_rate, fmt_bytes, normalize, padded};
    let t = &theme.tokens;
    let width = (area.width.saturating_sub(4) as usize).min(64);
    let (rx, tx, rx_txt, tx_txt, title, foot) = match selected_series(state) {
        Some(s) => {
            // 1 Mb/s minimum full scale so idle links stay flat.
            let floor = 125_000.0;
            (
                padded(
                    &normalize(&s.net_rx.values(), floor),
                    crate::metrics::store::window_len(),
                ),
                padded(
                    &normalize(&s.net_tx.values(), floor),
                    crate::metrics::store::window_len(),
                ),
                fmt_bits_rate(s.now.rx_bps),
                fmt_bits_rate(s.now.tx_bps),
                format!(
                    "Network · {}",
                    if s.now.first_iface.is_empty() {
                        "—"
                    } else {
                        &s.now.first_iface
                    }
                ),
                format!(
                    "total ↓ {}  ↑ {}  · drops {} · errs {}",
                    fmt_bytes(s.now.rx_total as f64),
                    fmt_bytes(s.now.tx_total as f64),
                    s.now.drops,
                    s.now.errs
                ),
            )
        }
        None if state.live => (
            vec![0.0; crate::metrics::store::window_len()],
            vec![0.0; crate::metrics::store::window_len()],
            String::from("—"),
            String::from("—"),
            String::from("Network"),
            String::from("collecting…"),
        ),
        None => (
            charts::series(31, 90, 0.45, 0.35, 0.12),
            charts::series(37, 90, 0.2, 0.25, 0.2),
            String::from("48.2 Mb/s"),
            String::from("6.1 Mb/s"),
            String::from("Network · vnet3"),
            String::from("total ↓ 18.4 GiB  ↑ 2.1 GiB  · drops 0 · errs 0 · bridge virbr0"),
        ),
    };
    let mut lines: Vec<Line> = charts::area(theme.graphs, &rx, width, 4, BrailleMode::Fill)
        .into_iter()
        .map(|r| Line::from(Span::styled(r, Style::default().fg(t.cyan).bg(t.bg))))
        .collect();
    lines.push(Line::from(Span::styled(
        "╌".repeat(width),
        Style::default().fg(t.gutter).bg(t.bg),
    )));
    for r in charts::area(theme.graphs, &tx, width, 3, BrailleMode::Down) {
        lines.push(Line::from(Span::styled(
            r,
            Style::default().fg(t.yellow).bg(t.bg),
        )));
    }
    lines.push(Line::from(Span::styled(foot, theme.dim())));
    let right = Line::from(vec![
        Span::styled(format!("↓ {rx_txt}"), Style::default().fg(t.cyan)),
        Span::styled("  ", theme.dim()),
        Span::styled(format!("↑ {tx_txt}"), Style::default().fg(t.yellow)),
    ]);
    let block = crate::ui::widgets::panel::block(theme, false, &title, Some(right), None, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_statusbar(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    let t = &theme.tokens;
    let sel = state.selected();
    let mut counts = (0, 0, 0, 0);
    for r in &state.rows {
        match r.state {
            DomainState::Running => counts.0 += 1,
            DomainState::Paused => counts.1 += 1,
            DomainState::Crashed => counts.2 += 1,
            _ => counts.3 += 1,
        }
    }
    let mut spans = vec![
        Span::styled(
            if state.visual_anchor.is_some() {
                " VISUAL "
            } else {
                " NORMAL "
            },
            Style::default()
                .fg(t.bg2)
                .bg(if state.visual_anchor.is_some() {
                    t.magenta
                } else {
                    t.blue
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" domains › {} ", sel.name),
            Style::default().fg(t.fg2).bg(t.hl),
        ),
        Span::styled(
            "  j/k move  ⏎ open  s start  S shutdown  p pause  m mark  ␣ leader  : cmd  ? help",
            theme.dim(),
        ),
        Span::styled(" ", Style::default().bg(t.hl)),
        Span::styled(format!("● {}", counts.0), Style::default().fg(t.green).bg(t.hl)),
        Span::styled(
            format!("  ‖ {}", counts.1),
            Style::default().fg(t.yellow).bg(t.hl),
        ),
        Span::styled(format!("  ✗ {}", counts.2), Style::default().fg(t.red).bg(t.hl)),
        Span::styled(
            format!("  ○ {} ", counts.3),
            Style::default().fg(t.comment).bg(t.hl),
        ),
    ];
    if !state.marks.is_empty() {
        spans.push(Span::styled(
            format!(" {} marked ", state.marks.len()),
            Style::default()
                .fg(t.bg2)
                .bg(t.magenta)
                .add_modifier(Modifier::BOLD),
        ));
    }
    crate::ui::chrome::statusbar(frame, theme, area, spans, 3);
}

fn render_message(frame: &mut Frame, theme: &Theme, state: &DashboardState, area: Rect) {
    crate::ui::chrome::message_line(frame, theme, area, &state.message);
}
