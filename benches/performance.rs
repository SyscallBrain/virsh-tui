//! Reproducible CPU benchmarks; no libvirt connection or terminal required.
use ratatui::{Terminal, backend::TestBackend};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
use virsh_tui::{
    theme,
    ui::{
        overlays::palette,
        views::{
            dashboard::{self, DashboardState},
            detail::{self, DetailState, DetailTab},
            host::{self, HostState},
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
    for query in ["snap", "dömäin"] {
        measure(&format!("fuzzy {query} 1000"), || {
            black_box(palette::fuzzy_rank(black_box(query), black_box(&refs)));
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
        })
        .collect();
    palette.query = "snap".into();
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
    Ok(())
}
