//! Settings view (screen 10, PLAN.md section 9.13, P13, `,` replaces the body).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::config::Config;
use crate::theme::{IconSet, Theme, builtin};
use crate::ui::widgets::charts;

/// Settings category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    General,
    Appearance,
    Keybindings,
    Connections,
    Monitoring,
    Confirmations,
    Console,
}

impl Category {
    pub const ALL: [(Self, &'static str, &'static str); 7] = [
        (Self::General, "⚙", "General"),
        (Self::Appearance, "◐", "Appearance"),
        (Self::Keybindings, "⌨", "Keybindings"),
        (Self::Connections, "⌁", "Connections"),
        (Self::Monitoring, "∿", "Monitoring"),
        (Self::Confirmations, "⚠", "Confirmations"),
        (Self::Console, "▭", "Console & viewer"),
    ];
}

/// Focused pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsPane {
    #[default]
    Categories,
    List,
    Options,
}

/// Settings state.
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub cat: usize,
    pub pane: SettingsPane,
    pub theme_idx: usize,
    pub opt_idx: usize,
    pub before_theme: String,
    pub themes: Vec<String>,
}

impl SettingsState {
    /// Open with the current config (theme pre-selected).
    pub fn open(config: &Config) -> Self {
        let mut themes: Vec<String> = builtin::all().iter().map(|t| t.name.to_string()).collect();
        for custom in custom_names() {
            if !themes.contains(&custom) {
                themes.push(custom);
            }
        }
        let theme_idx = themes
            .iter()
            .position(|n| *n == config.appearance.theme)
            .unwrap_or(0);
        Self {
            cat: 1,
            pane: SettingsPane::List,
            theme_idx,
            opt_idx: 0,
            before_theme: config.appearance.theme.clone(),
            themes,
        }
    }

    /// Selected theme name.
    pub fn selected_theme(&self) -> &str {
        self.themes
            .get(self.theme_idx)
            .map(|s| s.as_str())
            .unwrap_or("tokyo-night")
    }

    /// Move theme selection (live preview by caller).
    pub fn move_theme(&mut self, delta: i32) {
        let n = self.themes.len() as i32;
        if n > 0 {
            self.theme_idx = (self.theme_idx as i32 + delta).rem_euclid(n) as usize;
        }
    }
}

/// One row in a settings category: `editable` rows are changed with ⏎.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptRow {
    pub section: &'static str,
    pub label: String,
    pub value: String,
    pub editable: bool,
}

fn row(section: &'static str, label: &str, value: impl Into<String>, editable: bool) -> OptRow {
    OptRow {
        section,
        label: label.to_string(),
        value: value.into(),
        editable,
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "[x]" } else { "[ ]" }
}

/// Rows of a non-Appearance category (Appearance keeps its own layout).
pub fn category_rows(cat: Category, config: &Config) -> Vec<OptRow> {
    let g = &config.general;
    let m = &config.monitoring;
    let c = &config.confirmations;
    match cat {
        Category::General => vec![
            row("Connection", "default URI", g.default_uri.clone(), true),
            row(
                "Refresh",
                "refresh interval",
                format!("{}s", g.refresh_interval_secs),
                true,
            ),
            row("Start", "start view", g.start_view.clone(), true),
            row(
                "Editor",
                "editor",
                std::env::var("VISUAL")
                    .or_else(|_| std::env::var("EDITOR"))
                    .unwrap_or_else(|_| String::from("vi")),
                false,
            ),
        ],
        Category::Keybindings => vec![
            row("Keymap", "file", "~/.config/virsh-tui/keymap.toml", false),
            row(
                "Keymap",
                "format",
                "[normal.domains] \"shutdown\" = [\"S\"]",
                false,
            ),
            row("Keymap", "reference", "? shows the live keymap", false),
        ],
        Category::Connections => config
            .connections
            .uris
            .iter()
            .map(|u| {
                let mark = if *u == g.default_uri {
                    "● default"
                } else {
                    "⏎ make default"
                };
                row("Saved URIs", u, mark, true)
            })
            .collect(),
        Category::Monitoring => vec![
            row("Guest memory", "guest memory stats", on_off(m.balloon_on), true),
            row(
                "Guest memory",
                "stats period",
                format!("{}s", m.balloon_period_secs),
                true,
            ),
            row("Host", "per-thread CPU meters", on_off(m.thread_sampling), true),
        ],
        Category::Confirmations => vec![
            row("Ask before", "destroy", on_off(c.destroy), true),
            row("Ask before", "reset", on_off(c.reset), true),
            row("Ask before", "undefine", on_off(c.undefine), true),
            row(
                "Ask before",
                "type the name to undefine",
                on_off(c.type_name_for_undefine),
                true,
            ),
            row("Ask before", "snapshot revert", on_off(c.revert), true),
            row("Ask before", "delete volume", on_off(c.delete_volume), true),
            row("Ask before", "wipe volume", on_off(c.wipe), true),
        ],
        Category::Console => vec![
            row("Viewer", "graphical viewer", config.console.viewer.clone(), true),
            row("Console", "escape", config.console.escape.clone(), false),
        ],
        Category::Appearance => Vec::new(),
    }
}

/// Number of selectable options in a category.
pub fn option_count(cat: Category, config: &Config) -> usize {
    if cat == Category::Appearance {
        6
    } else {
        category_rows(cat, config).len()
    }
}

/// Explanation shown under some options.
fn option_hint(cat: Category, idx: usize) -> &'static str {
    match (cat, idx) {
        (Category::Monitoring, 0) => {
            "Runs `dommemstat <vm> --period N --live` once per running guest so the balloon driver reports \
             used memory. It changes the running guests (not their config). Off: memory shows host RSS."
        }
        (Category::General, 0) => "Used when virsh-tui starts without -c.",
        (Category::General, 1) => "How often domstats is sampled (charts use 60 samples).",
        _ => "",
    }
}

/// Custom theme names from the themes dir.
pub fn custom_names() -> Vec<String> {
    let mut out = Vec::new();
    let dir = directories::ProjectDirs::from("", "", "virsh-tui")
        .map(|d| d.config_dir().to_path_buf().join("themes"));
    if let Some(dir) = dir
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".toml") {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    out
}

/// Resolve a theme by name: builtin, custom file, or tokyo-night fallback.
pub fn resolve(name: &str) -> (Theme, Option<String>) {
    if let Some(builtin) = builtin::all().into_iter().find(|t| t.name == name) {
        return (builtin, None);
    }
    match crate::theme::load::load_custom(name) {
        Ok(theme) => (theme, None),
        Err(e) => (
            builtin::tokyo_night(),
            Some(format!("theme {name}: {e} — using tokyo-night")),
        ),
    }
}

/// Apply appearance flags onto a theme.
pub fn apply_appearance(mut theme: Theme, config: &Config) -> Theme {
    use ratatui::widgets::BorderType as B;
    theme.border = match config.appearance.borders.as_str() {
        "plain" => B::Plain,
        "double" => B::Double,
        "thick" => B::Thick,
        _ => B::Rounded,
    };
    theme.transparent = config.appearance.transparent;
    theme.dim_modals = config.appearance.dim_modals;
    theme.graphs = crate::theme::GraphStyle::parse(&config.appearance.graphs);
    theme.gradient = config.appearance.gradient;
    theme.icons = IconSet::parse(&config.appearance.icons);
    theme
}

/// Render settings (replaces the body, not a modal).
pub fn render(frame: &mut Frame, theme: &Theme, config: &Config, state: &SettingsState) {
    let area = frame.area();
    // Settings replaces the whole screen: wipe what the current view drew.
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(Paragraph::new("").style(theme.base()), area);
    if crate::ui::layout::breakpoint(area.width, area.height) == crate::ui::layout::Breakpoint::TooSmall {
        crate::ui::layout::render_too_small(frame, theme);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    render_topbar(frame, theme, rows[0]);
    render_body(frame, theme, config, state, rows[1]);
    render_statusbar(frame, theme, state, rows[2]);
    render_message(frame, theme, config, state, rows[3]);
}

fn render_topbar(frame: &mut Frame, theme: &Theme, area: Rect) {
    crate::ui::chrome::topbar_settings(frame, theme, area, "14:40:03");
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

fn render_body(frame: &mut Frame, theme: &Theme, config: &Config, state: &SettingsState, area: Rect) {
    let body = Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), area.height);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(29), Constraint::Length(50), Constraint::Min(0)])
        .spacing(1)
        .split(body);
    render_cats(frame, theme, state, cols[0]);
    let cat = Category::ALL[state.cat.min(Category::ALL.len() - 1)].0;
    if cat != Category::Appearance {
        let rest = Rect::new(
            cols[1].x,
            cols[1].y,
            cols[1].width + 1 + cols[2].width,
            cols[1].height,
        );
        render_category(frame, theme, config, state, cat, rest);
        return;
    }
    render_list(frame, theme, config, state, cols[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(12), Constraint::Min(0)])
        .split(cols[2]);
    render_appearance(frame, theme, config, state, right[0]);
    render_preview(frame, theme, state, right[1]);
}

fn render_cats(frame: &mut Frame, theme: &Theme, state: &SettingsState, area: Rect) {
    let t = &theme.tokens;
    let lines: Vec<Line> = Category::ALL
        .iter()
        .enumerate()
        .map(|(i, (_, icon, name))| {
            let selected = i == state.cat;
            let bg = if selected && state.pane == SettingsPane::Categories {
                t.sel
            } else {
                t.bg
            };
            Line::from(vec![
                Span::styled(format!("{icon} "), Style::default().fg(t.fg2).bg(bg)),
                Span::styled(name.to_string(), Style::default().fg(t.fg).bg(bg)),
            ])
        })
        .collect();
    let block = panel_block(theme, false, "Settings", None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn swatch_colors(theme: &Theme) -> [ratatui::style::Color; 8] {
    let t = &theme.tokens;
    [
        t.blue, t.cyan, t.magenta, t.green, t.yellow, t.orange, t.red, t.teal,
    ]
}

fn render_list(frame: &mut Frame, theme: &Theme, config: &Config, state: &SettingsState, area: Rect) {
    let t = &theme.tokens;
    let preview = resolve(state.selected_theme()).0;
    let sw = swatch_colors(&preview);
    let mut lines: Vec<Line> = Vec::new();
    for (i, name) in state.themes.iter().enumerate() {
        let selected = i == state.theme_idx;
        let bg = if selected { t.sel } else { t.bg };
        let mut spans = vec![
            Span::styled(
                if selected { "❯" } else { " " },
                Style::default().fg(t.blue).bg(bg),
            ),
            Span::raw(" "),
            Span::styled(
                name.clone(),
                Style::default().fg(t.fg).bg(bg).add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            ),
            Span::raw(" "),
        ];
        for c in sw {
            spans.push(Span::styled("██", Style::default().fg(c).bg(bg)));
        }
        lines.push(Line::from(spans).style(Style::default().bg(bg)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "+ custom: ~/.config/virsh-tui/themes/*.toml",
        theme.dim(),
    )));
    let right = format!("{} built-in", builtin::all().len());
    let block = panel_block(theme, state.pane == SettingsPane::List, "Theme", Some(right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
    let _ = config;
}

/// A non-Appearance category: sections, label/value rows, cursor, hint.
fn render_category(
    frame: &mut Frame,
    theme: &Theme,
    config: &Config,
    state: &SettingsState,
    cat: Category,
    area: Rect,
) {
    let t = &theme.tokens;
    let rows = category_rows(cat, config);
    let focused = state.pane != SettingsPane::Categories;
    let mut lines: Vec<Line> = Vec::new();
    let mut last = "";
    for (i, r) in rows.iter().enumerate() {
        if r.section != last {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                r.section,
                Style::default().fg(t.blue).add_modifier(Modifier::BOLD),
            )));
            last = r.section;
        }
        let cur = focused && i == state.opt_idx;
        let bg = if cur { t.sel } else { t.bg };
        let value_style = match r.value.as_str() {
            "[x]" | "● default" => Style::default().fg(t.green),
            "[ ]" | "⏎ make default" => theme.dim(),
            _ if r.editable => Style::default().fg(t.fg).bg(if cur { t.sel } else { t.hl }),
            _ => theme.secondary(),
        };
        lines.push(
            Line::from(vec![
                Span::styled(if cur { "❯ " } else { "  " }, Style::default().fg(t.blue)),
                Span::styled(
                    format!("{:<30}", r.label),
                    if r.editable { theme.text() } else { theme.dim() },
                ),
                Span::styled(format!(" {} ", r.value), value_style),
            ])
            .style(Style::default().bg(bg)),
        );
    }
    let hint = option_hint(cat, state.opt_idx);
    if focused && !hint.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(hint, theme.dim())));
    }
    let name = Category::ALL[state.cat].2;
    let footer = if rows.iter().any(|r| r.editable) {
        Some("⏎ change · saved immediately")
    } else {
        None
    };
    let block = crate::ui::widgets::panel::block(theme, focused, name, None, None, footer);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(theme.base()),
        area,
    );
}

fn render_appearance(frame: &mut Frame, theme: &Theme, config: &Config, state: &SettingsState, area: Rect) {
    let t = &theme.tokens;
    let radio = |on: bool| {
        if on {
            Span::styled("(●)", Style::default().fg(t.blue).bg(t.bg))
        } else {
            Span::styled("( )", theme.dim())
        }
    };
    let check = |on: bool| {
        if on {
            Span::styled("[x]", Style::default().fg(t.green).bg(t.bg))
        } else {
            Span::styled("[ ]", theme.dim())
        }
    };
    let a = &config.appearance;
    let lines = vec![
        Line::from(Span::styled(
            "Borders",
            Style::default().fg(t.blue).bg(t.bg).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            radio(a.borders == "rounded"),
            Span::styled(" ╭─╮ rounded ", theme.text()),
            radio(a.borders == "plain"),
            Span::styled(" ┌─┐ plain ", theme.secondary()),
            radio(a.borders == "double"),
            Span::styled(" ╔═╗ double ", theme.secondary()),
            radio(a.borders == "thick"),
            Span::styled(" ┏━┓ thick", theme.secondary()),
        ]),
        Line::from(Span::styled(
            "Icons",
            Style::default().fg(t.blue).bg(t.bg).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            radio(a.icons == "unicode"),
            Span::styled(" unicode ● ◆ ◎ ", theme.text()),
            radio(a.icons == "nerd"),
            Span::styled(" nerd font ", theme.secondary()),
            radio(a.icons == "ascii"),
            Span::styled(" ascii * # o", theme.secondary()),
        ]),
        Line::from(Span::styled(
            "Graphs",
            Style::default().fg(t.blue).bg(t.bg).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            radio(a.graphs == "braille"),
            Span::styled(" braille ⣿ ", theme.text()),
            radio(a.graphs == "block"),
            Span::styled(" block █ ", theme.secondary()),
            radio(a.graphs == "tty"),
            Span::styled(" tty   ", theme.secondary()),
            check(a.gradient),
            Span::styled(" vertical gradient", theme.text()),
        ]),
        Line::from(Span::styled(
            "Background",
            Style::default().fg(t.blue).bg(t.bg).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            check(a.transparent),
            Span::styled(" transparent (inherit terminal) ", theme.secondary()),
            check(a.dim_modals),
            Span::styled(" dim behind modals", theme.text()),
        ]),
    ];
    let mut lines = lines;
    if state.pane == SettingsPane::Options {
        let names = [
            "borders",
            "icons",
            "graphs",
            "vertical gradient",
            "transparent",
            "dim behind modals",
        ];
        lines.push(Line::from(vec![
            Span::styled("❯ ", Style::default().fg(t.blue)),
            Span::styled(
                names[state.opt_idx.min(5)],
                theme.text().add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ⏎ change · j/k option", theme.dim()),
        ]));
    }
    let block = panel_block(theme, state.pane == SettingsPane::Options, "Appearance", None);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

fn render_preview(frame: &mut Frame, theme: &Theme, state: &SettingsState, area: Rect) {
    let t = &theme.tokens;
    let preview = apply_preview_flags(resolve(state.selected_theme()).0, state);
    let names = [
        "bg", "bg2", "hl", "sel", "gutter", "comment", "fg", "fg2", "blue", "cyan", "magenta", "green",
        "yellow", "orange", "red", "teal",
    ];
    let colors = [
        preview.tokens.bg,
        preview.tokens.bg2,
        preview.tokens.hl,
        preview.tokens.sel,
        preview.tokens.gutter,
        preview.tokens.comment,
        preview.tokens.fg,
        preview.tokens.fg2,
        preview.tokens.blue,
        preview.tokens.cyan,
        preview.tokens.magenta,
        preview.tokens.green,
        preview.tokens.yellow,
        preview.tokens.orange,
        preview.tokens.red,
        preview.tokens.teal,
    ];
    let mut lines: Vec<Line> = Vec::new();
    for row in 0..2 {
        let mut spans = Vec::new();
        for i in 0..8 {
            let idx = row * 8 + i;
            spans.push(Span::styled(
                "████████",
                Style::default().fg(colors[idx]).bg(preview.tokens.bg),
            ));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
        let mut names_line = Vec::new();
        for i in 0..8 {
            names_line.push(Span::styled(format!("{:8} ", names[row * 8 + i]), theme.dim()));
        }
        lines.push(Line::from(names_line));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("▌", Style::default().fg(t.blue).bg(t.sel)),
        Span::styled("● ", Style::default().fg(t.green).bg(t.sel)),
        Span::styled(
            "arch-dev        ",
            Style::default().fg(t.fg).bg(t.sel).add_modifier(Modifier::BOLD),
        ),
        Span::styled("running", Style::default().fg(t.green).bg(t.sel)),
    ]));
    let (on, off) = charts::lg(0.68, 22);
    lines.push(Line::from(vec![
        Span::styled("CPU  ", theme.dim()),
        Span::styled(on, Style::default().fg(t.green).bg(t.bg)),
        Span::styled(off, Style::default().fg(t.hl).bg(t.bg)),
        Span::styled(" 68%", theme.secondary()),
    ]));
    let data = charts::series(7, 60, 0.42, 0.22, 0.1);
    for row in charts::area(theme.graphs, &data, 30, 5, charts::BrailleMode::Fill) {
        lines.push(Line::from(Span::styled(
            row,
            Style::default().fg(t.cyan).bg(t.bg),
        )));
    }
    let right = format!("Preview · {}", state.selected_theme());
    let block = panel_block(&preview, false, "Preview", Some(right));
    frame.render_widget(Paragraph::new(lines).block(block).style(theme.base()), area);
}

/// Preview theme with the current option flags applied (borders/transparent).
fn apply_preview_flags(theme: Theme, _state: &SettingsState) -> Theme {
    theme
}

fn render_statusbar(frame: &mut Frame, theme: &Theme, state: &SettingsState, area: Rect) {
    let t = &theme.tokens;
    let spans = vec![
        Span::styled(
            " NORMAL ",
            Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " settings › appearance › theme ",
            Style::default().fg(t.fg2).bg(t.hl),
        ),
        Span::styled(
            "  j/k preview theme  ⏎ apply  h/l pane  ⎋ revert & close",
            theme.dim(),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.topbar()), area);
    let _ = state;
}

fn render_message(frame: &mut Frame, theme: &Theme, config: &Config, state: &SettingsState, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                "saved to ~/.config/virsh-tui/config.toml · theme = \"{}\"",
                state.selected_theme()
            ),
            theme.dim(),
        )))
        .style(theme.base()),
        area,
    );
    let _ = config;
}
