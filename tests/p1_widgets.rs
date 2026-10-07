//! P1 widget parity tests (RED first): Rust ports must equal the Node fixtures.

use std::path::PathBuf;

fn fixture(name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/charts")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn braille_fill_matches_js() {
    let expected = fixture("braille_fill_7_60x8");
    let data = virsh_tui::ui::widgets::charts::series(7, 90, 0.42, 0.22, 0.1);
    let got = virsh_tui::ui::widgets::charts::braille(
        &data,
        60,
        8,
        virsh_tui::ui::widgets::charts::BrailleMode::Fill,
    );
    assert_eq!(serde_json::to_value(&got).unwrap(), expected);
}

#[test]
fn spark_rows_match_js() {
    let expected = fixture("spark_mem_11_28x3");
    let data = virsh_tui::ui::widgets::charts::series(11, 60, 0.74, 0.06, 0.2);
    let got = virsh_tui::ui::widgets::charts::spark_rows(&data, 28, 3);
    assert_eq!(serde_json::to_value(&got).unwrap(), expected);
}

#[test]
fn hbar_matches_js() {
    let expected = fixture("hbar_042_16");
    let (fill, rest) = virsh_tui::ui::widgets::charts::hbar(0.42, 16);
    let got = serde_json::json!({"fill": fill, "rest": rest});
    assert_eq!(got, expected);
}

#[test]
fn lg_matches_js() {
    let expected = fixture("lg_038_22");
    let (on, off) = virsh_tui::ui::widgets::charts::lg(0.38, 22);
    let got = serde_json::json!({"on": on, "off": off});
    assert_eq!(got, expected);
}
