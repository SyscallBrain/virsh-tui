//! stacked_bar widget (PLAN.md section 7).
//! Stacked bar (PLAN.md 7.9).
pub const PALETTE: [&str; 9] = [
    "orange", "blue", "magenta", "cyan", "green", "yellow", "teal", "red", "fg2",
];
/// Widths for segments of total width w, each at least 1.
pub fn widths(values: &[f64], w: usize) -> Vec<usize> {
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return vec![0; values.len()];
    }
    values
        .iter()
        .map(|v| ((v / total * w as f64).round() as usize).max(1))
        .collect()
}
