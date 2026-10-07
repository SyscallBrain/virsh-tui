//! Command palette (PLAN.md section 9.7, P12): fuzzy actions + virsh reference.

use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::theme::Theme;

/// Fuzzy-rank items by query (best first). Empty query keeps order.
pub fn fuzzy_rank(query: &str, items: &[&str]) -> Vec<String> {
    if query.is_empty() {
        return items.iter().map(|s| s.to_string()).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut scored: Vec<(u16, &str)> = Vec::new();
    for item in items {
        let mut haystack_buf = Vec::new();
        let mut needle_buf = Vec::new();
        let haystack = Utf32Str::new(item, &mut haystack_buf);
        let needle = Utf32Str::new(query, &mut needle_buf);
        if let Some(score) = matcher.fuzzy_match(haystack, needle) {
            scored.push((score, item));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    scored.into_iter().map(|(_, s)| s.to_string()).collect()
}

/// Palette prefix source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prefix {
    #[default]
    Actions,
    Ex,
    Domains,
    Docs,
}

impl Prefix {
    pub fn of(query: &str) -> Self {
        match query.chars().next() {
            Some(':') => Self::Ex,
            Some('/') => Self::Domains,
            Some('#') => Self::Docs,
            _ => Self::Actions,
        }
    }
}

/// One palette result.
#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub icon: &'static str,
    pub category: String,
    pub title: String,
    pub key: String,
    pub virsh_cmd: String,
}

/// Curated app actions.
pub fn curated() -> Vec<PaletteItem> {
    [
        ("◆", "Snapshot", "snapshot: create…", "␣sc", "snapshot-create-as"),
        ("◆", "Snapshot", "snapshot: revert…", "␣sr", "snapshot-revert"),
        ("◆", "Snapshot", "snapshot: delete…", "␣sd", "snapshot-delete"),
        ("◆", "Snapshot", "snapshot: tree view", "gs", "snapshot-list"),
        ("●", "Domain", "start", "s", "start"),
        ("●", "Domain", "shutdown", "S", "shutdown"),
        ("◎", "Media", "attach ISO", "␣mi", "change-media"),
        ("◫", "Storage", "volume: create", "n", "vol-create-as"),
        ("▣", "CPU", "set vcpus", "", "setvcpus"),
        ("▤", "Memory", "set live", "", "setmem"),
    ]
    .into_iter()
    .map(|(icon, category, title, key, virsh_cmd)| PaletteItem {
        icon,
        category: category.to_string(),
        title: title.to_string(),
        key: key.to_string(),
        virsh_cmd: virsh_cmd.to_string(),
    })
    .collect()
}

/// Palette state.
#[derive(Debug, Clone)]
pub struct PaletteState {
    pub query: String,
    pub items: Vec<PaletteItem>,
    pub selected: usize,
    pub context: String,
    pub docs: Vec<crate::backend::virsh::parse_help::HelpDoc>,
    /// Generic-form staged command.
    pub staged: String,
    /// Enabled option flags.
    pub enabled: std::collections::HashSet<String>,
    /// Option cursor in the reference pane.
    pub opt_cursor: usize,
    /// Reference pane focused (C-l).
    pub pane_focused: bool,
    /// Recently run actions: (title, context).
    pub recent: Vec<(String, String)>,
}

impl PaletteState {
    /// Demo with query "snap" (acceptance snapshot).
    pub fn demo() -> Self {
        let mut st = Self {
            query: String::from("snap"),
            items: curated(),
            selected: 0,
            context: String::from("arch-dev"),
            docs: vec![snapshot_create_doc()],
            staged: String::from("virsh snapshot-create-as arch-dev --name pre-upgrade-2026-10-06 --atomic"),
            enabled: ["--name", "--atomic"].iter().map(|s| s.to_string()).collect(),
            opt_cursor: 0,
            pane_focused: false,
            recent: vec![
                (String::from("Domain: start"), String::from("k8s-worker-02")),
                (String::from("Media: attach ISO"), String::from("arch-dev · sda")),
                (String::from("Memory: set live"), String::from("truenas-scale")),
            ],
        };
        st.refilter(&curated_names());
        st
    }

    /// Open with live help index entries appended as raw commands.
    pub fn open_live(context: &str, index: &[crate::backend::virsh::parse_help::HelpEntry]) -> Self {
        let mut items = curated();
        for e in index {
            if curated().iter().any(|c| c.virsh_cmd == e.name) {
                continue;
            }
            items.push(PaletteItem {
                icon: "⌨",
                category: String::from("virsh"),
                title: e.name.clone(),
                key: String::new(),
                virsh_cmd: e.name.clone(),
            });
        }
        Self {
            query: String::new(),
            items,
            selected: 0,
            context: context.to_string(),
            docs: Vec::new(),
            staged: String::new(),
            enabled: std::collections::HashSet::new(),
            opt_cursor: 0,
            pane_focused: false,
            recent: Vec::new(),
        }
    }

    /// The highlighted result.
    pub fn selected_item(&self) -> Option<&PaletteItem> {
        let v = self.visible();
        v.get(self.selected.min(v.len().saturating_sub(1))).copied()
    }

    /// Reference doc for the highlighted result, when loaded.
    pub fn doc_for_selected(&self) -> Option<&crate::backend::virsh::parse_help::HelpDoc> {
        let cmd = self.selected_item()?.virsh_cmd.clone();
        self.docs.iter().find(|d| d.name == cmd)
    }

    /// virsh command whose help still has to be loaded for the reference pane.
    pub fn needs_doc(&self) -> Option<String> {
        let item = self.selected_item()?;
        (!item.virsh_cmd.is_empty() && self.doc_for_selected().is_none()).then(|| item.virsh_cmd.clone())
    }

    /// Move the selection; flags staged for the previous command are dropped.
    pub fn move_selection(&mut self, delta: i32) {
        let n = self.visible().len() as i32;
        if n == 0 {
            return;
        }
        let next = (self.selected as i32 + delta).clamp(0, n - 1) as usize;
        if next != self.selected {
            self.selected = next;
            self.enabled.clear();
            self.staged.clear();
            self.opt_cursor = 0;
        }
    }

    /// The command line the current selection + flags produce (no program).
    pub fn command_line(&self) -> Option<String> {
        let item = self.selected_item()?;
        if !self.staged.is_empty() {
            return Some(self.staged.trim_start_matches("virsh ").to_string());
        }
        let mut parts = vec![item.virsh_cmd.clone()];
        if !self.context.is_empty() {
            parts.push(self.context.clone());
        }
        Some(parts.join(" "))
    }

    /// Toggle the option under the cursor into the staged command.
    pub fn toggle_opt(&mut self) {
        let Some(doc) = self.doc_for_selected().cloned() else {
            return;
        };
        let Some(opt) = doc.options.get(self.opt_cursor).map(|o| o.flag.clone()) else {
            return;
        };
        if !self.enabled.remove(&opt) {
            self.enabled.insert(opt);
        }
        self.restage(&doc);
    }

    fn restage(&mut self, doc: &crate::backend::virsh::parse_help::HelpDoc) {
        let mut parts = vec![String::from("virsh"), doc.name.clone()];
        if !self.context.is_empty() {
            parts.push(self.context.clone());
        }
        // Keep the doc's option order; values are filled in on the `:` line.
        for opt in &doc.options {
            if self.enabled.contains(&opt.flag) {
                parts.push(opt.flag.clone());
            }
        }
        self.staged = parts.join(" ");
    }

    fn refilter(&mut self, _all: &[String]) {}

    /// Visible items for the query.
    pub fn visible(&self) -> Vec<&PaletteItem> {
        let titles: Vec<&str> = self.items.iter().map(|i| i.title.as_str()).collect();
        let ranked = fuzzy_rank(&self.query, &titles);
        ranked
            .into_iter()
            .filter_map(|t| self.items.iter().find(|i| i.title == t))
            .collect()
    }
}

fn curated_names() -> Vec<String> {
    curated().into_iter().map(|i| i.title).collect()
}

fn snapshot_create_doc() -> crate::backend::virsh::parse_help::HelpDoc {
    use crate::backend::virsh::parse_help::{HelpDoc, HelpOpt};
    HelpDoc {
        name: String::from("snapshot-create-as"),
        summary: String::from("create a snapshot from a set of args"),
        synopsis: String::from(
            "snapshot-create-as <domain> [--name <s>] [--description <s>] [--print-xml] [--no-metadata]",
        ),
        description: String::from("Creates a snapshot of a domain."),
        options: vec![
            HelpOpt {
                flag: String::from("--name"),
                desc: String::from("name of snapshot"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--description"),
                desc: String::from("free-text description"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--disk-only"),
                desc: String::from("capture disk state but not vm state"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--live"),
                desc: String::from("take a live snapshot"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--quiesce"),
                desc: String::from("quiesce guest fs (needs qemu-ga)"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--atomic"),
                desc: String::from("require atomic operation"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--halt"),
                desc: String::from("halt domain after snapshot"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--no-metadata"),
                desc: String::from("create without libvirt metadata"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--reuse-external"),
                desc: String::from("reuse existing external files"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--memspec"),
                desc: String::from("[file=]name[,snapshot=type]"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--diskspec"),
                desc: String::from("disk[,snapshot=type][,file=name]…"),
                required: false,
            },
            HelpOpt {
                flag: String::from("--print-xml"),
                desc: String::from("print XML instead of creating"),
                required: false,
            },
        ],
    }
}

/// Render the palette modal (148×34 at top).
pub fn render(frame: &mut Frame, theme: &Theme, st: &PaletteState) {
    let t = &theme.tokens;
    let area = frame.area();
    let w = 148u16.min(area.width.saturating_sub(4));
    let h = 34u16.min(area.height.saturating_sub(6));
    let modal = Rect::new(area.x + (area.width.saturating_sub(w)) / 2, area.y + 3, w, h);
    frame.render_widget(Clear, modal);
    let right = Line::from(vec![
        Span::styled("context: ", theme.dim()),
        Span::styled(
            st.context.clone(),
            Style::default().fg(t.fg2).add_modifier(Modifier::BOLD),
        ),
    ]);
    let block = crate::ui::widgets::panel::block(theme, true, "Command palette", Some(right), None, None)
        .padding(ratatui::widgets::Padding::ZERO)
        .style(Style::default().bg(t.bg));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);
    // Input row (bg2), right-aligned "shown / total".
    let visible = st.visible();
    let count = format!("{} / {}  ", visible.len(), st.items.len());
    let left = vec![
        Span::styled(
            "  ❯ ",
            Style::default().fg(t.magenta).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            st.query.clone(),
            Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default().bg(t.fg)),
    ];
    crate::ui::chrome::render_split(
        frame,
        rows[0],
        left,
        vec![Span::styled(count, Style::default().fg(t.comment).bg(t.bg2))],
        Style::default().bg(t.bg2),
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(inner.width as usize),
            Style::default().fg(t.hl),
        )),
        rows[1],
    );
    // Results | reference, separated by a vertical rule.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(66), Constraint::Length(1), Constraint::Min(0)])
        .split(rows[2]);
    let rule: Vec<Line> = (0..cols[1].height)
        .map(|_| Line::from(Span::styled("│", Style::default().fg(t.hl))))
        .collect();
    frame.render_widget(Paragraph::new(rule), cols[1]);
    let res_w = cols[0].width as usize;
    let sel = st.selected.min(visible.len().saturating_sub(1));
    let list_h = (cols[0].height as usize).saturating_sub(if st.recent.is_empty() { 0 } else { 5 });
    let start = sel.saturating_sub(list_h.saturating_sub(1));
    let mut lines: Vec<Line> = Vec::new();
    for (i, item) in visible.iter().enumerate().skip(start).take(list_h) {
        let selected = i == sel;
        let bg = if selected { t.sel } else { t.bg };
        let base = Style::default().fg(t.fg).add_modifier(if selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
        let icon_color = match item.icon {
            "◆" => t.magenta,
            "●" => t.green,
            "◎" => t.yellow,
            "◫" => t.green,
            "▣" => t.blue,
            "▤" => t.magenta,
            _ => t.comment,
        };
        let title = capitalize_first(&item.title);
        let (pre, m, post) = split_match3(&title, &st.query);
        let mut spans = vec![
            Span::styled(format!("  {} ", item.icon), Style::default().fg(icon_color)),
            Span::styled(pre, base),
            Span::styled(m, Style::default().fg(t.orange).add_modifier(Modifier::BOLD)),
            Span::styled(post, base),
        ];
        let used: usize = spans.iter().map(Span::width).sum();
        let key_w = item.key.chars().count();
        spans.push(Span::raw(
            " ".repeat(res_w.saturating_sub(used + key_w + 2).max(1)),
        ));
        spans.push(Span::styled(item.key.clone(), theme.key()));
        lines.push(Line::from(spans).style(Style::default().bg(bg)));
    }
    if visible.is_empty() {
        lines.push(Line::from(Span::styled("  no matching actions", theme.dim())));
    }
    if !st.recent.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  recent", theme.dim())));
        for (action, ctx) in st.recent.iter().take(3) {
            lines.push(Line::from(vec![
                Span::styled("  ↺ ", theme.dim()),
                Span::styled(action.clone(), theme.secondary()),
                Span::styled(format!("  {ctx}"), theme.dim()),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines).style(theme.base()), cols[0]);
    // Reference pane for the highlighted action.
    let ref_area = Rect::new(
        cols[2].x + 2,
        cols[2].y + 1,
        cols[2].width.saturating_sub(3),
        cols[2].height.saturating_sub(1),
    );
    let heading = Style::default().fg(t.blue).add_modifier(Modifier::BOLD);
    let ref_lines = match st.doc_for_selected() {
        Some(doc) => {
            let mut v = vec![
                Line::from(vec![
                    Span::styled(
                        format!("virsh {}", doc.name),
                        Style::default().fg(t.green).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  — {}", doc.summary), theme.dim()),
                ]),
                Line::from(""),
                Line::from(Span::styled("SYNOPSIS", heading)),
            ];
            let synopsis: Vec<Span> = doc
                .synopsis
                .split(' ')
                .flat_map(|w| {
                    let style = if w.trim_start_matches('[').starts_with("--") {
                        Style::default().fg(t.cyan)
                    } else if w.starts_with('<') || w.starts_with("[<") {
                        Style::default().fg(t.fg2)
                    } else {
                        theme.secondary()
                    };
                    [Span::styled(w.to_string(), style), Span::raw(" ")]
                })
                .collect();
            v.push(Line::from(synopsis));
            v.push(Line::from(""));
            v.push(Line::from(Span::styled("OPTIONS", heading)));
            let budget = (ref_area.height as usize).saturating_sub(v.len() + 3);
            let first = st.opt_cursor.saturating_sub(budget.saturating_sub(1));
            for (i, opt) in doc.options.iter().enumerate().skip(first).take(budget) {
                let on = st.enabled.contains(&opt.flag);
                let cur = st.pane_focused && i == st.opt_cursor;
                let row = if cur {
                    Style::default().bg(t.sel)
                } else {
                    Style::default()
                };
                v.push(
                    Line::from(vec![
                        Span::styled(
                            if on { "[x] " } else { "[ ] " },
                            Style::default().fg(if on { t.green } else { t.comment }),
                        ),
                        Span::styled(format!("{:<18}", opt.flag), Style::default().fg(t.cyan)),
                        Span::styled(opt.desc.clone(), theme.secondary()),
                    ])
                    .style(row),
                );
            }
            v.push(Line::from(""));
            if let Some(cmd) = st.command_line() {
                let step = crate::command::plan::CommandStep {
                    program: String::from("virsh"),
                    argv: shlex::split(&cmd).unwrap_or_default(),
                    stdin: None,
                };
                v.push(
                    Line::from(
                        crate::command::display::spans(theme, &step)
                            .into_iter()
                            .map(|s| Span::styled(s.content.into_owned(), s.style.bg(t.bg2)))
                            .collect::<Vec<_>>(),
                    )
                    .style(Style::default().bg(t.bg2)),
                );
            }
            v
        }
        None if st.selected_item().is_some() => {
            vec![Line::from(Span::styled("loading virsh help…", theme.dim()))]
        }
        None => Vec::new(),
    };
    frame.render_widget(
        Paragraph::new(ref_lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(theme.base()),
        ref_area,
    );
    // Footer on bg2.
    let footer_left = vec![
        Span::styled("  ⏎", theme.key()),
        Span::styled(" open in :  ", theme.dim()),
        Span::styled("C-j/k", theme.key()),
        Span::styled(" move  ", theme.dim()),
        Span::styled("C-l", theme.key()),
        Span::styled(" options  ", theme.dim()),
        Span::styled("⇥", theme.key()),
        Span::styled(" toggle flag  ", theme.dim()),
        Span::styled("C-y", theme.key()),
        Span::styled(" copy cmd  ", theme.dim()),
        Span::styled("⎋", theme.key()),
        Span::styled(" close", theme.dim()),
    ];
    let footer_right = vec![
        Span::styled("❯", Style::default().fg(t.magenta).bg(t.bg2)),
        Span::styled(" actions  ", Style::default().fg(t.comment).bg(t.bg2)),
        Span::styled(":", Style::default().fg(t.orange).bg(t.bg2)),
        Span::styled(" ex  ", Style::default().fg(t.comment).bg(t.bg2)),
    ];
    crate::ui::chrome::render_split(
        frame,
        rows[3],
        footer_left,
        footer_right,
        Style::default().bg(t.bg2),
    );
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Split a title into (before, match, after) for a case-insensitive substring match.
fn split_match3(name: &str, query: &str) -> (String, String, String) {
    if !query.is_empty() {
        let lower = name.to_lowercase();
        if let Some(i) = lower.find(&query.to_lowercase()) {
            let end = i + query.len();
            if name.is_char_boundary(i) && name.is_char_boundary(end) {
                return (
                    name[..i].to_string(),
                    name[i..end].to_string(),
                    name[end..].to_string(),
                );
            }
        }
    }
    (name.to_string(), String::new(), String::new())
}
