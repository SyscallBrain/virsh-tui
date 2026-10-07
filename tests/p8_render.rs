//! P8 snapshots tab snapshot with the revert CONFIRM modal open.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::views::detail::{DetailState, DetailTab, SnapModal, render},
};

#[test]
fn snapshots_revert_confirm() {
    let th = theme::builtin::tokyo_night();
    let mut state = DetailState::demo("arch-dev");
    state.tab = DetailTab::Snapshots;
    state.snap_modal = SnapModal::Revert;
    let backend = TestBackend::new(174, 43);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render(f, &th, &state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..43 {
        for x in 0..174 {
            text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        text.push('\n');
    }
    insta::assert_snapshot!("snapshots_revert_confirm", text);
}
