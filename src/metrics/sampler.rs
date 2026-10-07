//! Sampler: turn cumulative libvirt counters into rates.

/// CPU percent from `cpu.time` nanosecond counters.
///
/// `before`/`after` are cumulative ns values, `wall_ns` is the elapsed wall time in ns,
/// and `vcpus` is the current vCPU count. Result is clamped to 0-100.
pub fn cpu_pct(before: u64, after: u64, wall_ns: u64, vcpus: u32) -> f64 {
    if wall_ns == 0 || vcpus == 0 {
        return 0.0;
    }
    let after = after.max(before);
    let delta = (after - before) as f64;
    (delta / (wall_ns as f64 * f64::from(vcpus)) * 100.0).clamp(0.0, 100.0)
}

/// Byte rate from cumulative counters.
pub fn rate(before: u64, after: u64, wall_secs: f64) -> f64 {
    if wall_secs <= 0.0 {
        return 0.0;
    }
    let after = after.max(before);
    (after - before) as f64 / wall_secs
}

/// Guest memory used from balloon counters (KiB).
pub fn mem_used_kib(available: u64, unused: u64) -> u64 {
    available.saturating_sub(unused)
}

#[cfg(test)]
mod tests {
    use super::{cpu_pct, mem_used_kib, rate};

    #[test]
    fn cpu_clamps() {
        assert_eq!(cpu_pct(0, 10, 0, 1), 0.0);
        assert!(cpu_pct(0, u64::MAX, 1, 1) <= 100.0);
    }

    #[test]
    fn counter_reset_gives_zero() {
        assert_eq!(rate(100, 50, 1.0), 0.0);
        assert_eq!(mem_used_kib(100, 200), 0);
    }
}
