//! Overlay rendering helpers and the `--dump` frame writer.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Which-key rows for a pending prefix, from the live keymap (so
/// `keymap.toml` remaps show up): next key and what it does.
pub(super) fn which_key_rows(
    map: &std::collections::HashMap<Vec<String>, crate::input::engine::KeyAction>,
    prefix: &str,
) -> Vec<(String, String)> {
    let prefix: Vec<&str> = prefix.split(' ').filter(|t| !t.is_empty()).collect();
    let mut rows: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (seq, action) in map {
        if seq.len() <= prefix.len() || seq.iter().zip(&prefix).any(|(a, b)| a != b) {
            continue;
        }
        let next = seq[prefix.len()].clone();
        if seq.len() == prefix.len() + 1 {
            rows.insert(next, action.name().replace('_', " "));
        } else {
            rows.entry(format!("{next} …"))
                .or_insert_with(|| String::from("more"));
        }
    }
    rows.into_iter().collect()
}

/// Render the which-key popup bottom-right for a pending prefix.
pub(super) fn render_which_key(
    f: &mut ratatui::Frame,
    theme: &Theme,
    prefix: &str,
    map: &std::collections::HashMap<Vec<String>, crate::input::engine::KeyAction>,
) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};
    let t = &theme.tokens;
    let rows = which_key_rows(map, prefix);
    let w = rows
        .iter()
        .map(|(k, d)| (k.chars().count() + d.chars().count() + 4) as u16)
        .max()
        .unwrap_or(10)
        .max(22);
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(k.clone(), theme.key()),
                Span::styled(format!("  {d}"), theme.dim()),
            ])
        })
        .collect();
    let h = (lines.len() as u16 + 2).min(f.area().height);
    let area = f.area();
    let w = w.min(area.width);
    let h = h.min(area.height.saturating_sub(2));
    let rect = ratatui::layout::Rect::new(
        area.x + area.width.saturating_sub(w + 1),
        area.y + area.height.saturating_sub(h + 2),
        w,
        h,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme.border)
        .border_style(Style::default().fg(t.gutter).bg(t.bg));
    f.render_widget(Paragraph::new(lines).block(block).style(theme.base()), rect);
}

pub(super) fn parse_size(size: &str) -> (u16, u16) {
    let mut parts = size.split('x');
    let w = parts.next().and_then(|s| s.parse().ok()).unwrap_or(174);
    let h = parts.next().and_then(|s| s.parse().ok()).unwrap_or(43);
    (w, h)
}

/// Render the `:` ex line on the message row.
pub(super) fn render_ex_line(f: &mut ratatui::Frame, theme: &Theme, ex: &crate::input::textinput::TextInput) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;
    let t = &theme.tokens;
    let area = f.area();
    if area.height < 2 {
        return;
    }
    // COMMAND pill over the status bar's mode pill (DESIGN.md: COMMAND = yellow).
    let pill = ratatui::layout::Rect::new(area.x, area.y + area.height - 2, 9, 1);
    f.render_widget(
        Paragraph::new(Span::styled(
            if ex.filter { " FILTER  " } else { " COMMAND " },
            Style::default()
                .fg(t.bg2)
                .bg(if ex.filter { t.green } else { t.yellow })
                .add_modifier(ratatui::style::Modifier::BOLD),
        )),
        pill,
    );
    let row = ratatui::layout::Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    // The ex line replaces the message row entirely.
    f.render_widget(ratatui::widgets::Clear, row);
    let text = ex.value();
    let (before, rest) = text.split_at(ex.cursor().min(text.len()));
    let mut chars = rest.chars();
    let at = chars.next().map_or(String::from(" "), String::from);
    let line = Line::from(vec![
        Span::styled(if ex.filter { "/" } else { ":" }, Style::default().fg(t.yellow)),
        Span::raw(before.to_string()),
        Span::styled(at, Style::default().fg(t.bg).bg(t.fg)),
        Span::raw(chars.as_str().to_string()),
    ]);
    f.render_widget(Paragraph::new(line).style(theme.base()), row);
}

/// Wizard status + message override (INSERT pill, breadcrumb, autosaved).
pub(super) fn render_wizard_status(
    f: &mut ratatui::Frame,
    theme: &Theme,
    wz: &ui::overlays::wizard::WizardState,
) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;
    let t = &theme.tokens;
    let area = f.area();
    let step_name = ui::overlays::wizard::STEP_NAMES[wz.step.min(5)].to_lowercase();
    if area.height < 2 {
        return;
    }
    let status = Rect::new(area.x, area.y + area.height - 2, area.width, 1);
    let msg_line = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    let line = Line::from(vec![
        Span::styled(
            " INSERT ",
            Style::default()
                .fg(t.bg2)
                .bg(t.green)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::styled(
            format!(" new domain › {step_name} "),
            Style::default().fg(t.fg2).bg(t.hl),
        ),
        Span::raw(""),
        Span::styled(" draft autosaved ", Style::default().fg(t.fg2).bg(t.hl)),
    ]);
    f.render_widget(Paragraph::new(line).style(theme.topbar()), status);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled("-- INSERT --", theme.dim()))).style(theme.base()),
        msg_line,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dump_frame(
    theme: &Theme,
    state: &ChromeState,
    dash: &ui::views::dashboard::DashboardState,
    host: &ui::views::host::HostState,
    nets: &ui::views::networks::NetworksState,
    store: &ui::views::storage::StorageState,
    screen: Option<&str>,
    path: &str,
    w: u16,
    h: u16,
) -> Result<()> {
    use ratatui::backend::TestBackend;
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend)?;
    let screen = screen.unwrap_or("dashboard");
    terminal.draw(|f| match screen {
        "gallery" => ui::views::gallery::render(f, theme),
        "chrome" => ui::render(f, theme, state),
        "host" => ui::views::host::render(f, theme, host),
        "networks" => ui::views::networks::render(f, theme, nets),
        "storage" => ui::views::storage::render(f, theme, store, false),
        "settings" => ui::views::settings::render(
            f,
            theme,
            &crate::config::Config::default(),
            &ui::views::settings::SettingsState::open(&crate::config::Config::default()),
        ),
        "events" => ui::views::events::render(f, theme, &ui::views::events::EventsState::demo()),
        "wizard" => {
            ui::views::dashboard::render(f, theme, dash);
            ui::widgets::modal::dim_background(f, theme);
            ui::overlays::wizard::render(f, theme, &ui::overlays::wizard::WizardState::demo_step3());
        }
        "storage-picker" | "media" => ui::views::storage::render(f, theme, store, true),
        "hardware" => {
            let mut st = ui::views::detail::DetailState::demo("arch-dev");
            st.tab = ui::views::detail::DetailTab::Hardware;
            st.hw.selected = 1;
            if let Some(form) = st.hw.forms.get_mut("cpus") {
                form.focus = 1;
            }
            st.hw.set_field("cpus", "max", "16");
            st.hw_insert = true;
            ui::views::detail::render(f, theme, &st);
        }
        "snapshots" => {
            let mut st = ui::views::detail::DetailState::demo("arch-dev");
            st.tab = ui::views::detail::DetailTab::Snapshots;
            st.snap_modal = ui::views::detail::SnapModal::Revert;
            ui::views::detail::render(f, theme, &st);
        }
        "detail" | "overview" => {
            ui::views::detail::render(f, theme, &ui::views::detail::DetailState::demo("arch-dev"));
        }
        "monitor" => {
            let mut st = ui::views::detail::DetailState::demo("arch-dev");
            st.tab = ui::views::detail::DetailTab::Monitor;
            ui::views::detail::render(f, theme, &st);
        }
        "palette" => {
            ui::views::dashboard::render(f, theme, dash);
            ui::widgets::modal::dim_background(f, theme);
            ui::overlays::palette::render(f, theme, &ui::overlays::palette::PaletteState::demo());
        }
        "help" => {
            ui::views::dashboard::render(f, theme, dash);
            ui::widgets::modal::dim_background(f, theme);
            ui::overlays::help::render(f, theme, "", &crate::input::keymap::default_map());
        }
        _ => ui::views::dashboard::render(f, theme, dash),
    })?;
    let buffer = terminal.backend().buffer().clone();
    std::fs::write(path, buffer_to_ansi(&buffer, w, h))?;
    Ok(())
}

/// Render a buffer as truecolor ANSI text (`cat` it in a terminal to preview).
pub(super) fn buffer_to_ansi(buffer: &ratatui::buffer::Buffer, w: u16, h: u16) -> String {
    use ratatui::style::{Color, Modifier};
    fn sgr(c: Color, fg: bool) -> String {
        let base = if fg { 38 } else { 48 };
        match c {
            Color::Rgb(r, g, b) => format!("\x1b[{base};2;{r};{g};{b}m"),
            Color::Indexed(i) => format!("\x1b[{base};5;{i}m"),
            _ => format!("\x1b[{}m", if fg { 39 } else { 49 }),
        }
    }
    let mut out = String::new();
    for y in 0..h {
        let mut last: Option<(Color, Color, Modifier)> = None;
        for x in 0..w {
            let Some(cell) = buffer.cell((x, y)) else { continue };
            let key = (cell.fg, cell.bg, cell.modifier);
            if last != Some(key) {
                out.push_str("\x1b[0m");
                out.push_str(&sgr(cell.fg, true));
                out.push_str(&sgr(cell.bg, false));
                if cell.modifier.contains(Modifier::BOLD) {
                    out.push_str("\x1b[1m");
                }
                last = Some(key);
            }
            out.push_str(cell.symbol());
        }
        out.push_str("\x1b[0m\n");
    }
    out
}

/// Completion popup above the `:` line: candidates with hints, the selected
/// one highlighted; theme names get a colour swatch.
pub(super) fn render_completion(f: &mut ratatui::Frame, theme: &Theme, c: &super::ExCompletion) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Clear, Paragraph};
    let t = &theme.tokens;
    let area = f.area();
    let max_rows = 10usize.min(area.height.saturating_sub(4) as usize).max(1);
    let first = c.idx.saturating_sub(max_rows - 1);
    let shown: Vec<&crate::input::complete::Item> = c.items.iter().skip(first).take(max_rows).collect();
    let value_w = shown.iter().map(|i| i.value.chars().count()).max().unwrap_or(0);
    let hint_w = shown
        .iter()
        .map(|i| i.hint.chars().count())
        .max()
        .unwrap_or(0)
        .min(48);
    let is_theme = c.base.trim_start().starts_with("theme");
    let swatch_w = if is_theme { 17 } else { 0 };
    let hint_extra = if hint_w > 0 { hint_w + 2 } else { 0 };
    let w = (value_w + hint_extra + swatch_w + 4).min(area.width as usize) as u16;
    let h = shown.len() as u16 + 2;
    let x = (area.x + 1 + c.base.chars().count() as u16).min(area.width.saturating_sub(w));
    let y = area.y + area.height.saturating_sub(h + 1);
    let rect = Rect::new(x, y, w, h);
    let mut lines = Vec::new();
    for (i, it) in shown.iter().enumerate() {
        let sel = first + i == c.idx;
        let bg = if sel { t.sel } else { t.bg };
        let mut spans = vec![
            Span::styled(if sel { "❯ " } else { "  " }, Style::default().fg(t.blue)),
            Span::styled(
                format!("{:<value_w$}", it.value),
                Style::default().fg(t.fg).add_modifier(if sel {
                    ratatui::style::Modifier::BOLD
                } else {
                    ratatui::style::Modifier::empty()
                }),
            ),
        ];
        if is_theme {
            let (th, _) = crate::ui::views::settings::resolve(&it.value);
            let k = &th.tokens;
            spans.push(Span::raw(" "));
            for col in [
                k.bg, k.blue, k.cyan, k.magenta, k.green, k.yellow, k.orange, k.red,
            ] {
                spans.push(Span::styled("██", Style::default().fg(col)));
            }
        }
        if !it.hint.is_empty() {
            let hint: String = it.hint.chars().take(hint_w).collect();
            spans.push(Span::styled(format!("  {hint}"), theme.dim()));
        }
        lines.push(Line::from(spans).style(Style::default().bg(bg)));
    }
    let more = (c.items.len() > shown.len()).then(|| format!("{}/{}", c.idx + 1, c.items.len()));
    let block = crate::ui::widgets::panel::block(theme, true, "complete", None, None, more.as_deref())
        .padding(ratatui::widgets::Padding::ZERO);
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(lines).block(block).style(theme.base()), rect);
}
