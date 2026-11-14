//! New storage pool dialog (`N` in Storage): type, source, target and the
//! `virsh pool-define-as` / `pool-build` / `pool-start` / `pool-autostart`
//! commands it will run.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::command::plan::{CommandPlan, CommandStep};
use crate::input::textinput::TextInput;
use crate::theme::Theme;
use crate::ui::widgets::form::{checkbox, radio};

/// Pool types offered, with what they are.
pub const TYPES: [(&str, &str); 4] = [
    ("dir", "a folder on this host"),
    ("netfs", "an NFS export, mounted by libvirt"),
    ("logical", "an existing LVM volume group"),
    ("fs", "a block device with a file system, mounted by libvirt"),
];

/// Text fields; which ones show depends on the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Text {
    Name,
    Host,
    SourcePath,
    VolumeGroup,
    Device,
    Target,
}

/// One focusable row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Text(Text),
    Type,
    Build,
    Start,
    Autostart,
}

/// Dialog state.
#[derive(Debug, Clone)]
pub struct PoolState {
    pub kind: usize,
    pub name: TextInput,
    pub host: TextInput,
    pub source_path: TextInput,
    pub vg: TextInput,
    pub device: TextInput,
    pub target: TextInput,
    /// The target was typed (else it follows the name / VG).
    pub target_edited: bool,
    pub build: bool,
    pub start: bool,
    pub autostart: bool,
    pub focus: usize,
    /// Pool names that exist already.
    pub existing: Vec<String>,
    pub error: Option<String>,
}

/// What a key did.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    None,
    Close,
    Submit(String, CommandPlan),
}

impl PoolState {
    pub fn new(existing: Vec<String>) -> Self {
        Self {
            kind: 0,
            name: TextInput::with(""),
            host: TextInput::with(""),
            source_path: TextInput::with(""),
            vg: TextInput::with(""),
            device: TextInput::with(""),
            target: TextInput::with(""),
            target_edited: false,
            build: true,
            start: true,
            autostart: true,
            focus: 0,
            existing,
            error: None,
        }
    }

    pub fn kind(&self) -> &'static str {
        TYPES[self.kind].0
    }

    /// Rows for the current type, in focus order.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Text(Text::Name), Row::Type];
        match self.kind() {
            "netfs" => rows.extend([Row::Text(Text::Host), Row::Text(Text::SourcePath)]),
            "logical" => rows.push(Row::Text(Text::VolumeGroup)),
            "fs" => rows.push(Row::Text(Text::Device)),
            _ => {}
        }
        rows.push(Row::Text(Text::Target));
        if self.can_build() {
            rows.push(Row::Build);
        }
        rows.extend([Row::Start, Row::Autostart]);
        rows
    }

    /// `pool-build` only creates a folder for these; for `fs` it may format
    /// the device and `logical` needs an existing volume group.
    fn can_build(&self) -> bool {
        matches!(self.kind(), "dir" | "netfs")
    }

    fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.focus.min(rows.len() - 1)]
    }

    fn input(&self, t: Text) -> &TextInput {
        match t {
            Text::Name => &self.name,
            Text::Host => &self.host,
            Text::SourcePath => &self.source_path,
            Text::VolumeGroup => &self.vg,
            Text::Device => &self.device,
            Text::Target => &self.target,
        }
    }

    fn input_mut(&mut self, t: Text) -> &mut TextInput {
        match t {
            Text::Name => &mut self.name,
            Text::Host => &mut self.host,
            Text::SourcePath => &mut self.source_path,
            Text::VolumeGroup => &mut self.vg,
            Text::Device => &mut self.device,
            Text::Target => &mut self.target,
        }
    }

    /// The target path used: typed, or derived from the name / VG.
    pub fn target(&self) -> String {
        if self.target_edited {
            return self.target.value().trim().to_string();
        }
        let name = self.name.value().trim();
        match self.kind() {
            "logical" if !self.vg.value().trim().is_empty() => format!("/dev/{}", self.vg.value().trim()),
            "netfs" | "fs" if !name.is_empty() => format!("/mnt/libvirt/{name}"),
            "dir" if !name.is_empty() => format!("/var/lib/libvirt/images/{name}"),
            _ => String::new(),
        }
    }

    /// The commands, or why they cannot be built.
    pub fn plan(&self) -> Result<(String, CommandPlan), String> {
        let name = self.name.value().trim().to_string();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        {
            return Err(String::from("name: letters, digits, - _ ."));
        }
        if self.existing.contains(&name) {
            return Err(format!("a pool named {name} exists"));
        }
        let target = self.target();
        if !target.starts_with('/') {
            return Err(String::from("target must be an absolute path"));
        }
        let mut define: Vec<String> = vec![
            String::from("pool-define-as"),
            name.clone(),
            self.kind().to_string(),
        ];
        let need = |v: &TextInput, what: &str| {
            let v = v.value().trim().to_string();
            if v.is_empty() {
                Err(format!("{what} is required"))
            } else {
                Ok(v)
            }
        };
        match self.kind() {
            "netfs" => {
                define.extend([String::from("--source-host"), need(&self.host, "NFS server")?]);
                let path = need(&self.source_path, "export path")?;
                if !path.starts_with('/') {
                    return Err(String::from("export path must start with /"));
                }
                define.extend([String::from("--source-path"), path]);
                define.extend([String::from("--source-format"), String::from("nfs")]);
            }
            "logical" => define.extend([String::from("--source-name"), need(&self.vg, "volume group")?]),
            "fs" => {
                let dev = need(&self.device, "device")?;
                if !dev.starts_with("/dev/") {
                    return Err(String::from("device must be under /dev/"));
                }
                define.extend([String::from("--source-dev"), dev]);
            }
            _ => {}
        }
        define.extend([String::from("--target"), target]);
        let step = |argv: Vec<String>| CommandStep {
            program: String::from("virsh"),
            argv,
            stdin: None,
        };
        let mut steps = vec![step(define)];
        if self.can_build() && self.build {
            steps.push(step(vec![String::from("pool-build"), name.clone()]));
        }
        if self.start {
            steps.push(step(vec![String::from("pool-start"), name.clone()]));
        }
        if self.autostart {
            steps.push(step(vec![String::from("pool-autostart"), name.clone()]));
        }
        Ok((
            name.clone(),
            CommandPlan {
                summary: format!("Created pool {name}"),
                steps,
            },
        ))
    }

    /// Handle a key.
    pub fn key(&mut self, key: crossterm::event::KeyEvent) -> Outcome {
        use crossterm::event::{KeyCode, KeyModifiers};
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let n = self.rows().len();
        self.error = None;
        match key.code {
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Enter => {
                return match self.plan() {
                    Ok((name, plan)) => Outcome::Submit(name, plan),
                    Err(e) => {
                        self.error = Some(e);
                        Outcome::None
                    }
                };
            }
            KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % n,
            KeyCode::BackTab | KeyCode::Up => self.focus = (self.focus + n - 1) % n,
            _ => match self.row() {
                Row::Text(t) => {
                    if t == Text::Target && !self.target_edited {
                        // Start editing from the suggested path.
                        self.target = TextInput::with(&self.target());
                        self.target_edited = true;
                    }
                    let input = self.input_mut(t);
                    match key.code {
                        KeyCode::Backspace => input.backspace(),
                        KeyCode::Delete => input.delete(),
                        KeyCode::Left => input.move_left(),
                        KeyCode::Right => input.move_right(),
                        KeyCode::Home => input.home(),
                        KeyCode::End => input.end(),
                        KeyCode::Char('u') if ctrl => input.kill_to_start(),
                        KeyCode::Char(c) if !ctrl => input.insert(c),
                        _ => {}
                    }
                }
                Row::Type => match key.code {
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.kind = (self.kind + TYPES.len() - 1) % TYPES.len()
                    }
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => {
                        self.kind = (self.kind + 1) % TYPES.len()
                    }
                    _ => {}
                },
                row => {
                    if matches!(key.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                        match row {
                            Row::Build => self.build = !self.build,
                            Row::Start => self.start = !self.start,
                            Row::Autostart => self.autostart = !self.autostart,
                            _ => {}
                        }
                    }
                }
            },
        }
        Outcome::None
    }
}

fn label(t: Text) -> &'static str {
    match t {
        Text::Name => "Name     ",
        Text::Host => "Server   ",
        Text::SourcePath => "Export   ",
        Text::VolumeGroup => "VG       ",
        Text::Device => "Device   ",
        Text::Target => "Target   ",
    }
}

/// Draw the dialog centered over the screen.
pub fn render(frame: &mut Frame, theme: &Theme, st: &PoolState) {
    let t = &theme.tokens;
    let area = frame.area();
    let w = 78.min(area.width.saturating_sub(4));
    crate::ui::widgets::modal::dim_background(frame, theme);
    let focus = st.row();
    let mark = |row: Row| {
        Span::styled(
            if row == focus { "❯ " } else { "  " },
            Style::default().fg(t.orange),
        )
    };
    let mut lines: Vec<Line> = Vec::new();
    for row in st.rows() {
        let line = match row {
            Row::Text(field) => {
                let (value, placeholder) = if field == Text::Target && !st.target_edited {
                    (st.target(), true)
                } else {
                    (st.input(field).value().to_string(), false)
                };
                let cursor = if row == focus { "▏" } else { "" };
                Line::from(vec![
                    mark(row),
                    Span::styled(label(field), theme.secondary()),
                    Span::styled(
                        format!("{value}{cursor}"),
                        if placeholder {
                            theme.dim()
                        } else {
                            Style::default().fg(t.cyan)
                        },
                    ),
                ])
            }
            Row::Type => {
                let mut spans = vec![mark(row), Span::styled("Type     ", theme.secondary())];
                for (i, (kind, _)) in TYPES.iter().enumerate() {
                    let on = i == st.kind;
                    spans.push(Span::styled(
                        format!("{} {kind}  ", radio(on)),
                        if on && row == focus {
                            theme.text().bg(t.sel)
                        } else {
                            theme.text()
                        },
                    ));
                }
                lines.push(Line::from(spans));
                Line::from(Span::styled(
                    format!("           {}", TYPES[st.kind].1),
                    theme.dim(),
                ))
            }
            Row::Build | Row::Start | Row::Autostart => {
                let (on, text) = match row {
                    Row::Build => (st.build, "build (create the target folder)"),
                    Row::Start => (st.start, "start now"),
                    _ => (st.autostart, "start with the host (autostart)"),
                };
                Line::from(vec![
                    mark(row),
                    Span::styled(format!("{} ", checkbox(on)), theme.text()),
                    Span::styled(
                        text,
                        if row == focus {
                            theme.text().bg(t.sel)
                        } else {
                            theme.text()
                        },
                    ),
                ])
            }
        };
        lines.push(line);
    }
    lines.push(Line::from(""));
    match st.plan() {
        Ok((_, plan)) => {
            for step in &plan.steps {
                lines.push(Line::from(
                    crate::command::display::spans(theme, step)
                        .into_iter()
                        .map(|span| Span::styled(span.content.into_owned(), span.style))
                        .collect::<Vec<_>>(),
                ));
            }
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
    let h = (lines.len() as u16 + 4).min(area.height.saturating_sub(2));
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
        .border_style(Style::default().fg(t.blue))
        .style(theme.base())
        .title(Span::styled(
            " ◫ New storage pool ",
            Style::default().fg(t.blue).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let body = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height.saturating_sub(1),
    );
    frame.render_widget(Paragraph::new(lines).style(theme.base()), body);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ⎋ cancel ", theme.secondary().bg(t.hl)),
            Span::raw("  "),
            Span::styled(
                " ⏎ create ",
                Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ⇥/↑↓ move · ←→ type · ␣ toggle", theme.dim()),
        ])),
        Rect::new(body.x, inner.bottom().saturating_sub(1), body.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(st: &mut PoolState, text: &str) {
        for c in text.chars() {
            st.key(key(KeyCode::Char(c)));
        }
    }

    fn argv(plan: &CommandPlan) -> Vec<String> {
        plan.steps.iter().map(|s| s.argv.join(" ")).collect()
    }

    #[test]
    fn dir_pool_with_the_default_target() {
        let mut st = PoolState::new(vec![]);
        typed(&mut st, "fast");
        let (name, plan) = st.plan().unwrap();
        assert_eq!(name, "fast");
        assert_eq!(
            argv(&plan),
            [
                "pool-define-as fast dir --target /var/lib/libvirt/images/fast",
                "pool-build fast",
                "pool-start fast",
                "pool-autostart fast",
            ]
        );
    }

    #[test]
    fn netfs_needs_server_and_export() {
        let mut st = PoolState::new(vec![]);
        typed(&mut st, "nas");
        st.key(key(KeyCode::Tab)); // type
        st.key(key(KeyCode::Right)); // netfs
        assert!(st.plan().unwrap_err().contains("NFS server"));
        st.key(key(KeyCode::Tab));
        typed(&mut st, "nas.lan");
        st.key(key(KeyCode::Tab));
        typed(&mut st, "/export/vm");
        let (_, plan) = st.plan().unwrap();
        assert_eq!(
            argv(&plan)[0],
            "pool-define-as nas netfs --source-host nas.lan --source-path /export/vm --source-format nfs --target /mnt/libvirt/nas"
        );
    }

    #[test]
    fn logical_and_fs_never_build() {
        let mut st = PoolState::new(vec![String::from("taken")]);
        typed(&mut st, "taken");
        assert!(st.plan().unwrap_err().contains("exists"));
        st.name = TextInput::with("lvm");
        st.kind = 2;
        st.vg = TextInput::with("vg0");
        let (_, plan) = st.plan().unwrap();
        assert_eq!(
            argv(&plan)[0],
            "pool-define-as lvm logical --source-name vg0 --target /dev/vg0"
        );
        assert!(!argv(&plan).iter().any(|a| a.starts_with("pool-build")));
        st.kind = 3;
        st.device = TextInput::with("sdb1");
        assert!(st.plan().unwrap_err().contains("/dev/"));
        st.device = TextInput::with("/dev/sdb1");
        assert!(
            !argv(&st.plan().unwrap().1)
                .iter()
                .any(|a| a.starts_with("pool-build"))
        );
    }

    #[test]
    fn typing_the_target_starts_from_the_suggestion() {
        let mut st = PoolState::new(vec![]);
        typed(&mut st, "p");
        st.focus = st
            .rows()
            .iter()
            .position(|r| *r == Row::Text(Text::Target))
            .unwrap();
        typed(&mut st, "2");
        assert_eq!(st.target(), "/var/lib/libvirt/images/p2");
    }
}
