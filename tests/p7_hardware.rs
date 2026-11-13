//! P7 hardware editor tests (RED first).

#[test]
fn set_vcpus_round_trip() {
    let xml = std::fs::read_to_string("tests/fixtures/virsh/dumpxml_arch.txt").unwrap();
    let out = virsh_tui::xml::edit::set_vcpus(&xml, 8, 16).unwrap();
    assert!(out.contains(">16<"));
    let cfg = virsh_tui::backend::virsh::parse_xml::parse_domain_xml(&out).unwrap();
    assert_eq!(cfg.vcpus, 16);
}

#[test]
fn topology_auto_adjusts_to_max() {
    let xml = std::fs::read_to_string("tests/fixtures/virsh/dumpxml_arch.txt").unwrap();
    let out = virsh_tui::xml::edit::set_topology(&xml, 1, 8, 2).unwrap();
    assert!(out.contains("cores=\"8\""));
}

#[test]
fn diff_shows_minus_plus() {
    let diff = virsh_tui::xml::diff::diff_lines("<a>1</a>\n", "<a>2</a>\n");
    assert!(
        diff.iter()
            .any(|l| l.kind == virsh_tui::xml::diff::DiffKind::Minus)
    );
    assert!(
        diff.iter()
            .any(|l| l.kind == virsh_tui::xml::diff::DiffKind::Plus)
    );
}

#[test]
fn hardware_builders_native_first() {
    use virsh_tui::command::builders::hardware;
    let plans = hardware::set_vcpus("arch-dev", 8, 16, true, true, true);
    let argv: Vec<Vec<String>> = plans
        .iter()
        .flat_map(|p| p.steps.iter().map(|s| s.argv.clone()))
        .collect();
    assert!(argv.iter().any(|a| a.contains(&"--maximum".to_string())));
    assert!(argv.iter().any(|a| a.contains(&"--live".to_string())));
    let mem = hardware::set_memory("d", "8G", Some("16G"), true, true);
    assert!(!mem.is_empty());
}

#[test]
fn size_field_parses_units() {
    use virsh_tui::ui::widgets::form::parse_size_mib;
    assert_eq!(parse_size_mib("8G"), Some(8192));
    assert_eq!(parse_size_mib("512M"), Some(512));
    assert_eq!(parse_size_mib("1.5T"), Some(1572864));
    assert_eq!(parse_size_mib("bogus"), None);
}

#[test]
fn pending_tracks_and_undoes() {
    use virsh_tui::ui::views::detail::hardware::HardwareState;
    let mut st = HardwareState::demo();
    assert_eq!(st.pending_count(), 0);
    st.set_field("cpus", "max", "16");
    // vcpu section + auto-adjusted cpu/topology section.
    assert_eq!(st.pending_count(), 2);
    st.undo_field("cpus", "max");
    assert_eq!(st.pending_count(), 0);
}

#[test]
fn draft_identity_tracks_edits_but_not_navigation() {
    use virsh_tui::ui::views::detail::hardware::HardwareState;
    let mut state = HardwareState::demo();
    let before = state.draft_fingerprint();
    state.selected = 1;
    assert_eq!(before, state.draft_fingerprint());
    assert_eq!(before, state.clone().draft_fingerprint());
    state.set_field("cpus", "max", "16");
    assert_ne!(before, state.draft_fingerprint());
}

#[test]
fn rebase_preserves_pending_values_and_updates_successful_fields() {
    use virsh_tui::{
        backend::virsh::parse_xml::parse_domain_xml, ui::views::detail::hardware::HardwareState,
    };
    let xml = std::fs::read_to_string("tests/fixtures/virsh/dumpxml_arch.txt").unwrap();
    let config = parse_domain_xml(&xml).unwrap();
    let mut state = HardwareState::from_config(&config, &xml, true);
    state.set_field("cpus", "max", "16");
    state.set_field("memory", "max", "20G");
    let updated = virsh_tui::xml::edit::set_vcpus(&xml, config.current_vcpus, 16).unwrap();
    let config = parse_domain_xml(&updated).unwrap();
    let rebased = state.rebase(&config, &updated, true).unwrap();
    let cpus = &rebased.forms["cpus"].fields;
    let max = cpus.iter().find(|field| field.id == "max").unwrap();
    assert_eq!(max.current, "16");
    assert_eq!(max.original, "16");
    let memory = rebased.forms["memory"]
        .fields
        .iter()
        .find(|field| field.id == "max")
        .unwrap();
    assert!(memory.dirty());
    assert_eq!(memory.current, "20G");
}
