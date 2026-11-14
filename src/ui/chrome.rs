//! Shared chrome: top bar and message line (DESIGN.md section 3).
//!
//! Every view renders these through here, so tabs, the right-hand segment and
//! message colouring are identical everywhere. In live mode the hostname and
//! clock are real; demo/tests keep the mockup values (`forge`, frozen time).

use std::sync::{OnceLock, RwLock};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::Theme;

/// Live chrome facts (set once at startup in live mode; the hostname follows
/// the connection through [`set_hostname`]).
#[derive(Debug, Clone)]
pub struct LiveChrome {
    pub hostname: String,
}

static LIVE: OnceLock<LiveChrome> = OnceLock::new();

/// Hostname of the current connection, replacing [`LiveChrome::hostname`]
/// once set (`:connect` to a remote host, `virsh hostname`).
static HOSTNAME: RwLock<Option<String>> = RwLock::new(None);

/// Switch the chrome to live values (real hostname + wall clock).
pub fn set_live(info: LiveChrome) {
    let _ = LIVE.set(info);
}

/// Show `name` as the host in the top bar (live mode only).
pub fn set_hostname(name: &str) {
    if let Ok(mut h) = HOSTNAME.write() {
        *h = Some(name.to_string());
    }
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

/// The brand at the left of the top bar: the logo glyph `▤` (a rack of VMs,
/// as in the logo) and the name in bold blue on the bar, then a `│` divider.
pub fn brand_spans(theme: &Theme) -> Vec<Span<'static>> {
    let t = &theme.tokens;
    vec![
        Span::styled(
            " ▤ virsh-tui ",
            Style::default().fg(t.blue).bg(t.bg2).add_modifier(Modifier::BOLD),
        ),
        Span::styled("│", Style::default().fg(t.gutter).bg(t.bg2)),
    ]
}

/// One tab. The active tab is a solid blue pill (`bg2` text, bold) so focus
/// reads at a glance; the others are an orange number and a dim name.
pub fn tab_spans(theme: &Theme, n: &str, label: &str, on: bool) -> Vec<Span<'static>> {
    let t = &theme.tokens;
    if on {
        let pill = Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD);
        return vec![Span::styled(format!(" {n} {label} "), pill)];
    }
    vec![
        Span::styled(
            format!(" {n}"),
            Style::default()
                .fg(t.orange)
                .bg(t.bg2)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {label} "), Style::default().fg(t.fg2).bg(t.bg2)),
    ]
}

/// Top bar: brand, tabs (`active` highlighted), right-aligned
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
    let mut left = brand_spans(theme);
    for (i, (n, label)) in TABS.iter().enumerate() {
        left.extend(tab_spans(theme, n, label, active == Some(i)));
    }
    let (host, refresh, time) = match LIVE.get() {
        Some(l) => (
            HOSTNAME
                .read()
                .ok()
                .and_then(|h| h.clone())
                .unwrap_or_else(|| l.hostname.clone()),
            format!("{}s", crate::config::refresh_interval_secs()),
            now_hms(),
        ),
        None => (String::from("forge"), String::from("1s"), demo_time.to_string()),
    };
    let dim = Style::default().fg(t.comment).bg(t.bg2);
    let val = Style::default().fg(t.fg2).bg(t.bg2);
    let mut right = connection_badges(theme);
    right.push(Span::styled("⌁ ", dim));
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

/// Pills before the URI: the connection's profile label (in its colour)
/// and `read-only`.
fn connection_badges(theme: &Theme) -> Vec<Span<'static>> {
    let t = &theme.tokens;
    let mut out = Vec::new();
    if let Some((label, color)) = crate::config::badge() {
        let bg = color
            .as_deref()
            .and_then(|c| theme.named_color(c))
            .unwrap_or(t.blue);
        out.push(Span::styled(
            format!(" {label} "),
            Style::default().fg(t.bg2).bg(bg).add_modifier(Modifier::BOLD),
        ));
        out.push(Span::styled(" ", Style::default().bg(t.bg2)));
    }
    if crate::config::read_only() {
        out.push(Span::styled(
            " read-only ",
            Style::default()
                .fg(t.bg2)
                .bg(t.yellow)
                .add_modifier(Modifier::BOLD),
        ));
        out.push(Span::styled(" ", Style::default().bg(t.bg2)));
    }
    out
}

/// Settings variant of the top bar (`⚙ settings │ time` on the right).
pub fn topbar_settings(frame: &mut Frame, theme: &Theme, area: Rect, demo_time: &str) {
    let t = &theme.tokens;
    let bar = Style::default().bg(t.bg2);
    let mut left = brand_spans(theme);
    for (n, label) in TABS {
        left.extend(tab_spans(theme, n, label, false));
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
/// rest right-aligned (counters, `[+] modified`, extra pills), as in the design spec.
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
        None => spans.push(Span::styled(head.to_string(), theme.secondary())),
    }
    if let Some(cmd) = tail {
        spans.push(Span::styled(format!("  ── {cmd}"), theme.dim()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);
}

/// Short status hints retain the modal state even when breadcrumbs are omitted.
pub fn compact_status(frame: &mut Frame, theme: &Theme, area: Rect, mode: &str, hints: &str) {
    let color = if mode == "VISUAL" {
        theme.tokens.magenta
    } else {
        theme.tokens.blue
    };
    let mut spans = vec![Span::styled(
        format!(" {mode} "),
        Style::default()
            .fg(theme.tokens.bg2)
            .bg(color)
            .add_modifier(Modifier::BOLD),
    )];
    spans.extend(hint_spans(theme, &format!(" {hints}")));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.topbar()), area);
}

/// Key hints (`"  j/k move  ⏎ open"` or `"j/k move · ⏎ open"`): each item's
/// first word is the key, in the bold orange key style used by the top-bar
/// tab numbers; the label and separators stay secondary text.
/// Empty when key hints are off in the settings.
pub fn hint_spans(theme: &Theme, hints: &str) -> Vec<Span<'static>> {
    if !crate::config::key_hints() {
        return Vec::new();
    }
    styled_hints(theme, hints)
}

/// [`hint_spans`] regardless of the setting.
fn styled_hints(theme: &Theme, hints: &str) -> Vec<Span<'static>> {
    let body = hints.trim_start();
    let mut spans = Vec::new();
    if body.len() < hints.len() {
        spans.push(Span::styled(
            hints[..hints.len() - body.len()].to_string(),
            theme.secondary(),
        ));
    }
    let sep = if body.contains(" · ") { " · " } else { "  " };
    for (i, item) in body.split(sep).enumerate() {
        if i > 0 {
            spans.push(Span::styled(sep, theme.secondary()));
        }
        match item.split_once(' ') {
            Some((key, label)) => {
                spans.push(Span::styled(key.to_string(), theme.key()));
                spans.push(Span::styled(format!(" {label}"), theme.secondary()));
            }
            None => spans.push(Span::styled(item.to_string(), theme.key())),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::styled_hints as hint_spans;
    use crate::theme::builtin::by_name;

    #[test]
    fn topbar_shows_the_profile_label_and_read_only() {
        use ratatui::{Terminal, backend::TestBackend};
        let uri = "qemu+ssh://admin@hv01/system";
        let mut cfg = crate::config::Config::default();
        cfg.connections.profiles.insert(
            uri.to_string(),
            crate::config::Profile {
                label: Some(String::from("prod")),
                color: Some(String::from("red")),
                read_only: Some(true),
                ..Default::default()
            },
        );
        crate::config::set_active_uri(uri);
        crate::config::set_runtime(&cfg);
        let theme = by_name("tokyonight");
        let mut terminal = Terminal::new(TestBackend::new(174, 1)).unwrap();
        terminal
            .draw(|f| super::topbar(f, &theme, f.area(), Some(0), uri, true, "14:40:03"))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let text: String = (0..174).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(
            text.contains(" prod   read-only  ⌁ qemu+ssh://admin@hv01/system"),
            "{text}"
        );
        let x = text.chars().position(|c| c == 'p').unwrap() as u16;
        assert_eq!(buf[(x, 0)].bg, theme.tokens.red);
        crate::config::set_active_uri("");
        crate::config::set_runtime(&crate::config::Config::default());
    }

    #[test]
    fn hint_keys_use_the_key_style() {
        let theme = by_name("tokyonight");
        for hints in ["  j/k move  ⏎ open  ? help", "j/k move · ⏎ open · ? help"] {
            let spans = hint_spans(&theme, hints);
            let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
            assert_eq!(text, hints);
            let keys: Vec<&str> = spans
                .iter()
                .filter(|s| s.style == theme.key())
                .map(|s| s.content.as_ref())
                .collect();
            assert_eq!(keys, ["j/k", "⏎", "?"]);
        }
    }
}
