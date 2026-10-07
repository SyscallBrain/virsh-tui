//! P11 wizard snapshot at step 3 (CPU & Memory).

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::overlays::wizard::{self, WizardState},
};

#[test]
fn wizard_step3() {
    let th = theme::builtin::tokyo_night();
    let state = WizardState::demo_step3();
    let backend = TestBackend::new(174, 43);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| wizard::render(f, &th, &state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..43 {
        for x in 0..174 {
            text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        text.push('\n');
    }
    insta::assert_snapshot!("wizard_step3", text);
}
