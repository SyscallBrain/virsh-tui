//! P4 execution tests: dry-run formatting and history.

#[test]
fn dry_run_formats_message() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let plan = virsh_tui::command::builders::lifecycle::start("arch-dev");
        let msg = virsh_tui::command::exec::execute("qemu:///system", &plan, true)
            .await
            .unwrap();
        assert!(msg.starts_with("[dry-run] Started arch-dev"));
        assert!(msg.contains("virsh start arch-dev"));
    });
}

#[test]
fn history_push_and_last() {
    let mut h = virsh_tui::command::history::History::new(2);
    h.push("virsh start a");
    h.push("virsh start b");
    h.push("virsh start c");
    assert_eq!(h.all().len(), 2);
    assert_eq!(h.last(), Some("virsh start c"));
}
