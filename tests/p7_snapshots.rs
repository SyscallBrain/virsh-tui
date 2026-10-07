//! P7 hardware snapshot: INSERT on max vCPUs with pending diff.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::views::detail::{DetailState, DetailTab, render},
};

#[test]
fn hardware_insert_max() {
    let th = theme::builtin::tokyo_night();
    let mut state = DetailState::demo("arch-dev");
    state.tab = DetailTab::Hardware;
    // Focus the max field, type 16 like the mockup (INSERT, topology auto-adjusted).
    state.hw.selected = 1;
    if let Some(form) = state.hw.forms.get_mut("cpus") {
        form.focus = 1;
    }
    state.hw.set_field("cpus", "max", "16");
    state.hw_insert = true;
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
    insta::assert_snapshot!("hardware_insert_max", text);
}

#[test]
fn hardware_compact() {
    use virsh_tui::ui::views::detail::{DetailState, DetailTab, render};
    let th = virsh_tui::theme::builtin::tokyo_night();
    let mut state = DetailState::demo("arch-dev");
    state.tab = DetailTab::Hardware;
    let backend = TestBackend::new(120, 36);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render(f, &th, &state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..36 {
        for x in 0..120 {
            text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        text.push('\n');
    }
    insta::assert_snapshot!("hardware_120x36", text);
}
