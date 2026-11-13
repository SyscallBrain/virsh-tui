use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::overlays::wizard::{self, WizardState},
};

fn shot(st: &WizardState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| wizard::render(frame, &theme::builtin::tokyo_night(), st))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

#[test]
fn compact_wizard_keeps_every_focused_field_and_actions_visible() {
    for (width, height) in [(80, 24), (40, 12)] {
        for alternate in [false, true] {
            let mut state = WizardState::demo_step3();
            for step in 0..6 {
                state.step = step;
                state.disk_new = !alternate;
                state.mac_auto = !alternate;
                state.media = if step == 1 && alternate {
                    wizard::MediaKind::Url
                } else {
                    wizard::MediaKind::Iso
                };
                for focus in 0..state.fields().len() {
                    state.focus = focus;
                    let text = shot(&state, width, height);
                    let label = WizardState::label(state.focused());
                    assert!(
                        text.contains(&format!("❯ {label}")),
                        "{width}x{height}, step {step}, focus {focus}:\n{text}"
                    );
                    let value = state.value(state.focused());
                    if !value.is_empty() && value.len() <= width as usize - 7 {
                        let focused_row = text
                            .lines()
                            .position(|line| line.contains(&format!("❯ {label}")))
                            .unwrap();
                        let value_row = text.lines().nth(focused_row + 1).unwrap();
                        let expected = if matches!(state.kind(state.focused()), wizard::WKind::Check) {
                            format!("[{value}]")
                        } else {
                            value
                        };
                        assert!(value_row.contains(&expected), "focused value hidden: {text}");
                    }
                    assert!(text.contains("⎋ cancel"), "{text}");
                    assert!(text.contains("⏎"), "{text}");
                    assert!(text.contains("C-v"), "{text}");
                }
            }
        }
    }
}

#[test]
fn command_review_preserves_every_wrapped_character_and_clamps_last_page() {
    let mut state = WizardState::demo_step3();
    state.command_review = true;
    state.desc = format!(
        "spaces and 'quotes' {} end-of-description",
        "very-long-path/".repeat(24)
    );
    state.autostart = true;
    let plan = virsh_tui::command::builders::install::virt_install(&state);
    let expected = virsh_tui::command::display::display(&plan.steps[0]);
    for (width, height) in [(80, 24), (40, 12)] {
        let content_width = width as usize - 4;
        // Read one newly exposed row per scroll, including trailing spaces
        // inside the command. Border and review headings are excluded.
        let mut exposed = String::new();
        let row_count = expected.chars().count().div_ceil(content_width);
        for row in 0..row_count {
            state.command_scroll = row as u16;
            let text = shot(&state, width, height);
            let max_scroll = row_count.saturating_sub(height as usize - 4);
            let visible_row = row - row.min(max_scroll);
            let line = text.lines().nth(2 + visible_row).unwrap();
            exposed.extend(line.chars().skip(2).take(content_width));
            assert!(text.contains("⎋/C-v back"));
        }
        assert_eq!(exposed.trim_end(), expected);
        state.command_scroll = u16::MAX;
        let text = shot(&state, width, height);
        state.command_scroll = row_count.saturating_sub(height as usize - 4) as u16;
        assert_eq!(text, shot(&state, width, height));
    }
    assert!(!state.content_snapshot().contains("command_scroll"));
    assert!(!state.content_snapshot().contains("command_review"));
}
