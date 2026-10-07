//! slider widget (PLAN.md section 7).
//! Slider widget (PLAN.md 7.10).
pub fn render(v: f64, w: usize) -> String {
    let n = (v * w as f64).round() as usize;
    format!(
        "{}\u{25cf}{}",
        "\u{2501}".repeat(n),
        "\u{2500}".repeat(w.saturating_sub(n))
    )
}
