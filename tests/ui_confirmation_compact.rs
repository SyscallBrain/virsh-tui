use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::views::detail::{self, DetailState, DetailTab, SnapModal},
};

fn shot(w: u16, h: u16, state: &DetailState) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|f| detail::render(f, &theme::builtin::tokyo_night(), state))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..h)
        .map(|y| (0..w).map(|x| buffer[(x, y)].symbol()).collect::<String>() + "\n")
        .collect()
}

#[test]
fn snapshot_confirmations_remain_visible_in_minimal_layout() {
    for (w, h) in [(80, 24), (40, 12)] {
        for (modal, title, action) in [
            (SnapModal::Revert, "Revert snapshot", "y revert"),
            (SnapModal::Delete, "Delete snapshot", "y delete"),
        ] {
            let mut state = DetailState::demo("arch-dev");
            state.tab = DetailTab::Snapshots;
            state.snap_modal = modal;
            let text = shot(w, h, &state);
            assert!(text.contains(title), "{w}x{h}: {text}");
            assert!(text.contains(action), "{w}x{h}: {text}");
            assert!(text.contains("cancel"), "{w}x{h}: {text}");
            assert!(text.contains("arch-dev"), "target must be visible: {text}");
        }
    }
}

#[test]
fn every_bulk_command_and_long_path_can_be_reviewed_with_fixed_actions() {
    use virsh_tui::ui::overlays::confirm::{self, Confirm, ConfirmKind};
    let commands: Vec<String> = (0..24)
        .map(|i| format!("virsh destroy vt-test-guest-{i:02}-{}", "long-name-".repeat(8)))
        .collect();
    let state = Confirm {
        kind: ConfirmKind::Always,
        title: "Destroy domains".into(),
        question: "Destroy 24 domains?".into(),
        commands,
        confirm_label: "y destroy".into(),
        type_name: None,
        typed: String::new(),
    };
    for (w, h) in [(174, 43), (80, 24), (40, 12)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut max = 0;
        terminal
            .draw(|f| {
                max =
                    confirm::render_scrolled(f, &theme::builtin::tokyo_night(), f.area(), &state, usize::MAX);
            })
            .unwrap();
        let mut seen = String::new();
        for scroll in 0..=max {
            terminal
                .draw(|f| {
                    confirm::render_scrolled(f, &theme::builtin::tokyo_night(), f.area(), &state, scroll);
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            let text: String = (0..h)
                .map(|y| (0..w).map(|x| buffer[(x, y)].symbol()).collect::<String>() + "\n")
                .collect();
            assert!(text.contains("y destroy") && text.contains("cancel"), "{text}");
            seen.push_str(&text);
        }
        assert!(
            seen.contains("vt-test-guest-23"),
            "last bulk target is unavailable at {w}x{h}"
        );
        assert!(max > 0);
    }
}

#[test]
fn wrapping_retains_spaces_and_unicode_command_values() {
    use ratatui::text::Line;
    let command = "virsh define '/tmp/虚拟机 e\u{301}  name.xml'";
    let rows = virsh_tui::ui::widgets::review::wrap_lines(vec![Line::from(command)], 9);
    assert!(rows.iter().all(|line| line.width() <= 9));
    assert_eq!(
        rows.iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect::<String>(),
        command
    );
}

#[test]
fn typed_unicode_confirmation_keeps_suffix_and_caret_visible() {
    use virsh_tui::ui::overlays::confirm::{self, Confirm, ConfirmKind};
    let name = "虚".repeat(20) + "終";
    let state = Confirm {
        kind: ConfirmKind::DomainUndefine,
        title: "Undefine".into(),
        question: format!("Undefine {name}?"),
        commands: vec![],
        confirm_label: "y undefine".into(),
        type_name: Some(name.clone()),
        typed: name,
    };
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal
        .draw(|f| confirm::render(f, &theme::builtin::tokyo_night(), f.area(), &state))
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    let buffer = terminal.backend().buffer();
    let cursor = (0..12)
        .flat_map(|y| (0..40).map(move |x| (x, y)))
        .find(|&(x, y)| buffer[(x, y)].symbol() == "▏");
    let (x, y) = cursor.unwrap_or_else(|| panic!("confirmation caret must be visible: {text}"));
    assert!(x >= 2);
    assert_eq!(
        buffer[(x - 2, y)].symbol(),
        "終",
        "typed suffix must precede the caret"
    );
    assert!(text.contains("⏎ undefine"), "{text}");
}
