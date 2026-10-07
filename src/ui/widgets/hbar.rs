//! hbar widget (PLAN.md section 7).
//! HBar widget: thin wrapper over charts::hbar.
pub use super::charts::hbar;
/// Render an HBar cell: bar + right-aligned percent.
pub fn cell(v: f64, w: usize) -> String {
    let (fill, rest) = hbar(v, w);
    format!("{fill}{rest}{:>5}", format!("{}%", (v * 100.0).round() as u32))
}
