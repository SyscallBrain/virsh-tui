//! P3 dashboard state tests (RED first): sort, filter, chips, marks, selection.

#[test]
fn sort_default_is_state_then_name() {
    let state = virsh_tui::ui::views::dashboard::DashboardState::demo();
    let names: Vec<&str> = state.visible_rows().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names[0], "alpine-edge");
    assert_eq!(names[1], "arch-dev");
    assert_eq!(names[9], "nixos-lab");
    assert_eq!(names[10], "ubuntu-24-ci");
    assert_eq!(names[11], "debian-bookworm");
}

#[test]
fn chip_filter_running_shows_9() {
    let mut state = virsh_tui::ui::views::dashboard::DashboardState::demo();
    state.cycle_chip();
    assert_eq!(state.chip, virsh_tui::ui::views::dashboard::Chip::Running);
    assert_eq!(state.visible_rows().len(), 9);
}

#[test]
fn text_filter_matches_name() {
    let mut state = virsh_tui::ui::views::dashboard::DashboardState::demo();
    state.set_filter("k8s");
    assert_eq!(state.visible_rows().len(), 3);
}

#[test]
fn marks_default_three_k8s() {
    let state = virsh_tui::ui::views::dashboard::DashboardState::demo();
    assert_eq!(state.marks.len(), 3);
}

#[test]
fn empty_rows_never_panics() {
    use virsh_tui::ui::views::dashboard::DashboardState;
    let mut state = DashboardState::demo();
    state.rows.clear();
    assert_eq!(state.visible_rows().len(), 0);
    assert_eq!(state.selected().name, "");
}

#[test]
fn sort_cycles_through_columns() {
    let mut state = virsh_tui::ui::views::dashboard::DashboardState::demo();
    let first = state.sort;
    state.cycle_sort();
    assert_ne!(state.sort, first);
}
