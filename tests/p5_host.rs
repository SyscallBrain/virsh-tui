//! P5 host monitor tests (RED first).

#[test]
fn parse_proc_stat_threads() {
    let text = std::fs::read_to_string("tests/fixtures/host/proc_stat.txt").unwrap();
    let cpus = virsh_tui::backend::virsh::host_local::parse_proc_stat(&text);
    assert_eq!(cpus.len(), 6);
    assert!(cpus[0].total > 0);
}

#[test]
fn parse_meminfo_total() {
    let text = std::fs::read_to_string("tests/fixtures/host/meminfo.txt").unwrap();
    let mem = virsh_tui::backend::virsh::host_local::parse_meminfo(&text);
    assert!(mem.total_kib > 0);
    assert!(mem.available_kib > 0);
}

#[test]
fn engine_digit_flush_switches_view() {
    use virsh_tui::input::{engine::KeyEngine, key::KeySeq};
    let mut eng = KeyEngine::new();
    assert!(eng.feed(&KeySeq::parse("2")).is_empty());
    let flushed = eng.flush();
    assert_eq!(flushed.len(), 1);
    assert_eq!(flushed[0].action, virsh_tui::input::engine::KeyAction::ViewHost);
}

#[test]
fn engine_multidigit_count_still_works() {
    use virsh_tui::input::{engine::KeyEngine, key::KeySeq};
    let mut eng = KeyEngine::new();
    assert!(eng.feed(&KeySeq::parse("1")).is_empty());
    assert!(eng.feed(&KeySeq::parse("5")).is_empty());
    let a = eng.feed(&KeySeq::parse("j"));
    assert_eq!(a[0].count, 15);
}

#[test]
fn host_demo_has_9_running() {
    let state = virsh_tui::ui::views::host::HostState::demo();
    assert_eq!(state.domains.len(), 9);
    assert_eq!(state.threads_total, 32);
}
