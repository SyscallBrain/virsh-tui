//! Regression tests for bugs found in the code review (2026-10-06).

use virsh_tui::model::{DomainState, DomainSummary};
use virsh_tui::ui::views::dashboard::{DashboardState, PendingShutdown, Sort};

fn summary(name: &str, state: DomainState) -> DomainSummary {
    DomainSummary {
        name: name.to_string(),
        uuid: String::new(),
        id: None,
        state,
        autostart: false,
        persistent: true,
        vcpus: 1,
        max_vcpus: 1,
        mem_kib: 1024 * 1024,
        max_mem_kib: 1024 * 1024,
        has_managed_save: false,
    }
}

/// The selection used to be restored with the raw `virsh list` index while the
/// table shows a sorted/filtered list, so every refresh could move it to
/// another domain (and `S`/`D` acted on the wrong VM).
#[test]
fn selection_survives_refresh_by_name() {
    let list = [
        summary("zeta", DomainState::ShutOff),
        summary("alpha", DomainState::ShutOff),
        summary("mid", DomainState::Running),
    ];
    let mut dash = DashboardState::from_summaries(&list);
    dash.sort = Sort::Name;
    dash.select_name("zeta");
    assert_eq!(dash.selected().name, "zeta");
    // Same domains, different raw order (as virsh may return them).
    let reordered = [list[2].clone(), list[0].clone(), list[1].clone()];
    dash.apply_summaries(&reordered);
    assert_eq!(dash.selected().name, "zeta");
    dash.cycle_sort();
    assert_eq!(dash.selected().name, "zeta", "changing the sort keeps the domain");
}

/// `virsh shutdown` only asks the guest (ACPI) and exits 0 immediately: the UI
/// must not report success until the domain is actually off.
#[test]
fn shutdown_is_tracked_until_off_or_timeout() {
    let mut dash = DashboardState::from_summaries(&[summary("vm", DomainState::Running)]);
    dash.track_shutdown("vm");
    dash.message.clear();
    dash.check_shutdowns();
    assert!(dash.message.is_empty(), "still running: no verdict yet");
    assert_eq!(dash.shutdowns.len(), 1);

    // Guest ignored the request for longer than the timeout.
    dash.shutdowns[0] = PendingShutdown {
        name: String::from("vm"),
        since: std::time::Instant::now()
            - std::time::Duration::from_secs(virsh_tui::ui::views::dashboard::SHUTDOWN_TIMEOUT_SECS + 1),
    };
    dash.check_shutdowns();
    assert!(
        dash.message.starts_with("⚠ vm is still running"),
        "{}",
        dash.message
    );
    assert!(dash.shutdowns.is_empty());

    // Guest honoured it.
    dash.track_shutdown("vm");
    dash.apply_summaries(&[summary("vm", DomainState::ShutOff)]);
    dash.check_shutdowns();
    assert_eq!(dash.message, "✓ vm shut off");
}

/// The hardware editor used to start from the demo `arch-dev` forms and the
/// repository fixture XML for *every* domain, so `:w` could redefine the wrong
/// domain. It must be built from the real domain's XML.
#[test]
fn hardware_editor_uses_the_real_domain_xml() {
    use virsh_tui::backend::virsh::parse_xml::parse_domain_xml;
    use virsh_tui::ui::views::detail::hardware::HardwareState;
    let xml = "<domain type='kvm'><name>real-vm</name><uuid>u-1</uuid>\
        <memory unit='KiB'>2097152</memory><currentMemory unit='KiB'>2097152</currentMemory>\
        <vcpu placement='static'>2</vcpu><os><type arch='x86_64' machine='q35'>hvm</type></os>\
        <cpu mode='host-model'/><devices/></domain>";
    let cfg = parse_domain_xml(xml).unwrap();
    let mut hw = HardwareState::from_config(&cfg, xml, false);
    assert_eq!(hw.field_value("cpus", "max"), "2");
    assert_eq!(hw.field_value("memory", "max"), "2G");
    assert!(
        hw.devices.iter().all(|d| !d.name.contains("vnet3")),
        "no demo devices"
    );

    // Changing max vCPUs on a shut-off domain: config-only native commands.
    hw.set_field("cpus", "max", "4");
    let plans = hw.will_run("real-vm");
    let argv: Vec<Vec<String>> = plans
        .iter()
        .flat_map(|p| p.steps.iter().map(|s| s.argv.clone()))
        .collect();
    // Topology auto-adjusts → one define of the edited real XML, no --live steps.
    assert_eq!(argv[0][0], "define");
    assert!(argv.iter().all(|a| !a.contains(&String::from("--live"))));
    let edited = hw.edited_xml();
    assert!(edited.contains("<name>real-vm</name>"));
    assert!(edited.contains(">4</vcpu>"));
    assert!(!edited.contains("arch-dev"));
}

/// Demo mode must never spawn virsh (it used to run real commands against
/// qemu:///system for demo domain names).
#[tokio::test]
async fn demo_mode_never_executes() {
    virsh_tui::command::exec::set_demo(true);
    let plan = virsh_tui::command::builders::lifecycle::destroy("definitely-not-a-real-domain");
    let out = virsh_tui::command::exec::execute("qemu:///nonexistent", &plan, false).await;
    assert!(out.unwrap().starts_with("✓ Destroyed"));
    assert!(
        virsh_tui::backend::virsh::exec::run("qemu:///system", &["list"])
            .await
            .is_err()
    );
    virsh_tui::command::exec::set_demo(false);
}
