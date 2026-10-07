//! Deterministic simulation: seeded random-walk metrics.

use crate::ui::widgets::charts;

/// Pre-fill a history from a mockup seed triple.
pub fn history(seed: u32, n: usize, base: f64, amp: f64, pull: f64) -> Vec<f64> {
    charts::series(seed, n, base, amp, pull)
}

/// Advance a walk by one tick (deterministic RNG passed in).
pub fn tick(rng: &mut charts::Rng, current: f64, base: f64, amp: f64, pull: f64) -> f64 {
    let mut v = current + (rng.next_f64() - 0.5) * amp;
    v += (base - v) * pull;
    v.clamp(0.02, 0.98)
}
