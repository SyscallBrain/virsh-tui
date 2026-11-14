//! Settings › Doctor: the doctor report, re-runnable, with fixes.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::settings::{SettingsPane, SettingsState};
use crate::doctor::{Report, Status};
use crate::theme::Theme;

/// A fix waiting for `y`/`n`.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingFix {
    /// The fix that was asked about (a re-run may change the rows meanwhile).
    pub fix: crate::doctor::Fix,
    pub question: String,
}

/// Doctor category state (lives in [`SettingsState`]).
#[derive(Debug, Clone, Default)]
pub struct DoctorState {
    pub report: Option<Report>,
    pub running: bool,
    /// Bumped by every run; stale results are dropped.
    pub serial: u64,
    /// Index into `report.checks`.
    pub selected: usize,
    pub pending: Option<PendingFix>,
    pub message: Option<String>,
}

impl DoctorState {
    /// Selected check.
    pub fn current(&self) -> Option<&crate::doctor::Check> {
        self.report.as_ref()?.checks.get(self.selected)
    }

    /// Move the selection, clamped.
    pub fn move_by(&mut self, delta: i32) {
        let n = self.report.as_ref().map_or(0, |r| r.checks.len());
        if n > 0 {
            self.selected = (self.selected as i32 + delta).clamp(0, n as i32 - 1) as usize;
        }
    }
}

/// Status bar keys for the Doctor pane.
pub fn status_hints(st: &DoctorState) -> String {
    if st.pending.is_some() {
        return String::from("  y yes  n no");
    }
    let mut hints = String::from("  j/k check  r re-run");
    if let Some(check) = st.current() {
        if check.fix.is_some() {
            hints.push_str("  f fix");
        }
        if check.command.is_some() {
            hints.push_str("  y copy command");
        }
    }
    hints.push_str("  Y copy report  h/l pane  ⎋ close");
    hints
}

/// Message line text for the Doctor category.
pub fn message(st: &DoctorState) -> String {
    if let Some(p) = &st.pending {
        return format!("{}  y/n", p.question);
    }
    st.message
        .clone()
        .unwrap_or_else(|| String::from("checks are read-only · f applies a fix after asking"))
}

fn glyph_style(theme: &Theme, status: Status) -> Style {
    let t = &theme.tokens;
    match status {
        Status::Ok => Style::default().fg(t.green),
        Status::Warn => Style::default().fg(t.yellow),
        Status::Fail => Style::default().fg(t.red),
        Status::Info | Status::Skip => Style::default().fg(t.comment),
    }
}

/// Render the report into `area` (both layouts; `compact` below 140 columns).
pub fn render(frame: &mut Frame, theme: &Theme, state: &SettingsState, area: Rect, compact: bool) {
    let t = &theme.tokens;
    let st = &state.doctor;
    let focused = state.pane != SettingsPane::Categories;
    let summary = match (&st.report, st.running) {
        (_, true) => String::from("checking…"),
        (Some(report), false) => report.counts().summary(),
        (None, false) => String::new(),
    };
    let footer = (crate::config::key_hints() && !compact).then_some("r re-run · f fix · y copy · Y report");
    let block = crate::ui::widgets::panel::block(
        theme,
        focused,
        "Doctor",
        Some(crate::ui::widgets::panel::right(summary, theme)),
        None,
        footer,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let label_width = if compact { 17 } else { 20 };
    let mut lines: Vec<Line> = Vec::new();
    let mut selected_line = 0;
    // Hint, fix and command of the selected row: a fixed strip at the bottom
    // so they stay visible however short the terminal is.
    let mut detail: Vec<Line<'static>> = Vec::new();
    match &st.report {
        None => lines.push(Line::from(Span::styled("◌ checking…", theme.secondary()))),
        Some(report) => {
            let mut section = None;
            for (i, check) in report.checks.iter().enumerate() {
                if section != Some(check.section) {
                    if section.is_some() {
                        lines.push(Line::from(""));
                    }
                    section = Some(check.section);
                    lines.push(Line::from(Span::styled(
                        check.section.title(),
                        Style::default().fg(t.blue).add_modifier(Modifier::BOLD),
                    )));
                }
                let cur = focused && i == st.selected;
                if cur {
                    selected_line = lines.len();
                }
                let bg = if cur { t.sel } else { t.bg };
                let (glyph, gstyle) = if st.running {
                    ("◌", theme.secondary())
                } else {
                    (check.status.glyph(), glyph_style(theme, check.status))
                };
                let mut marks = Vec::new();
                if check.fix.is_some() {
                    marks.push("f fix");
                }
                if check.command.is_some() {
                    marks.push("y copy");
                }
                let marks = if marks.is_empty() || !crate::config::key_hints() {
                    String::new()
                } else {
                    format!("  {}", marks.join(" · "))
                };
                lines.push(
                    Line::from(vec![
                        Span::styled(if cur { "❯ " } else { "  " }, Style::default().fg(t.blue)),
                        Span::styled(format!("{glyph} "), gstyle),
                        Span::styled(format!("{:<label_width$}", check.label), theme.text()),
                        Span::styled(check.value.clone(), theme.secondary()),
                        Span::styled(marks, Style::default().fg(t.orange)),
                    ])
                    .style(Style::default().bg(bg)),
                );
            }
            if focused && let Some(check) = st.current() {
                if let Some(hint) = &check.hint {
                    detail.push(Line::from(Span::styled(format!("→ {hint}"), theme.secondary())));
                }
                if let Some(fix) = &check.fix {
                    detail.push(Line::from(vec![
                        Span::styled("fix: ", theme.secondary()),
                        Span::styled(fix.describe(), Style::default().fg(t.cyan)),
                    ]));
                }
                if let Some(command) = &check.command {
                    detail.push(Line::from(vec![
                        Span::styled("run: ", theme.secondary()),
                        Span::styled(command.clone(), Style::default().fg(t.cyan)),
                    ]));
                }
            }
        }
    }
    let detail = crate::ui::widgets::review::wrap_lines(detail, inner.width);
    // Keep at least three list rows; the strip takes the rest it needs (+1 gap).
    let strip = if detail.is_empty() {
        0
    } else {
        (detail.len() as u16 + 1).min(inner.height.saturating_sub(3))
    };
    let list_area = Rect::new(inner.x, inner.y, inner.width, inner.height - strip);
    let list_height = list_area.height as usize;
    let start = (selected_line + 1).saturating_sub(list_height);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((start.min(u16::MAX as usize) as u16, 0))
            .style(theme.base()),
        list_area,
    );
    if strip > 0 {
        let strip_area = Rect::new(inner.x, inner.y + list_area.height + 1, inner.width, strip - 1);
        frame.render_widget(Paragraph::new(detail).style(theme.base()), strip_area);
    }
}
