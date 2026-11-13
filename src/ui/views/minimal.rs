//! Degraded single-column layout for small terminals (below 100×30, at least
//! 40×12): one header line, one list, a key line and the message line.
//! Every key keeps working; only the side panels and charts are dropped.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::Theme;

/// One list row: text plus an optional accent colour.
pub struct Row {
    pub text: String,
    pub style: Option<Style>,
}

impl Row {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style: Some(style),
        }
    }
}

/// Render `title` + `rows` (cursor on `selected`) + `keys` + `message`.
#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    theme: &Theme,
    title: &str,
    header: &str,
    rows: &[Row],
    selected: Option<usize>,
    keys: &str,
    message: &str,
) {
    let t = &theme.tokens;
    let area = frame.area();
    frame.render_widget(Paragraph::new("").style(theme.base()), area);
    if area.height < 4 {
        return;
    }
    let line = |y: u16| Rect::new(area.x, area.y + y, area.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!(" {title} "),
            Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
        )]))
        .style(theme.topbar()),
        line(0),
    );
    frame.render_widget(
        Paragraph::new(Span::styled(header.to_string(), theme.dim())),
        line(1),
    );
    let list_h = area.height.saturating_sub(4) as usize;
    let selected_row = selected.unwrap_or(0).min(rows.len().saturating_sub(1));
    let first = selected_row.saturating_sub(list_h.saturating_sub(1));
    for (i, row) in rows.iter().enumerate().skip(first).take(list_h) {
        let is_selected = selected == Some(i);
        let base = row.style.unwrap_or_else(|| Style::default().fg(t.fg));
        let style = if is_selected {
            base.bg(t.sel).add_modifier(Modifier::BOLD)
        } else {
            base
        };
        let marker = if is_selected { "▌" } else { " " };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.blue)),
                Span::styled(row.text.clone(), style),
            ])),
            line(2 + (i - first) as u16),
        );
    }
    if rows.is_empty() {
        frame.render_widget(Paragraph::new(Span::styled(" (empty)", theme.dim())), line(2));
    }
    frame.render_widget(
        Paragraph::new(Span::styled(keys.to_string(), theme.dim())).style(theme.topbar()),
        line(area.height - 2),
    );
    frame.render_widget(
        Paragraph::new(Span::raw(message.to_string())).style(theme.base()),
        line(area.height - 1),
    );
}

/// Pad/cut `s` to `w` display columns.
pub fn col(s: &str, w: usize) -> String {
    let mut out: String = s.chars().take(w).collect();
    let n = out.chars().count();
    out.extend(std::iter::repeat_n(' ', w.saturating_sub(n) + 1));
    out
}
