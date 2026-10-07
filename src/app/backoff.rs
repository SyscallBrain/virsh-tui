//! Reconnect backoff for lost libvirt connections.

/// Delay in seconds before retry N (1, 2, 4, … capped at 30).
pub fn delay_secs(failures: u32) -> u64 {
    (1u64 << failures.min(5)).min(30)
}
