//! Networks view (screen 07, PLAN.md section 9.10, P9).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::theme::Theme;
use crate::ui::widgets::{
    charts::{self, BrailleMode},
    form::{Field, FieldKind, Form},
};

/// Virtual network row.
#[derive(Debug, Clone)]
pub struct NetRow {
    pub name: String,
    pub mode: String,
    pub bridge: String,
    pub subnet: String,
    pub active: bool,
    pub autostart: bool,
    /// Parsed definition (detail panel, static hosts).
    pub cfg: crate::model::NetworkConfig,
}

/// Host interface row.
#[derive(Debug, Clone)]
pub struct IfaceRow {
    pub name: String,
    pub kind: String,
    pub speed: String,
    pub addr: String,
    pub up: bool,
}

/// DHCP lease row.
#[derive(Debug, Clone)]
pub struct LeaseRow {
    pub mac: String,
    pub ip: String,
    pub hostname: String,
    pub expires: String,
    pub statik: bool,
    /// Domain owning the MAC (from `domiflist`), when known.
    pub domain: String,
}

/// Live traffic of the selected network's bridge (0–1 series + current rates).
#[derive(Debug, Clone, Default)]
pub struct NetTraffic {
    pub iface: String,
    pub rx: Vec<f64>,
    pub tx: Vec<f64>,
    pub rx_bps: f64,
    pub tx_bps: f64,
}

/// Which left pane has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetFocus {
    #[default]
    Networks,
    Leases,
}

/// Networks screen state.
#[derive(Debug, Clone, Default)]
pub struct NetworksState {
    pub networks: Vec<NetRow>,
    pub ifaces: Vec<IfaceRow>,
    pub attached: Vec<(String, String)>,
    pub leases: Vec<LeaseRow>,
    pub selected: usize,
    pub lease_selected: usize,
    pub focus: NetFocus,
    pub message: String,
    pub uri: String,
    pub connected: bool,
    pub new_form: Option<Form>,
    /// Live mode (no mockup data in panels).
    pub live: bool,
    pub traffic: Option<NetTraffic>,
    /// Domain to open after `⏎` on a lease (handled by the app loop).
    pub pending_open: Option<String>,
}

impl NetworksState {
    /// Demo state from docs/mockups/Networks.dc.html renderVals().
    pub fn demo() -> Self {
        let networks = vec![
            NetRow {
                name: String::from("default"),
                mode: String::from("nat"),
                bridge: String::from("virbr0"),
                subnet: String::from("192.168.122.0/24"),
                active: true,
                autostart: true,
                cfg: crate::model::NetworkConfig::default(),
            },
            NetRow {
                name: String::from("lab-isolated"),
                mode: String::from("isolated"),
                bridge: String::from("virbr1"),
                subnet: String::from("10.10.0.0/24"),
                active: true,
                autostart: true,
                cfg: crate::model::NetworkConfig::default(),
            },
            NetRow {
                name: String::from("br0"),
                mode: String::from("bridge"),
                bridge: String::from("br0"),
                subnet: String::from("— (LAN)"),
                active: true,
                autostart: true,
                cfg: crate::model::NetworkConfig::default(),
            },
            NetRow {
                name: String::from("ipv6-routed"),
                mode: String::from("route"),
                bridge: String::from("virbr2"),
                subnet: String::from("fd00:42::/64"),
                active: false,
                autostart: false,
                cfg: crate::model::NetworkConfig::default(),
            },
        ];
        let ifaces = vec![
            IfaceRow {
                name: String::from("enp6s0"),
                kind: String::from("ethernet"),
                speed: String::from("2.5G"),
                addr: String::from("192.168.1.20/24"),
                up: true,
            },
            IfaceRow {
                name: String::from("br0"),
                kind: String::from("bridge"),
                speed: String::from("2.5G"),
                addr: String::from("192.168.1.21/24"),
                up: true,
            },
            IfaceRow {
                name: String::from("virbr0"),
                kind: String::from("bridge"),
                speed: String::from("—"),
                addr: String::from("192.168.122.1/24"),
                up: true,
            },
            IfaceRow {
                name: String::from("virbr1"),
                kind: String::from("bridge"),
                speed: String::from("—"),
                addr: String::from("10.10.0.1/24"),
                up: true,
            },
            IfaceRow {
                name: String::from("wlp7s0"),
                kind: String::from("wireless"),
                speed: String::from("—"),
                addr: String::from("down"),
                up: false,
            },
        ];
        let attached = vec![
            (String::from("vnet0"), String::from("k8s-cp-01")),
            (String::from("vnet3"), String::from("arch-dev")),
            (String::from("vnet1"), String::from("k8s-worker-01")),
            (String::from("vnet5"), String::from("win11-gaming")),
            (String::from("vnet2"), String::from("k8s-worker-02")),
            (String::from("vnet7"), String::from("alpine-edge")),
        ];
        let leases = vec![
            LeaseRow {
                mac: String::from("52:54:00:11:c0:01"),
                ip: String::from("192.168.122.10"),
                hostname: String::from("k8s-cp-01"),
                expires: String::from("static"),
                domain: String::from("k8s-cp-01"),
                statik: true,
            },
            LeaseRow {
                mac: String::from("52:54:00:11:c0:02"),
                ip: String::from("192.168.122.11"),
                hostname: String::from("k8s-worker-01"),
                expires: String::from("static"),
                domain: String::from("k8s-worker-01"),
                statik: true,
            },
            LeaseRow {
                mac: String::from("52:54:00:11:c0:03"),
                ip: String::from("192.168.122.12"),
                hostname: String::from("k8s-worker-02"),
                expires: String::from("static"),
                domain: String::from("k8s-worker-02"),
                statik: true,
            },
            LeaseRow {
                mac: String::from("52:54:00:a3:1f:7c"),
                ip: String::from("192.168.122.48"),
                hostname: String::from("arch-dev"),
                expires: String::from("2026-10-06 15:31"),
                domain: String::from("arch-dev"),
                statik: false,
            },
            LeaseRow {
                mac: String::from("52:54:00:5e:90:2d"),
                ip: String::from("192.168.122.77"),
                hostname: String::from("DESKTOP-W11G"),
                expires: String::from("2026-10-06 15:12"),
                domain: String::from("win11-gaming"),
                statik: false,
            },
            LeaseRow {
                mac: String::from("52:54:00:0b:44:e1"),
                ip: String::from("192.168.122.92"),
                hostname: String::from("alpine-edge"),
                expires: String::from("2026-10-06 14:58"),
                domain: String::from("alpine-edge"),
                statik: false,
            },
            LeaseRow {
                mac: String::from("52:54:00:7f:20:aa"),
                ip: String::from("192.168.122.20"),
                hostname: String::from("haos"),
                expires: String::from("static"),
                domain: String::from("haos"),
                statik: true,
            },
        ];
        let mut networks = networks;
        networks[0].cfg = crate::model::NetworkConfig {
            name: String::from("default"),
            uuid: String::from("4a1c6e3b-7d2e-4f0a-9b1c-93d0be1c7f02"),
            forward_mode: String::from("nat"),
            forward_dev: String::from("enp6s0"),
            nat_ports: Some((String::from("1024"), String::from("65535"))),
            bridge: String::from("virbr0"),
            stp: true,
            delay: String::from("0"),
            mtu: String::from("1500"),
            ipv4: Some(String::from("192.168.122.1/24")),
            dhcp_range: Some((String::from("192.168.122.2"), String::from("192.168.122.254"))),
            static_hosts: vec![Default::default(); 4],
            dns_domain: String::from("lab.local"),
            ipv6: None,
        };
        Self {
            networks,
            ifaces,
            attached,
            leases,
            selected: 0,
            lease_selected: 0,
            focus: NetFocus::Networks,
            message: String::from("── virsh net-dhcp-leases default"),
            uri: String::from("qemu:///system"),
            connected: true,
            new_form: None,
            live: false,
            traffic: None,
            pending_open: None,
        }
    }

    /// Selected network name.
    pub fn selected_name(&self) -> &str {
        self.networks
            .get(self.selected)
            .map(|n| n.name.as_str())
            .unwrap_or("")
    }

    /// Selected lease.
    pub fn selected_lease(&self) -> Option<&LeaseRow> {
        self.leases.get(self.lease_selected)
    }

    /// Move selection in the focused pane.
    pub fn move_selection(&mut self, delta: i32) {
        if self.focus == NetFocus::Leases {
            let n = self.leases.len() as i32;
            if n > 0 {
                self.lease_selected = (self.lease_selected as i32 + delta).clamp(0, n - 1) as usize;
            }
        } else {
            let n = self.networks.len() as i32;
            if n > 0 {
                self.selected = (self.selected as i32 + delta).clamp(0, n - 1) as usize;
            }
        }
    }

    /// Open the new-network form with DHCP range auto-computed.
    pub fn open_new_form(&mut self) {
        let mut form = Form::new(vec![
            Field::new("name", "name", FieldKind::Text, ""),
            Field::new(
                "mode",
                "mode",
                FieldKind::Select(vec![
                    String::from("nat"),
                    String::from("isolated"),
                    String::from("route"),
                    String::from("bridge"),
                    String::from("open"),
                ]),
                "isolated",
            ),
            Field::new("bridge", "bridge", FieldKind::Text, "virbr9"),
            Field::new("cidr", "IPv4 CIDR", FieldKind::Text, "10.20.0.0/24"),
            Field::new("dhcp", "DHCP range", FieldKind::Text, "10.20.0.2 – 10.20.0.254"),
            Field::new("dns", "DNS domain", FieldKind::Text, ""),
        ]);
        let _ = &mut form;
        self.new_form = Some(form);
    }
}

/// Auto-compute DHCP range (.2 – .254) for an IPv4 CIDR.
pub fn dhcp_range(cidr: &str) -> Option<(String, String)> {
    let (addr, prefix) = cidr.split_once('/')?;
    prefix.parse::<u8>().ok()?;
    let mut parts: Vec<&str> = addr.split('.').collect();
    if parts.len() != 4 || parts.iter().any(|p| p.parse::<u8>().is_err()) {
        return None;
    }
    parts[3] = "2";
    let start = parts.join(".");
    parts[3] = "254";
    let end = parts.join(".");
    Some((start, end))
}

/// Render the networks screen.
pub fn render(frame: &mut Frame, theme: &Theme, state: &NetworksState) {
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
    if state.new_form.is_some() {
        render_new_form(frame, theme, state);
    }
}

fn render_topbar(frame: &mut Frame, theme: &Theme, area: Rect, uri: &str, connected: bool) {
    crate::ui::chrome::topbar(frame, theme, area, Some(2), uri, connected, "14:35:12");
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

fn render_body(frame: &mut Frame, theme: &Theme, state: &NetworksState, area: Rect) {
    let body = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), area.height);
    if body.width < 160 {
        // Compact: stack (ifaces move to the leases view in follow-up).
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(9), Constraint::Length(9), Constraint::Min(0)])
            .split(body);
        render_net_list(frame, theme, state, rows[0]);
        let mid = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(42)])
            .spacing(1)
            .split(rows[1]);
        render_net_detail(frame, theme, state, mid[0]);
        render_traffic(frame, theme, state, mid[1]);
        render_leases(frame, theme, state, rows[2]);
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(72), Constraint::Min(0)])
        .spacing(1)
        .split(body);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(0)])
        .split(cols[0]);
    render_net_list(frame, theme, state, left[0]);
    render_ifaces(frame, theme, state, left[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(0)])
        .split(cols[1]);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(42)])
        .spacing(1)
        .split(right[0]);
    render_net_detail(frame, theme, state, top[0]);
    render_traffic(frame, theme, state, top[1]);
    render_leases(frame, theme, state, right[1]);
}

fn render_net_list(frame: &mut Frame, theme: &Theme, state: &NetworksState, area: Rect) {
    let t = &theme.tokens;
    let active = state.networks.iter().filter(|n| n.active).count();
    let mut lines = vec![Line::from(Span::styled(
        "  NAME          MODE      BRIDGE      SUBNET             A",
        Style::default()
            .fg(t.comment)
            .bg(t.bg)
            .add_modifier(Modifier::BOLD),
    ))];
    for (i, n) in state.networks.iter().enumerate() {
        let selected = i == state.selected && state.focus == NetFocus::Networks;
        let bg = if selected { t.sel } else { t.bg };
        let glyph = if n.active { "●" } else { "○" };
        let glyph_color = if n.active { t.green } else { t.comment };
        lines.push(
            Line::from(vec![
                Span::styled(
                    if selected { "▌" } else { " " },
                    Style::default().fg(t.blue).bg(bg),
                ),
                Span::raw(" "),
                Span::styled(format!("{glyph} "), Style::default().fg(glyph_color).bg(bg)),
                Span::styled(
                    format!("{:13}", n.name.chars().take(13).collect::<String>()),
                    Style::default().fg(t.fg).bg(bg).add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
                Span::raw(" "),
                Span::styled(format!("{:9}", n.mode), Style::default().fg(t.fg2).bg(bg)),
                Span::raw(" "),
                Span::styled(
                    format!("{:11}", n.bridge.chars().take(11).collect::<String>()),
                    Style::default().fg(t.fg2).bg(bg),
                ),
                Span::raw(" "),
                Span::styled(format!("{:18}", n.subnet), Style::default().fg(t.cyan).bg(bg)),
                Span::raw(" "),
                Span::styled(
                    if n.autostart { "✓" } else { "·" },
                    Style::default().fg(t.green).bg(bg),
                ),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    let right = format!("{active}/{} active", state.networks.len());
    let block = panel_block(
        theme,
        state.focus == NetFocus::Networks,
        "Virtual networks",
        Some(right),
    );
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_ifaces(frame: &mut Frame, theme: &Theme, state: &NetworksState, area: Rect) {
    let t = &theme.tokens;
    let mut lines = vec![Line::from(Span::styled(
        "  IFACE      TYPE     SPEED    ADDRESS",
        Style::default()
            .fg(t.comment)
            .bg(t.bg)
            .add_modifier(Modifier::BOLD),
    ))];
    for iface in &state.ifaces {
        let glyph = if iface.up { "▲" } else { "▽" };
        let color = if iface.up { t.green } else { t.comment };
        lines.push(Line::from(vec![
            Span::styled(format!("{glyph} "), Style::default().fg(color).bg(t.bg)),
            Span::styled(format!("{:10} ", iface.name), theme.text()),
            Span::styled(format!("{:8} ", iface.kind), theme.secondary()),
            Span::styled(format!("{:8} ", iface.speed), theme.secondary()),
            Span::styled(iface.addr.clone(), theme.dim()),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("─ attached to {} ─", state.selected_name()),
        theme.dim(),
    )]));
    for chunk in state.attached.chunks(2) {
        let mut spans = Vec::new();
        for (vnet, dom) in chunk {
            spans.push(Span::styled(
                format!("{vnet} "),
                Style::default().fg(t.cyan).bg(t.bg),
            ));
            spans.push(Span::styled(format!("{dom:15}"), theme.secondary()));
        }
        lines.push(Line::from(spans));
    }
    let block = panel_block(theme, false, "Host interfaces", Some(String::from("iface-list")));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_net_detail(frame: &mut Frame, theme: &Theme, state: &NetworksState, area: Rect) {
    let t = &theme.tokens;
    let sel = state.networks.get(state.selected);
    let title = sel.map(|n| n.name.clone()).unwrap_or_else(|| String::from("—"));
    let right = sel.map(|n| {
        Line::from(vec![
            Span::styled(
                if n.active { "● active" } else { "○ inactive" },
                Style::default().fg(if n.active { t.green } else { t.comment }),
            ),
            Span::styled(
                format!(
                    " · {}autostart · persistent",
                    if n.autostart { "" } else { "no " }
                ),
                theme.dim(),
            ),
        ])
    });
    let c = sel.map(|n| n.cfg.clone()).unwrap_or_default();
    let dash = |s: &str| {
        if s.is_empty() {
            String::from("—")
        } else {
            s.to_string()
        }
    };
    let kv = |k: &str, v: Vec<Span<'static>>| {
        let mut spans = vec![Span::styled(format!("{k:<10}"), theme.dim())];
        spans.extend(v);
        Line::from(spans)
    };
    let mut forward = vec![Span::raw(c.forward_mode.clone())];
    if !c.forward_dev.is_empty() {
        forward.push(Span::styled(" → ", theme.dim()));
        forward.push(Span::raw(c.forward_dev.clone()));
    }
    if let Some((a, b)) = &c.nat_ports {
        forward.push(Span::styled(format!(" · ports {a}-{b}"), theme.dim()));
    }
    let mut bridge = vec![Span::raw(dash(&c.bridge))];
    if !c.bridge.is_empty() {
        bridge.push(Span::styled(
            format!(
                " · stp {} · delay {} · mtu {}",
                if c.stp { "on" } else { "off" },
                dash(&c.delay),
                if c.mtu.is_empty() {
                    String::from("1500")
                } else {
                    c.mtu.clone()
                }
            ),
            theme.dim(),
        ));
    }
    let dhcp = match &c.dhcp_range {
        Some((a, b)) => {
            let short = |ip: &str| {
                ip.rsplit_once('.')
                    .map_or(ip.to_string(), |(_, last)| format!(".{last}"))
            };
            vec![
                Span::raw(format!("{} – {}", short(a), short(b))),
                Span::styled(" · ", theme.dim()),
                Span::raw(format!("{} static hosts", c.static_hosts.len())),
            ]
        }
        None => vec![Span::styled("—", theme.dim())],
    };
    let lines = vec![
        kv("uuid", vec![Span::styled(dash(&c.uuid), theme.secondary())]),
        kv("forward", forward),
        kv("bridge", bridge),
        kv(
            "ipv4",
            vec![Span::styled(
                c.ipv4.clone().unwrap_or_else(|| String::from("—")),
                Style::default().fg(t.cyan),
            )],
        ),
        kv("dhcp", dhcp),
        kv(
            "dns",
            if c.dns_domain.is_empty() {
                vec![Span::raw("on")]
            } else {
                vec![
                    Span::raw("on"),
                    Span::styled(" · domain ", theme.dim()),
                    Span::raw(c.dns_domain.clone()),
                ]
            },
        ),
        kv(
            "ipv6",
            vec![Span::styled(
                c.ipv6.clone().unwrap_or_else(|| String::from("—")),
                theme.dim(),
            )],
        ),
    ];
    let block = crate::ui::widgets::panel::block(theme, false, &title, right, None, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_traffic(frame: &mut Frame, theme: &Theme, state: &NetworksState, area: Rect) {
    let t = &theme.tokens;
    let w = (area.width.saturating_sub(4) as usize).min(38);
    let (rx_d, tx_d, rx_t, tx_t, iface) = match &state.traffic {
        Some(tr) => (
            crate::metrics::store::padded(&tr.rx, crate::metrics::store::window_len()),
            crate::metrics::store::padded(&tr.tx, crate::metrics::store::window_len()),
            crate::metrics::store::fmt_bits_rate(tr.rx_bps),
            crate::metrics::store::fmt_bits_rate(tr.tx_bps),
            tr.iface.clone(),
        ),
        None if state.live => (
            vec![0.0; 2],
            vec![0.0; 2],
            String::from("—"),
            String::from("—"),
            state
                .networks
                .get(state.selected)
                .map(|n| n.bridge.clone())
                .unwrap_or_default(),
        ),
        None => (
            charts::series(51, 80, 0.45, 0.4, 0.15),
            charts::series(53, 80, 0.2, 0.3, 0.2),
            String::from("62.4 Mb/s"),
            String::from("9.8 Mb/s"),
            String::from("virbr0"),
        ),
    };
    let mut lines: Vec<Line> = charts::area(theme.graphs, &rx_d, w, 3, BrailleMode::Fill)
        .into_iter()
        .map(|r| Line::from(Span::styled(r, Style::default().fg(t.cyan))))
        .collect();
    for r in charts::area(theme.graphs, &tx_d, w, 2, BrailleMode::Down) {
        lines.push(Line::from(Span::styled(r, Style::default().fg(t.yellow))));
    }
    lines.push(Line::from(vec![
        Span::styled(format!("↓ {rx_t}"), Style::default().fg(t.cyan)),
        Span::raw("  "),
        Span::styled(format!("↑ {tx_t}"), Style::default().fg(t.yellow)),
    ]));
    let right = Line::from(vec![
        Span::styled("↓", Style::default().fg(t.cyan)),
        Span::styled("/", theme.dim()),
        Span::styled("↑", Style::default().fg(t.yellow)),
    ]);
    let title = if iface.is_empty() {
        String::from("Traffic")
    } else {
        format!("Traffic · {iface}")
    };
    let block = crate::ui::widgets::panel::block(theme, false, &title, Some(right), None, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_leases(frame: &mut Frame, theme: &Theme, state: &NetworksState, area: Rect) {
    let t = &theme.tokens;
    let statics = state.leases.iter().filter(|l| l.statik).count();
    let mut lines = vec![Line::from(Span::styled(
        "  MAC                IP               HOSTNAME         EXPIRES             DOMAIN",
        Style::default()
            .fg(t.comment)
            .bg(t.bg)
            .add_modifier(Modifier::BOLD),
    ))];
    for (i, l) in state.leases.iter().enumerate() {
        let selected = i == state.lease_selected && state.focus == NetFocus::Leases;
        let bg = if selected { t.sel } else { t.bg };
        let glyph = if l.statik { "◆" } else { "●" };
        let color = if l.statik { t.magenta } else { t.green };
        let dom = if l.domain.is_empty() {
            "—"
        } else {
            l.domain.as_str()
        };
        lines.push(
            Line::from(vec![
                Span::styled(format!("{glyph} "), Style::default().fg(color).bg(bg)),
                Span::styled(format!("{:18}", l.mac), theme.dim()),
                Span::raw(" "),
                Span::styled(format!("{:16}", l.ip), Style::default().fg(t.cyan).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:16}", l.hostname), Style::default().fg(t.fg).bg(bg)),
                Span::raw(" "),
                Span::styled(format!("{:19}", l.expires), Style::default().fg(t.fg2).bg(bg)),
                Span::raw(" "),
                Span::styled(dom.to_string(), Style::default().fg(t.fg2).bg(bg)),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("◆", Style::default().fg(t.magenta).bg(t.bg)),
        Span::styled(" static (ip-dhcp-host)   ", theme.dim()),
        Span::styled("●", Style::default().fg(t.green).bg(t.bg)),
        Span::styled(" dynamic", theme.dim()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" ␣l pin lease as static ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::raw(" "),
        Span::styled(" ␣L remove static ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::raw(" "),
        Span::styled(" yi copy ip ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::raw(" "),
        Span::styled(" ⏎ go to domain ", Style::default().fg(t.fg2).bg(t.hl)),
    ]));
    let right = format!("{} leases · {statics} static", state.leases.len());
    let block = crate::ui::widgets::panel::block(
        theme,
        state.focus == NetFocus::Leases,
        "DHCP leases",
        Some(crate::ui::widgets::panel::right(right, theme)),
        None,
        Some("net-update · live + config"),
    );
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_statusbar(frame: &mut Frame, theme: &Theme, state: &NetworksState, area: Rect) {
    let t = &theme.tokens;
    let spans = vec![
        Span::styled(
            " NORMAL ",
            Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" networks › {} ", state.selected_name()),
            Style::default().fg(t.fg2).bg(t.hl),
        ),
        Span::styled(
            "  j/k move  l leases  s start  D destroy  a autostart  e edit xml  n new  X undefine",
            theme.dim(),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.topbar()), area);
}

fn render_message(frame: &mut Frame, theme: &Theme, state: &NetworksState, area: Rect) {
    crate::ui::chrome::message_line(frame, theme, area, &state.message);
}

fn render_new_form(frame: &mut Frame, theme: &Theme, state: &NetworksState) {
    use ratatui::widgets::Clear;
    let t = &theme.tokens;
    let area = frame.area();
    let w = 72u16.min(area.width.saturating_sub(4));
    let h = 16u16.min(area.height.saturating_sub(4));
    let modal = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    let form = state.new_form.as_ref().unwrap();
    let mut lines: Vec<Line> = form
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            Line::from(vec![
                Span::styled(format!("{:12}", f.label), theme.secondary()),
                Span::styled(
                    format!(" {} ", f.current),
                    if i == form.focus {
                        Style::default().fg(t.fg).bg(t.sel)
                    } else {
                        Style::default().fg(t.fg2).bg(t.hl)
                    },
                ),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            " ⏎ create ",
            Style::default()
                .fg(t.bg2)
                .bg(t.green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(" ⎋ cancel ", Style::default().fg(t.fg2).bg(t.hl)),
    ]));
    let block = panel_block(theme, false, "New network", Some(String::from("net-define")));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), modal);
}
