//! P4 input + command tests (RED first).

#[test]
fn key_engine_count_and_sequence() {
    use virsh_tui::input::{engine::KeyEngine, key::KeySeq};
    let mut eng = KeyEngine::new();
    // 3j => count 3, action MoveDown.
    let actions = eng.feed(&KeySeq::parse("3"));
    assert!(actions.is_empty());
    let actions = eng.feed(&KeySeq::parse("j"));
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].count, 3);
}

#[test]
fn gg_goes_top() {
    use virsh_tui::input::{engine::KeyEngine, key::KeySeq};
    let mut eng = KeyEngine::new();
    assert!(eng.feed(&KeySeq::parse("g")).is_empty());
    let actions = eng.feed(&KeySeq::parse("g"));
    assert_eq!(actions[0].action, virsh_tui::input::engine::KeyAction::GoTop);
}

#[test]
fn ex_parses_start() {
    let cmd = virsh_tui::input::ex::parse(":start arch-dev").unwrap();
    assert_eq!(
        cmd,
        virsh_tui::input::ex::ExCommand::Start("arch-dev".to_string())
    );
}

#[test]
fn lifecycle_builders_exact_argv() {
    use virsh_tui::command::builders::lifecycle;
    let plan = lifecycle::start("arch-dev");
    assert_eq!(plan.steps[0].argv, vec!["start", "arch-dev"]);
    let plan = lifecycle::shutdown("arch-dev");
    assert_eq!(plan.steps[0].argv, vec!["shutdown", "arch-dev"]);
    let plan = lifecycle::send_key("arch-dev");
    assert_eq!(
        plan.steps[0].argv,
        vec![
            "send-key",
            "arch-dev",
            "KEY_LEFTCTRL",
            "KEY_LEFTALT",
            "KEY_DELETE"
        ]
    );
}

#[test]
fn display_quotes_paths() {
    use virsh_tui::command::{display::display, plan::CommandStep};
    let step = CommandStep::new("virsh", vec!["define", "/tmp/vt-arch.xml"]);
    assert_eq!(display(&step), "virsh define /tmp/vt-arch.xml");
}
