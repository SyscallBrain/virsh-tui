//! P6 detail snapshots: one per tab at reference size.

use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::views::detail::{DetailState, DetailTab, render},
};

fn buffer_text_sized(tab: DetailTab, w: u16, h: u16) -> String {
    let th = theme::builtin::tokyo_night();
    let mut state = DetailState::demo("arch-dev");
    state.tab = tab;
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render(f, &th, &state)).unwrap();
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

fn buffer_text(tab: DetailTab) -> String {
    buffer_text_sized(tab, 174, 43)
}

#[test]
fn detail_overview_compact() {
    insta::assert_snapshot!(
        "detail_overview_120x36",
        buffer_text_sized(DetailTab::Overview, 120, 36)
    );
}

#[test]
fn detail_overview() {
    insta::assert_snapshot!("detail_overview", buffer_text(DetailTab::Overview));
}

#[test]
fn detail_monitor() {
    insta::assert_snapshot!("detail_monitor", buffer_text(DetailTab::Monitor));
}

#[test]
fn detail_console() {
    insta::assert_snapshot!("detail_console", buffer_text(DetailTab::Console));
}

#[test]
fn detail_xml() {
    insta::assert_snapshot!("detail_xml", buffer_text(DetailTab::Xml));
}
