//! XML syntax highlighting for the XML tab.

use ratatui::{style::Style, text::Line, text::Span};

use crate::theme::Theme;

/// Highlight one XML line: tags blue, values yellow, comments dim.
pub fn highlight_line(theme: &Theme, line: &str) -> Line<'static> {
    let t = &theme.tokens;
    let trimmed = line.trim_start();
    if trimmed.starts_with("<!--") {
        return Line::from(Span::styled(line.to_string(), theme.dim()));
    }
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(lt) = rest.find('<') {
        if lt > 0 {
            spans.push(Span::styled(rest[..lt].to_string(), theme.text()));
        }
        let after = &rest[lt..];
        let end = after.find('>').map(|i| lt + i + 1).unwrap_or(rest.len());
        let tag = &rest[lt..end];
        spans.push(Span::styled(
            tag.to_string(),
            Style::default().fg(t.blue).bg(t.bg),
        ));
        rest = &rest[end..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_string(), theme.text()));
    }
    if spans.is_empty() {
        spans.push(Span::raw(line.to_string()));
    }
    Line::from(spans)
}
