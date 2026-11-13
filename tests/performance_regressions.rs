//! Behavioral guards for performance-sensitive rendering and searching.
use ratatui::{Terminal, backend::TestBackend};
use virsh_tui::ui::{
    overlays::palette::{PaletteItem, PaletteState},
    views::detail::{self, DetailState, DetailTab},
    widgets::charts::{self, BrailleMode},
};

#[test]
fn empty_chart_dimensions_are_safe() {
    for mode in [BrailleMode::Line, BrailleMode::Fill, BrailleMode::Down] {
        assert!(charts::braille(&[0.5], 12, 0, mode).is_empty());
        assert_eq!(charts::braille(&[0.5], 0, 3, mode), vec!["", "", ""]);
    }
}

#[test]
fn palette_keeps_distinct_items_with_duplicate_titles() {
    let mut state = PaletteState::demo();
    state.items = ["first", "second"]
        .into_iter()
        .map(|cmd| PaletteItem {
            icon: "●",
            category: "Domain".into(),
            title: "same title".into(),
            key: String::new(),
            virsh_cmd: cmd.into(),
        })
        .collect();
    for query in ["", "same"] {
        state.query = query.into();
        let actual: Vec<_> = state.visible().iter().map(|i| i.virsh_cmd.as_str()).collect();
        assert_eq!(actual, ["first", "second"]);
    }
}

#[test]
fn xml_scroll_does_not_truncate_large_line_numbers() {
    let mut state = DetailState::demo("arch-dev");
    state.tab = DetailTab::Xml;
    state.xml = (1..=70_000).map(|n| format!("<line>{n}</line>\n")).collect();
    state.xml_cursor = 69_000;
    let mut terminal = Terminal::new(TestBackend::new(174, 43)).unwrap();
    terminal
        .draw(|f| detail::render(f, &virsh_tui::theme::builtin::tokyo_night(), &state))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("<line>69001</line>"));
    assert!(!text.contains("<line>3465</line>"));
}

#[allow(clippy::needless_range_loop)]
fn reference_braille(d: &[f64], w: usize, h: usize, mode: BrailleMode) -> Vec<String> {
    let gw = w * 2;
    let gh = h * 4;
    let mut g = vec![vec![false; gw]; gh];
    let mut py: Option<usize> = None;
    for x in 0..gw {
        let v = charts::sample(d, x, gw);
        match mode {
            BrailleMode::Down => {
                let y = (v * (gh - 1) as f64).round() as usize;
                for yy in 0..=y.min(gh - 1) {
                    g[yy][x] = true;
                }
            }
            BrailleMode::Fill => {
                let y = ((1.0 - v) * (gh - 1) as f64).round() as usize;
                for yy in y.min(gh)..gh {
                    g[yy][x] = true;
                }
            }
            BrailleMode::Line => {
                let y = ((1.0 - v) * (gh - 1) as f64).round() as usize;
                let prev = py.unwrap_or(y);
                py = Some(y);
                for yy in prev.min(y)..=prev.max(y).min(gh - 1) {
                    g[yy][x] = true;
                }
            }
        }
    }
    const BITS: [[u32; 2]; 4] = [[0x1, 0x8], [0x2, 0x10], [0x4, 0x20], [0x40, 0x80]];
    let mut rows = Vec::with_capacity(h);
    for r in 0..h {
        let mut s = String::with_capacity(w);
        for c in 0..w {
            let mut code = 0u32;
            for dy in 0..4 {
                for dx in 0..2 {
                    if g[r * 4 + dy][c * 2 + dx] {
                        code |= BITS[dy][dx];
                    }
                }
            }
            s.push(char::from_u32(0x2800 + code).unwrap_or(' '));
        }
        rows.push(s);
    }
    rows
}

#[test]
fn braille_matches_reference_for_all_modes_and_edges() {
    let mut datasets = vec![
        vec![],
        vec![0.0],
        vec![1.0],
        vec![0.5],
        vec![-1.0, 2.0, 0.0, 1.0],
        vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY],
    ];
    for seed in 1..=12 {
        datasets.push(charts::series(seed, 31, 0.5, 0.8, 0.15));
    }
    for data in datasets {
        for w in [0, 1, 2, 13, 60] {
            for h in [1, 2, 3, 8, 17] {
                for mode in [BrailleMode::Fill, BrailleMode::Down, BrailleMode::Line] {
                    assert_eq!(
                        charts::braille(&data, w, h, mode),
                        reference_braille(&data, w, h, mode),
                        "{w}x{h} {mode:?} {data:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn fuzzy_preserves_unicode_ranking_and_tie_order() {
    use nucleo_matcher::{Config, Matcher, Utf32Str};
    let items = [
        "dömäin",
        "dömäin-snapshot",
        "東京-domain",
        "snapshot",
        "snapshot-create-as",
        "start",
        "Snapshot",
        "σnapshot",
    ];
    for query in ["", "snap", "ö", "東京", "ss", "no match", "σ"] {
        let expected = if query.is_empty() {
            items.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        } else {
            let mut matcher = Matcher::new(Config::DEFAULT);
            let mut scored = Vec::new();
            for item in items {
                let mut h = Vec::new();
                let mut n = Vec::new();
                if let Some(score) =
                    matcher.fuzzy_match(Utf32Str::new(item, &mut h), Utf32Str::new(query, &mut n))
                {
                    scored.push((score, item));
                }
            }
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
            scored.into_iter().map(|(_, s)| s.to_string()).collect()
        };
        assert_eq!(
            virsh_tui::ui::overlays::palette::fuzzy_rank(query, &items),
            expected
        );
    }
}

#[test]
fn xml_last_page_and_fold_marker_remain_visible() {
    let mut state = DetailState::demo("arch-dev");
    state.tab = DetailTab::Xml;
    state.xml = (1..=100).map(|n| format!("<line>{n}</line>\n")).collect();
    state.xml_cursor = usize::MAX;
    state.fold.toggle(99);
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal
        .draw(|f| detail::render(f, &virsh_tui::theme::builtin::tokyo_night(), &state))
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("<line>100</line>  …"));
    assert!(!text.contains("<line>1</line>"));
    assert!(text.contains("of 100 · j/k scroll · e edit"));
}

#[test]
fn spark_rows_preserve_interpolation_and_rounding() {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    for data in [
        vec![],
        vec![0.0],
        vec![1.0],
        charts::series(99, 120, 0.5, 0.3, 0.15),
    ] {
        for w in [0, 1, 13, 60] {
            for h in [0, 1, 3, 8] {
                let expected: Vec<String> = (0..h)
                    .map(|r| {
                        (0..w)
                            .map(|x| {
                                let level = (charts::sample(&data, x, w) * h as f64 * 8.0).round() as i32
                                    - (h - 1 - r) as i32 * 8;
                                if level <= 0 {
                                    ' '
                                } else {
                                    BLOCKS[(level - 1).min(7) as usize]
                                }
                            })
                            .collect()
                    })
                    .collect();
                assert_eq!(charts::spark_rows(&data, w, h), expected);
                if h == 1 {
                    assert_eq!(charts::spark(&data, w), expected[0]);
                }
            }
        }
    }
}

#[test]
fn metrics_totals_match_device_deltas_with_missing_fields() {
    use std::time::{Duration, Instant};
    use virsh_tui::{backend::virsh::parse_domstats::parse_domstats_raw, metrics::store::MetricsStore};
    let stats = |multiplier: u64| {
        parse_domstats_raw(&format!(
        "Domain: 'vt-test-metrics'\n  state.state=1\n  block.count=2\n  block.0.name=vda\n  block.1.name=vdb\n  block.0.rd.bytes={}\n  block.1.wr.bytes={}\n  block.0.rd.reqs={}\n  block.1.wr.reqs={}\n  net.count=2\n  net.0.name=vnet0\n  net.1.name=vnet1\n  net.0.rx.bytes={}\n  net.1.tx.bytes={}\n  net.0.rx.drop=3\n  net.1.tx.drop=4\n  net.0.tx.errs=5\n  net.1.rx.errs=6\n",
        multiplier * 100, multiplier * 200, multiplier * 3, multiplier * 7, multiplier * 300, multiplier * 400
    )).unwrap()
    };
    let mut metrics = MetricsStore::default();
    let start = Instant::now();
    metrics.ingest_domains(&stats(1), start);
    metrics.ingest_domains(&stats(2), start + Duration::from_secs(1));
    let now = &metrics.domain("vt-test-metrics").unwrap().now;
    assert_eq!((now.rd_bps, now.wr_bps, now.iops), (100.0, 200.0, 10.0));
    assert_eq!((now.rx_bps, now.tx_bps), (300.0, 400.0));
    assert_eq!(
        (now.rx_total, now.tx_total, now.drops, now.errs),
        (600, 800, 7, 11)
    );
    assert_eq!(now.disks.iter().map(|d| d.rd_bps).sum::<f64>(), now.rd_bps);
    assert_eq!(now.disks.iter().map(|d| d.wr_bps).sum::<f64>(), now.wr_bps);
    assert_eq!(now.nics.iter().map(|n| n.rx_bps).sum::<f64>(), now.rx_bps);
    assert_eq!(now.nics.iter().map(|n| n.tx_bps).sum::<f64>(), now.tx_bps);
}
