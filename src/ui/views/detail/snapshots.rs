//! Snapshots tab: tree, details, create/revert/delete.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::model::Snapshot;
use crate::theme::Theme;

/// One tree row.
#[derive(Debug, Clone)]
pub struct TreeRow {
    pub prefix: String,
    pub name: String,
    pub date: String,
    pub state: String,
    pub kind: String,
    pub current: bool,
}

/// Build tree rows from parent links (roots first, depth-first).
pub fn build_tree(snaps: &[Snapshot], current: Option<&str>) -> Vec<TreeRow> {
    use std::collections::{HashMap, HashSet};
    let names: HashSet<&str> = snaps.iter().map(|s| s.name.as_str()).collect();
    let mut children: HashMap<Option<String>, Vec<&Snapshot>> = HashMap::new();
    for s in snaps {
        // A parent missing from the list (deleted metadata) makes a root,
        // otherwise the snapshot would never be drawn.
        let parent = s.parent.clone().filter(|p| names.contains(p.as_str()));
        children.entry(parent).or_default().push(s);
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| a.name.cmp(&b.name));
    }
    let mut rows = Vec::new();
    fn walk(
        parent: Option<String>,
        prefix: String,
        last_stack: Vec<bool>,
        children: &HashMap<Option<String>, Vec<&Snapshot>>,
        current: Option<&str>,
        rows: &mut Vec<TreeRow>,
    ) {
        let Some(list) = children.get(&parent) else {
            return;
        };
        for (i, s) in list.iter().enumerate() {
            let last = i + 1 == list.len();
            let mut mine = String::new();
            for last_above in &last_stack {
                mine.push_str(if *last_above { "   " } else { "│  " });
            }
            if !prefix.is_empty() || !last_stack.is_empty() {
                mine.push_str(if last { "└─ " } else { "├─ " });
            }
            let _ = prefix;
            rows.push(TreeRow {
                prefix: mine,
                name: s.name.clone(),
                date: s.created.chars().take(10).collect(),
                state: s.state.clone(),
                kind: if s.state == "disk-snapshot" {
                    String::from("disk")
                } else if matches!(s.state.as_str(), "running" | "paused") {
                    String::from("mem+disk")
                } else {
                    String::from("disk")
                },
                current: current == Some(s.name.as_str()),
            });
            let mut stack = last_stack.clone();
            stack.push(last);
            walk(
                Some(s.name.clone()),
                String::new(),
                stack,
                children,
                current,
                rows,
            );
        }
    }
    walk(None, String::new(), Vec::new(), &children, current, &mut rows);
    rows
}

/// After-revert state radio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AfterRevert {
    #[default]
    Running,
    Paused,
    AsSaved,
}

impl AfterRevert {
    pub fn flag(self) -> Option<&'static str> {
        match self {
            Self::Running => Some("--running"),
            Self::Paused => Some("--paused"),
            Self::AsSaved => None,
        }
    }
}

/// Snapshot tab state.
#[derive(Debug, Clone)]
pub struct SnapshotState {
    pub rows: Vec<TreeRow>,
    pub details: Vec<(String, String)>,
    pub description_title: String,
    pub description: String,
    pub current: Option<String>,
    pub selected: usize,
    /// Revert modal options.
    pub after: AfterRevert,
    pub force: bool,
    pub safety: bool,
    /// Delete modal options.
    pub del_children: bool,
    pub del_children_only: bool,
    pub del_metadata: bool,
    /// Create form values.
    pub create_name: String,
    pub create_desc: String,
    pub create_focus: u8,
    pub create_atomic: bool,
    pub create_quiesce: bool,
    /// Frozen clock for demo/snapshots (mockup shows 1438).
    pub frozen: bool,
    /// Live details per snapshot (from `snapshot-dumpxml`); the selected
    /// one fills `details` and `description`.
    pub review_scroll: std::cell::Cell<usize>,
    pub infos: std::collections::HashMap<String, SnapInfo>,
}

/// Details of one snapshot for the side panel.
#[derive(Debug, Clone, Default)]
pub struct SnapInfo {
    pub details: Vec<(String, String)>,
    pub description: String,
}

/// Details from `virsh snapshot-dumpxml` (+ the tree for parent/children).
pub fn snap_info(xml: &str, rows: &[Snapshot]) -> Option<SnapInfo> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let root = doc.root_element();
    let text = |tag: &str| {
        root.children()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text())
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    };
    let name = text("name");
    let created = text("creationTime")
        .parse::<i64>()
        .ok()
        .and_then(|t| jiff::Timestamp::from_second(t).ok())
        .map(|t| t.strftime("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    let state = text("state");
    let memory = root
        .children()
        .find(|n| n.has_tag_name("memory"))
        .and_then(|m| m.attribute("snapshot"))
        .unwrap_or("no");
    let disks: Vec<String> = root
        .children()
        .find(|n| n.has_tag_name("disks"))
        .map(|d| {
            d.children()
                .filter(|n| n.has_tag_name("disk"))
                .map(|n| {
                    format!(
                        "{} {}",
                        n.attribute("name").unwrap_or("?"),
                        match n.attribute("snapshot").unwrap_or("internal") {
                            "no" => "excluded",
                            other => other,
                        }
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let parent = rows
        .iter()
        .find(|r| r.name == name)
        .and_then(|r| r.parent.clone())
        .unwrap_or_else(|| String::from("—"));
    let children: Vec<&str> = rows
        .iter()
        .filter(|r| r.parent.as_deref() == Some(name.as_str()))
        .map(|r| r.name.as_str())
        .collect();
    let mut details = vec![
        (String::from("created   "), created),
        (
            String::from("state     "),
            if memory == "no" {
                state
            } else {
                format!("{state} (memory included)")
            },
        ),
        (String::from("parent    "), parent),
        (
            String::from("children  "),
            if children.is_empty() {
                String::from("0")
            } else {
                format!("{} · {}", children.len(), children.join(", "))
            },
        ),
    ];
    if !disks.is_empty() {
        details.push((String::from("disks     "), disks.join(" · ")));
    }
    Some(SnapInfo {
        details,
        description: text("description"),
    })
}

impl SnapshotState {
    /// Demo 7-snapshot tree from the original Snapshots mockup.
    pub fn demo() -> Self {
        let raw = [
            ("", "base-install", "2026-07-02", "shutoff", "disk", false, false),
            (
                "├─ ",
                "post-config",
                "2026-07-03",
                "shutoff",
                "disk",
                false,
                false,
            ),
            (
                "│  ├─ ",
                "pre-kde-6",
                "2026-08-14",
                "running",
                "mem+disk",
                false,
                false,
            ),
            (
                "│  │  └─ ",
                "pre-kernel-6.16",
                "2026-09-01",
                "running",
                "mem+disk",
                false,
                true,
            ),
            (
                "│  │     └─ ",
                "pre-kernel-6.17",
                "2026-10-06",
                "running",
                "mem+disk",
                true,
                false,
            ),
            (
                "│  └─ ",
                "experiment-zfs",
                "2026-08-20",
                "shutoff",
                "disk",
                false,
                false,
            ),
            (
                "└─ ",
                "minimal-clean",
                "2026-07-05",
                "shutoff",
                "disk",
                false,
                false,
            ),
        ];
        let rows = raw
            .iter()
            .map(|(prefix, name, date, state, kind, current, _)| TreeRow {
                prefix: prefix.to_string(),
                name: name.to_string(),
                date: date.to_string(),
                state: state.to_string(),
                kind: kind.to_string(),
                current: *current,
            })
            .collect();
        Self {
            rows,
            details: vec![
                (String::from("created   "), String::from("2026-09-01 14:02:17")),
                (
                    String::from("state     "),
                    String::from("running (memory included)"),
                ),
                (String::from("parent    "), String::from("pre-kde-6")),
                (String::from("children  "), String::from("1 · pre-kernel-6.17")),
                (
                    String::from("disks     "),
                    String::from("vda internal · vdb excluded"),
                ),
                (String::from("size      "), String::from("~4.2 GiB memory state")),
            ],
            description_title: String::from("description"),
            description: String::from("before pacman -Syu with linux 6.16; nvidia-open 570 working."),
            current: Some(String::from("pre-kernel-6.17")),
            selected: 3,
            after: AfterRevert::Running,
            force: false,
            safety: true,
            del_children: false,
            del_children_only: false,
            del_metadata: false,
            create_name: String::from("snap-2026-10-06-1438"),
            create_desc: String::new(),
            create_focus: 0,
            create_atomic: true,
            create_quiesce: false,
            frozen: true,
            review_scroll: std::cell::Cell::new(0),
            infos: Default::default(),
        }
    }

    /// Show the live details of the selected snapshot (no-op without them).
    pub fn sync_details(&mut self) {
        if let Some(info) = self.infos.get(self.selected_name()).cloned() {
            self.details = info.details;
            self.description_title = String::from("description");
            self.description = if info.description.is_empty() {
                String::from("—")
            } else {
                info.description
            };
        }
    }

    /// Selected snapshot name.
    pub fn selected_name(&self) -> &str {
        self.rows
            .get(self.selected)
            .map(|r| r.name.as_str())
            .unwrap_or("")
    }

    /// Move selection.
    pub fn move_selection(&mut self, delta: i32) {
        let n = self.rows.len() as i32;
        if n == 0 {
            return;
        }
        self.selected = (self.selected as i32 + delta).clamp(0, n - 1) as usize;
        self.sync_details();
    }
}

fn panel_block(theme: &Theme, focused: bool, title: &str, right: Option<String>) -> Block<'static> {
    crate::ui::widgets::panel::block(
        theme,
        focused,
        title,
        right.map(|r| crate::ui::widgets::panel::right(r, theme)),
        None,
        None,
    )
}

/// Render tree + details columns.
pub fn render(frame: &mut Frame, theme: &Theme, st: &SnapshotState, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(92), Constraint::Min(0)])
        .spacing(1)
        .split(area);
    render_tree(frame, theme, st, cols[0]);
    render_details(frame, theme, st, cols[1]);
}

fn render_tree(frame: &mut Frame, theme: &Theme, st: &SnapshotState, area: Rect) {
    let t = &theme.tokens;
    let lines: Vec<Line> = st
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let selected = i == st.selected;
            let bg = if selected { t.sel } else { t.bg };
            let glyph = if r.current { "●" } else { "◆" };
            let glyph_color = if r.current { t.green } else { t.magenta };
            let state_color = if r.state == "running" { t.green } else { t.comment };
            let mut spans = vec![
                Span::styled(r.prefix.clone(), Style::default().fg(t.comment).bg(bg)),
                Span::styled(format!("{glyph} "), Style::default().fg(glyph_color).bg(bg)),
                Span::styled(
                    r.name.clone(),
                    Style::default().fg(t.fg).bg(bg).add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
            ];
            if r.current {
                spans.push(Span::styled("  ← current", Style::default().fg(t.green).bg(bg)));
            }
            // Fixed columns (mockup grid): name (rest) | date 12 | state 9 | kind 10.
            let used: usize = spans.iter().map(|s| s.width()).sum();
            let name_w = (area.width as usize).saturating_sub(4 + 12 + 9 + 10 + 3);
            spans.push(Span::raw(" ".repeat(name_w.saturating_sub(used).max(1))));
            spans.push(Span::styled(
                format!("{:<12} ", r.date),
                Style::default().fg(t.fg2),
            ));
            spans.push(Span::styled(
                format!("{:<9} ", r.state),
                Style::default().fg(state_color),
            ));
            spans.push(Span::styled(
                format!("{:<10}", r.kind),
                Style::default().fg(t.comment),
            ));
            Line::from(spans).style(Style::default().bg(bg))
        })
        .collect();
    let right = format!("{} snapshots · internal", st.rows.len());
    let block = crate::ui::widgets::panel::block(
        theme,
        true,
        "Snapshot tree",
        Some(crate::ui::widgets::panel::right(right, theme)),
        None,
        Some("␣sc new · ␣sr revert · ␣sd delete · e edit"),
    );
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_details(frame: &mut Frame, theme: &Theme, st: &SnapshotState, area: Rect) {
    let title = st.selected_name().to_string();
    let mut lines: Vec<Line> = st
        .details
        .iter()
        .map(|(k, v)| Line::from(vec![Span::styled(k.clone(), theme.dim()), Span::raw(v.clone())]))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        st.description_title.clone(),
        theme.dim(),
    )));
    lines.push(Line::from(Span::styled(
        st.description.clone(),
        theme.secondary(),
    )));
    let block = panel_block(theme, false, &title, None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

/// Render the revert CONFIRM modal (red border, CONFIRM mode).
pub fn render_revert(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    domain: &str,
    snap: &str,
    st: &SnapshotState,
) {
    if area.width < 100 || area.height < 22 {
        render_compact_modal(frame, theme, area, domain, super::SnapModal::Revert, st);
        return;
    }
    let t = &theme.tokens;
    // Mockup: 680×400 px ≈ 88×20 cells, centered.
    let w = 88u16.min(area.width.saturating_sub(4));
    let h = 20u16.min(area.height.saturating_sub(2));
    let modal = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    let block = crate::ui::widgets::panel::block(
        theme,
        false,
        "⚠ Revert snapshot",
        Some(crate::ui::widgets::panel::right("snapshot-revert", theme)),
        None,
        None,
    )
    .border_style(Style::default().fg(t.red))
    .title_style(Style::default().fg(t.red).add_modifier(Modifier::BOLD))
    .padding(ratatui::widgets::Padding::new(2, 2, 1, 0))
    .style(Style::default().bg(t.bg));
    let radio = |on: bool| {
        if on {
            Span::styled("(●)", Style::default().fg(t.blue))
        } else {
            Span::styled("( )", theme.dim())
        }
    };
    let check = |on: bool| {
        if on {
            Span::styled("[x]", Style::default().fg(t.green))
        } else {
            Span::styled("[ ]", theme.dim())
        }
    };
    let after = match st.after {
        AfterRevert::Running => Some("--running"),
        AfterRevert::Paused => Some("--paused"),
        AfterRevert::AsSaved => None,
    };
    // Show exactly what will run (same builder as the executor).
    let mut plan = crate::command::builders::snapshot::revert(domain, snap, after, st.force, st.safety);
    if st.frozen {
        for step in &mut plan.steps {
            for a in &mut step.argv {
                if a.starts_with("auto-before-revert-") {
                    *a = String::from("auto-before-revert-1438");
                }
            }
        }
    }
    let bold = Style::default().fg(t.fg).add_modifier(Modifier::BOLD);
    let consequence = match &st.current {
        Some(cur) if cur != snap => vec![
            Span::styled(
                "The current running state is discarded. Work done since the last snapshot ",
                theme.secondary(),
            ),
            Span::styled(cur.clone(), Style::default().fg(t.magenta)),
            Span::styled(" is lost.", theme.secondary()),
        ],
        _ => vec![Span::styled(
            "The current running state is discarded and replaced by the snapshot.",
            theme.secondary(),
        )],
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Revert ", bold),
            Span::styled(domain.to_string(), bold.fg(t.blue)),
            Span::styled(" to ", bold),
            Span::styled(snap.to_string(), bold.fg(t.magenta)),
            Span::styled("?", bold),
        ]),
        Line::from(""),
        Line::from(consequence),
        Line::from(""),
        Line::from(vec![
            Span::styled("after revert   ", theme.dim()),
            radio(st.after == AfterRevert::Running),
            Span::styled(" running   ", theme.text()),
            radio(st.after == AfterRevert::Paused),
            Span::styled(" paused   ", theme.secondary()),
            radio(st.after == AfterRevert::AsSaved),
            Span::styled(" as saved", theme.secondary()),
        ]),
        Line::from(vec![
            Span::styled("options        ", theme.dim()),
            check(st.force),
            Span::styled(" --force", theme.secondary()),
            Span::styled(" (risky; ABI changes)", theme.dim()),
        ]),
        Line::from(vec![
            Span::styled("               ", theme.dim()),
            check(st.safety),
            Span::styled(" snapshot current state first ", theme.text()),
            Span::styled("(auto-safety)", theme.dim()),
        ]),
        Line::from(""),
    ];
    for step in &plan.steps {
        lines.push(
            Line::from(
                crate::command::display::spans(theme, step)
                    .into_iter()
                    .map(|s| Span::styled(s.content.into_owned(), s.style.bg(t.bg2)))
                    .collect::<Vec<_>>(),
            )
            .style(Style::default().bg(t.bg2)),
        );
    }
    let inner_w = w.saturating_sub(6) as usize;
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(inner_w.saturating_sub(23))),
        Span::styled(" n cancel ", Style::default().fg(t.fg2).bg(t.hl)),
        Span::raw("  "),
        Span::styled(
            " y revert ",
            Style::default().fg(t.bg2).bg(t.red).add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(theme.base()),
        modal,
    );
}

/// Render the delete CONFIRM modal.
pub fn render_delete(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    domain: &str,
    snap: &str,
    st: &SnapshotState,
) {
    if area.width < 100 || area.height < 18 {
        render_compact_modal(frame, theme, area, domain, super::SnapModal::Delete, st);
        return;
    }
    let t = &theme.tokens;
    let w = 72u16.min(area.width.saturating_sub(4));
    let h = 14u16.min(area.height.saturating_sub(4));
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
        .border_style(Style::default().fg(t.red).bg(t.bg))
        .title(Span::styled(
            " ⚠ Delete snapshot ",
            Style::default().fg(t.red).bg(t.bg).add_modifier(Modifier::BOLD),
        ))
        .title_top(Line::from(Span::styled("snapshot-delete", theme.dim())).right_aligned());
    let check = |on: bool| {
        if on {
            Span::styled("[x]", Style::default().fg(t.green).bg(t.bg))
        } else {
            Span::styled("[ ]", theme.dim())
        }
    };
    let flag = if st.del_children_only {
        " --children-only"
    } else if st.del_children {
        " --children"
    } else {
        ""
    };
    let meta = if st.del_metadata { " --metadata" } else { "" };
    let lines = vec![
        Line::from(vec![
            Span::styled("Delete ", theme.text()),
            Span::styled(snap.to_string(), Style::default().fg(t.magenta).bg(t.bg)),
            Span::styled(format!(" on {domain}?"), theme.text()),
        ]),
        Line::from(""),
        Line::from(vec![
            check(st.del_children),
            Span::styled(" --children", theme.secondary()),
        ]),
        Line::from(vec![
            check(st.del_children_only),
            Span::styled(" --children-only", theme.secondary()),
        ]),
        Line::from(vec![
            check(st.del_metadata),
            Span::styled(" --metadata", theme.secondary()),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("$ virsh snapshot-delete {domain} {snap}{flag}{meta}"),
            Style::default().fg(t.yellow).bg(t.bg2),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" n cancel ", Style::default().fg(t.fg2).bg(t.hl)),
            Span::raw("  "),
            Span::styled(
                " y delete ",
                Style::default().fg(t.bg2).bg(t.red).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), modal);
}

/// Render the create form modal.
pub fn render_create(frame: &mut Frame, theme: &Theme, area: Rect, st: &SnapshotState) {
    let t = &theme.tokens;
    let w = 72u16.min(area.width.saturating_sub(4));
    let h = 14u16.min(area.height.saturating_sub(4));
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
        .border_style(Style::default().fg(t.blue).bg(t.bg))
        .title(Span::styled(
            " New snapshot ",
            Style::default().fg(t.blue).bg(t.bg).add_modifier(Modifier::BOLD),
        ))
        .title_top(Line::from(Span::styled("snapshot-create-as", theme.dim())).right_aligned());
    let lines = vec![
        Line::from(vec![
            Span::styled("name          ", theme.secondary()),
            Span::styled(
                format!(" {} ", st.create_name),
                Style::default().fg(t.fg).bg(t.hl),
            ),
        ]),
        Line::from(vec![
            Span::styled("description   ", theme.secondary()),
            Span::styled(
                format!(" {} ", st.create_desc),
                Style::default().fg(t.fg).bg(t.hl),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                if st.create_atomic { "[x]" } else { "[ ]" },
                Style::default().fg(t.green).bg(t.bg),
            ),
            Span::styled(" --atomic", theme.secondary()),
        ]),
        Line::from(vec![
            Span::styled(
                if st.create_quiesce { "[x]" } else { "[ ]" },
                Style::default().fg(t.green).bg(t.bg),
            ),
            Span::styled(" --quiesce  (Tab to select, Space to toggle)", theme.secondary()),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(
                "$ virsh snapshot-create-as <domain> --name {}{}{}",
                st.create_name,
                if st.create_atomic { " --atomic" } else { "" },
                if st.create_quiesce { " --quiesce" } else { "" }
            ),
            Style::default().fg(t.yellow).bg(t.bg2),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " ⏎ create ",
                Style::default()
                    .fg(t.bg2)
                    .bg(t.green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(" ⎋ cancel ", Style::default().fg(t.fg2).bg(t.hl)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), modal);
}

/// Compact dialogs are independent of the detail layout; the preview scrolls
/// while the affirmative/cancel actions remain visible.
pub fn render_compact_modal(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    domain: &str,
    modal: super::SnapModal,
    st: &SnapshotState,
) {
    use super::SnapModal;
    use crate::ui::overlays::confirm::{Confirm, ConfirmKind, render_scrolled};
    let snap = st.selected_name();
    let (title, question, label, mut plan) = match modal {
        SnapModal::Revert => (
            "Revert snapshot",
            format!(
                "Revert {domain} to {snap}?\nCurrent running state will be discarded.\n1/2/3 state: {:?}\nf force: {} · s safety snapshot: {}",
                st.after, st.force, st.safety
            ),
            "y revert",
            crate::command::builders::snapshot::revert(domain, snap, st.after.flag(), st.force, st.safety),
        ),
        SnapModal::Delete => (
            "Delete snapshot",
            format!(
                "Delete {snap} on {domain}?\nc children: {} · o children-only: {}\nm metadata only: {}",
                st.del_children, st.del_children_only, st.del_metadata
            ),
            "y delete",
            crate::command::builders::snapshot::delete(
                domain,
                snap,
                st.del_children,
                st.del_children_only,
                st.del_metadata,
            ),
        ),
        SnapModal::Create => {
            render_compact_create(frame, theme, area, domain, st);
            return;
        }
        SnapModal::None => return,
    };
    if st.frozen {
        for step in &mut plan.steps {
            for a in &mut step.argv {
                if a.starts_with("auto-before-revert-") {
                    *a = "auto-before-revert-1438".into();
                }
            }
        }
    }
    let confirm = Confirm {
        kind: ConfirmKind::Always,
        title: title.into(),
        question,
        commands: plan.steps.iter().map(crate::command::display::display).collect(),
        confirm_label: label.into(),
        type_name: None,
        typed: String::new(),
    };
    let max = render_scrolled(frame, theme, area, &confirm, st.review_scroll.get());
    st.review_scroll.set(st.review_scroll.get().min(max));
}

fn render_compact_create(frame: &mut Frame, theme: &Theme, area: Rect, domain: &str, st: &SnapshotState) {
    let t = &theme.tokens;
    let block = Block::bordered()
        .border_type(theme.border)
        .border_style(theme.accent())
        .title(" New snapshot ")
        .style(theme.base());
    frame.render_widget(Clear, area);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let fields = [
        ("name", st.create_name.clone()),
        ("description", st.create_desc.clone()),
        ("atomic", st.create_atomic.to_string()),
        ("quiesce", st.create_quiesce.to_string()),
    ];
    let mut lines = vec![Line::from(Span::styled(domain.to_owned(), theme.accent()))];
    for (index, (label, value)) in fields.into_iter().enumerate() {
        let selected = index == st.create_focus as usize;
        let value = if selected {
            let max = inner.width.saturating_sub(label.len() as u16 + 4) as usize;
            value
                .chars()
                .rev()
                .take(max)
                .collect::<String>()
                .chars()
                .rev()
                .collect()
        } else {
            value
        };
        lines.push(
            Line::from(format!("{} {label}: {value}", if selected { "❯" } else { " " })).style(if selected {
                theme.text().bg(t.sel)
            } else {
                theme.secondary()
            }),
        );
    }
    lines.push(Line::from("Tab field · Space toggle"));
    let body = Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(1));
    frame.render_widget(Paragraph::new(lines), body);
    frame.render_widget(
        Paragraph::new("⏎ create · Esc cancel").style(theme.secondary()),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
}
