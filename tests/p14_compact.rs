//! P14 compact snapshots for remaining screens (120x36).

use ratatui::{Terminal, backend::TestBackend};

fn shot(w: u16, h: u16, draw: impl FnOnce(&mut ratatui::Frame)) -> String {
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
fn snapshots_compact() {
    use virsh_tui::{
        theme,
        ui::views::detail::{DetailState, DetailTab, SnapModal, render},
    };
    let th = theme::builtin::tokyo_night();
    let mut state = DetailState::demo("arch-dev");
    state.tab = DetailTab::Snapshots;
    state.snap_modal = SnapModal::Revert;
    insta::assert_snapshot!(
        "snapshots_revert_120x36",
        shot(120, 36, |f| render(f, &th, &state))
    );
}

#[test]
fn wizard_compact() {
    use virsh_tui::{
        theme,
        ui::overlays::wizard::{self, WizardState},
    };
    let th = theme::builtin::tokyo_night();
    let state = WizardState::demo_step3();
    insta::assert_snapshot!("wizard_120x36", shot(120, 36, |f| wizard::render(f, &th, &state)));
}

#[test]
fn palette_compact() {
    use virsh_tui::{theme, ui::overlays};
    let th = theme::builtin::tokyo_night();
    let st = overlays::palette::PaletteState::demo();
    insta::assert_snapshot!(
        "palette_120x36",
        shot(120, 36, |f| overlays::palette::render(f, &th, &st))
    );
}

#[test]
fn help_compact() {
    use virsh_tui::{theme, ui::overlays};
    let th = theme::builtin::tokyo_night();
    insta::assert_snapshot!(
        "help_120x36",
        shot(120, 36, |f| overlays::help::render(
            f,
            &th,
            "",
            &virsh_tui::input::keymap::default_map()
        ))
    );
}

#[test]
fn settings_compact() {
    use virsh_tui::{
        config::Config,
        theme,
        ui::views::settings::{self, SettingsState},
    };
    let th = theme::builtin::tokyo_night();
    let cfg = Config::default();
    let state = SettingsState::open(&cfg);
    insta::assert_snapshot!(
        "settings_120x36",
        shot(120, 36, |f| settings::render(f, &th, &cfg, &state))
    );
}

#[test]
fn events_screens() {
    use virsh_tui::{
        theme,
        ui::views::events::{self, EventsState},
    };
    let th = theme::builtin::tokyo_night();
    let state = EventsState::demo();
    insta::assert_snapshot!("events_174x43", shot(174, 43, |f| events::render(f, &th, &state)));
    insta::assert_snapshot!("events_120x36", shot(120, 36, |f| events::render(f, &th, &state)));
}

#[test]
fn gallery_compact() {
    use virsh_tui::theme;
    let th = theme::builtin::tokyo_night();
    insta::assert_snapshot!(
        "gallery_120x36",
        shot(120, 36, |f| virsh_tui::ui::views::gallery::render(f, &th))
    );
}
