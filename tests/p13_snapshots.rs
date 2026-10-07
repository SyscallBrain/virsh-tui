//! P13 settings snapshot + per-theme dashboard style maps.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{config::Config, theme, ui::views::dashboard::DashboardState};

fn token_letter(name: &str) -> char {
    match name {
        "bg" => 'B',
        "bg2" => '2',
        "hl" => 'H',
        "sel" => 'S',
        "gutter" => 'G',
        "comment" => 'C',
        "fg" => 'F',
        "fg2" => 'f',
        "blue" => 'b',
        "cyan" => 'c',
        "magenta" => 'm',
        "green" => 'g',
        "yellow" => 'y',
        "orange" => 'o',
        "red" => 'r',
        "teal" => 't',
        _ => '?',
    }
}

#[test]
fn settings_174x43() {
    use virsh_tui::ui::views::settings::{self, SettingsState};
    let th = theme::builtin::tokyo_night();
    let cfg = Config::default();
    let state = SettingsState::open(&cfg);
    let backend = TestBackend::new(174, 43);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| settings::render(f, &th, &cfg, &state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..43 {
        for x in 0..174 {
            text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        text.push('\n');
    }
    insta::assert_snapshot!("settings_174x43", text);
}

#[test]
fn dashboard_style_map_per_theme() {
    let state = DashboardState::demo();
    let mut out = String::new();
    for th in theme::builtin::all() {
        out.push_str(&format!("## {}\n", th.name));
        let backend = TestBackend::new(174, 43);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| virsh_tui::ui::views::dashboard::render(f, &th, &state))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        for y in 0..43 {
            let mut row = String::new();
            for x in 0..174 {
                let cell = buffer.cell((x, y)).unwrap();
                row.push(token_letter(th.token_name(cell.fg)));
                row.push(token_letter(th.token_name(cell.bg)));
            }
            out.push_str(&row);
            out.push('\n');
        }
    }
    insta::assert_snapshot!("dashboard_style_maps", out);
}
