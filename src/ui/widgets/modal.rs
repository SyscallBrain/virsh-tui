//! Modal helpers (PLAN.md 7.13).

use ratatui::{Frame, style::Modifier};

use crate::theme::Theme;

/// Centered rect of at most `w`×`h` inside `area`.
pub fn centered(area: ratatui::layout::Rect, w: u16, h: u16) -> ratatui::layout::Rect {
    let w = w.min(area.width.saturating_sub(4));
    let h = h.min(area.height.saturating_sub(4));
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    ratatui::layout::Rect::new(x, y, w, h)
}

/// "Dim behind modals": recolour everything drawn so far to `gutter` on a flat
/// background. Call right before drawing a modal. No-op when disabled.
pub fn dim_background(frame: &mut Frame, theme: &Theme) {
    if !theme.dim_modals {
        return;
    }
    let t = &theme.tokens;
    let area = frame.area();
    let buf = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                let bars = cell.bg == t.bg2;
                cell.set_fg(t.gutter);
                cell.set_bg(if bars { t.bg2 } else { t.bg });
                cell.modifier = Modifier::empty();
            }
        }
    }
}

/// Transparent background: every cell painted with the theme `bg` becomes
/// `Reset` so the terminal background shows through. Call after a frame.
pub fn apply_transparency(frame: &mut Frame, theme: &Theme) {
    if !theme.transparent {
        return;
    }
    let bg = theme.tokens.bg;
    let area = frame.area();
    let buf = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y))
                && cell.bg == bg
            {
                cell.set_bg(ratatui::style::Color::Reset);
            }
        }
    }
}
