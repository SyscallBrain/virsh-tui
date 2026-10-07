//! P6 domain detail tests (RED first).

#[test]
fn detail_tabs_cycle() {
    use virsh_tui::ui::views::detail::{DetailState, DetailTab};
    let mut st = DetailState::demo("arch-dev");
    assert_eq!(st.tab, DetailTab::Overview);
    st.next_tab();
    assert_eq!(st.tab, DetailTab::Monitor);
    st.prev_tab();
    assert_eq!(st.tab, DetailTab::Overview);
}

#[test]
fn overview_parses_disks_and_nics() {
    let text = std::fs::read_to_string("tests/fixtures/virsh/detail_arch.xml").unwrap();
    let cfg = virsh_tui::backend::virsh::parse_xml::parse_domain_xml(&text).unwrap();
    assert!(!cfg.disks.is_empty());
    assert_eq!(cfg.disks[0].target, "vda");
}

#[test]
fn xml_fold_toggles() {
    use virsh_tui::ui::views::detail::XmlFold;
    let mut fold = XmlFold::new(10);
    assert!(!fold.is_folded(3));
    fold.toggle(3);
    assert!(fold.is_folded(3));
}

#[test]
fn edit_loop_prefixes_error_comment() {
    let out = virsh_tui::ui::views::detail::edit_error_comment("bad xml", "error: line 1");
    assert!(out.starts_with("<!-- virsh-tui edit error:"));
    assert!(out.contains("bad xml"));
}
