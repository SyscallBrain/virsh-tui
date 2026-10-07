//! P12 palette + help snapshots.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{theme, ui::overlays};

fn buffer_text(w: u16, h: u16, draw: impl FnOnce(&mut ratatui::Frame)) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(draw).unwrap();
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
fn palette_snap() {
    let th = theme::builtin::tokyo_night();
    let st = overlays::palette::PaletteState::demo();
    insta::assert_snapshot!(
        "palette_snap",
        buffer_text(174, 43, |f| overlays::palette::render(f, &th, &st))
    );
}

#[test]
fn help_overlay() {
    let th = theme::builtin::tokyo_night();
    insta::assert_snapshot!(
        "help_overlay",
        buffer_text(174, 43, |f| overlays::help::render(
            f,
            &th,
            "",
            &virsh_tui::input::keymap::default_map()
        ))
    );
}
