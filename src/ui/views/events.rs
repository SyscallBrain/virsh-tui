//! Events view (tab 5, PLAN.md section 9.12, P12, not mocked).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::model::LibvirtEvent;
use crate::theme::Theme;

/// Event filter chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventFilter {
    #[default]
    All,
    Lifecycle,
    Devices,
    Jobs,
    Network,
    Storage,
    App,
}

impl EventFilter {
    pub const ALL: [Self; 7] = [
        Self::All,
        Self::Lifecycle,
        Self::Devices,
        Self::Jobs,
        Self::Network,
        Self::Storage,
        Self::App,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Lifecycle => "lifecycle",
            Self::Devices => "devices",
            Self::Jobs => "jobs",
            Self::Network => "network",
            Self::Storage => "storage",
            Self::App => "app",
        }
    }

    fn matches(self, ev: &LibvirtEvent) -> bool {
        let k = ev.kind.as_str();
        match self {
            Self::All => true,
            Self::Lifecycle => {
                ev.scope == "domain" && matches!(k, "lifecycle" | "reboot" | "pm-wakeup" | "pm-suspend")
            }
            Self::Devices => {
                ev.scope == "domain"
                    && (k.starts_with("device-")
                        || matches!(k, "tray-change" | "disk-change" | "balloon-change"))
            }
            Self::Jobs => k.contains("job"),
            Self::Network => ev.scope == "network",
            Self::Storage => ev.scope == "pool",
            Self::App => ev.scope == "app",
        }
    }
}

/// Events screen state (keeps the last 5,000 in memory).
#[derive(Debug, Clone, Default)]
pub struct EventsState {
    pub events: Vec<LibvirtEvent>,
    pub filter: EventFilter,
    pub query: String,
    pub selected: usize,
    pub uri: String,
    pub connected: bool,
}

impl EventsState {
    /// Demo events.
    pub fn demo() -> Self {
        let mut st = Self {
            events: vec![
                LibvirtEvent {
                    timestamp: String::from("14:31:55"),
                    object: String::from("k8s-worker-02"),
                    event: String::from("Started"),
                    detail: String::from("Booted"),
                    kind: String::from("lifecycle"),
                    scope: String::from("domain"),
                },
                LibvirtEvent {
                    timestamp: String::from("14:30:12"),
                    object: String::from("arch-dev"),
                    event: String::from("Device added"),
                    detail: String::from("vda resize 120G"),
                    kind: String::from("device-added"),
                    scope: String::from("domain"),
                },
                LibvirtEvent {
                    timestamp: String::from("14:28:44"),
                    object: String::from("win11-gaming"),
                    event: String::from("Suspended"),
                    detail: String::from("Paused by user"),
                    kind: String::from("lifecycle"),
                    scope: String::from("domain"),
                },
                LibvirtEvent {
                    timestamp: String::from("14:25:01"),
                    object: String::from("nixos-lab"),
                    event: String::from("Suspended"),
                    detail: String::from("Paused"),
                    kind: String::from("lifecycle"),
                    scope: String::from("domain"),
                },
            ],
            filter: EventFilter::All,
            query: String::new(),
            selected: 0,
            uri: String::from("qemu:///system"),
            connected: true,
        };
        // Stored oldest first (the stream appends); the view lists newest first.
        st.events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        st
    }

    /// Push an event (cap 5,000).
    pub fn push(&mut self, ev: LibvirtEvent) {
        if self.events.len() >= 5000 {
            self.events.remove(0);
        }
        self.events.push(ev);
    }

    /// Visible events after filter + search.
    pub fn visible(&self) -> Vec<&LibvirtEvent> {
        self.events
            .iter()
            .rev()
            .filter(|e| self.filter.matches(e))
            .filter(|e| {
                self.query.is_empty()
                    || format!("{} {} {} {} {}", e.timestamp, e.kind, e.object, e.event, e.detail)
                        .to_lowercase()
                        .contains(&self.query.to_lowercase())
            })
            .collect()
    }
}

/// Render the events view.
pub fn render(frame: &mut Frame, theme: &Theme, state: &EventsState) {
    let area = frame.area();
    if crate::ui::layout::breakpoint(area.width, area.height) == crate::ui::layout::Breakpoint::TooSmall {
        crate::ui::layout::render_too_small(frame, theme);
        return;
    }
    let t = &theme.tokens;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    // Top bar (Events active).
    crate::ui::chrome::topbar(
        frame,
        theme,
        rows[0],
        Some(4),
        &state.uri,
        state.connected,
        "14:32:07",
    );
    // Body.
    let body = Rect::new(
        rows[1].x + 1,
        rows[1].y,
        rows[1].width.saturating_sub(2),
        rows[1].height,
    );
    let mut lines: Vec<Line> = Vec::new();
    let mut chips = Vec::new();
    for f in EventFilter::ALL {
        chips.push(Span::styled(
            format!(" {} ", f.label()),
            if f == state.filter {
                Style::default().fg(t.fg).bg(t.sel)
            } else {
                Style::default().fg(t.fg2).bg(t.hl)
            },
        ));
        chips.push(Span::raw(" "));
    }
    lines.push(Line::from(chips));
    lines.push(Line::from(Span::styled(
        format!(
            "  {:<10}{:<14}{:<20}{:<13}{}",
            "TIME", "TYPE", "OBJECT", "EVENT", "DETAIL"
        ),
        Style::default()
            .fg(t.comment)
            .bg(t.bg)
            .add_modifier(Modifier::BOLD),
    )));
    let visible = state.visible();
    let body_h = (body.height as usize).saturating_sub(4);
    let sel = state.selected.min(visible.len().saturating_sub(1));
    let start = sel.saturating_sub(body_h.saturating_sub(3));
    for (i, ev) in visible.iter().enumerate().skip(start).take(body_h) {
        let color = match ev.event.as_str() {
            "Started" | "Resumed" => t.green,
            "Suspended" | "PMSuspended" => t.yellow,
            "Crashed" => t.red,
            "Stopped" | "Shutdown" => t.comment,
            "Defined" | "Undefined" => t.blue,
            _ => t.magenta,
        };
        let selected = i == sel;
        let bg = if selected { t.sel } else { t.bg };
        let scope = if ev.scope.is_empty() || ev.scope == "domain" {
            String::new()
        } else {
            format!("{} ", ev.scope)
        };
        lines.push(
            Line::from(vec![
                Span::styled(if selected { "▌ " } else { "  " }, Style::default().fg(t.blue)),
                Span::styled(format!("{:<10}", ev.timestamp), theme.dim()),
                Span::styled(fit(&format!("{scope}{}", ev.kind), 14), theme.secondary()),
                Span::styled(
                    fit(&ev.object, 20),
                    Style::default().fg(t.fg).add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
                Span::styled(fit(&ev.event, 13), Style::default().fg(color)),
                Span::styled(ev.detail.clone(), theme.dim()),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    if visible.is_empty() {
        lines.push(Line::from(Span::styled(
            if state.events.is_empty() {
                "  waiting for events…"
            } else {
                "  no events match the filter"
            },
            theme.dim(),
        )));
    }
    let right = format!("{} / {} events", visible.len(), state.events.len());
    let search = if state.query.is_empty() {
        String::new()
    } else {
        format!("/{}", state.query)
    };
    let block = crate::ui::widgets::panel::block(
        theme,
        true,
        "Events",
        Some(crate::ui::widgets::panel::right(right, theme)),
        if search.is_empty() { None } else { Some(&search) },
        Some("event · net-event · pool-event"),
    );
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), body);
    // Status + message.
    let status = Line::from(vec![
        Span::styled(
            " NORMAL ",
            Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" events ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::styled("  j/k move  f/F filter  / search  ⏎ jump to object", theme.dim()),
    ]);
    frame.render_widget(Paragraph::new(status).style(theme.topbar()), rows[2]);
    crate::ui::chrome::message_line(frame, theme, rows[3], "");
}

/// Pad to `w` columns, keeping one blank column; longer text ends in `…`
/// (a long type such as `channel-lifecycle` ran into the object name).
fn fit(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n < w {
        format!("{s:<w$}")
    } else {
        let cut: String = s.chars().take(w.saturating_sub(2)).collect();
        format!("{cut}… ")
    }
}
