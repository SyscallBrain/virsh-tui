//! Responsive layout breakpoints (PLAN.md section 9.14).

/// Terminal size class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakpoint {
    /// 160+ columns: reference two-column layouts.
    Wide,
    /// 100-159 columns: stacked single column.
    Compact,
    /// Below 100x30: too-small panel.
    TooSmall,
}

/// Classify a terminal size.
pub fn breakpoint(width: u16, height: u16) -> Breakpoint {
    if width < 100 || height < 30 {
        Breakpoint::TooSmall
    } else if width < 160 {
        Breakpoint::Compact
    } else {
        Breakpoint::Wide
    }
}

/// Centered rect, clamped to the area.
pub fn centered(area: ratatui::layout::Rect, w: u16, h: u16) -> ratatui::layout::Rect {
    // Never larger than the area itself (a 20x3 terminal panicked here).
    let w = w.min(area.width.saturating_sub(4)).max(10).min(area.width);
    let h = h.min(area.height.saturating_sub(4)).max(5).min(area.height);
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    ratatui::layout::Rect::new(x, y, w, h)
}

/// Too-small terminal panel.
pub fn render_too_small(frame: &mut ratatui::Frame, theme: &crate::theme::Theme) {
    use ratatui::{
        style::{Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Paragraph},
    };
    let area = frame.area();
    let w = area.width;
    let h = area.height;
    let msg = format!("Terminal too small — {w}×{h}, need 100×30");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme.border)
        .border_style(Style::default().fg(theme.tokens.gutter).bg(theme.bg()));
    let inner = centered(area, msg.len() as u16 + 8, 5);
    let line = Line::from(Span::styled(
        msg,
        Style::default()
            .fg(theme.tokens.fg)
            .bg(theme.bg())
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(line).block(block).style(theme.base()), inner);
}

#[cfg(test)]
mod tests {
    #[test]
    fn centered_fits_tiny_areas() {
        for (w, h) in [(1, 1), (8, 3), (20, 3), (200, 60)] {
            let area = ratatui::layout::Rect::new(0, 0, w, h);
            let r = super::centered(area, 60, 20);
            assert!(r.right() <= area.right() && r.bottom() <= area.bottom());
        }
    }
}
