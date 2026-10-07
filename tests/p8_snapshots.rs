//! P8 snapshots tests (RED first).

#[test]
fn tree_builds_prefixes() {
    use virsh_tui::model::Snapshot;
    use virsh_tui::ui::views::detail::snapshots::build_tree;
    let snaps = vec![
        Snapshot {
            name: String::from("base"),
            parent: None,
            state: String::from("shutoff"),
        },
        Snapshot {
            name: String::from("child"),
            parent: Some(String::from("base")),
            state: String::from("shutoff"),
        },
    ];
    let rows = build_tree(&snaps, None);
    assert_eq!(rows.len(), 2);
    assert!(rows[1].prefix.contains("─"));
}

#[test]
fn snapshot_builders() {
    use virsh_tui::command::builders::snapshot;
    let create = snapshot::create("d", "s1", "desc", true, false);
    assert!(create.steps[0].argv.contains(&"--atomic".to_string()));
    let revert = snapshot::revert("d", "s1", Some("running"), false, true);
    assert_eq!(revert.steps.len(), 2);
    assert!(revert.steps[1].argv.contains(&"--running".to_string()));
    let del = snapshot::delete("d", "s1", true, false, false);
    assert!(del.steps[0].argv.contains(&"--children".to_string()));
}

#[test]
fn demo_has_7_with_current() {
    use virsh_tui::ui::views::detail::snapshots::SnapshotState;
    let st = SnapshotState::demo();
    assert_eq!(st.rows.len(), 7);
    assert_eq!(st.current.as_deref(), Some("pre-kernel-6.17"));
    assert_eq!(st.selected_name(), "pre-kernel-6.16");
}
