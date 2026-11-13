//! `general.key_hints = false` hides every key hint: status bar, the
//! selected domain's actions and the not-running start hint.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    config::Config,
    theme,
    ui::views::dashboard::{self, DashboardState},
};

fn buffer_text(w: u16, h: u16, state: &DashboardState) -> String {
    let th = theme::builtin::tokyo_night();
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
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
fn dashboard_without_key_hints() {
    let state = DashboardState::demo();
    let with = buffer_text(174, 43, &state);
    assert!(with.contains("j/k move") && with.contains("shutdown"));

    let mut config = Config::default();
    config.general.key_hints = false;
    virsh_tui::config::set_runtime(&config);
    let without = buffer_text(174, 43, &state);
    for hint in ["j/k move", "S shutdown", "e edit xml", "? help"] {
        assert!(!without.contains(hint), "`{hint}` still shown:\n{without}");
    }
    assert!(without.contains("NORMAL"), "the mode pill stays");
    assert!(without.contains("Uptime"), "the domain info stays");
}
