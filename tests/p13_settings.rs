//! P13 settings/themes tests (RED first).

#[test]
fn custom_theme_loads() {
    let toml = "[tokens]\nbg = \"#000000\"\nbg2 = \"#111111\"\nhl = \"#222222\"\nsel = \"#333333\"\ngutter = \"#444444\"\ncomment = \"#555555\"\nfg = \"#ffffff\"\nfg2 = \"#eeeeee\"\nblue = \"#0000ff\"\ncyan = \"#00ffff\"\nmagenta = \"#ff00ff\"\ngreen = \"#00ff00\"\nyellow = \"#ffff00\"\norange = \"#ff8800\"\nred = \"#ff0000\"\nteal = \"#00ff88\"\n";
    let theme = virsh_tui::theme::load::from_toml("custom", toml).unwrap();
    assert_eq!(theme.name, "custom");
}

#[test]
fn custom_theme_missing_key_errors() {
    let err = virsh_tui::theme::load::from_toml("bad", "[tokens]\nbg = \"#000000\"\n").unwrap_err();
    assert!(err.to_string().contains("bg2"));
}

#[test]
fn config_round_trip_new_fields() {
    let mut cfg = virsh_tui::config::Config::default();
    cfg.appearance.theme = String::from("dracula");
    cfg.general.refresh_interval_secs = 2;
    let text = toml::to_string_pretty(&cfg).unwrap();
    let back: virsh_tui::config::Config = toml::from_str(&text).unwrap();
    assert_eq!(back.appearance.theme, "dracula");
    assert_eq!(back.general.refresh_interval_secs, 2);
}

#[test]
fn icon_sets() {
    use virsh_tui::{model::DomainState, theme::IconSet};
    assert_eq!(DomainState::Running.glyph_in(IconSet::Unicode), '●');
    assert_eq!(DomainState::Running.glyph_in(IconSet::Ascii), '*');
    assert_eq!(DomainState::ShutOff.glyph_in(IconSet::Ascii), 'o');
}
