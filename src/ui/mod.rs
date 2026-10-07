//! Root render: top bar, body, status bar, message line.

pub mod chrome;
pub mod layout;
pub mod overlays;
pub mod views;
pub mod widgets;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::theme::Theme;

/// Minimal P0 app state: chrome only.
#[derive(Debug, Clone)]
pub struct ChromeState {
    pub uri: String,
    pub hostname: String,
    pub theme_name: String,
    pub message: String,
    pub active_tab: usize,
}

impl Default for ChromeState {
    fn default() -> Self {
        Self {
            uri: String::from("qemu:///system"),
            hostname: String::from("forge"),
            theme_name: String::from("tokyo-night"),
            message: String::new(),
            active_tab: 0,
        }
    }
}

const TABS: [&str; 5] = ["1 Domains", "2 Host", "3 Networks", "4 Storage", "5 Events"];

/// Render the P0 chrome into the frame.
pub fn render(frame: &mut Frame, theme: &Theme, state: &ChromeState) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    render_topbar(frame, theme, state, rows[0]);
    render_body(frame, theme, rows[1]);
    render_statusbar(frame, theme, state, rows[2]);
    render_messageline(frame, theme, state, rows[3]);
}

fn render_topbar(frame: &mut Frame, theme: &Theme, state: &ChromeState, area: ratatui::layout::Rect) {
    let t = &theme.tokens;
    let mut spans = vec![Span::styled(
        " ◆ virsh-tui ",
        Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
    )];
    for (i, tab) in TABS.iter().enumerate() {
        if i == state.active_tab {
            spans.push(Span::styled(
                format!(" {tab} "),
                Style::default().fg(t.blue).bg(t.bg).add_modifier(Modifier::BOLD),
            ));
        } else {
            let mut parts = tab.splitn(2, ' ');
            let n = parts.next().unwrap_or("");
            let label = parts.next().unwrap_or("");
            spans.push(Span::styled(
                format!(" {n}"),
                Style::default().fg(t.orange).bg(t.bg2),
            ));
            spans.push(Span::styled(
                format!(" {label} "),
                Style::default().fg(t.comment).bg(t.bg2),
            ));
        }
    }
    let right = format!(" ⌁ {}  │  {}  │  ⟳ 1s  │  14:32:07 ", state.uri, state.hostname);
    spans.push(Span::styled(right, Style::default().fg(t.fg2).bg(t.bg2)));
    let line = Line::from(spans);
    let block = ratatui::widgets::Paragraph::new(line).style(theme.topbar());
    frame.render_widget(block, area);
}

fn render_body(frame: &mut Frame, theme: &Theme, area: ratatui::layout::Rect) {
    let text = ratatui::widgets::Paragraph::new("").style(theme.base());
    frame.render_widget(text, area);
}

fn render_statusbar(frame: &mut Frame, theme: &Theme, _state: &ChromeState, area: ratatui::layout::Rect) {
    let t = &theme.tokens;
    let spans = vec![
        Span::styled(
            " NORMAL ",
            Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" domains ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::styled("  q quit", theme.dim()),
    ];
    let line = Line::from(spans);
    frame.render_widget(ratatui::widgets::Paragraph::new(line).style(theme.topbar()), area);
}

fn render_messageline(frame: &mut Frame, theme: &Theme, state: &ChromeState, area: ratatui::layout::Rect) {
    let line = Line::from(Span::styled(state.message.clone(), theme.dim()));
    frame.render_widget(ratatui::widgets::Paragraph::new(line).style(theme.base()), area);
}
