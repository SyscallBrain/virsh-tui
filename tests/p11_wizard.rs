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
    assert_eq!(
        detect_os("/isos/Fedora-Server-dvd-x86_64-41-1.4.iso"),
        Some("fedora41".to_string())
    );
    assert_eq!(detect_os("Archcraft-Linux-2026.08-desktop.iso"), None);
    assert_eq!(
        detect_os("ubuntu-24.04.1-live-server-amd64.iso"),
        Some("ubuntu24.04".to_string())
    );
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

/// Regression: without /dev/kvm, libvirt rejects `--cpu host-passthrough`
/// ("CPU mode 'host-passthrough' … is not supported by hypervisor").
#[test]
fn without_kvm_the_wizard_avoids_host_passthrough() {
    use virsh_tui::ui::overlays::wizard::{HostFacts, WizardState};
    let mut st = WizardState {
        name: String::from("vm"),
        iso_path: String::from("/isos/a.iso"),
        cpu_model: String::from("host-passthrough"),
        host: HostFacts {
            no_kvm: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let problems = st.problems();
    assert!(
        problems.iter().any(|(s, m)| *s == 2 && m.contains("KVM")),
        "host-passthrough without KVM must block creation: {problems:?}"
    );
    st.adapt_to_host();
    assert_eq!(st.cpu_model, "host-model");
    assert!(st.problems().is_empty());

    // With KVM the default stays host-passthrough.
    let mut st = WizardState::default();
    st.adapt_to_host();
    assert_eq!(st.cpu_model, "host-passthrough");
}

/// Regression: virt-install prints WARNING lines before the real ERROR, and
/// the job summary showed the warning instead of the cause.
#[test]
fn stderr_summary_prefers_the_error_line() {
    use virsh_tui::command::job::stderr_summary;
    let stderr = "WARNING  KVM acceleration not available, using 'qemu'\n\
                  WARNING  Using --osinfo generic\n\
                  ERROR    unsupported configuration: CPU mode 'host-passthrough' is not supported\n\
                  Domain installation does not appear to have been successful.\n";
    assert_eq!(
        stderr_summary(stderr),
        "unsupported configuration: CPU mode 'host-passthrough' is not supported"
    );
    // virsh style: the first error line is generic, the next ones carry the
    // cause (issue #5): keep them all, on one line.
    assert_eq!(
        stderr_summary("error: failed to get domain 'x'\nerror: Domain not found\n"),
        "error: failed to get domain 'x': Domain not found"
    );
    assert_eq!(
        stderr_summary(
            "error: Failed to define domain from /tmp/vt-q2.xml\n\
             error: at line 59: Extra content at the end of the document\n\
             <domain type=\"kvm\">\n\
             ^\n"
        ),
        "error: Failed to define domain from /tmp/vt-q2.xml: at line 59: Extra content at the end of the document"
    );
    // No error marker: fall back to the first non-empty line.
    assert_eq!(stderr_summary("\nsomething broke\n"), "something broke");
    assert_eq!(stderr_summary(""), "unknown error");
}

/// ⏎ on the ISO path opens a picker over the pool and folder ISOs; choosing
/// fills the field, and the browser remembers its folder.
#[test]
fn iso_picker_fills_the_iso_path() {
    use virsh_tui::ui::overlays::iso_picker::{iso_dirs, merge, scan_dir};
    use virsh_tui::ui::overlays::wizard::{HostFacts, WField, WizardState};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("archlinux-2026.11.01-x86_64.iso"), b"x").unwrap();
    std::fs::write(dir.path().join("Fedora-42.iso"), b"x").unwrap();
    let dirs = iso_dirs(&[dir.path().display().to_string()]);
    let mut st = WizardState {
        step: 1,
        focus: 1,
        host: HostFacts {
            isos: merge(vec![], dirs.iter().flat_map(|d| scan_dir(d)).collect()),
            local: true,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(st.focused(), WField::Iso);
    assert!(st.hint(WField::Iso).contains("2 ISOs"));
    st.open_iso_picker();
    for c in "fedora".chars() {
        st.picker.as_mut().unwrap().type_char(c);
    }
    st.picker_enter();
    assert!(st.picker.is_none(), "choosing closes the picker");
    assert_eq!(
        st.iso_path,
        dir.path().join("Fedora-42.iso").display().to_string()
    );

    // Browser: start in the configured folder, choose, remember it.
    st.open_iso_picker();
    let start = st.browse_start(&[]);
    assert_eq!(start, dir.path(), "starts next to the current ISO");
    st.picker.as_mut().unwrap().toggle_browse(start);
    st.picker.as_mut().unwrap().query = String::from("arch");
    st.picker.as_mut().unwrap().selected = 1; // `..` stays first
    st.picker_enter();
    assert!(st.iso_path.ends_with("archlinux-2026.11.01-x86_64.iso"));
    assert_eq!(st.last_browse_dir, dir.path().display().to_string());
    assert!(
        !st.content_snapshot().contains("last_browse_dir = \"/"),
        "the browse folder is not draft content"
    );
}
