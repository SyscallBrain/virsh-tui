//! P10 storage tests (RED first).

#[test]
fn storage_builders() {
    use virsh_tui::command::builders::storage;
    let create = storage::vol_create("default", "test.qcow2", "10G", "qcow2", None);
    assert!(create.steps[0].argv.contains(&"vol-create-as".to_string()));
    let resize = storage::vol_resize("default", "test.qcow2", "20G", false);
    assert!(resize.steps[0].argv.contains(&"vol-resize".to_string()));
    let wipe = storage::vol_wipe("default", "test.qcow2", "zero");
    assert!(wipe.steps[0].argv.contains(&"vol-wipe".to_string()));
    let pool = storage::pool_start("default");
    assert_eq!(pool.steps[0].argv, vec!["pool-start", "default"]);
}

#[test]
fn picker_filters_isos() {
    use virsh_tui::ui::views::storage::StorageState;
    let mut st = StorageState::demo_picker();
    assert_eq!(st.picker_isos().len(), 3);
    st.picker.as_mut().unwrap().query = String::from("09.01");
    assert_eq!(st.picker_isos().len(), 1);
}

#[test]
fn used_by_rules() {
    use virsh_tui::ui::views::storage::used_by;
    assert_eq!(
        used_by(
            "/var/lib/libvirt/images/arch-dev.qcow2",
            &[("arch-dev", "/var/lib/libvirt/images/arch-dev.qcow2")]
        ),
        "arch-dev"
    );
    assert_eq!(used_by("/orphan.qcow2", &[]), "⚠ orphan");
}
