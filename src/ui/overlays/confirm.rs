//! CONFIRM modal (red border, CONFIRM mode).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
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

/// Render a confirmation at its first page.
pub fn render(frame: &mut Frame, theme: &Theme, area: Rect, confirm: &Confirm) {
    render_scrolled(frame, theme, area, confirm, 0);
}

/// Wrapped preview with fixed actions. Returns the last valid scroll offset.
pub fn render_scrolled(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    confirm: &Confirm,
    scroll: usize,
) -> usize {
    let t = &theme.tokens;
    if crate::ui::layout::breakpoint(area.width, area.height) == crate::ui::layout::Breakpoint::TooSmall {
        crate::ui::layout::render_too_small(frame, theme);
        return 0;
    }
    let w = 88.min(area.width);
    let extra = if confirm.type_name.is_some() { 3 } else { 0 };
    let mut lines: Vec<Line> = confirm
        .question
        .lines()
        .map(|line| {
            Line::from(Span::styled(
                line.to_owned(),
                theme.text().add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    lines.push(Line::from(""));
    for cmd in &confirm.commands {
        // Parse only for syntax colouring. Display the original command unchanged.
        let step = crate::command::plan::CommandStep {
            program: cmd.split_whitespace().next().unwrap_or("").to_owned(),
            argv: shlex::split(cmd)
                .unwrap_or_default()
                .into_iter()
                .skip(1)
                .collect(),
            stdin: None,
        };
        lines.push(Line::from(
            crate::command::display::spans(theme, &step)
                .into_iter()
                .map(|span| Span::styled(span.content.into_owned(), span.style))
                .collect::<Vec<_>>(),
        ));
    }
    let lines = crate::ui::widgets::review::wrap_lines(lines, w.saturating_sub(4));
    let content_rows = lines.len();
    let paragraph = Paragraph::new(lines).style(theme.base());
    let h = (content_rows.saturating_add(5 + extra)).min(area.height as usize) as u16;
    let modal = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme.border)
        .border_style(Style::default().fg(t.red))
        .style(theme.base())
        .title(Span::styled(
            format!(" {} ", confirm.title),
            Style::default().fg(t.red).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let footer_h = 2 + extra as u16;
    let body = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height.saturating_sub(footer_h),
    );
    let max_scroll = content_rows
        .saturating_sub(body.height as usize)
        .min(u16::MAX as usize);
    let scroll = scroll.min(max_scroll);
    frame.render_widget(paragraph.scroll((scroll as u16, 0)), body);
    let mut y = body.bottom();
    if let Some(name) = &confirm.type_name {
        // Keep the tail of long names and input visible; arrows scroll the preview.
        let tail = |value: &str| crate::ui::widgets::review::tail(value, body.width.saturating_sub(7));
        frame.render_widget(
            Paragraph::new(format!("type {}", tail(name))).style(theme.secondary()),
            Rect::new(body.x, y, body.width, 1),
        );
        y += 1;
        frame.render_widget(
            Paragraph::new(format!("❯ {}▏", tail(&confirm.typed))).style(theme.text()),
            Rect::new(body.x, y, body.width, 1),
        );
        y += 1;
        // Reserve one spacer above the footer.
        y += 1;
    }
    let hint = if max_scroll > 0 {
        format!(
            "{}{}/{} · {} cmd",
            if crate::config::key_hints() {
                "↑↓/PgUp/Dn "
            } else {
                ""
            },
            scroll + 1,
            max_scroll + 1,
            confirm.commands.len()
        )
    } else {
        format!("{} command(s)", confirm.commands.len())
    };
    frame.render_widget(
        Paragraph::new(hint).style(theme.secondary()),
        Rect::new(body.x, y, body.width, 1),
    );
    y += 1;
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
    let action_style = if confirm.satisfied() {
        Style::default().fg(t.bg2).bg(t.red).add_modifier(Modifier::BOLD)
    } else {
        theme.secondary().bg(t.hl)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(cancel, theme.secondary().bg(t.hl)),
            Span::raw("  "),
            Span::styled(format!(" {label} "), action_style),
        ])),
        Rect::new(body.x, y, body.width, 1),
    );
    max_scroll
}
