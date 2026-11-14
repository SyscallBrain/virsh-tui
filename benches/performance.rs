//! Reproducible CPU benchmarks; no libvirt connection or terminal required.
use ratatui::{Terminal, backend::TestBackend};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
use virsh_tui::{
    config::Config,
    input::{complete, keymap},
    theme,
    ui::{
        overlays::{
            confirm::{self, Confirm, ConfirmKind},
            help, messages, palette, wizard,
        },
        views::{
            dashboard::{self, DashboardState},
            detail::{self, DetailState, DetailTab},
            events::{self, EventsState},
            gallery,
            host::{self, HostState},
            networks::{self, NetworksState},
            settings::{self, SettingsState},
            storage::{self, StorageState},
        },
        widgets::charts::{self, BrailleMode},
    },
};

fn measure(name: &str, mut run: impl FnMut()) {
    for _ in 0..20 {
        run();
    }
    let mut samples = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        let mut count = 0u64;
        while start.elapsed() < Duration::from_millis(150) {
            for _ in 0..10 {
                run();
            }
            count += 10;
        }
        samples.push(start.elapsed().as_nanos() as f64 / count as f64);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{name:36} {:12.1} ns/op (median; min {:.1}, max {:.1})",
        samples[4], samples[0], samples[8]
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = charts::series(42, 120, 0.5, 0.3, 0.15);
    for mode in [BrailleMode::Fill, BrailleMode::Down, BrailleMode::Line] {
        measure(&format!("braille {mode:?} 60x8"), || {
            black_box(charts::braille(black_box(&data), 60, 8, mode));
        });
    }
    measure("spark rows 60x8", || {
        black_box(charts::spark_rows(black_box(&data), 60, 8));
    });
    let names: Vec<_> = (0..1000)
        .map(|i| format!("vt-test-domain-{i:04}-snapshot"))
        .collect();
    let refs: Vec<_> = names.iter().map(String::as_str).collect();
    let unicode_names: Vec<_> = (0..1000)
        .map(|i| format!("vt-test-dömäin-{i:04}-snapshot"))
        .collect();
    let unicode_refs: Vec<_> = unicode_names.iter().map(String::as_str).collect();
    // (label, query, haystacks, expected matches). Asserting the match count
    // keeps a workload from silently measuring an empty early-exit path.
    let fuzzy_cases = [
        ("fuzzy snap 1000", "snap", &refs, 1000),
        ("fuzzy snap 1000 unicode haystacks", "snap", &unicode_refs, 1000),
        ("fuzzy dömäin 1000 no match", "dömäin", &refs, 0),
    ];
    for (label, query, items, expected) in fuzzy_cases {
        assert_eq!(palette::fuzzy_rank(query, items).len(), expected, "{label}");
        measure(label, || {
            black_box(palette::fuzzy_rank(black_box(query), black_box(items)));
        });
    }
    let mut palette = palette::PaletteState::demo();
    palette.items = names
        .iter()
        .map(|title| palette::PaletteItem {
            icon: "●",
            category: "Domain".into(),
            title: title.clone(),
            key: String::new(),
            virsh_cmd: title.clone(),
            action_command: None,
        })
        .collect();
    palette.query = "snap".into();
    assert_eq!(palette.visible().len(), 1000);
    measure("palette visible 1000", || {
        black_box(palette.visible());
    });
    let fixture = include_str!("../tests/fixtures/virsh/domstats_running.txt");
    for count in [0, 1, 10, 100, 1000] {
        let stats_text: String = (0..count)
            .map(|i| fixture.replace("arch-dev", &format!("vt-test-{i}")))
            .collect();
        let stats = virsh_tui::backend::virsh::parse_domstats::parse_domstats_raw(&stats_text)?;
        let mut metrics = virsh_tui::metrics::store::MetricsStore::default();
        let mut now = Instant::now();
        metrics.ingest_domains(&stats, now);
        assert!((0..count).all(|i| metrics.domain(&format!("vt-test-{i}")).is_some()));
        measure(&format!("ingest metrics {count} domains"), || {
            now += Duration::from_secs(1);
            metrics.ingest_domains(black_box(&stats), now);
            black_box(&metrics);
        });
    }
    let th = theme::builtin::tokyo_night();
    let mut dashboard = DashboardState::demo();
    let host = HostState::demo();
    let mut detail = DetailState::demo("arch-dev");
    detail.tab = DetailTab::Monitor;
    for (w, h) in [(120, 36), (174, 43)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h))?;
        measure(&format!("dashboard {w}x{h}"), || {
            terminal
                .draw(|f| dashboard::render(f, &th, black_box(&dashboard)))
                .unwrap();
        });
        measure(&format!("host {w}x{h}"), || {
            terminal.draw(|f| host::render(f, &th, black_box(&host))).unwrap();
        });
        measure(&format!("monitor {w}x{h}"), || {
            terminal
                .draw(|f| detail::render(f, &th, black_box(&detail)))
                .unwrap();
        });
    }
    let mut terminal = Terminal::new(TestBackend::new(174, 43))?;
    measure("dashboard navigation 174x43", || {
        dashboard.selection = (dashboard.selection + 1) % dashboard.rows.len();
        terminal
            .draw(|f| dashboard::render(f, &th, black_box(&dashboard)))
            .unwrap();
    });
    palette
        .items
        .iter_mut()
        .for_each(|i| i.virsh_cmd = "snapshot-create-as".into());
    measure("palette navigation 1000 174x43", || {
        palette.selected = (palette.selected + 1) % palette.items.len();
        terminal
            .draw(|f| palette::render(f, &th, black_box(&palette)))
            .unwrap();
    });
    detail.tab = DetailTab::Xml;
    detail.xml = "  <disk type='file'><source file='/vm/disk.qcow2'/></disk>\n".repeat(10_000);
    detail.xml_cursor = 5000;
    let mut terminal = Terminal::new(TestBackend::new(174, 43))?;
    measure("XML 10000 lines middle page", || {
        terminal
            .draw(|f| detail::render(f, &th, black_box(&detail)))
            .unwrap();
    });

    // ---- remaining views and overlays ----
    let config = Config::default();
    let nets = NetworksState::demo();
    let store = StorageState::demo();
    let events = EventsState::demo();
    let settings = SettingsState::open(&config);
    let wizard = wizard::WizardState::demo_step3();
    let confirm_modal = Confirm {
        kind: ConfirmKind::DomainDestroy,
        title: "⚠ Destroy domain".into(),
        question: "Destroy arch-dev?".into(),
        commands: vec!["virsh destroy arch-dev".into()],
        confirm_label: "y destroy".into(),
        type_name: None,
        typed: String::new(),
    };
    let messages_lines: Vec<String> = (0..200)
        .map(|i| format!("line {i}: something happened"))
        .collect();
    let map = keymap::default_map();
    let mut terminal = Terminal::new(TestBackend::new(174, 43))?;
    measure("networks 174x43", || {
        terminal
            .draw(|f| networks::render(f, &th, black_box(&nets)))
            .unwrap();
    });
    measure("storage 174x43", || {
        terminal
            .draw(|f| storage::render(f, &th, black_box(&store), false))
            .unwrap();
    });
    measure("events 174x43", || {
        terminal
            .draw(|f| events::render(f, &th, black_box(&events)))
            .unwrap();
    });
    measure("settings 174x43", || {
        terminal
            .draw(|f| {
                settings::render(
                    f,
                    &th,
                    &config,
                    black_box(&settings),
                    &virsh_tui::input::keymap::default_map(),
                    &virsh_tui::input::keymap_file::KeymapFile::in_memory(),
                )
            })
            .unwrap();
    });
    measure("gallery 174x43", || {
        terminal.draw(|f| gallery::render(f, &th)).unwrap();
    });
    measure("help 174x43", || {
        terminal
            .draw(|f| help::render(f, &th, "", black_box(&map)))
            .unwrap();
    });
    measure("wizard 174x43", || {
        terminal
            .draw(|f| wizard::render(f, &th, black_box(&wizard)))
            .unwrap();
    });
    measure("confirm 174x43", || {
        terminal
            .draw(|f| confirm::render(f, &th, f.area(), black_box(&confirm_modal)))
            .unwrap();
    });
    measure("messages 200 lines 174x43", || {
        terminal
            .draw(|f| messages::render(f, &th, black_box(&messages_lines), 0))
            .unwrap();
    });
    let mut detail = DetailState::demo("arch-dev");
    for tab in [
        DetailTab::Overview,
        DetailTab::Hardware,
        DetailTab::Snapshots,
        DetailTab::Console,
        DetailTab::Xml,
    ] {
        detail.tab = tab;
        measure(&format!("detail {tab:?} 174x43"), || {
            terminal
                .draw(|f| detail::render(f, &th, black_box(&detail)))
                .unwrap();
        });
    }

    // ---- input path: Tab completion ----
    let ctx = complete::Context {
        domains: (0..1000)
            .map(|i| (format!("vt-test-{i:04}"), String::from("running")))
            .collect(),
        virsh: (0..285)
            .map(|i| (format!("virsh-cmd-{i:03}"), String::from("does something")))
            .collect(),
        ..Default::default()
    };
    let no_flags = |_c: &str| Vec::new();
    measure("completion command 1000+285", || {
        black_box(complete::complete(black_box("sta"), black_box(&ctx), &no_flags));
    });
    measure("completion domain 1000", || {
        black_box(complete::complete(
            black_box("start vt"),
            black_box(&ctx),
            &no_flags,
        ));
    });
    Ok(())
}
