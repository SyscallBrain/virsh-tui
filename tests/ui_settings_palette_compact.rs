use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    config::Config,
    theme,
    ui::{overlays::palette, views::settings},
};

fn draw(w: u16, h: u16, render: impl FnOnce(&mut ratatui::Frame)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(render).unwrap();
    (0..h)
        .map(|y| {
            let mut row = (0..w)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>();
            row.push('\n');
            row
        })
        .collect()
}

#[test]
fn compact_settings_shows_all_editable_appearance_values() {
    let cfg = Config::default();
    let th = theme::builtin::tokyo_night();
    let mut st = settings::SettingsState::open(&cfg);
    st.pane = settings::SettingsPane::Options;
    st.opt_idx = 5;
    let text = draw(80, 24, |f| settings::render(f, &th, &cfg, &st));
    for value in [
        "borders",
        "icons",
        "graphs",
        "vertical gradient",
        "transparent",
        "dim behind modals",
        "h/l",
        "Esc",
    ] {
        assert!(text.contains(value), "missing {value}:\n{text}");
    }
    assert!(text.contains("❯ dim behind modals"));
}

#[test]
fn compact_settings_theme_selection_follows_focus() {
    let cfg = Config::default();
    let th = theme::builtin::tokyo_night();
    let mut st = settings::SettingsState::open(&cfg);
    st.themes = (0..40).map(|i| format!("custom-theme-{i:02}")).collect();
    st.theme_idx = 39;
    let text = draw(80, 24, |f| settings::render(f, &th, &cfg, &st));
    assert!(text.contains("❯ custom-theme-39"), "{text}");
}

#[test]
fn compact_palette_reference_has_readable_synopsis_and_command() {
    let th = theme::builtin::tokyo_night();
    let mut st = palette::PaletteState::demo();
    st.pane_focused = true;
    for (w, h) in [(100, 30), (80, 24)] {
        let text = draw(w, h, |f| palette::render(f, &th, &st));
        assert!(text.contains("snapshot-create-as <domain>"), "{text}");
        assert!(text.contains("pre-upgrade-2026-10-06"), "{text}");
        assert!(text.contains("PgUp/PgDn"), "{text}");
    }
}

#[test]
fn compact_settings_connection_selection_follows_focus() {
    let mut cfg = Config::default();
    cfg.connections.uris = (0..40)
        .map(|i| format!("qemu+ssh://host-{i:02}/system"))
        .collect();
    let th = theme::builtin::tokyo_night();
    let mut st = settings::SettingsState::open(&cfg);
    st.cat = 3;
    st.pane = settings::SettingsPane::Options;
    st.opt_idx = 39;
    let text = draw(80, 24, |f| settings::render(f, &th, &cfg, &st));
    assert!(text.contains("❯ qemu+ssh://host-39/system"), "{text}");
}

#[test]
fn compact_palette_scroll_exposes_full_command_and_last_help_option() {
    let th = theme::builtin::tokyo_night();
    let mut st = palette::PaletteState::demo();
    st.pane_focused = true;
    st.staged = format!(
        "virsh snapshot-create-as {} --atomic",
        "very-long-domain-name".repeat(12)
    );
    let mut seen = String::new();
    for offset in 0..30 {
        st.reference_scroll = offset;
        let text = draw(80, 24, |f| palette::render(f, &th, &st));
        seen.push_str(&text);
    }
    assert!(seen.contains("--atomic"));
    assert!(seen.contains("--print-xml"));
    assert!(seen.contains("Creates a snapshot of a domain."));
    st.opt_cursor = 11;
    st.reference_scroll = 0;
    let text = draw(80, 24, |f| palette::render(f, &th, &st));
    assert!(text.contains("❯ [ ] --print-xml"), "{text}");
    st.reference_scroll = -100;
    let text = draw(80, 24, |f| palette::render(f, &th, &st));
    assert!(text.contains("virsh snapshot-create-as"), "{text}");
}

#[test]
fn smallest_palette_keeps_close_and_page_navigation_hints_visible() {
    let th = theme::builtin::tokyo_night();
    let text = draw(40, 12, |frame| {
        palette::render(frame, &th, &palette::PaletteState::demo())
    });
    for hint in ["C-l", "Esc", "Enter", "Tab", "PgUp/PgDn"] {
        assert!(text.contains(hint), "missing {hint}:\n{text}");
    }
}
