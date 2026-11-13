//! `:messages` overlay: the full message history (job output included).

use ratatui::{
    Frame,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::theme::Theme;

/// Render the history, newest at the bottom; `scroll` counts lines up from it.
pub fn render(frame: &mut Frame, theme: &Theme, lines: &[String], scroll: usize) {
    let t = &theme.tokens;
    let area = frame.area();
    let w = area.width.saturating_sub(8).clamp(1, 160);
    let h = area.height.saturating_sub(4).max(3);
    let modal = crate::ui::widgets::modal::centered(area, w, h);
    frame.render_widget(Clear, modal);
    let rows = usize::from(modal.height.saturating_sub(2));
    let end = lines.len().saturating_sub(scroll);
    let start = end.saturating_sub(rows);
    let body: Vec<Line> = if lines.is_empty() {
        vec![Line::from(Span::styled("no messages yet", theme.dim()))]
    } else {
        lines[start..end]
            .iter()
            .map(|l| {
                let style = if l.starts_with('✗') {
                    Style::default().fg(t.red)
                } else if l.starts_with('⚠') {
                    Style::default().fg(t.yellow)
                } else {
                    Style::default().fg(t.fg)
                };
                Line::from(Span::styled(l.clone(), style))
            })
            .collect()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme.border)
        .border_style(Style::default().fg(t.blue))
        .style(Style::default().bg(t.bg))
        .title(Span::styled(
            format!(" :messages · {}/{} ", end, lines.len()),
            Style::default().fg(t.fg2),
        ))
        .title_bottom(
            Line::from(Span::styled(" j/k C-d/C-u scroll · ⎋ close ", theme.dim())).right_aligned(),
        );
    frame.render_widget(Paragraph::new(body).block(block), modal);
}
