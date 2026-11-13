//! CONFIRM modal (red border, CONFIRM mode).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

use crate::theme::Theme;

/// What a confirmation guards. Settings › Confirmations is keyed on this,
/// never on the title or label text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfirmKind {
    DomainDestroy,
    DomainReset,
    DomainUndefine,
    VolumeDelete,
    VolumeWipe,
    /// Always asks (pool destroy, network destroy/undefine, detach, clone,
    /// managed-save, raw destructive commands, …).
    #[default]
    Always,
}

/// Pending confirmation.
#[derive(Debug, Clone)]
pub struct Confirm {
    /// What this confirmation guards (drives Settings › Confirmations).
    pub kind: ConfirmKind,
    /// Title, e.g. `⚠ Destroy domain`.
    pub title: String,
    /// Question line.
    pub question: String,
    /// Command preview lines.
    pub commands: Vec<String>,
    /// Confirm chip label, e.g. `y destroy`.
    pub confirm_label: String,
    /// Type-the-name requirement (undefine).
    pub type_name: Option<String>,
    /// What the user typed so far (type-the-name mode).
    pub typed: String,
}

impl Confirm {
    /// True when `y`/Enter may proceed.
    pub fn satisfied(&self) -> bool {
        self.type_name.as_ref().is_none_or(|n| *n == self.typed)
    }

    /// The destructive list always confirms (Appendix A): commands that
    /// destroy data or running state, or that a typo turns into an outage.
    pub fn destructive(action: &str) -> bool {
        matches!(
            action,
            "destroy"
                | "reset"
                | "undefine"
                | "managedsave-remove"
                | "snapshot-revert"
                | "snapshot-delete"
                | "checkpoint-delete"
                | "detach-disk"
                | "detach-interface"
                | "detach-device"
                | "detach-device-alias"
                | "blockresize"
                | "blockcommit"
                | "blockpull"
                | "migrate"
                | "net-destroy"
                | "net-undefine"
                | "pool-destroy"
                | "pool-delete"
                | "pool-undefine"
                | "vol-delete"
                | "vol-wipe"
                | "vol-upload"
                | "iface-destroy"
                | "iface-undefine"
                | "nodedev-destroy"
                | "nwfilter-undefine"
                | "secret-undefine"
                | "qemu-monitor-command"
                | "qemu-agent-command"
                | "migrate-undefine"
        )
    }

    /// [`Confirm::destructive`] for a whole argv (`vol-resize --shrink`).
    pub fn destructive_argv(argv: &[String]) -> bool {
        argv.first().is_some_and(|a| Self::destructive(a))
            || (argv.first().is_some_and(|a| a == "vol-resize") && argv.iter().any(|a| a == "--shrink"))
    }
}

/// Render a centered confirm modal (same look as the snapshot revert modal).
pub fn render(frame: &mut Frame, theme: &Theme, area: Rect, confirm: &Confirm) {
    let t = &theme.tokens;
    let w = 88u16.min(area.width.saturating_sub(4));
    let extra = if confirm.type_name.is_some() { 3 } else { 0 };
    let h = (9 + confirm.commands.len() as u16 + extra).min(area.height.saturating_sub(2));
    let modal = Rect::new(
        area.x + area.width.saturating_sub(w) / 2,
        area.y + area.height.saturating_sub(h) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme.border)
        .border_style(Style::default().fg(t.red))
        .padding(Padding::new(2, 2, 1, 0))
        .style(Style::default().bg(t.bg))
        .title(Span::styled(
            format!(" {} ", confirm.title),
            Style::default().fg(t.red).add_modifier(Modifier::BOLD),
        ));
    let mut lines = vec![
        Line::from(Span::styled(
            confirm.question.clone(),
            Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for cmd in &confirm.commands {
        let step = crate::command::plan::CommandStep {
            program: cmd.split_whitespace().next().unwrap_or("").to_string(),
            argv: shlex::split(cmd)
                .unwrap_or_default()
                .into_iter()
                .skip(1)
                .collect(),
            stdin: None,
        };
        lines.push(
            Line::from(
                crate::command::display::spans(theme, &step)
                    .into_iter()
                    .map(|s| Span::styled(s.content.into_owned(), s.style.bg(t.bg2)))
                    .collect::<Vec<_>>(),
            )
            .style(Style::default().bg(t.bg2)),
        );
    }
    if let Some(name) = &confirm.type_name {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("type ", theme.dim()),
            Span::styled(
                name.clone(),
                Style::default().fg(t.blue).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to confirm", theme.dim()),
        ]));
        let ok = confirm.satisfied();
        lines.push(Line::from(vec![
            Span::styled("❯ ", Style::default().fg(t.magenta)),
            Span::styled(
                confirm.typed.clone(),
                Style::default().fg(if ok { t.green } else { t.fg }),
            ),
            Span::styled(" ", Style::default().bg(t.fg)),
        ]));
    }
    lines.push(Line::from(""));
    let label = if confirm.type_name.is_some() {
        confirm.confirm_label.replacen("y ", "⏎ ", 1)
    } else {
        confirm.confirm_label.clone()
    };
    let cancel = if confirm.type_name.is_some() {
        " ⎋ cancel "
    } else {
        " n cancel "
    };
    let inner = w.saturating_sub(6) as usize;
    let used = cancel.chars().count() + label.chars().count() + 4;
    let confirm_style = if confirm.satisfied() {
        Style::default().fg(t.bg2).bg(t.red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(t.comment).bg(t.hl)
    };
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(inner.saturating_sub(used))),
        Span::styled(cancel, Style::default().fg(t.fg2).bg(t.hl)),
        Span::raw("  "),
        Span::styled(format!(" {label} "), confirm_style),
    ]));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(theme.base()),
        modal,
    );
}
