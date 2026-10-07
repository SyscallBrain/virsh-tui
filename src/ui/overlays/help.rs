//! Keybindings overlay (PLAN.md section 9.8, P12): generated from the live keymap.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::theme::Theme;

use crate::input::engine::KeyAction as A;

/// Live keymap: key sequence -> action.
pub type Keymap = std::collections::HashMap<Vec<String>, A>;

/// One section: title, section colour, rows (key, description, destructive).
#[derive(Debug, Clone)]
pub struct Section {
    pub title: String,
    pub color: &'static str,
    pub rows: Vec<(String, String, bool)>,
}

/// Display form of a bound sequence (`Space s c` → `␣sc`, `g g` → `gg`).
fn seq_label(seq: &[String]) -> String {
    seq.iter()
        .map(|t| match t.as_str() {
            "Space" => "␣",
            "Enter" => "⏎",
            "Backspace" => "⌫",
            "Esc" => "⎋",
            other => other,
        })
        .collect()
}

/// Keys bound to `actions` in the live map, `a / b` joined; `None` if the
/// row has no actions (static rows) or nothing is bound.
fn live_label(map: &Keymap, actions: &[A]) -> Option<String> {
    if actions.is_empty() {
        return None;
    }
    let parts: Vec<String> = actions
        .iter()
        .map(|a| {
            let mut seqs: Vec<String> = map
                .iter()
                .filter(|(_, v)| *v == a)
                .map(|(k, _)| seq_label(k))
                .collect();
            seqs.sort_by_key(|s| (s.chars().count(), s.clone()));
            seqs.first().cloned().unwrap_or_else(|| String::from("—"))
        })
        .collect();
    // `␣sc / ␣sr / ␣sd` → `␣s c/r/d` when only the last key differs.
    if parts.len() > 2 {
        let prefix: String = parts[0]
            .chars()
            .take(parts[0].chars().count().saturating_sub(1))
            .collect();
        if !prefix.is_empty()
            && parts
                .iter()
                .all(|p| p.starts_with(&prefix) && p.chars().count() == prefix.chars().count() + 1)
        {
            let lasts: Vec<String> = parts
                .iter()
                .map(|p| p.chars().last().unwrap_or(' ').to_string())
                .collect();
            return Some(format!("{prefix} {}", lasts.join("/")));
        }
    }
    Some(parts.join(" / "))
}

fn sec(map: &Keymap, title: &str, color: &'static str, rows: &[(&str, &str, bool, &[A])]) -> Section {
    Section {
        title: title.to_string(),
        color,
        rows: rows
            .iter()
            .map(|(k, d, c, acts)| {
                (
                    live_label(map, acts).unwrap_or_else(|| k.to_string()),
                    d.to_string(),
                    *c,
                )
            })
            .collect(),
    }
}

/// Sections in mockup order, with keys taken from the live keymap (so
/// `keymap.toml` remaps show up).
pub fn sections(map: &Keymap) -> Vec<Section> {
    vec![
        sec(
            map,
            "Navigation",
            "blue",
            &[
                ("j / k", "down / up", false, &[A::MoveDown, A::MoveUp]),
                ("gg / G", "top / bottom", false, &[A::GoTop, A::GoBottom]),
                ("C-d / C-u", "half page", false, &[A::HalfDown, A::HalfUp]),
                ("h / l", "focus pane ← →", false, &[]),
                ("H / L", "prev / next tab", false, &[A::PrevTab, A::NextTab]),
                ("1 … 5", "switch view", false, &[]),
                ("⏎", "open", false, &[A::Open]),
                ("⌫ / q", "back", false, &[]),
                ("/", "filter", false, &[A::Filter]),
                ("n / N", "next / prev match", false, &[A::NextMatch, A::PrevMatch]),
            ],
        ),
        sec(
            map,
            "Modes",
            "green",
            &[
                ("i / a", "edit field (INSERT)", false, &[]),
                ("⎋", "back to NORMAL", false, &[]),
                ("V", "visual select", false, &[A::Visual]),
                ("m", "mark / unmark", false, &[A::Mark]),
                (":", "ex command", false, &[A::Ex]),
                (": ⇥", "complete in : line", false, &[]),
                ("C-p", "command palette", false, &[A::Palette]),
            ],
        ),
        sec(
            map,
            "Domain",
            "magenta",
            &[
                ("s", "start", false, &[A::Start]),
                ("S", "shutdown (ACPI)", false, &[A::Shutdown]),
                ("D", "destroy (force off)", true, &[A::Destroy]),
                ("r", "reboot", false, &[A::Reboot]),
                ("R", "reset", true, &[A::Reset]),
                ("p", "pause / resume", false, &[A::Pause]),
                ("Z", "suspend to disk", false, &[A::ManagedSave]),
                ("a", "toggle autostart", false, &[A::Autostart]),
                ("c", "serial console", false, &[A::Console]),
                ("v", "graphical viewer", false, &[A::Viewer]),
                ("e", "edit XML ($EDITOR)", false, &[A::EditXml]),
                ("X", "undefine", true, &[A::Undefine]),
            ],
        ),
        sec(
            map,
            "Yank",
            "cyan",
            &[
                ("yy", "name", false, &[A::YankName]),
                ("yu", "uuid", false, &[A::YankUuid]),
                ("yi", "ip address", false, &[A::YankIp]),
                ("yc", "last virsh command", false, &[A::YankCmd]),
            ],
        ),
        sec(
            map,
            "Leader  ␣",
            "orange",
            &[
                ("␣n", "new domain", false, &[A::NewDomain]),
                ("␣c", "clone", false, &[A::Clone]),
                ("␣r", "rename", false, &[A::Rename]),
                ("␣M", "migrate", false, &[A::Migrate]),
                (
                    "␣s c/r/d",
                    "snapshot new/revert/del",
                    false,
                    &[A::SnapshotNew, A::SnapshotRevert, A::SnapshotDelete],
                ),
                (
                    "␣mi / ␣me",
                    "insert / eject ISO",
                    false,
                    &[A::InsertMedia, A::EjectMedia],
                ),
                (
                    "␣da / ␣dr",
                    "disk attach / resize",
                    false,
                    &[A::DiskAttach, A::DiskResize],
                ),
                (
                    "␣ia / ␣il",
                    "NIC attach / link",
                    false,
                    &[A::NicAttach, A::NicLink],
                ),
                ("␣x", "export XML", false, &[A::ExportXml]),
                ("␣b", "boot order", false, &[A::BootOrder]),
            ],
        ),
        sec(
            map,
            "Monitor",
            "teal",
            &[
                ("t", "60s · 5m · 1h", false, &[A::WindowCycle]),
                ("o", "cycle sort", false, &[A::SortCycle]),
                (
                    "f / F",
                    "cycle filter chips",
                    false,
                    &[A::ChipCycle, A::ChipCycleBack],
                ),
            ],
        ),
        sec(
            map,
            "Ex commands",
            "yellow",
            &[
                (":start", "arch-dev", false, &[]),
                (":setmem", "arch-dev 8G --live", false, &[]),
                (":attach-iso", "arch-dev ~/iso/…", false, &[]),
                (":connect", "qemu+ssh://h/system", false, &[]),
                (":theme", "tokyo-night", false, &[]),
                (":sort", "cpu | mem | name", false, &[]),
                (":w / :q", "apply / close", false, &[]),
                (":!virsh", "raw passthrough", false, &[]),
            ],
        ),
        sec(
            map,
            "App",
            "blue",
            &[
                ("?", "this help", false, &[A::Help]),
                (",", "settings", false, &[A::Settings]),
                ("C-r", "force refresh", false, &[A::Refresh]),
                ("C-l", "redraw", false, &[A::Redraw]),
                ("ZZ / :qa", "quit", false, &[]),
            ],
        ),
    ]
}

/// Help overlay state (live `/` filter).
#[derive(Debug, Clone, Default)]
pub struct HelpState {
    pub filter: String,
}

/// Render the help modal (156×37, 4 columns).
pub fn render(frame: &mut Frame, theme: &Theme, filter: &str, map: &Keymap) {
    let t = &theme.tokens;
    let area = frame.area();
    let w = 156u16.min(area.width.saturating_sub(4));
    let h = 37u16.min(area.height.saturating_sub(4));
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
        .title(Span::styled(" ? Keybindings ", theme.panel_title(true)))
        .title_top(Line::from(Span::styled(" / search · ⎋ close ", theme.dim())).right_aligned());
    let inner = Rect::new(modal.x + 2, modal.y + 1, modal.width - 4, modal.height - 2);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .spacing(1)
        .split(Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(2),
        ));
    let sections = sections(map);
    let groups: [&[Section]; 4] = [&sections[0..2], &sections[2..4], &sections[4..6], &sections[6..8]];
    for (col, group) in cols.iter().zip(groups) {
        let mut lines: Vec<Line> = Vec::new();
        for sec in group {
            let color = match sec.color {
                "blue" => t.blue,
                "green" => t.green,
                "magenta" => t.magenta,
                "cyan" => t.cyan,
                "orange" => t.orange,
                "teal" => t.teal,
                "yellow" => t.yellow,
                _ => t.fg,
            };
            lines.push(Line::from(vec![Span::styled(
                sec.title.clone(),
                Style::default().fg(color).bg(t.bg).add_modifier(Modifier::BOLD),
            )]));
            for (k, d, destructive) in &sec.rows {
                if !filter.is_empty() && !format!("{k} {d}").to_lowercase().contains(&filter.to_lowercase()) {
                    continue;
                }
                lines.push(Line::from(vec![
                    Span::styled(format!("{k:<12}"), theme.key()),
                    Span::styled(
                        d.clone(),
                        if *destructive {
                            Style::default().fg(t.red).bg(t.bg)
                        } else {
                            theme.secondary()
                        },
                    ),
                ]));
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(Paragraph::new(lines).style(theme.base()), *col);
    }
    let footer = Line::from(vec![
        Span::styled("● asks for confirmation  ", Style::default().fg(t.red).bg(t.bg)),
        Span::styled("3j counts work everywhere  ", theme.key()),
        Span::styled(". repeats last action", theme.dim()),
    ]);
    let footer_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(Paragraph::new(footer).style(theme.base()), footer_area);
    frame.render_widget(block, modal);
}
