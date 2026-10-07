//! Shared chrome: top bar and message line (DESIGN.md section 3).
//!
//! Every view renders these through here, so tabs, the right-hand segment and
//! message colouring are identical everywhere. In live mode the hostname and
//! clock are real; demo/tests keep the mockup values (`forge`, frozen time).

use std::sync::OnceLock;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::Theme;

/// Live chrome facts (set once at startup in live mode).
#[derive(Debug, Clone)]
pub struct LiveChrome {
    pub hostname: String,
    pub refresh_secs: u64,
}

static LIVE: OnceLock<LiveChrome> = OnceLock::new();

/// Switch the chrome to live values (real hostname + wall clock).
pub fn set_live(info: LiveChrome) {
    let _ = LIVE.set(info);
}

/// True once live mode is active (false in demo and tests).
pub fn is_live() -> bool {
    LIVE.get().is_some()
}

const TABS: [(&str, &str); 5] = [
    ("1", "Domains"),
    ("2", "Host"),
    ("3", "Networks"),
    ("4", "Storage"),
    ("5", "Events"),
];

fn now_hms() -> String {
    jiff::Zoned::now().strftime("%H:%M:%S").to_string()
}

/// Top bar: logo pill, tabs (`active` highlighted), right-aligned
/// `⌁ uri │ host │ ⟳ Ns │ HH:MM:SS`. `demo_time` is the mockup clock.
pub fn topbar(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    active: Option<usize>,
    uri: &str,
    connected: bool,
    demo_time: &str,
) {
    let t = &theme.tokens;
    let bar = Style::default().bg(t.bg2);
    let mut left = vec![Span::styled(
        " ◆ virsh-tui ",
        Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
    )];
    for (i, (n, label)) in TABS.iter().enumerate() {
        let on = active == Some(i);
        let bg = if on { t.bg } else { t.bg2 };
        let label_style = if on {
            Style::default().fg(t.blue).bg(bg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.comment).bg(bg)
        };
        left.push(Span::styled(
            format!(" {n}"),
            Style::default().fg(t.orange).bg(bg).add_modifier(Modifier::BOLD),
        ));
        left.push(Span::styled(format!(" {label} "), label_style));
    }
    let (host, refresh, time) = match LIVE.get() {
        Some(l) => (l.hostname.clone(), format!("{}s", l.refresh_secs), now_hms()),
        None => (String::from("forge"), String::from("1s"), demo_time.to_string()),
    };
    let dim = Style::default().fg(t.comment).bg(t.bg2);
    let val = Style::default().fg(t.fg2).bg(t.bg2);
    let mut right = vec![Span::styled("⌁ ", dim)];
    if connected {
        right.push(Span::styled(uri.to_string(), val));
    } else {
        right.push(Span::styled(
            "disconnected",
            Style::default().fg(t.red).bg(t.bg2).add_modifier(Modifier::BOLD),
        ));
    }
    for (sep, v) in [("  │  ", host), ("  │  ⟳ ", refresh), ("  │  ", time)] {
        right.push(Span::styled(sep, dim));
        right.push(Span::styled(v, val));
    }
    right.push(Span::styled("  ", dim));
    render_split(frame, area, left, right, bar);
}

/// Settings variant of the top bar (`⚙ settings │ time` on the right).
pub fn topbar_settings(frame: &mut Frame, theme: &Theme, area: Rect, demo_time: &str) {
    let t = &theme.tokens;
    let bar = Style::default().bg(t.bg2);
    let mut left = vec![Span::styled(
        " ◆ virsh-tui ",
        Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
    )];
    for (n, label) in TABS {
        left.push(Span::styled(
            format!(" {n}"),
            Style::default()
                .fg(t.orange)
                .bg(t.bg2)
                .add_modifier(Modifier::BOLD),
        ));
        left.push(Span::styled(
            format!(" {label} "),
            Style::default().fg(t.comment).bg(t.bg2),
        ));
    }
    let time = if LIVE.get().is_some() {
        now_hms()
    } else {
        demo_time.to_string()
    };
    let right = vec![
        Span::styled("⚙ settings  │  ", Style::default().fg(t.comment).bg(t.bg2)),
        Span::styled(format!("{time}  "), Style::default().fg(t.fg2).bg(t.bg2)),
    ];
    render_split(frame, area, left, right, bar);
}

/// Status bar: `spans[..right_from]` left (pill, breadcrumb, hints) and the
/// rest right-aligned (counters, `[+] modified`, extra pills), as in the mockups.
pub fn statusbar(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    mut spans: Vec<Span<'static>>,
    right_from: usize,
) {
    let right = spans.split_off(right_from.min(spans.len()));
    render_split(frame, area, spans, right, theme.topbar());
}

/// Render `left` then `right` right-aligned on one line, filling with `bar`.
pub fn render_split(
    frame: &mut Frame,
    area: Rect,
    left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    bar: Style,
) {
    let lw: usize = left.iter().map(Span::width).sum();
    let rw: usize = right.iter().map(Span::width).sum();
    let gap = (area.width as usize).saturating_sub(lw + rw);
    let mut spans = left;
    spans.push(Span::styled(" ".repeat(gap), bar));
    spans.extend(right);
    frame.render_widget(Paragraph::new(Line::from(spans)).style(bar), area);
}

/// Styled message line: `✓` green, `✗` red, `⚠` yellow, `⏻` blue, `[dry-run]`
/// yellow; the `  ── <command>` tail is dimmed. Other text is dim (mode hints).
pub fn message_line(frame: &mut Frame, theme: &Theme, area: Rect, msg: &str) {
    let t = &theme.tokens;
    let (head, tail) = match msg.split_once("  ── ") {
        Some((h, c)) => (h, Some(c)),
        None => (msg, None),
    };
    let mut spans = Vec::new();
    let prefixes = [
        ("✓ ", t.green),
        ("✗ ", t.red),
        ("⚠ ", t.yellow),
        ("⏻ ", t.blue),
        ("[dry-run] ", t.yellow),
    ];
    match prefixes.iter().find(|(p, _)| head.starts_with(p)) {
        Some((p, c)) => {
            spans.push(Span::styled(
                p.to_string(),
                Style::default().fg(*c).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(head[p.len()..].to_string(), theme.text()));
        }
        None => spans.push(Span::styled(head.to_string(), theme.dim())),
    }
    if let Some(cmd) = tail {
        spans.push(Span::styled(format!("  ── {cmd}"), theme.dim()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);
}
