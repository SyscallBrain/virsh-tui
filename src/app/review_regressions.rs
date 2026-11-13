//! Regression tests for the 2026-11-12 review (docs/REVIEW-2026-11-12.md).
//! Each test is the inverted probe of a finding.

use super::tests::fixture_app;
use super::*;
use crate::model::DomainState;
use crossterm::event::{KeyEventKind, KeyEventState};
use ui::views::detail::{DetailState, DetailTab, SnapModal};

fn ev(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}
fn plain(c: char) -> KeyEvent {
    ev(KeyCode::Char(c), KeyModifiers::empty())
}
fn shift(c: char) -> KeyEvent {
    ev(KeyCode::Char(c), KeyModifiers::SHIFT)
}
fn ctrl(c: char) -> KeyEvent {
    ev(KeyCode::Char(c), KeyModifiers::CONTROL)
}
fn enter() -> KeyEvent {
    ev(KeyCode::Enter, KeyModifiers::empty())
}
fn job_argv(app: &App) -> Vec<Vec<String>> {
    app.active_job
        .as_ref()
        .map(|j| {
            j.plans
                .iter()
                .flat_map(|(_, p)| p.steps.iter().map(|s| s.argv.clone()))
                .collect()
        })
        .unwrap_or_default()
}
fn run_ex(app: &mut App, line: &str) -> bool {
    app.ex_line = Some(crate::input::textinput::TextInput::with(line));
    app.handle_key_event(enter())
}

#[tokio::test]
async fn p0_1_first_z_does_nothing_and_zz_quits() {
    let mut app = fixture_app();
    assert!(!app.handle_key_event(shift('Z')));
    assert!(app.active_job.is_none());
    assert!(app.confirm.is_none());
    assert!(app.handle_key_event(shift('Z')), "ZZ quits");
}

#[tokio::test]
async fn p0_1_lone_z_asks_before_managed_save() {
    let mut app = fixture_app();
    app.handle_key_event(shift('Z'));
    app.last_key = std::time::Instant::now() - Z_TIMEOUT;
    app.on_z_timeout();
    assert!(app.active_job.is_none());
    let (c, _, targets) = app.confirm.as_ref().expect("managed-save asks first");
    assert!(c.commands[0].contains("managedsave"));
    assert!(!targets.is_empty());
}

#[tokio::test]
async fn p0_1_z_then_other_key_cancels() {
    let mut app = fixture_app();
    app.handle_key_event(shift('Z'));
    app.handle_key_event(plain('j'));
    assert!(!app.pending_quit_z);
    app.last_key = std::time::Instant::now() - Z_TIMEOUT;
    app.on_z_timeout();
    assert!(app.confirm.is_none());
}

#[tokio::test]
async fn p0_7_pool_destroy_always_confirms() {
    let mut app = fixture_app();
    app.view = View::Storage;
    let mut cfg = app.config.clone();
    cfg.confirmations.delete_volume = false;
    crate::config::set_runtime(&cfg);
    app.handle_key_event(shift('D'));
    crate::config::set_runtime(&app.config);
    assert!(app.active_job.is_none());
    let (c, _, _) = app.confirm.as_ref().expect("pool destroy confirms");
    assert_eq!(c.title, "⚠ Destroy pool");
    assert_eq!(c.kind, crate::ui::overlays::confirm::ConfirmKind::Always);
}

#[tokio::test]
async fn p0_7_pool_destroy_without_selected_volume() {
    let mut app = fixture_app();
    app.view = View::Storage;
    app.store.volumes.clear();
    app.handle_key_event(shift('D'));
    assert!(app.confirm.is_some());
}

#[tokio::test]
async fn p1_20_storage_prefills_use_virsh_argument_order() {
    let mut app = fixture_app();
    app.view = View::Storage;
    let vol = app.store.selected_vol().unwrap().name.clone();
    let pool = app.store.selected_pool().to_string();
    app.handle_key_event(shift('R'));
    let line = app.ex_line.take().unwrap().value().to_string();
    assert!(
        line.starts_with(&format!("vol-resize {vol} --pool {pool}")),
        "{line}"
    );
}

#[tokio::test]
async fn p1_21_picker_ctrl_j_moves_down() {
    let mut app = fixture_app();
    app.view = View::Storage;
    app.store.picker = Some(ui::views::storage::MediaPicker {
        domain: "vt-test-a".into(),
        target: "sda".into(),
        bus: "sata".into(),
        current: String::new(),
        query: String::new(),
        pool_idx: 0,
        pools: vec!["isos".into()],
        selected: 1,
        live: false,
        config: true,
    });
    let total = app.store.picker_isos().len();
    assert!(total >= 3);
    app.handle_key_event(ctrl('j'));
    assert_eq!(app.store.picker.as_ref().unwrap().selected, 2);
    app.handle_key_event(ctrl('k'));
    app.handle_key_event(ev(KeyCode::Up, KeyModifiers::empty()));
    assert_eq!(app.store.picker.as_ref().unwrap().selected, 0);
    for _ in 0..total + 3 {
        app.handle_key_event(ev(KeyCode::Down, KeyModifiers::empty()));
    }
    assert_eq!(app.store.picker.as_ref().unwrap().selected, total - 1);
}

#[test]
fn p0_2_selection_stays_on_the_domain_under_cpu_sort() {
    use crate::backend::RawDomainStats;
    use std::collections::HashMap;
    let mk = |name: &str| crate::model::DomainSummary {
        name: name.into(),
        uuid: String::new(),
        id: None,
        state: DomainState::Running,
        autostart: false,
        persistent: true,
        vcpus: 1,
        max_vcpus: 1,
        mem_kib: 1_048_576,
        max_mem_kib: 1_048_576,
        has_managed_save: false,
    };
    let summaries = vec![mk("a"), mk("b"), mk("c")];
    let stats = |cpu: [u64; 3]| -> Vec<RawDomainStats> {
        ["a", "b", "c"]
            .iter()
            .zip(cpu)
            .map(|(n, c)| {
                let mut v = HashMap::new();
                v.insert("state.state".to_string(), "1".to_string());
                v.insert("vcpu.current".to_string(), "1".to_string());
                v.insert("cpu.time".to_string(), c.to_string());
                RawDomainStats {
                    domain: n.to_string(),
                    values: v,
                }
            })
            .collect()
    };
    let mut dash = ui::views::dashboard::DashboardState::from_summaries(&summaries);
    dash.sort = ui::views::dashboard::Sort::Cpu;
    let t0 = std::time::Instant::now();
    let sample = |dash: &mut ui::views::dashboard::DashboardState, cpu: [u64; 3], secs: u64| {
        apply_live_msg(
            "test:///default",
            dash,
            LiveMsg::Sample {
                summaries: summaries.clone(),
                stats: stats(cpu),
                proc_stat: None,
                uptimes: HashMap::new(),
                ifaces: HashMap::new(),
                at: t0 + Duration::from_secs(secs),
            },
        );
    };
    sample(&mut dash, [0, 0, 0], 0);
    sample(&mut dash, [100_000_000, 500_000_000, 900_000_000], 1);
    dash.select_name("a");
    sample(&mut dash, [200_000_000, 1_000_000_000, 1_800_000_000], 2);
    assert_eq!(dash.selected().name, "a");
}

#[test]
fn p0_2_uptime_sorts_by_seconds() {
    let mut dash = ui::views::dashboard::DashboardState::demo();
    dash.rows.truncate(3);
    for (row, (label, secs)) in
        dash.rows
            .iter_mut()
            .zip([("10h 00m", 36_000), ("2m", 120), ("3d 04h", 273_600)])
    {
        row.uptime = label.into();
        row.uptime_secs = Some(secs);
    }
    dash.sort = ui::views::dashboard::Sort::Uptime;
    let order: Vec<String> = dash
        .visible_rows()
        .iter()
        .take(3)
        .map(|r| r.uptime.clone())
        .collect();
    assert_eq!(order, ["3d 04h", "10h 00m", "2m"]);
}

#[tokio::test]
async fn p0_3_stale_detail_is_not_the_picker_target() {
    let mut app = fixture_app();
    app.detail = Some(DetailState::demo("stale-vm"));
    app.view = View::Dashboard;
    let selected = app.dash.selected().name.clone();
    assert_eq!(app.current_domain_name(), selected);
    app.request_picker();
    let target = app.store.picker.as_ref().map(|p| p.domain.clone());
    assert_ne!(target.as_deref(), Some("stale-vm"));
}

#[test]
fn p0_4_sizes_need_units_and_reach_virsh_with_one() {
    use ui::views::detail::hardware::{HardwareState, size_to_kib};
    assert_eq!(size_to_kib("8192"), None);
    assert_eq!(size_to_kib("8G"), Some(8 * 1024 * 1024));
    assert_eq!(size_to_kib("512MiB"), Some(512 * 1024));
    assert_eq!(size_to_kib("1GB"), None);
    let argv = |hw: &HardwareState| -> Vec<Vec<String>> {
        hw.will_run("vm")
            .iter()
            .flat_map(|p| p.steps.iter().map(|s| s.argv.clone()))
            .collect()
    };
    let mut hw = HardwareState::demo();
    hw.set_field("memory", "current", "8192");
    assert!(argv(&hw).is_empty());
    assert!(hw.problems().iter().any(|p| p.contains("memory.current")));
    hw.set_field("memory", "current", "8G");
    let a = argv(&hw);
    assert!(a.iter().any(|a| a[0] == "setmem" && a[2] == "8G"), "{a:?}");
    // Only `current` changed: no setmaxmem.
    assert!(a.iter().all(|a| a[0] != "setmaxmem"), "{a:?}");

    let mut hw = HardwareState::demo();
    hw.set_field("cpus", "current", "");
    assert!(argv(&hw).is_empty());

    // Shrinking a disk is refused; growing emits an explicit unit.
    let mut hw = HardwareState::demo();
    hw.set_field("disk-vda", "size", "200M");
    assert!(argv(&hw).is_empty());
    assert!(hw.problems().iter().any(|p| p.contains("smaller")));
    hw.set_field("disk-vda", "size", "200G");
    assert!(argv(&hw).iter().any(|a| a[0] == "blockresize" && a[3] == "200G"));
}

#[test]
fn p1_12_memory_past_the_running_maximum_is_config_only() {
    let mut hw = ui::views::detail::hardware::HardwareState::demo();
    hw.set_field("memory", "max", "32G");
    hw.set_field("memory", "current", "24G");
    let argv: Vec<Vec<String>> = hw
        .will_run("vm")
        .iter()
        .flat_map(|p| p.steps.iter().map(|s| s.argv.clone()))
        .collect();
    assert_eq!(argv[0], ["setmaxmem", "vm", "32G", "--config"]);
    assert_eq!(argv[1], ["setmem", "vm", "24G", "--config"]);
    assert!(!hw.restart_notes().is_empty());
}

#[test]
fn p1_15_disk_resize_on_a_shut_off_domain_uses_vol_resize() {
    let mut hw = ui::views::detail::hardware::HardwareState::demo();
    hw.running = false;
    hw.set_field("disk-vda", "size", "200G");
    let argv = hw.will_run("vm")[0].steps[0].argv.clone();
    assert_eq!(argv[0], "vol-resize");
    assert_eq!(argv[2], "200G");
}

#[tokio::test]
async fn p0_5_and_p1_5_short_ex_forms_follow_state_and_format() {
    let mut app = fixture_app();
    let off = app
        .dash
        .rows
        .iter()
        .find(|r| r.state == DomainState::ShutOff)
        .map(|r| r.name.clone())
        .unwrap();
    run_ex(&mut app, &format!("attach-disk {off} /x.qcow2 vdb"));
    let disk = job_argv(&app);
    assert!(
        disk[0].windows(2).any(|w| w == ["--subdriver", "qcow2"]),
        "{disk:?}"
    );
    assert!(!disk[0].iter().any(|a| a == "--live"));
    app.active_job.take();

    run_ex(&mut app, &format!("attach-disk {off} /nonexistent/x.img vdb"));
    assert!(app.active_job.is_none());
    assert!(app.dash.message.contains("image format"));

    run_ex(&mut app, &format!("attach-iso {off} /x.iso"));
    let iso = job_argv(&app);
    assert!(!iso.iter().any(|a| a.iter().any(|x| x == "--live")), "{iso:?}");
}

#[test]
fn p1_5_detach_disk_on_shut_off_domain_has_no_live() {
    let mut d = DetailState::demo("vt-test-a");
    d.state = DomainState::ShutOff;
    d.hw.selected =
        d.hw.devices
            .iter()
            .position(|x| x.id.starts_with("disk"))
            .unwrap();
    let mut confirm = None;
    stage_detach(&mut d, &mut confirm);
    let commands = confirm.map(|(c, _, _)| c.commands).unwrap_or_default();
    assert!(!commands[0].contains("--live"), "{commands:?}");
}

#[tokio::test]
async fn p1_6_ex_shutdown_ignores_the_filter() {
    let mut app = fixture_app();
    let running = app
        .dash
        .rows
        .iter()
        .find(|r| r.state == DomainState::Running)
        .map(|r| r.name.clone())
        .unwrap();
    app.dash.set_filter("zzz-no-match");
    run_ex(&mut app, &format!("shutdown {running}"));
    assert_eq!(job_argv(&app)[0][0], "shutdown");
}

#[tokio::test]
async fn p1_9_q_keeps_pending_hardware_edits() {
    let mut app = fixture_app();
    let mut d = DetailState::demo("vt-test-a");
    d.tab = DetailTab::Hardware;
    d.hw.set_field("cpus", "current", "4");
    app.detail = Some(d);
    app.view = View::Detail;
    app.handle_key_event(plain('q'));
    assert!(app.detail.is_some());
    assert_eq!(app.view, View::Detail);
    app.handle_key_event(plain('1'));
    app.last_key = std::time::Instant::now() - Duration::from_millis(200);
    app.on_digit_timeout();
    assert_eq!(app.view, View::Detail);
}

#[tokio::test]
async fn p1_10_uppercase_is_typed_in_text_inputs() {
    let mut app = fixture_app();
    let mut d = DetailState::demo("vt-test-a");
    d.tab = DetailTab::Snapshots;
    d.snap_modal = SnapModal::Create;
    d.snaps.create_name.clear();
    d.snaps.create_focus = 0;
    app.detail = Some(d);
    app.view = View::Detail;
    for k in [shift('A'), plain('b'), shift('C')] {
        app.handle_key_event(k);
    }
    assert_eq!(app.detail.as_ref().unwrap().snaps.create_name, "AbC");

    let mut app = fixture_app();
    let mut d = DetailState::demo("vt-test-a");
    d.tab = DetailTab::Hardware;
    d.hw_insert = true;
    d.hw.selected = d.hw.devices.iter().position(|x| x.id == "boot").unwrap();
    let focus = d.hw.forms["boot"]
        .fields
        .iter()
        .position(|f| f.id == "order")
        .unwrap();
    d.hw.forms.get_mut("boot").unwrap().focus = focus;
    app.detail = Some(d);
    app.view = View::Detail;
    app.handle_key_event(shift('L'));
    let d = app.detail.as_ref().unwrap();
    assert_eq!(d.tab, DetailTab::Hardware);
    assert!(d.hw.forms["boot"].fields[focus].current.ends_with('L'));
}

#[test]
fn p1_11_nic_link_uses_the_mac_and_xml_state() {
    let xml = include_str!("../../tests/fixtures/virsh/dumpxml_arch_inactive.txt");
    let cfg = crate::backend::virsh::parse_xml::parse_domain_xml(xml).unwrap();
    let mut hw = ui::views::detail::hardware::HardwareState::from_config(&cfg, xml, false);
    let nic = hw
        .devices
        .iter()
        .find(|d| d.id.starts_with("nic-"))
        .unwrap()
        .id
        .clone();
    let mac = hw.field_value(&nic, "mac");
    hw.set_field(&nic, "link", "");
    let argv = hw.will_run("vm")[0].steps[0].argv.clone();
    assert_eq!(argv, ["domif-setlink", "vm", mac.as_str(), "down", "--config"]);
}

#[test]
fn p1_13_max_vcpus_without_topology_leaves_it_alone() {
    let xml = include_str!("../../tests/fixtures/virsh/dumpxml_arch_inactive.txt");
    let cfg = crate::backend::virsh::parse_xml::parse_domain_xml(xml).unwrap();
    assert!(cfg.topology.is_none());
    let mut hw = ui::views::detail::hardware::HardwareState::from_config(&cfg, xml, true);
    hw.forms.get_mut("cpus").unwrap().focus = 1;
    edit_focused(&mut hw, "cpus", EditOp::Adjust(2));
    let first = hw.will_run("vm")[0].steps[0].argv.clone();
    assert_ne!(first[0], "define");
    assert!(
        !crate::xml::edit::set_vcpus("<domain><vcpu placement='auto'>2</vcpu></domain>", 2, 4)
            .unwrap()
            .contains("static")
    );
}

#[tokio::test]
async fn p1_26_overlays_swallow_unhandled_keys() {
    let mut app = fixture_app();
    app.settings = Some(ui::views::settings::SettingsState::open(&app.config));
    app.handle_key_event(plain('s'));
    assert!(app.active_job.is_none());
    assert!(!app.handle_key_event(plain('q')));

    let mut app = fixture_app();
    app.help_filter = Some(String::new());
    app.handle_key_event(ev(KeyCode::F(2), KeyModifiers::empty()));
    app.handle_key_event(shift('D'));
    assert!(app.confirm.is_none());
}

#[test]
fn p1_19_and_p0_6_disk_sources_parse_xml() {
    let xml = "<domain><devices>\
        <disk type='file'><source file='/v/a&amp;b.qcow2'/></disk>\
        <disk type='volume'><source pool='p1' volume='v1'/></disk>\
        <disk type='file'><source file='/v/top.qcow2'/><backingStore type='file'><source file='/v/base.qcow2'/></backingStore></disk>\
        </devices></domain>";
    let refs = disk_sources(xml);
    assert!(refs.contains(&String::from("/v/a&b.qcow2")));
    assert!(refs.contains(&ui::views::storage::volume_key("p1", "v1")));
    assert!(refs.contains(&String::from("/v/base.qcow2")));
    let vols = parse_vol_list(
        " Name         Path\n-----------------------------------\n my vol.iso   /default-pool/my vol.iso\n",
    );
    assert_eq!(
        vols,
        [(
            String::from("my vol.iso"),
            String::from("/default-pool/my vol.iso")
        )]
    );
}

fn draw_at(app: &App, w: u16, h: u16) -> Result<(), String> {
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let backend = ratatui::backend::TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
    }));
    res.map_err(|e| {
        e.downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    })
}

/// RB-1 / P1-40: every view, tab, modal and overlay draws at every size
/// (debug build: arithmetic overflow panics).
#[tokio::test]
async fn rb1_every_screen_draws_at_every_size() {
    let sizes: Vec<(u16, u16)> = vec![
        (1, 1),
        (3, 2),
        (5, 3),
        (7, 5),
        (10, 4),
        (12, 6),
        (20, 3),
        (20, 8),
        (40, 10),
        (40, 12),
        (60, 5),
        (80, 24),
        (99, 29),
        (100, 30),
        (120, 30),
        (174, 43),
    ];
    let mut failures: Vec<String> = Vec::new();
    type Make = Box<dyn Fn() -> App>;
    let variants: Vec<(&str, Make)> = vec![
        ("dashboard", Box::new(fixture_app)),
        (
            "host",
            Box::new(|| {
                let mut a = fixture_app();
                a.view = View::Host;
                a
            }),
        ),
        (
            "networks+form",
            Box::new(|| {
                let mut a = fixture_app();
                a.view = View::Networks;
                a.nets.open_new_form();
                a
            }),
        ),
        (
            "storage",
            Box::new(|| {
                let mut a = fixture_app();
                a.view = View::Storage;
                a
            }),
        ),
        (
            "events",
            Box::new(|| {
                let mut a = fixture_app();
                a.view = View::Events;
                a
            }),
        ),
        (
            "help",
            Box::new(|| {
                let mut a = fixture_app();
                a.help_filter = Some(String::new());
                a
            }),
        ),
        (
            "messages",
            Box::new(|| {
                let mut a = fixture_app();
                a.messages.push("✗ something failed".into());
                a.messages_view = Some(0);
                a
            }),
        ),
        (
            "settings",
            Box::new(|| {
                let mut a = fixture_app();
                a.settings = Some(ui::views::settings::SettingsState::open(&a.config));
                a
            }),
        ),
        (
            "palette",
            Box::new(|| {
                let mut a = fixture_app();
                a.palette = Some(ui::overlays::palette::PaletteState::demo());
                a
            }),
        ),
        (
            "wizard",
            Box::new(|| {
                let mut a = fixture_app();
                a.wizard = Some(ui::overlays::wizard::WizardState::demo_step3());
                a
            }),
        ),
        (
            "confirm",
            Box::new(|| {
                let mut a = fixture_app();
                ask_confirm(
                    &mut a.confirm,
                    crate::ui::overlays::confirm::ConfirmKind::DomainDestroy,
                    "⚠ Destroy domain",
                    "destroy",
                    &["a".to_string()],
                    crate::command::builders::lifecycle::destroy,
                );
                a
            }),
        ),
        (
            "whichkey",
            Box::new(|| {
                let mut a = fixture_app();
                a.engine.feed(&crate::input::key::KeySeq::parse("Space"));
                a.which_key_since = Some(std::time::Instant::now() - Duration::from_secs(1));
                a
            }),
        ),
    ];
    for tab in DetailTab::ALL {
        let mut app = fixture_app();
        let mut d = DetailState::demo("arch-dev");
        d.tab = tab;
        app.detail = Some(d);
        app.view = View::Detail;
        for (w, h) in &sizes {
            if let Err(e) = draw_at(&app, *w, *h) {
                failures.push(format!("detail {tab:?} {w}x{h}: {e}"));
            }
        }
    }
    for (name, make) in &variants {
        let app = make();
        for (w, h) in &sizes {
            if let Err(e) = draw_at(&app, *w, *h) {
                failures.push(format!("{name} {w}x{h}: {e}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

fn add_rows(app: &mut App, rows: &[(&str, DomainState, bool)]) {
    for (name, state, autostart) in rows {
        let mut row = app.dash.rows[0].clone();
        row.name = name.to_string();
        row.state = *state;
        row.state_label = state.label();
        row.autostart = *autostart;
        app.dash.rows.push(row);
    }
}

#[tokio::test]
async fn p1_2_pause_and_autostart_direction_is_per_target() {
    use crate::input::engine::KeyAction as A;
    let mut app = fixture_app();
    add_rows(
        &mut app,
        &[
            ("vt-test-a", DomainState::Running, true),
            ("vt-test-b", DomainState::Paused, false),
        ],
    );
    app.dash.marks = ["vt-test-a", "vt-test-b"].iter().map(|s| s.to_string()).collect();
    let plans = app.lifecycle_plans(A::Pause).unwrap();
    let verbs: Vec<(String, String)> = plans
        .iter()
        .map(|(t, p)| (t.clone(), p.steps[0].argv[0].clone()))
        .collect();
    assert!(
        verbs.contains(&("vt-test-a".into(), "suspend".into())),
        "{verbs:?}"
    );
    assert!(
        verbs.contains(&("vt-test-b".into(), "resume".into())),
        "{verbs:?}"
    );
    let plans = app.lifecycle_plans(A::Autostart).unwrap();
    for (t, p) in &plans {
        let disable = p.steps[0].argv.iter().any(|a| a == "--disable");
        assert_eq!(disable, t == "vt-test-a", "{t}: {:?}", p.steps[0].argv);
    }
}

#[tokio::test]
async fn p1_3_hidden_marks_are_not_targets_and_bulk_asks() {
    let mut app = fixture_app();
    let marks: Vec<String> = app.dash.marks.iter().cloned().collect();
    assert!(marks.len() > 1);
    app.dash.set_filter("arch");
    let t = targets(&app.dash);
    let visible: Vec<String> = app.dash.visible_rows().iter().map(|r| r.name.clone()).collect();
    assert!(t.iter().all(|n| visible.contains(n)), "{t:?} vs {visible:?}");

    let mut app = fixture_app();
    app.handle_key_event(plain('r'));
    assert!(
        app.active_job.is_none(),
        "bulk reboot runs only after confirmation"
    );
    assert!(app.confirm.as_ref().unwrap().0.commands.len() > 1);
}

#[test]
fn p1_4_undefine_handles_uefi_and_managed_save() {
    let argv = crate::command::builders::lifecycle::undefine("vm", false, false, true).steps[0]
        .argv
        .clone();
    for flag in ["--keep-nvram", "--snapshots-metadata", "--managed-save"] {
        assert!(argv.iter().any(|a| a == flag), "{argv:?}");
    }
}

#[tokio::test]
async fn p1_7_clone_name_is_editable_and_running_source_refused() {
    let mut app = fixture_app();
    let running = app
        .dash
        .rows
        .iter()
        .find(|r| r.state == DomainState::Running)
        .unwrap()
        .name
        .clone();
    app.dash.select_name(&running);
    app.handle_key_event(ev(KeyCode::Char(' '), KeyModifiers::empty()));
    app.handle_key_event(plain('c'));
    let line = app.ex_line.take().unwrap().value().to_string();
    assert_eq!(line, format!("clone {running} {running}-clone"));
    run_ex(&mut app, &line);
    assert!(app.active_job.is_none());
    assert!(app.dash.message.contains("is running"), "{}", app.dash.message);
}

#[test]
fn p1_17_snapshot_tree_keeps_orphans_and_fills_columns() {
    use crate::model::Snapshot;
    let snaps = vec![
        Snapshot {
            name: "child".into(),
            parent: Some("deleted-parent".into()),
            state: "running".into(),
            created: "2026-09-01 10:00:00 +0000".into(),
        },
        Snapshot {
            name: "root".into(),
            parent: None,
            state: "shutoff".into(),
            created: "2026-08-01 10:00:00 +0000".into(),
        },
    ];
    let rows = ui::views::detail::snapshots::build_tree(&snaps, None);
    assert_eq!(rows.len(), 2);
    let child = rows.iter().find(|r| r.name == "child").unwrap();
    assert_eq!(child.date, "2026-09-01");
    assert_eq!(child.kind, "mem+disk");
    let xml = "<domainsnapshot><name>root</name><description>clean install</description><state>shutoff</state><creationTime>1754042400</creationTime><memory snapshot='no'/><disks><disk name='vda' snapshot='internal'/></disks></domainsnapshot>";
    let info = ui::views::detail::snapshots::snap_info(xml, &snaps).unwrap();
    assert_eq!(info.description, "clean install");
    assert!(
        info.details
            .iter()
            .any(|(k, v)| k.trim() == "disks" && v.contains("vda internal"))
    );
}

#[tokio::test]
async fn p1_17_revert_refused_without_snapshots() {
    let mut app = fixture_app();
    let mut d = DetailState::demo("vt-test-a");
    d.tab = DetailTab::Snapshots;
    d.snaps.rows.clear();
    app.detail = Some(d);
    app.view = View::Detail;
    app.handle_key_event(ev(KeyCode::Char(' '), KeyModifiers::empty()));
    app.handle_key_event(plain('s'));
    app.handle_key_event(plain('r'));
    assert_eq!(app.detail.as_ref().unwrap().snap_modal, SnapModal::None);
}

#[test]
fn p1_24_gateway_is_inside_the_subnet() {
    use crate::command::builders::network as nb;
    assert_eq!(
        nb::gateway_for("192.168.100.128/25").unwrap().to_string(),
        "192.168.100.129"
    );
    assert_eq!(
        nb::gateway_for("10.0.0.254/24").unwrap().to_string(),
        "10.0.0.254"
    );
    assert_eq!(
        nb::default_dhcp_range("192.168.100.128/25"),
        Some(("192.168.100.130".into(), "192.168.100.254".into()))
    );
    assert!(nb::parse_cidr("10.0.0.0/31").is_none());
}

#[tokio::test]
async fn p1_27_messages_overlay_shows_history() {
    let mut app = fixture_app();
    for i in 0..5 {
        app.messages.push(format!("line {i}"));
    }
    run_ex(&mut app, "messages");
    assert_eq!(app.messages_view, Some(0));
    app.handle_key_event(plain('k'));
    assert_eq!(app.messages_view, Some(1));
    app.handle_key_event(plain('s'));
    assert!(app.active_job.is_none(), "keys do not leak under :messages");
    app.handle_key_event(ev(KeyCode::Esc, KeyModifiers::empty()));
    assert!(app.messages_view.is_none());
}

#[tokio::test]
async fn p1_28_set_validates_and_updates_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = fixture_app();
    app.config.source = Some(dir.path().join("config.toml"));
    crate::config::set_runtime(&app.config);
    run_ex(&mut app, "set viewer=remote-viewer");
    assert_eq!(crate::config::runtime().console.viewer, "remote-viewer");
    run_ex(&mut app, "set viewer=bogus");
    assert!(app.dash.message.starts_with('✗'));
    assert_eq!(app.config.console.viewer, "remote-viewer");
    run_ex(&mut app, "set refresh_interval_secs=0");
    assert!(app.dash.message.starts_with('✗'));
    crate::config::set_runtime(&Config::default());
}

#[test]
fn p1_30_destructive_list_covers_data_loss_commands() {
    use crate::ui::overlays::confirm::Confirm;
    for cmd in [
        "vol-upload",
        "managedsave-remove",
        "detach-disk",
        "blockresize",
        "migrate",
        "qemu-monitor-command",
    ] {
        assert!(Confirm::destructive(cmd), "{cmd}");
    }
    assert!(!Confirm::destructive("detach"));
    let shrink: Vec<String> = ["vol-resize", "v", "1G", "--shrink"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(Confirm::destructive_argv(&shrink));
}

#[test]
fn p1_37_host_cpu_ignores_guest_columns() {
    use crate::backend::virsh::host_local;
    let a = host_local::parse_proc_stat("cpu0 100 0 0 100 0 0 0 0 100 0\n");
    let b = host_local::parse_proc_stat("cpu0 200 0 0 200 0 0 0 0 200 0\n");
    let busy = host_local::cpu_busy_fraction(a[0], b[0]);
    assert!((busy - 0.5).abs() < 1e-9, "{busy}");
}

#[test]
fn p1_34_and_p1_35_wizard_lets_virt_install_detect_and_prints_one_xml() {
    let st = ui::overlays::wizard::WizardState {
        name: "vm".into(),
        iso_path: "/isos/Fedora-Server-dvd-x86_64-41-1.4.iso".into(),
        ..Default::default()
    };
    assert_eq!(
        crate::command::builders::install::osinfo(&st),
        "detect=on,require=off"
    );
    let mut st = st;
    st.vcpus = 0;
    assert!(st.problems().iter().any(|(_, p)| p.contains("vCPU")));
    st.vcpus = 2;
    st.balloon = false;
    st.max_mem_gib = st.mem_gib + 4;
    assert!(st.problems().iter().any(|(_, p)| p.contains("balloon")));
}

#[tokio::test]
async fn p1_39_connect_clears_the_previous_host() {
    let mut app = fixture_app();
    app.dash.host_gauges = vec![("CPU".into(), 0.5, "50%".into())];
    app.dash.host_title = "old host".into();
    run_ex(&mut app, "connect qemu+ssh://admin@srv/system");
    assert!(app.dash.host_gauges.is_empty());
    assert!(app.dash.host_title.is_empty());
    assert_eq!(app.dash.hostname, "srv");
    assert_eq!(app.events_state.uri, "qemu+ssh://admin@srv/system");
}

#[tokio::test]
async fn p1_49_one_read_does_not_strand_another() {
    let mut app = fixture_app();
    app.selection_task.insert(
        requests::ReadKind::Picker,
        SelectionTask(tokio::spawn(std::future::pending())),
    );
    app.begin_read(requests::ReadKind::Detail);
    assert!(app.selection_task.contains_key(&requests::ReadKind::Picker));
    app.selection_task.clear();
}

#[tokio::test]
async fn p1_50_volume_cursor_survives_a_reload() {
    let mut app = fixture_app();
    app.view = View::Storage;
    let pool = app.store.selected_pool().to_string();
    let volumes = app.store.volumes.clone();
    assert!(volumes.len() > 2);
    app.store.pool_volumes.insert(pool, volumes);
    app.store.vol_selected = 2;
    let name = app.store.selected_vol().unwrap().name.clone();
    let fresh = app.store.clone();
    let serial = {
        app.request_view_load(View::Storage);
        app.view_load_serial[&View::Storage]
    };
    app.on_loaded(Loaded::Store(
        app.uri.clone(),
        app.connection_epoch,
        serial,
        Box::new(fresh),
    ));
    assert_eq!(app.store.selected_vol().unwrap().name, name);
    app.view_load_tasks.clear();
}

#[tokio::test]
async fn rb2_runaway_count_is_clamped() {
    let mut app = fixture_app();
    for c in "3000000".chars() {
        app.handle_key_event(plain(c));
    }
    let t = std::time::Instant::now();
    app.handle_key_event(plain('j'));
    assert!(t.elapsed() < Duration::from_millis(500));
    assert_eq!(app.dash.selection, app.dash.visible_rows().len() - 1);
}

#[test]
fn rb5_invalid_config_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[general\nbroken").unwrap();
    let cfg = Config::load_file(&path);
    assert!(cfg.load_error.is_some());
    assert!(cfg.save().is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[general\nbroken");
}

#[tokio::test]
async fn ux3_ex_line_history_and_editing() {
    let mut app = fixture_app();
    run_ex(&mut app, "sort cpu");
    app.handle_key_event(plain(':'));
    app.handle_key_event(ev(KeyCode::Up, KeyModifiers::empty()));
    assert_eq!(app.ex_line.as_ref().unwrap().value(), "sort cpu");
    app.handle_key_event(ctrl('w'));
    assert_eq!(app.ex_line.as_ref().unwrap().value(), "sort ");
    app.handle_key_event(ctrl('u'));
    assert_eq!(app.ex_line.as_ref().unwrap().value(), "");
    assert_eq!(
        crate::input::ex::parse("attach-iso vm \"/isos/My Distro.iso\"").unwrap(),
        crate::input::ex::ExCommand::Media("vm".into(), String::new(), Some("/isos/My Distro.iso".into()))
    );
    assert!(crate::input::ex::parse("start a b").is_err());
}

#[test]
fn ux7_which_key_comes_from_the_keymap() {
    let map = crate::input::keymap::default_map();
    let rows = render::which_key_rows(&map, "g");
    assert!(rows.iter().any(|(k, d)| k == "g" && d == "go top"), "{rows:?}");
    assert!(!rows.iter().any(|(k, _)| k.contains("1-5")));
    let rows = render::which_key_rows(&map, "Space");
    assert!(rows.iter().any(|(k, _)| k == "s …"));
}

#[test]
fn ux8_unknown_key_tokens_are_reported() {
    let (map, warnings) = crate::input::keymap::apply_overrides(
        crate::input::keymap::default_map(),
        "[normal]\nshutdown = [\"Ctrl-q\"]\n",
    );
    assert!(warnings.iter().any(|w| w.contains("Ctrl-q")), "{warnings:?}");
    assert!(!map.keys().any(|k| k == &vec![String::from("Ctrl-q")]));
}

#[test]
fn ux10_boot_reorder_moves_the_selected_entry() {
    let mut d = DetailState::demo("vt-test-a");
    boot_cursor_next(&mut d, 1);
    boot_reorder(&mut d, 1);
    assert_eq!(d.hw.field_value("boot", "order"), "hd,network,cdrom");
}

#[test]
fn pf6_visible_rows_cache_follows_changes() {
    let mut dash = ui::views::dashboard::DashboardState::demo();
    let before: Vec<String> = dash.visible_rows().iter().map(|r| r.name.clone()).collect();
    dash.set_filter("k8s");
    let after: Vec<String> = dash.visible_rows().iter().map(|r| r.name.clone()).collect();
    assert!(after.len() < before.len());
    assert!(after.iter().all(|n| n.contains("k8s")));
}
