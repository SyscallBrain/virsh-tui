//! P9 networks tests (RED first).

#[test]
fn network_builders() {
    use virsh_tui::command::builders::network;
    let start = network::start("default");
    assert_eq!(start.steps[0].argv, vec!["net-start", "default"]);
    let pin = network::pin_lease("default", "52:54:00:aa:bb:cc", "test", "192.168.122.50");
    assert!(pin.steps[0].argv.contains(&"net-update".to_string()));
    assert!(pin.steps[0].argv.contains(&"--live".to_string()));
    let unpin = network::unpin_lease("default", "52:54:00:aa:bb:cc");
    assert!(unpin.steps[0].argv.contains(&"delete".to_string()));
}

#[test]
fn dhcp_range_auto() {
    use virsh_tui::ui::views::networks::dhcp_range;
    assert_eq!(
        dhcp_range("192.168.122.0/24"),
        Some(("192.168.122.2".to_string(), "192.168.122.254".to_string()))
    );
    assert_eq!(dhcp_range("bogus"), None);
}

#[test]
fn demo_selection() {
    use virsh_tui::ui::views::networks::NetworksState;
    let st = NetworksState::demo();
    assert_eq!(st.networks.len(), 4);
    assert_eq!(st.selected_name(), "default");
    assert_eq!(st.leases.len(), 7);
}
