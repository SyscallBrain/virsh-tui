//! P5 host snapshots at reference and compact sizes.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::views::host::{self, HostState},
};

fn buffer_text(w: u16, h: u16, state: &HostState) -> String {
    let th = theme::builtin::tokyo_night();
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| host::render(f, &th, state)).unwrap();
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
fn host_174x43() {
    let state = HostState::demo();
    insta::assert_snapshot!("host_174x43", buffer_text(174, 43, &state));
}

#[test]
fn host_120x36() {
    let state = HostState::demo();
    insta::assert_snapshot!("host_120x36", buffer_text(120, 36, &state));
}
