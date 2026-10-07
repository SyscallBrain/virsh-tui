//! P3 dashboard snapshots at reference and compact sizes.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::views::dashboard::{self, DashboardState},
};

fn buffer_text(w: u16, h: u16, state: &DashboardState) -> String {
    let th = theme::builtin::tokyo_night();
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| dashboard::render(f, &th, state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..h {
        for x in 0..w {
            text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        text.push('\n');
    }
    text
}

#[test]
fn dashboard_174x43() {
    let state = DashboardState::demo();
    insta::assert_snapshot!("dashboard_174x43", buffer_text(174, 43, &state));
}

#[test]
fn dashboard_120x36() {
    let state = DashboardState::demo();
    insta::assert_snapshot!("dashboard_120x36", buffer_text(120, 36, &state));
}

#[test]
fn all_nine_themes_render() {
    use ratatui::{Terminal, backend::TestBackend};
    let state = DashboardState::demo();
    for theme in virsh_tui::theme::builtin::all() {
        let backend = TestBackend::new(174, 43);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| dashboard::render(f, &theme, &state)).unwrap();
    }
}
