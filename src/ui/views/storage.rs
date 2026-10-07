//! Storage view (screen 08, PLAN.md section 9.11, P10): pools, volumes, media picker.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::theme::Theme;
use crate::ui::widgets::charts;

/// Storage pool row (2 lines).
#[derive(Debug, Clone)]
pub struct PoolRow {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub frac: f64,
    pub value: String,
    pub active: bool,
}

/// Volume row.
#[derive(Debug, Clone)]
pub struct VolumeRow {
    pub name: String,
    pub format: String,
    pub capacity: String,
    pub alloc: String,
    pub path: String,
    pub used_by: String,
}

/// ISO entry for the picker.
#[derive(Debug, Clone)]
pub struct IsoEntry {
    pub pool: String,
    pub name: String,
    pub size: String,
    pub date: String,
}

/// Media picker state (PICKER mode).
#[derive(Debug, Clone)]
pub struct MediaPicker {
    pub domain: String,
    pub target: String,
    pub bus: String,
    pub current: String,
    pub query: String,
    pub pool_idx: usize,
    pub pools: Vec<String>,
    pub selected: usize,
    pub live: bool,
    pub config: bool,
}

/// Storage screen state.
#[derive(Debug, Clone, Default)]
pub struct StorageState {
    pub pools: Vec<PoolRow>,
    pub volumes: Vec<VolumeRow>,
    pub isos: Vec<IsoEntry>,
    pub pool_selected: usize,
    pub vol_selected: usize,
    /// True while the pools pane (not volumes) has focus (`h`/`l`).
    pub focus_pools: bool,
    pub message: String,
    pub uri: String,
    pub connected: bool,
    pub picker: Option<MediaPicker>,
    /// (domain, disk source path) for every defined domain (live mode).
    pub attachments: Vec<(String, String)>,
}

impl StorageState {
    /// Demo state from docs/mockups/Storage.dc.html renderVals().
    pub fn demo() -> Self {
        Self {
            pools: vec![
                PoolRow {
                    name: String::from("default"),
                    kind: String::from("dir"),
                    path: String::from("/var/lib/libvirt/images"),
                    frac: 0.66,
                    value: String::from("612 / 931 GiB"),
                    active: true,
                },
                PoolRow {
                    name: String::from("nvme-fast"),
                    kind: String::from("logical"),
                    path: String::from("vg_nvme"),
                    frac: 0.67,
                    value: String::from("1.2 / 1.8 TiB"),
                    active: true,
                },
                PoolRow {
                    name: String::from("isos"),
                    kind: String::from("dir"),
                    path: String::from("/var/lib/libvirt/isos"),
                    frac: 0.31,
                    value: String::from("62 / 200 GiB"),
                    active: true,
                },
                PoolRow {
                    name: String::from("nfs-backup"),
                    kind: String::from("netfs"),
                    path: String::from("nas:/export/vm"),
                    frac: 0.82,
                    value: String::from("6.6 / 8.0 TiB"),
                    active: true,
                },
                PoolRow {
                    name: String::from("zfs-tank"),
                    kind: String::from("zfs"),
                    path: String::from("tank/vms"),
                    frac: 0.0,
                    value: String::from("inactive"),
                    active: false,
                },
            ],
            volumes: vec![
                VolumeRow {
                    name: String::from("arch-dev.qcow2"),
                    format: String::from("qcow2"),
                    capacity: String::from("120G"),
                    alloc: String::from("48.2G"),
                    path: String::from("/vg_nvme/arch-dev.qcow2"),
                    used_by: String::from("arch-dev"),
                },
                VolumeRow {
                    name: String::from("arch-dev-data.qcow2"),
                    format: String::from("qcow2"),
                    capacity: String::from("500G"),
                    alloc: String::from("311G"),
                    path: String::from("/vg_nvme/arch-dev-data.qcow2"),
                    used_by: String::from("arch-dev"),
                },
                VolumeRow {
                    name: String::from("win11-gaming.qcow2"),
                    format: String::from("qcow2"),
                    capacity: String::from("256G"),
                    alloc: String::from("201G"),
                    path: String::from("/vg_nvme/win11-gaming.qcow2"),
                    used_by: String::from("win11-gaming"),
                },
                VolumeRow {
                    name: String::from("k8s-cp-01.qcow2"),
                    format: String::from("qcow2"),
                    capacity: String::from("40G"),
                    alloc: String::from("18.1G"),
                    path: String::from("/vg_nvme/k8s-cp-01.qcow2"),
                    used_by: String::from("k8s-cp-01"),
                },
                VolumeRow {
                    name: String::from("k8s-worker-01.qcow2"),
                    format: String::from("qcow2"),
                    capacity: String::from("80G"),
                    alloc: String::from("52.7G"),
                    path: String::from("/vg_nvme/k8s-worker-01.qcow2"),
                    used_by: String::from("k8s-worker-01"),
                },
                VolumeRow {
                    name: String::from("k8s-worker-02.qcow2"),
                    format: String::from("qcow2"),
                    capacity: String::from("80G"),
                    alloc: String::from("9.4G"),
                    path: String::from("/vg_nvme/k8s-worker-02.qcow2"),
                    used_by: String::from("k8s-worker-02"),
                },
                VolumeRow {
                    name: String::from("debian-base.qcow2"),
                    format: String::from("qcow2"),
                    capacity: String::from("20G"),
                    alloc: String::from("2.1G"),
                    path: String::from("/vg_nvme/debian-base.qcow2"),
                    used_by: String::from("◆ backing of 3"),
                },
                VolumeRow {
                    name: String::from("truenas-boot.raw"),
                    format: String::from("raw"),
                    capacity: String::from("32G"),
                    alloc: String::from("32G"),
                    path: String::from("/vg_nvme/truenas-boot.raw"),
                    used_by: String::from("truenas-scale"),
                },
                VolumeRow {
                    name: String::from("old-test.qcow2"),
                    format: String::from("qcow2"),
                    capacity: String::from("60G"),
                    alloc: String::from("14.0G"),
                    path: String::from("/vg_nvme/old-test.qcow2"),
                    used_by: String::from("⚠ orphan"),
                },
            ],
            isos: vec![
                IsoEntry {
                    pool: String::from("isos"),
                    name: String::from("archlinux-2026.10.01-x86_64.iso"),
                    size: String::from("1.2G"),
                    date: String::from("5 days ago"),
                },
                IsoEntry {
                    pool: String::from("isos"),
                    name: String::from("archlinux-2026.09.01-x86_64.iso"),
                    size: String::from("1.2G"),
                    date: String::from("5 weeks ago"),
                },
                IsoEntry {
                    pool: String::from("isos"),
                    name: String::from("Archcraft-Linux-2026.08-desktop.iso"),
                    size: String::from("3.4G"),
                    date: String::from("2 months ago"),
                },
            ],
            pool_selected: 1,
            vol_selected: 0,
            focus_pools: false,
            message: String::from("-- insert media --"),
            uri: String::from("qemu:///system"),
            connected: true,
            picker: None,
            attachments: Vec::new(),
        }
    }

    /// Demo with the media picker open (acceptance snapshot).
    pub fn demo_picker() -> Self {
        let mut st = Self::demo();
        st.picker_query_set("arch");
        st.picker = Some(MediaPicker {
            domain: String::from("arch-dev"),
            target: String::from("sda"),
            bus: String::from("sata"),
            current: String::from("archlinux-2026.09.01-x86_64.iso"),
            query: String::from("arch"),
            pool_idx: 0,
            pools: vec![String::from("isos"), String::from("default")],
            selected: 0,
            live: true,
            config: true,
        });
        st
    }

    fn picker_query_set(&mut self, _q: &str) {}

    /// ISOs filtered by picker query (fuzzy: substring, case-insensitive).
    pub fn picker_isos(&self) -> Vec<&IsoEntry> {
        let Some(p) = &self.picker else {
            return vec![];
        };
        let q = p.query.to_lowercase();
        self.isos
            .iter()
            .filter(|i| {
                (p.pools.get(p.pool_idx).is_none_or(|pool| &i.pool == pool))
                    && (q.is_empty() || i.name.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Selected pool name.
    pub fn selected_pool(&self) -> &str {
        self.pools
            .get(self.pool_selected)
            .map(|p| p.name.as_str())
            .unwrap_or("")
    }

    /// Selected volume.
    pub fn selected_vol(&self) -> Option<&VolumeRow> {
        self.volumes.get(self.vol_selected)
    }
}

/// USED BY for a volume path: matching domain, or orphan.
pub fn used_by(path: &str, attachments: &[(&str, &str)]) -> String {
    for (domain, src) in attachments {
        if *src == path {
            return domain.to_string();
        }
    }
    String::from("⚠ orphan")
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

fn usage_frac(cap: &str, alloc: &str) -> f64 {
    // Sizes carry their own unit (`120G`, `1.2T`, `512M`): compare in bytes.
    let num = |s: &str| {
        let s = s.trim().trim_end_matches("iB").trim_end_matches('B');
        let (n, mult) = match s.chars().last() {
            Some('K') => (&s[..s.len() - 1], 1024.0),
            Some('M') => (&s[..s.len() - 1], 1024.0 * 1024.0),
            Some('G') => (&s[..s.len() - 1], 1024.0 * 1024.0 * 1024.0),
            Some('T') => (&s[..s.len() - 1], 1024.0_f64.powi(4)),
            _ => (s, 1.0),
        };
        n.trim().parse::<f64>().unwrap_or(0.0) * mult
    };
    let c = num(cap);
    if c <= 0.0 {
        0.0
    } else {
        (num(alloc) / c).clamp(0.0, 1.0)
    }
}

/// Render storage (dimmed behind the picker, like modals).
pub fn render(frame: &mut Frame, theme: &Theme, state: &StorageState, dimmed: bool) {
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
    render_body(frame, theme, state, rows[1], dimmed);
    render_statusbar(frame, theme, state, rows[2], state.picker.is_some());
    render_message(frame, theme, state, rows[3]);
    if let Some(picker) = &state.picker {
        crate::ui::widgets::modal::dim_background(frame, theme);
        render_picker(frame, theme, state, picker);
    }
}

fn render_topbar(frame: &mut Frame, theme: &Theme, area: Rect, uri: &str, connected: bool) {
    crate::ui::chrome::topbar(frame, theme, area, Some(3), uri, connected, "14:36:02");
}

fn render_body(frame: &mut Frame, theme: &Theme, state: &StorageState, area: Rect, _dimmed: bool) {
    let body = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), area.height);
    if body.width < 160 {
        // Compact: pools above volumes.
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(12), Constraint::Min(0)])
            .split(body);
        render_pools(frame, theme, state, rows[0]);
        render_volumes(frame, theme, state, rows[1]);
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(60), Constraint::Min(0)])
        .spacing(1)
        .split(body);
    render_pools(frame, theme, state, cols[0]);
    render_volumes(frame, theme, state, cols[1]);
}

fn render_pools(frame: &mut Frame, theme: &Theme, state: &StorageState, area: Rect) {
    let t = &theme.tokens;
    let active = state.pools.iter().filter(|p| p.active).count();
    let mut lines: Vec<Line> = Vec::new();
    for (i, p) in state.pools.iter().enumerate() {
        let selected = i == state.pool_selected;
        let bg = if selected { t.hl } else { t.bg };
        let glyph = if p.active { "●" } else { "○" };
        let color = if p.active { t.green } else { t.comment };
        let (on, off) = charts::lg(p.frac, 34);
        lines.push(
            Line::from(vec![
                Span::styled(format!("{glyph} "), Style::default().fg(color).bg(bg)),
                Span::styled(
                    p.name.clone(),
                    Style::default().fg(t.fg).bg(bg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {} · {}", p.kind, p.path),
                    Style::default().fg(t.comment).bg(bg),
                ),
            ])
            .style(Style::default().bg(bg)),
        );
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(on, Style::default().fg(level_color(theme, p.frac)).bg(bg)),
            Span::styled(off, Style::default().fg(t.hl).bg(bg)),
            Span::styled(format!(" {}", p.value), Style::default().fg(t.fg2).bg(bg)),
        ]));
    }
    let right = format!("{} · {active} active", state.pools.len());
    let block = panel_block(theme, state.focus_pools, "Pools", Some(right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
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

fn render_volumes(frame: &mut Frame, theme: &Theme, state: &StorageState, area: Rect) {
    let t = &theme.tokens;
    let pool = state.pools.get(state.pool_selected);
    let title = format!("Volumes · {}", pool.map(|p| p.name.as_str()).unwrap_or(""));
    let right = pool
        .map(|p| format!("{} volumes · {}", state.volumes.len(), p.value))
        .unwrap_or_default();
    let mut lines = vec![Line::from(Span::styled(
        "  NAME                     FORMAT  CAPACITY  ALLOC     USAGE         USED BY",
        Style::default()
            .fg(t.comment)
            .bg(t.bg)
            .add_modifier(Modifier::BOLD),
    ))];
    for (i, v) in state.volumes.iter().enumerate() {
        let selected = i == state.vol_selected;
        let bg = if selected { t.sel } else { t.bg };
        let frac = usage_frac(&v.capacity, &v.alloc);
        let (on, off) = charts::lg(frac, 11);
        let used_style = if v.used_by.starts_with("⚠") {
            Style::default().fg(t.yellow).bg(bg)
        } else if v.used_by.starts_with('◆') {
            Style::default().fg(t.magenta).bg(bg)
        } else {
            Style::default().fg(t.fg2).bg(bg)
        };
        lines.push(
            Line::from(vec![
                Span::styled(
                    if selected { "▌" } else { " " },
                    Style::default().fg(t.blue).bg(bg),
                ),
                Span::raw(" "),
                Span::styled(format!("{:24}", v.name), Style::default().fg(t.fg).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:7}", v.format), Style::default().fg(t.fg2).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:9}", v.capacity), Style::default().fg(t.fg2).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:9}", v.alloc), Style::default().fg(t.fg2).bg(bg)),
                Span::raw(" "),
                Span::styled(on, Style::default().fg(level_color(theme, frac)).bg(bg)),
                Span::styled(off, Style::default().fg(t.hl).bg(bg)),
                Span::raw(" "),
                Span::styled(v.used_by.clone(), used_style),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    let block = panel_block(theme, !state.focus_pools, &title, Some(right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_statusbar(frame: &mut Frame, theme: &Theme, state: &StorageState, area: Rect, picking: bool) {
    let t = &theme.tokens;
    if picking {
        let spans = vec![
            Span::styled(
                " PICKER ",
                Style::default()
                    .fg(t.bg2)
                    .bg(t.magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " storage › {} › {} ",
                    state
                        .pools
                        .get(state.pool_selected)
                        .map(|p| p.name.as_str())
                        .unwrap_or(""),
                    state
                        .volumes
                        .get(state.vol_selected)
                        .map(|v| v.name.as_str())
                        .unwrap_or("")
                ),
                Style::default().fg(t.fg2).bg(t.hl),
            ),
            Span::styled(" ␣mi from any view ", Style::default().fg(t.fg2).bg(t.hl)),
        ];
        crate::ui::chrome::statusbar(frame, theme, area, spans, 2);
        return;
    }
    let spans = vec![
        Span::styled(
            " NORMAL ",
            Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " storage › {} ",
                state
                    .pools
                    .get(state.pool_selected)
                    .map(|p| p.name.as_str())
                    .unwrap_or("")
            ),
            Style::default().fg(t.fg2).bg(t.hl),
        ),
        Span::styled(
            "  j/k move  n new  R resize  C clone  u upload  W wipe  X delete  ␣mi media",
            theme.dim(),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.topbar()), area);
}

fn render_message(frame: &mut Frame, theme: &Theme, state: &StorageState, area: Rect) {
    crate::ui::chrome::message_line(frame, theme, area, &state.message);
}

fn render_picker(frame: &mut Frame, theme: &Theme, state: &StorageState, picker: &MediaPicker) {
    use ratatui::widgets::Clear;
    let t = &theme.tokens;
    let area = frame.area();
    let w = 110u16.min(area.width.saturating_sub(4));
    let h = 27u16.min(area.height.saturating_sub(4));
    let modal = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    let right = format!("{} › {} (cdrom, {})", picker.domain, picker.target, picker.bus);
    let block = panel_block(theme, false, "◎ Insert media", Some(right));
    let inner = Rect::new(modal.x + 2, modal.y + 1, modal.width - 4, modal.height - 2);
    let isos = state.picker_isos();
    let total = isos.len();
    let mut lines = vec![
        Line::from(vec![
            Span::styled("current: ", theme.dim()),
            Span::styled(picker.current.clone(), theme.secondary()),
            Span::styled("  will be ejected", theme.dim()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default()
                    .fg(t.magenta)
                    .bg(t.bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                picker.query.clone(),
                Style::default().fg(t.fg).bg(t.bg).add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(t.fg).bg(t.bg)),
            Span::raw("   "),
            Span::styled(
                format!(
                    "pool: {} ▾   {total} / {}",
                    picker.pools.get(picker.pool_idx).cloned().unwrap_or_default(),
                    state.isos.len()
                ),
                theme.dim(),
            ),
        ]),
        Line::from(""),
    ];
    for (i, iso) in isos.iter().enumerate() {
        let selected = i == picker.selected;
        let bg = if selected { t.sel } else { t.bg };
        let (head, rest) = split_match(&iso.name, &picker.query);
        lines.push(
            Line::from(vec![
                Span::styled("◎ ", Style::default().fg(t.yellow).bg(bg)),
                Span::styled(
                    head,
                    Style::default().fg(t.orange).bg(bg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    rest,
                    if selected {
                        Style::default().fg(t.fg).bg(bg).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(t.fg2).bg(bg)
                    },
                ),
                Span::raw("  "),
                Span::styled(format!("{:8}", iso.size), Style::default().fg(t.fg2).bg(bg)),
                Span::raw("  "),
                Span::styled(iso.date.clone(), Style::default().fg(t.comment).bg(bg)),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    lines.push(Line::from(""));
    let check = |on: bool| {
        if on {
            Span::styled("[x]", Style::default().fg(t.green).bg(t.bg))
        } else {
            Span::styled("[ ]", theme.dim())
        }
    };
    lines.push(Line::from(vec![
        check(picker.live),
        Span::styled(" live  ", theme.text()),
        check(picker.config),
        Span::styled(" config  ", theme.text()),
        check(false),
        Span::styled(" boot from cdrom next  ", theme.secondary()),
        check(false),
        Span::styled(" browse filesystem…", theme.secondary()),
    ]));
    lines.push(Line::from(""));
    let cmd = if !isos.is_empty() {
        let pool_path = picker
            .pools
            .get(picker.pool_idx)
            .and_then(|pool| state.pools.iter().find(|p| &p.name == pool))
            .map(|p| p.path.clone())
            .unwrap_or_else(|| String::from("/var/lib/libvirt/isos"));
        format!(
            "$ virsh change-media {} {} {}/{} --update --live --config",
            picker.domain,
            picker.target,
            pool_path,
            state.picker_isos().first().map(|i| i.name.as_str()).unwrap_or("")
        )
    } else {
        String::from("$ virsh change-media …")
    };
    lines.push(Line::from(vec![Span::styled(
        cmd,
        Style::default().fg(t.yellow).bg(t.bg2),
    )]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" ⏎ insert ", theme.key()),
        Span::styled(" C-j/k move ", theme.dim()),
        Span::styled(" C-e eject only ", theme.dim()),
        Span::styled(" ⇥ pool ", theme.dim()),
        Span::styled(" ⎋ cancel", theme.dim()),
    ]));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), inner);
}

/// Split a name into (match, rest) for query highlighting.
fn split_match(name: &str, query: &str) -> (String, String) {
    if query.is_empty() {
        return (String::new(), name.to_string());
    }
    let lower = name.to_lowercase();
    let q = query.to_lowercase();
    if let Some(i) = lower.find(&q) {
        let end = i + q.len();
        // Byte-safe: queries are ASCII in practice; fall back gracefully.
        if name.is_char_boundary(i) && name.is_char_boundary(end) {
            return (
                name[i..end].to_string(),
                format!("{}{}", &name[..i], &name[end..]),
            );
        }
    }
    (String::new(), name.to_string())
}
