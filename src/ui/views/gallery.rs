//! Gallery screen (P1): shows every widget with demo data.
use crate::theme::Theme;
use crate::ui::widgets::{
    charts::{self, BrailleMode},
    gauge, hbar,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    text::Line,
    widgets::Paragraph,
};

/// Render the gallery demo.
pub fn render(frame: &mut Frame, theme: &Theme) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);
    let cpu = charts::series(7, 90, 0.42, 0.22, 0.1);
    let braille_rows = charts::braille(&cpu, 60, 8, BrailleMode::Fill);
    // Clamp to available rows.
    let text: Vec<Line> = braille_rows
        .iter()
        .take(rows[0].height as usize)
        .map(|s| Line::from(s.clone()))
        .collect();
    frame.render_widget(Paragraph::new(text).style(theme.base()), rows[0]);
    let mem = charts::series(11, 60, 0.74, 0.06, 0.2);
    let spark = charts::spark_rows(&mem, 28, 3);
    let spark_text: Vec<Line> = spark.iter().map(|s| Line::from(s.clone())).collect();
    frame.render_widget(Paragraph::new(spark_text).style(theme.base()), rows[1]);
    let (fill, rest) = hbar::hbar(0.42, 16);
    let (on, off) = gauge::lg(0.38, 22);
    let line = format!("hbar {fill}{rest} 42%   gauge {on}{off} 38%");
    frame.render_widget(Paragraph::new(line).style(theme.base()), rows[2]);
    let chips = " all 15  \u{25cf} running 9  \u{2016} paused 1  \u{2717} crashed 1  \u{25cb} off 4";
    frame.render_widget(Paragraph::new(chips).style(theme.base()), rows[3]);
}
