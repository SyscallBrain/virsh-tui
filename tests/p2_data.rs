//! P2 data layer tests (RED first).

#[test]
fn parse_list_all_fixture() {
    let text = std::fs::read_to_string("tests/fixtures/virsh/list_all.txt").unwrap();
    let rows = virsh_tui::backend::virsh::parse_list::parse_list_all(&text).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].name, "archlinux-install");
}

#[test]
fn parse_domstats_raw_fixture() {
    let text = std::fs::read_to_string("tests/fixtures/virsh/domstats_raw.txt").unwrap();
    let stats = virsh_tui::backend::virsh::parse_domstats::parse_domstats_raw(&text).unwrap();
    assert_eq!(stats.len(), 3);
    assert_eq!(stats[0].domain, "metasploitable2");
}

#[test]
fn parse_dumpxml_arch() {
    let text = std::fs::read_to_string("tests/fixtures/virsh/dumpxml_arch.txt").unwrap();
    let cfg = virsh_tui::backend::virsh::parse_xml::parse_domain_xml(&text).unwrap();
    assert_eq!(cfg.name, "archlinux-install");
    assert!(cfg.vcpus >= 1);
}

#[test]
fn ring_buffer_push_and_window() {
    let mut ring = virsh_tui::metrics::ring::Ring::new(60);
    for i in 0..70 {
        ring.push(i as f32);
    }
    assert_eq!(ring.len(), 60);
    assert_eq!(ring.last(), Some(69.0));
}

#[test]
fn sampler_cpu_pct() {
    let pct = virsh_tui::metrics::sampler::cpu_pct(1_000_000_000, 2_000_000_000, 1_000_000_000, 2);
    assert!((pct - 50.0).abs() < 1e-6);
}

#[test]
fn demo_backend_has_15_domains() {
    use virsh_tui::backend::Backend;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let backend = virsh_tui::backend::demo::DemoBackend::new();
        let domains = backend.list_domains().await.unwrap();
        assert_eq!(domains.len(), 15);
    });
}
