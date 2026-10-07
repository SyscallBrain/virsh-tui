//! Command display: shlex-quoted single-line text plus styled spans.
//!
//! Style: `$ ` comment · program green · subcommand blue · object fg ·
//! flags cyan · numerics orange · paths yellow.

use ratatui::{style::Style, text::Span};

use crate::theme::Theme;

use super::plan::CommandStep;

/// Plain-text display with shell quoting for copy-to-clipboard.
pub fn display(step: &CommandStep) -> String {
    let mut parts = vec![step.program.clone()];
    for a in &step.argv {
        parts.push(
            shlex::try_quote(a)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| a.clone()),
        );
    }
    parts.join(" ")
}

/// Multi-step display (one line per step).
pub fn display_plan(steps: &[CommandStep]) -> String {
    steps.iter().map(display).collect::<Vec<_>>().join(" && ")
}

/// Styled spans for the preview box.
pub fn spans<'a>(theme: &Theme, step: &'a CommandStep) -> Vec<Span<'a>> {
    let t = &theme.tokens;
    let mut out = vec![
        Span::styled("$ ", theme.dim()),
        Span::styled(step.program.clone(), Style::default().fg(t.green).bg(t.bg)),
        Span::raw(" "),
    ];
    for (i, a) in step.argv.iter().enumerate() {
        let style = if i == 0 {
            Style::default().fg(t.blue).bg(t.bg)
        } else if a.starts_with("--") {
            Style::default().fg(t.cyan).bg(t.bg)
        } else if a.parse::<f64>().is_ok() {
            Style::default().fg(t.orange).bg(t.bg)
        } else if a.contains('/') || a.contains('.') {
            Style::default().fg(t.yellow).bg(t.bg)
        } else {
            theme.text()
        };
        out.push(Span::styled(a.clone(), style));
        out.push(Span::raw(" "));
    }
    out
}
