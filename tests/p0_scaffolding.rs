//! P0 scaffolding tests (RED first): theme tokens and config defaults.

#[test]
fn tokyo_night_bg_matches_spec() {
    assert_eq!(virsh_tui::theme::builtin::tokyo_night().bg_hex(), "#1a1b26");
}

#[test]
fn nine_builtin_themes_exist() {
    assert_eq!(virsh_tui::theme::builtin::all().len(), 9);
}

#[test]
fn config_default_uri_is_system() {
    let cfg = virsh_tui::config::Config::default();
    assert_eq!(cfg.general.default_uri, "qemu:///system");
}
