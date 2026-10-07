//! Panel container (PLAN.md section 7.1 / DESIGN.md "Panel").
//!
//! Rounded border (`gutter`, or `blue` when focused), bold title top-left,
//! optional right title top-right, and optional footers on the bottom border.
//! Every view builds its panels through `block` so the chrome stays identical.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph},
};

use crate::theme::Theme;

/// Build a panel block.
pub fn block(
    theme: &Theme,
    focused: bool,
    title: &str,
    title_right: Option<Line<'static>>,
    footer_left: Option<&str>,
    footer_right: Option<&str>,
) -> Block<'static> {
    let mut b = Block::default()
        .borders(Borders::ALL)
        .border_type(theme.border)
        .border_style(theme.panel_border(focused))
        .padding(Padding::horizontal(1))
        .title(Span::styled(format!(" {title} "), theme.panel_title(focused)));
    if let Some(mut rt) = title_right {
        rt.spans.insert(0, Span::styled(" ", theme.dim()));
        rt.spans.push(Span::styled(" ", theme.dim()));
        b = b.title_top(rt.style(theme.dim()).right_aligned());
    }
    if let Some(fl) = footer_left {
        b = b.title_bottom(Line::from(Span::styled(format!(" {fl} "), theme.dim())).left_aligned());
    }
    if let Some(fr) = footer_right {
        b = b.title_bottom(Line::from(Span::styled(format!(" {fr} "), theme.dim())).right_aligned());
    }
    b
}

/// Plain right-title helper (dim text).
pub fn right(text: impl Into<String>, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme.dim()))
}

/// Render a panel with a plain-text body.
#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut ratatui::Frame,
    area: Rect,
    theme: &Theme,
    focused: bool,
    title: &str,
    title_right: Option<&str>,
    footer_left: Option<&str>,
    footer_right: Option<&str>,
    body: &str,
) {
    let b = block(
        theme,
        focused,
        title,
        title_right.map(|t| right(t, theme)),
        footer_left,
        footer_right,
    );
    frame.render_widget(
        Paragraph::new(body.to_string()).block(b).style(theme.base()),
        area,
    );
}
