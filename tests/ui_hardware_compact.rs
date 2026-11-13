use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::{
    theme,
    ui::views::detail::hardware::{self, HardwareState},
};

fn shot(hw: &HardwareState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            hardware::render(
                frame,
                &theme::builtin::tokyo_night(),
                hw,
                "arch-dev",
                true,
                frame.area(),
            )
        })
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
fn compact_cpu_editor_exposes_values_and_follows_each_focused_field() {
    for (width, height) in [(120, 25), (80, 14), (40, 8)] {
        let mut hw = HardwareState::demo();
        for focus in 0..hw.forms["cpus"].fields.len() {
            hw.forms.get_mut("cpus").unwrap().focus = focus;
            let field = &hw.forms["cpus"].fields[focus];
            let text = shot(&hw, width, height);
            assert!(
                text.contains(&format!("❯ {}", field.label)),
                "{width}x{height} focus {focus}:\n{text}"
            );
            if !field.current.is_empty() {
                assert!(text.contains(&field.current), "missing value: {text}");
            }
            assert!(text.contains("F6 review"), "{text}");
        }
    }
}

#[test]
fn review_keeps_final_commands_and_long_line_tails_inspectable() {
    let mut hw = HardwareState::demo();
    hw.set_field("cpus", "current", "6");
    hw.set_field("memory", "current", "8G");
    hw.review = true;
    let mut seen = String::new();
    for row in 0..80 {
        for col in [0, 20, 40, 60] {
            hw.review_scroll = (row, col);
            seen.push_str(&shot(&hw, 40, 8));
        }
    }
    assert!(seen.contains("Pending · XML diff"), "diff is inaccessible");
    assert!(seen.contains("Will run"), "commands are inaccessible");
    assert!(seen.contains("setmem"), "final memory command is inaccessible");
    assert!(seen.contains("--config"), "command flag tail is inaccessible");
    assert!(seen.contains("Esc/F6 editor"), "return action must remain fixed");
}

#[test]
fn focused_long_disk_path_keeps_insertion_point_visible() {
    for width in [40, 80, 120] {
        let mut hw = HardwareState::demo();
        hw.selected = hw.devices.iter().position(|d| d.id == "disk-vda").unwrap();
        hw.set_field(
            "disk-vda",
            "source",
            &format!("/var/lib/{}/new-volume.qcow2", "images/".repeat(40)),
        );
        let text = shot(&hw, width, 14);
        assert!(text.contains("❯ source"), "{width}: {text}");
        assert!(
            text.contains("new-volume.qcow2 ▏"),
            "insertion point hidden at {width}: {text}"
        );
        assert!(text.contains('…'), "clipped prefix must be indicated: {text}");
    }
}
