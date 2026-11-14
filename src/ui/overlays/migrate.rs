//! Migrate dialog (`Space M`): destination, options and the `virsh migrate`
//! command it will run.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::input::textinput::TextInput;
use crate::theme::Theme;
use crate::ui::widgets::form::{checkbox, radio};

/// Storage copy for domains without shared storage.
pub const STORAGE: [&str; 3] = ["shared (no copy)", "copy all disks", "copy incremental"];

/// Rows of the dialog, in focus order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Dest,
    Live,
    Persistent,
    UndefineSource,
    Storage,
    AutoConverge,
    Compressed,
}

const ROWS: [Row; 7] = [
    Row::Dest,
    Row::Live,
    Row::Persistent,
    Row::UndefineSource,
    Row::Storage,
    Row::AutoConverge,
    Row::Compressed,
];

/// Dialog state.
#[derive(Debug, Clone)]
pub struct MigrateState {
    pub domain: String,
    /// The domain runs: live migration is possible (else `--offline`).
    pub running: bool,
    /// Current connection (not a valid destination).
    pub source_uri: String,
    pub dest: TextInput,
    /// Saved URIs offered with `C-n` / `C-p`.
    pub suggestions: Vec<String>,
    pub focus: usize,
    pub live: bool,
    pub persistent: bool,
    pub undefine_source: bool,
    pub storage: usize,
    pub auto_converge: bool,
    pub compressed: bool,
    pub error: Option<String>,
}

/// What a key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    None,
    Close,
    /// Run this `virsh` argv (without `-c`).
    Submit(Vec<String>),
}

impl MigrateState {
    pub fn new(domain: &str, running: bool, source_uri: &str, saved: &[String]) -> Self {
        let suggestions: Vec<String> = saved.iter().filter(|u| *u != source_uri).cloned().collect();
        Self {
            domain: domain.to_string(),
            running,
            source_uri: source_uri.to_string(),
            dest: TextInput::with(suggestions.first().map_or("qemu+ssh://", String::as_str)),
            suggestions,
            focus: 0,
            live: running,
            persistent: true,
            undefine_source: false,
            storage: 0,
            auto_converge: false,
            compressed: false,
            error: None,
        }
    }

    fn row(&self) -> Row {
        ROWS[self.focus % ROWS.len()]
    }

    /// `virsh migrate` arguments, or why they cannot be built.
    pub fn argv(&self) -> Result<Vec<String>, String> {
        let dest = self.dest.value().trim();
        if !dest.contains("://") || dest.ends_with("://") {
            return Err(String::from(
                "destination must be a libvirt URI, e.g. qemu+ssh://host/system",
            ));
        }
        if dest == self.source_uri {
            return Err(String::from("destination is the current connection"));
        }
        let mut argv = vec![String::from("migrate")];
        if self.running && self.live {
            argv.push(String::from("--live"));
        }
        if !self.running {
            // libvirt needs both to move a shut-off domain's definition.
            argv.extend([String::from("--offline"), String::from("--persistent")]);
        } else if self.persistent {
            argv.push(String::from("--persistent"));
        }
        if self.undefine_source {
            argv.push(String::from("--undefinesource"));
        }
        match self.storage {
            1 if self.running => argv.push(String::from("--copy-storage-all")),
            2 if self.running => argv.push(String::from("--copy-storage-inc")),
            _ => {}
        }
        if self.running && self.auto_converge {
            argv.push(String::from("--auto-converge"));
        }
        if self.running && self.compressed {
            argv.push(String::from("--compressed"));
        }
        argv.push(self.domain.clone());
        argv.push(dest.to_string());
        Ok(argv)
    }

    fn cycle_suggestion(&mut self, delta: i32) {
        if self.suggestions.is_empty() {
            return;
        }
        let n = self.suggestions.len() as i32;
        let cur = self
            .suggestions
            .iter()
            .position(|s| s == self.dest.value())
            .map_or(if delta > 0 { -1 } else { 0 }, |i| i as i32);
        let next = (cur + delta).rem_euclid(n) as usize;
        self.dest = TextInput::with(&self.suggestions[next]);
    }

    /// Handle a key.
    pub fn key(&mut self, key: crossterm::event::KeyEvent) -> Outcome {
        use crossterm::event::{KeyCode, KeyModifiers};
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        self.error = None;
        match key.code {
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Enter => {
                return match self.argv() {
                    Ok(argv) => Outcome::Submit(argv),
                    Err(e) => {
                        self.error = Some(e);
                        Outcome::None
                    }
                };
            }
            KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % ROWS.len(),
            KeyCode::BackTab | KeyCode::Up => self.focus = (self.focus + ROWS.len() - 1) % ROWS.len(),
            KeyCode::Char('n') if ctrl => self.cycle_suggestion(1),
            KeyCode::Char('p') if ctrl => self.cycle_suggestion(-1),
            _ if self.row() == Row::Dest => match key.code {
                KeyCode::Backspace => self.dest.backspace(),
                KeyCode::Delete => self.dest.delete(),
                KeyCode::Left => self.dest.move_left(),
                KeyCode::Right => self.dest.move_right(),
                KeyCode::Home => self.dest.home(),
                KeyCode::End => self.dest.end(),
                KeyCode::Char('u') if ctrl => self.dest.kill_to_start(),
                KeyCode::Char(c) if !ctrl => self.dest.insert(c),
                _ => {}
            },
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                let back = matches!(key.code, KeyCode::Left | KeyCode::Char('h'));
                match self.row() {
                    Row::Live if self.running => self.live = !self.live,
                    Row::Persistent if self.running => self.persistent = !self.persistent,
                    Row::UndefineSource => self.undefine_source = !self.undefine_source,
                    Row::Storage if self.running => {
                        self.storage =
                            (self.storage + if back { STORAGE.len() - 1 } else { 1 }) % STORAGE.len();
                    }
                    Row::AutoConverge if self.running => self.auto_converge = !self.auto_converge,
                    Row::Compressed if self.running => self.compressed = !self.compressed,
                    _ => {}
                }
            }
            KeyCode::Char('j') => self.focus = (self.focus + 1) % ROWS.len(),
            KeyCode::Char('k') => self.focus = (self.focus + ROWS.len() - 1) % ROWS.len(),
            _ => {}
        }
        Outcome::None
    }
}

/// Draw the dialog centered over the screen.
pub fn render(frame: &mut Frame, theme: &Theme, st: &MigrateState) {
    let t = &theme.tokens;
    let area = frame.area();
    let w = 76.min(area.width.saturating_sub(4));
    crate::ui::widgets::modal::dim_background(frame, theme);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme.border)
        .border_style(Style::default().fg(t.blue))
        .style(theme.base())
        .title(Span::styled(
            format!(" ⇄ Migrate {} ", st.domain),
            Style::default().fg(t.blue).add_modifier(Modifier::BOLD),
        ));
    let focus = st.row();
    let mark = |row: Row| if row == focus { "❯ " } else { "  " };
    let label = |row: Row, text: &str| {
        let style = if row == focus {
            theme.text().bg(t.sel)
        } else {
            theme.text()
        };
        Span::styled(text.to_string(), style)
    };
    let enabled = |on: bool| if on { theme.text() } else { theme.dim() };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(mark(Row::Dest), Style::default().fg(t.orange)),
            Span::styled("To   ", theme.secondary()),
            Span::styled(
                format!("{}{}", st.dest.value(), if focus == Row::Dest { "▏" } else { "" }),
                Style::default().fg(t.cyan),
            ),
        ]),
        Line::from(Span::styled(
            if st.suggestions.is_empty() {
                String::from("       add hosts to [connections] uris to pick them with C-n")
            } else {
                format!("       C-n / C-p: {}", st.suggestions.join(" · "))
            },
            theme.dim(),
        )),
        Line::from(""),
    ];
    let check = |row: Row, on: bool, text: &str, active: bool| {
        Line::from(vec![
            Span::styled(mark(row), Style::default().fg(t.orange)),
            Span::styled(format!("{} ", checkbox(on && active)), enabled(active)),
            if active {
                label(row, text)
            } else {
                Span::styled(text.to_string(), theme.dim())
            },
        ])
    };
    lines.push(check(
        Row::Live,
        st.live,
        "live (keep it running while it moves)",
        st.running,
    ));
    lines.push(check(
        Row::Persistent,
        st.persistent || !st.running,
        "persistent (define it on the destination)",
        st.running,
    ));
    lines.push(check(
        Row::UndefineSource,
        st.undefine_source,
        "undefine on this host afterwards",
        true,
    ));
    let mut storage = vec![
        Span::styled(mark(Row::Storage), Style::default().fg(t.orange)),
        Span::styled("disks ", enabled(st.running)),
    ];
    for (i, s) in STORAGE.iter().enumerate() {
        storage.push(Span::styled(
            format!("{} {s}  ", radio(st.storage == i)),
            if st.running && focus == Row::Storage && st.storage == i {
                theme.text().bg(t.sel)
            } else {
                enabled(st.running)
            },
        ));
    }
    lines.push(Line::from(storage));
    lines.push(check(
        Row::AutoConverge,
        st.auto_converge,
        "auto-converge (slow a busy guest so it can finish)",
        st.running,
    ));
    lines.push(check(Row::Compressed, st.compressed, "compressed", st.running));
    lines.push(Line::from(""));
    if !st.running {
        lines.push(Line::from(Span::styled(
            "Shut off: only the definition moves (--offline --persistent); copy the disks yourself.",
            Style::default().fg(t.yellow),
        )));
    }
    match st.argv() {
        Ok(argv) => {
            let step = crate::command::plan::CommandStep {
                program: String::from("virsh"),
                argv,
                stdin: None,
            };
            lines.push(Line::from(
                crate::command::display::spans(theme, &step)
                    .into_iter()
                    .map(|span| Span::styled(span.content.into_owned(), span.style))
                    .collect::<Vec<_>>(),
            ));
        }
        Err(e) => lines.push(Line::from(Span::styled(e, Style::default().fg(t.yellow)))),
    }
    if let Some(e) = &st.error {
        lines.push(Line::from(Span::styled(
            format!("✗ {e}"),
            Style::default().fg(t.red),
        )));
    }
    let lines = crate::ui::widgets::review::wrap_lines(lines, w.saturating_sub(4));
    // Borders, the lines, a blank row and the buttons.
    let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
    let modal = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let body = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height.saturating_sub(1),
    );
    frame.render_widget(Paragraph::new(lines).style(theme.base()), body);
    let footer = Rect::new(body.x, inner.bottom().saturating_sub(1), body.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ⎋ cancel ", theme.secondary().bg(t.hl)),
            Span::raw("  "),
            Span::styled(
                " ⏎ migrate ",
                Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ⇥/↑↓ move · ␣ toggle · ←→ disks", theme.dim()),
        ])),
        footer,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn running_domain_defaults_to_live_persistent() {
        let saved = vec![
            String::from("qemu:///system"),
            String::from("qemu+ssh://hv02/system"),
        ];
        let st = MigrateState::new("web", true, "qemu:///system", &saved);
        assert_eq!(
            st.argv().unwrap(),
            [
                "migrate",
                "--live",
                "--persistent",
                "web",
                "qemu+ssh://hv02/system"
            ]
        );
    }

    #[test]
    fn options_and_validation() {
        let mut st = MigrateState::new("web", true, "qemu:///system", &[]);
        assert!(st.argv().is_err(), "qemu+ssh:// alone is not a destination");
        for c in "hv02/system".chars() {
            st.key(key(KeyCode::Char(c)));
        }
        st.key(key(KeyCode::Tab)); // live
        st.key(key(KeyCode::Char(' '))); // off
        for _ in 0..3 {
            st.key(key(KeyCode::Tab)); // persistent, undefine, storage
        }
        st.key(key(KeyCode::Right)); // copy all
        assert_eq!(
            st.argv().unwrap(),
            [
                "migrate",
                "--persistent",
                "--copy-storage-all",
                "web",
                "qemu+ssh://hv02/system"
            ]
        );
        assert!(matches!(st.key(key(KeyCode::Enter)), Outcome::Submit(_)));
        let mut same = MigrateState::new("web", true, "qemu:///system", &[]);
        same.dest = TextInput::with("qemu:///system");
        assert_eq!(same.key(key(KeyCode::Enter)), Outcome::None);
        assert!(same.error.is_some());
    }

    #[test]
    fn shut_off_domains_move_offline() {
        let mut st = MigrateState::new("db", false, "qemu:///system", &[]);
        st.dest = TextInput::with("qemu+ssh://hv02/system");
        assert_eq!(
            st.argv().unwrap(),
            [
                "migrate",
                "--offline",
                "--persistent",
                "db",
                "qemu+ssh://hv02/system"
            ]
        );
    }

    #[test]
    fn renders_the_command() {
        let theme = crate::theme::builtin::by_name("tokyonight");
        let st = MigrateState::new(
            "web",
            true,
            "qemu:///system",
            &[String::from("qemu+ssh://hv02/system")],
        );
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| render(f, &theme, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let text: String = (0..30)
            .map(|y| {
                (0..100)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    + "\n"
            })
            .collect();
        assert!(text.contains("Migrate web"), "{text}");
        assert!(
            text.contains("virsh migrate --live --persistent web qemu+ssh://hv02/system"),
            "{text}"
        );
    }
}
