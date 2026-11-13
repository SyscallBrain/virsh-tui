//! Lossless cell wrapping for command reviews. Spaces and graphemes are retained.
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

pub fn wrap_lines(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width.max(1));
    let mut out = Vec::new();
    for line in lines {
        let mut spans = Vec::new();
        let mut used = 0;
        for grapheme in line.styled_graphemes(Style::default()) {
            let cells = grapheme.symbol.width();
            if used + cells > width && used > 0 {
                out.push(Line::from(std::mem::take(&mut spans)).style(line.style));
                used = 0;
            }
            spans.push(Span::styled(grapheme.symbol.to_owned(), grapheme.style));
            used += cells;
        }
        out.push(Line::from(spans).style(line.style));
    }
    out
}

/// Keep a suffix within terminal cells without splitting a grapheme cluster.
pub fn tail(value: &str, width: u16) -> String {
    let line = Line::from(value);
    let graphemes: Vec<String> = line
        .styled_graphemes(Style::default())
        .map(|g| g.symbol.to_owned())
        .collect();
    let mut used = 0;
    let mut suffix = Vec::new();
    for symbol in graphemes.iter().rev() {
        let cells = symbol.as_str().width();
        if used + cells > width as usize {
            break;
        }
        suffix.push(symbol.as_str());
        used += cells;
    }
    suffix.into_iter().rev().collect()
}
