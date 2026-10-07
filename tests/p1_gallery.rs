//! P1 gallery snapshots: one per widget group.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::widgets::charts::{self, BrailleMode},
};

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
fn gallery_174x43() {
    let th = theme::builtin::tokyo_night();
    let text = buffer_text(174, 43, |f| virsh_tui::ui::views::gallery::render(f, &th));
    insta::assert_snapshot!("gallery_174x43", text);
}

#[test]
fn widget_strings_match_fixtures() {
    let cpu = charts::series(7, 90, 0.42, 0.22, 0.1);
    let braille_rows = charts::braille(&cpu, 60, 8, BrailleMode::Fill);
    insta::assert_snapshot!("braille_fill_60x8", braille_rows.join("\n"));
    let mem = charts::series(11, 60, 0.74, 0.06, 0.2);
    insta::assert_snapshot!("spark_mem_28x3", charts::spark_rows(&mem, 28, 3).join("\n"));
    let (fill, rest) = charts::hbar(0.42, 16);
    insta::assert_snapshot!("hbar_042", format!("{fill}{rest}"));
    let (on, off) = charts::lg(0.38, 22);
    insta::assert_snapshot!("lg_038", format!("{on}{off}"));
}
