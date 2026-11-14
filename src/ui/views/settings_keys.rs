//! Settings › Keybindings: every remappable action with its live keys.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::input::catalog::{ActionInfo, CATALOG, keys_of, seq_text};
use crate::input::engine::KeyAction;
use crate::input::keymap::Keymap;
use crate::input::keymap_file::{Edit, KeymapFile};
use crate::input::textinput::TextInput;
use crate::theme::Theme;
use crate::ui::views::settings::{SettingsPane, SettingsState};

/// Keybindings category state (inside [`SettingsState`]).
#[derive(Debug, Clone, Default)]
pub struct KeysState {
    /// Index into [`visible_rows`].
    pub sel: usize,
    /// `/` input while typing.
    pub filter: Option<TextInput>,
    /// Filter kept after `⏎`.
    pub query: String,
    pub capture: Option<Capture>,
    /// Conflict or reset-all question waiting for `y`/`n`.
    pub pending: Option<PendingEdit>,
    pub message: Option<String>,
}

impl KeysState {
    /// The filter text in effect (live while typing).
    pub fn effective_query(&self) -> &str {
        self.filter.as_ref().map_or(self.query.as_str(), TextInput::value)
    }
}

/// Keys being captured for an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub action: KeyAction,
    /// `a`: added to the action's keys instead of replacing them.
    pub add: bool,
    pub tokens: Vec<String>,
}

/// An edit waiting for confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEdit {
    pub edit: Edit,
    pub question: String,
}

/// One selectable row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRow {
    Action(&'static ActionInfo),
    ResetAll,
}

/// Rows matching `query` (label, action name or key text, case-insensitive);
/// "reset all to defaults" is always last.
pub fn visible_rows(query: &str, map: &Keymap) -> Vec<KeyRow> {
    let q = query.trim().to_lowercase();
    let mut rows: Vec<KeyRow> = CATALOG
        .iter()
        .filter(|i| {
            q.is_empty()
                || i.label.to_lowercase().contains(&q)
                || i.action.name().contains(&q)
                || keys_of(map, i.action)
                    .iter()
                    .any(|k| seq_text(k).to_lowercase().contains(&q))
        })
        .map(KeyRow::Action)
        .collect();
    rows.push(KeyRow::ResetAll);
    rows
}

/// `S · Space q`, or `—` when unbound.
pub fn keys_label(map: &Keymap, action: KeyAction) -> String {
    let keys = keys_of(map, action);
    if keys.is_empty() {
        String::from("—")
    } else {
        keys.iter().map(|k| seq_text(k)).collect::<Vec<_>>().join(" · ")
    }
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Render the Keybindings category panel.
pub fn render(
    frame: &mut Frame,
    theme: &Theme,
    state: &SettingsState,
    map: &Keymap,
    file: &KeymapFile,
    area: Rect,
) {
    let t = &theme.tokens;
    let ks = &state.keys;
    let focused = state.pane != SettingsPane::Categories;
    let footer = if crate::config::key_hints() {
        Some("/ filter · ⏎ change · a add key · d default")
    } else {
        None
    };
    let block = crate::ui::widgets::panel::block(theme, focused, "Keybindings", None, None, footer);
    let inner = block.inner(area);
    frame.render_widget(block.style(theme.base()), area);

    // Bottom lines: filter, capture help, question, message.
    let mut bottom: Vec<Line> = Vec::new();
    if let Some(f) = &ks.filter {
        bottom.push(Line::from(vec![
            Span::styled("/", Style::default().fg(t.blue)),
            Span::styled(format!("{}▏", f.value()), theme.text()),
        ]));
    } else if !ks.query.is_empty() {
        bottom.push(Line::from(Span::styled(
            format!("filter: {}", ks.query),
            theme.secondary(),
        )));
    }
    if ks.capture.is_some() {
        bottom.push(Line::from(Span::styled(
            "⏎ confirm · ⌫ undo · ⎋ cancel",
            theme.secondary(),
        )));
    }
    if let Some(p) = &ks.pending {
        let buttons = if p.edit == Edit::ResetAll {
            "  y reset · n cancel"
        } else {
            "  y replace · n cancel"
        };
        bottom.push(Line::from(vec![
            Span::styled(p.question.clone(), Style::default().fg(t.yellow)),
            Span::styled(buttons, theme.secondary()),
        ]));
    }
    if let Some(m) = &ks.message {
        let style = if m.starts_with('✗') {
            Style::default().fg(t.red)
        } else {
            Style::default().fg(t.green)
        };
        bottom.push(Line::from(Span::styled(m.clone(), style)));
    }
    let bottom_h = if bottom.is_empty() {
        0
    } else {
        bottom.len() as u16 + 1
    };
    let [list_area, bottom_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(bottom_h)]).areas(inner);

    let width = list_area.width as usize;
    let rows = visible_rows(ks.effective_query(), map);
    let mut lines: Vec<Line> = Vec::new();
    let mut selected_line = 0;
    let mut last = "";
    for (i, row) in rows.iter().enumerate() {
        let cur = focused && i == ks.sel;
        let bg = if cur { t.sel } else { t.bg };
        let marker = Span::styled(if cur { "❯ " } else { "  " }, Style::default().fg(t.blue));
        match row {
            KeyRow::Action(info) => {
                if info.section != last {
                    if !lines.is_empty() {
                        lines.push(Line::from(""));
                    }
                    lines.push(Line::from(Span::styled(
                        info.section,
                        Style::default().fg(t.blue).add_modifier(Modifier::BOLD),
                    )));
                    last = info.section;
                }
                let capturing = ks.capture.as_ref().filter(|c| cur && c.action == info.action);
                let keys = match capturing {
                    Some(c) => format!("press keys… {}", c.tokens.join(" ")),
                    None => keys_label(map, info.action),
                };
                let mark = if file.is_custom(info.action) {
                    " ● custom"
                } else {
                    ""
                };
                let used = 2 + keys.chars().count() + mark.chars().count() + 1;
                let label_w = width.saturating_sub(used).max(4);
                let label = format!("{:<label_w$}", truncate(info.label, label_w));
                let label_style = if info.destructive {
                    Style::default().fg(t.red)
                } else {
                    theme.text()
                };
                let keys_style = if capturing.is_some() {
                    Style::default().fg(t.yellow)
                } else {
                    Style::default().fg(t.fg)
                };
                selected_line = if cur { lines.len() } else { selected_line };
                lines.push(
                    Line::from(vec![
                        marker,
                        Span::styled(label, label_style),
                        Span::raw(" "),
                        Span::styled(keys, keys_style),
                        Span::styled(mark, Style::default().fg(t.magenta)),
                    ])
                    .style(Style::default().bg(bg)),
                );
            }
            KeyRow::ResetAll => {
                lines.push(Line::from(""));
                selected_line = if cur { lines.len() } else { selected_line };
                let style = if file.custom_count() == 0 {
                    theme.secondary()
                } else {
                    theme.text()
                };
                lines.push(
                    Line::from(vec![marker, Span::styled("reset all to defaults", style)])
                        .style(Style::default().bg(bg)),
                );
            }
        }
    }
    let height = list_area.height as usize;
    let start = if height == 0 {
        0
    } else {
        (selected_line + 1).saturating_sub(height)
    };
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((start.min(u16::MAX as usize) as u16, 0))
            .style(theme.base()),
        list_area,
    );
    if !bottom.is_empty() {
        let mut with_gap = vec![Line::from("")];
        with_gap.extend(bottom);
        frame.render_widget(
            Paragraph::new(with_gap)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .style(theme.base()),
            bottom_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::input::engine::KeyAction;
    use crate::input::keymap_file::{Edit, KeymapFile};
    use crate::ui::views::settings::{SettingsPane, SettingsState};
    use ratatui::{Terminal, backend::TestBackend};

    fn state() -> SettingsState {
        let mut st = SettingsState::open(&Config::default());
        st.cat = 2;
        st.pane = SettingsPane::Options;
        st
    }

    fn draw(st: &SettingsState, file: &KeymapFile, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let map = file.map();
        terminal
            .draw(|f| render(f, &crate::theme::builtin::tokyo_night(), st, &map, file, f.area()))
            .unwrap();
        (0..height)
            .map(|y| {
                let mut row: String = (0..width)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect();
                row.push('\n');
                row
            })
            .collect()
    }

    #[test]
    fn keybindings_lists_sections_and_keys() {
        let screen = draw(&state(), &KeymapFile::in_memory(), 100, 120);
        for want in [
            "Navigation",
            "shutdown (ACPI)",
            "Space s c",
            "reset all to defaults",
        ] {
            assert!(screen.contains(want), "{want} missing:\n{screen}");
        }
    }

    #[test]
    fn custom_mark_shows_for_overridden_action() {
        let mut file = KeymapFile::in_memory();
        file.apply(&Edit::Replace(
            KeyAction::Shutdown,
            vec![String::from("Space"), String::from("q")],
        ));
        let screen = draw(&state(), &file, 100, 120);
        let line = screen.lines().find(|l| l.contains("shutdown (ACPI)")).unwrap();
        assert!(line.contains("Space q") && line.contains("● custom"), "{line}");
    }

    #[test]
    fn filter_matches_label_name_and_key() {
        let map = crate::input::keymap::default_map();
        let rows = visible_rows("snap", &map);
        assert_eq!(rows.len(), 4, "3 snapshot actions + reset all");
        assert!(matches!(rows.last(), Some(KeyRow::ResetAll)));
        assert!(
            visible_rows("C-p", &map)
                .iter()
                .any(|r| matches!(r, KeyRow::Action(i) if i.action == KeyAction::Palette))
        );
        assert!(
            visible_rows("yank_uuid", &map)
                .iter()
                .any(|r| matches!(r, KeyRow::Action(i) if i.action == KeyAction::YankUuid))
        );
    }

    #[test]
    fn selected_row_stays_visible() {
        let mut st = state();
        let map = crate::input::keymap::default_map();
        st.keys.sel = visible_rows("", &map).len() - 2; // last action (redraw)
        let screen = draw(&st, &KeymapFile::in_memory(), 100, 15);
        assert!(screen.contains("redraw"), "{screen}");
    }
}
