//! gauge widget (PLAN.md section 7).
//! LineGauge widget: thin wrapper over charts::lg.
pub use super::charts::{lg, lvl};
/// Render a line gauge cell.
pub fn cell(v: f64, w: usize, value: &str) -> String {
    let (on, off) = lg(v, w);
    format!("{on}{off} {value}")
}
