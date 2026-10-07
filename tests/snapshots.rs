//! Snapshot tests for P0 chrome.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::{self, ChromeState},
};

#[test]
fn chrome_174x43() {
    let th = theme::builtin::tokyo_night();
    let state = ChromeState::default();
    let backend = TestBackend::new(174, 43);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::render(f, &th, &state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..43 {
        for x in 0..174 {
            text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        text.push('\n');
    }
    insta::assert_snapshot!("chrome_174x43", text);
}
