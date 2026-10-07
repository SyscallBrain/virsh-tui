//! P11 wizard tests (RED first).

#[test]
fn name_validation() {
    use virsh_tui::ui::overlays::wizard::valid_name;
    assert!(valid_name("fedora-42-ws", &[]));
    assert!(!valid_name("bad name!", &[]));
    assert!(!valid_name("arch-dev", &["arch-dev"]));
}

#[test]
fn osinfo_autodetect() {
    use virsh_tui::ui::overlays::wizard::detect_os;
    assert_eq!(detect_os("Fedora-WS-Live-42.iso"), Some("fedora42".to_string()));
    assert_eq!(detect_os("random.bin"), None);
}

#[test]
fn virt_install_command() {
    use virsh_tui::command::builders::install::virt_install;
    use virsh_tui::ui::overlays::wizard::WizardState;
    let st = WizardState::demo_step3();
    let plan = virt_install(&st);
    let argv = plan.steps[0].argv.join(" ");
    assert!(argv.contains("--name=fedora-42-ws"));
    assert!(argv.contains("--vcpus"));
    assert!(argv.contains("--noautoconsole"));
}

#[test]
fn draft_round_trip() {
    use virsh_tui::ui::overlays::wizard::WizardState;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.toml");
    let mut st = WizardState::demo_step3();
    st.name = String::from("draft-vm");
    st.save_draft(&path).unwrap();
    let loaded = WizardState::load_draft(&path).unwrap();
    assert_eq!(loaded.name, "draft-vm");
}

/// Regression: names could not contain h/l/y (they were shortcuts) and
/// Backspace did nothing in the name field.
#[test]
fn name_field_types_every_letter_and_backspace() {
    use virsh_tui::ui::overlays::wizard::{WField, WizardState};
    let mut st = WizardState::default();
    assert_eq!(st.focused(), WField::Name);
    for c in "my-hilly-vmx".chars() {
        assert!(st.type_char(c));
    }
    st.backspace();
    assert_eq!(st.name, "my-hilly-vm");
    assert!(st.type_char(' '), "spaces are swallowed in names");
    assert_eq!(st.name, "my-hilly-vm");
}

/// Regression: a fresh wizard had 0 GiB / 0 vCPU and virt-install failed.
#[test]
fn fresh_wizard_has_sane_defaults_and_validation() {
    use virsh_tui::ui::overlays::wizard::WizardState;
    let st = WizardState::default();
    assert!(st.vcpus >= 1 && st.mem_gib >= 1);
    let problems = st.problems();
    assert!(problems.iter().any(|(s, _)| *s == 0), "name required");
    assert!(problems.iter().any(|(s, _)| *s == 1), "ISO required");
    let st = WizardState {
        name: String::from("vm"),
        iso_path: String::from("/isos/a.iso"),
        ..Default::default()
    };
    assert!(st.problems().is_empty());
}
