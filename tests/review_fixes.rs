use virsh_tui::{backend::virsh::parse_list::parse_snapshot_list, command::builders::hardware, xml::edit};

#[test]
fn snapshot_list_keeps_root_without_parent_column() {
    let rows = parse_snapshot_list(
        " Name  Creation Time  State  Parent\n----------------------------------\n root  2026-10-06  14:38:00  +0000  running\n child  2026-10-06  14:39:00  +0000  running  root\n",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].parent, None);
    assert_eq!(rows[1].parent.as_deref(), Some("root"));
}

#[test]
fn edited_xml_preserves_escaped_values() {
    let xml = "<domain><description>A &amp; B</description><cpu mode='host-model' custom='a &quot;b&quot; &amp; c'/></domain>";
    let output = edit::set_cpu_mode(xml, "host-model").unwrap();
    let parsed = xmltree::Element::parse(output.as_bytes()).unwrap();
    assert_eq!(
        parsed.get_child("description").unwrap().get_text().unwrap(),
        "A & B"
    );
    assert_eq!(
        parsed.get_child("cpu").unwrap().attributes["custom"],
        "a \"b\" & c"
    );
}

#[test]
fn live_only_memory_plan_does_not_change_maximum() {
    let plans = hardware::set_memory("vm", "1G", None, true, false);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].steps[0].argv[0], "setmem");
}

#[test]
fn clone_plan_names_the_connection() {
    let plan = virsh_tui::command::builders::lifecycle::clone_on("qemu+ssh://host/system", "vm", "vm-copy");
    assert_eq!(&plan.steps[0].argv[..2], ["--connect", "qemu+ssh://host/system"]);
}

#[test]
fn remote_host_updates_do_not_read_local_memory() {
    let mut host = virsh_tui::ui::views::host::HostState {
        mem_total_gib: 123.0,
        ..Default::default()
    };
    let mut dash = virsh_tui::ui::views::dashboard::DashboardState::from_summaries(&[]);
    dash.uri = "qemu+ssh://host/system".into();
    host.apply_live(&dash);
    assert_eq!(host.mem_total_gib, 123.0);
}
