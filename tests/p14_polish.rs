//! P14 polish tests (RED first).

#[test]
fn breakpoints() {
    use virsh_tui::ui::layout::{Breakpoint, breakpoint};
    assert_eq!(breakpoint(174, 43), Breakpoint::Wide);
    assert_eq!(breakpoint(120, 36), Breakpoint::Compact);
    assert_eq!(breakpoint(80, 24), Breakpoint::TooSmall);
}

#[test]
fn xterm_maps_primary_colors() {
    use ratatui::style::Color;
    use virsh_tui::theme::xterm256;
    assert_eq!(xterm256(Color::Rgb(0, 0, 0)), 16);
    assert_eq!(xterm256(Color::Rgb(255, 255, 255)), 231);
    assert_eq!(xterm256(Color::Rgb(255, 0, 0)), 196);
}

#[test]
fn backoff_grows_then_caps() {
    use virsh_tui::app::backoff::delay_secs;
    assert_eq!(delay_secs(0), 1);
    assert_eq!(delay_secs(1), 2);
    assert_eq!(delay_secs(10), 30);
}
