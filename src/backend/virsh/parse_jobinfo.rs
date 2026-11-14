//! `virsh domjobinfo <domain>`: progress of a migration, save, dump or
//! snapshot with memory.

/// Progress of the domain's active job.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainJob {
    /// `Outgoing migration`, `Save`, `Snapshot`, … (may be empty).
    pub operation: String,
    pub elapsed_ms: u64,
    pub processed: u64,
    pub remaining: u64,
    pub total: u64,
}

impl DomainJob {
    /// Done fraction 0.0–1.0 when libvirt reports a total.
    pub fn fraction(&self) -> Option<f64> {
        (self.total > 0).then(|| (self.total.saturating_sub(self.remaining)) as f64 / self.total as f64)
    }

    /// Seconds left at the average rate so far.
    pub fn eta_secs(&self) -> Option<u64> {
        let done = self.total.saturating_sub(self.remaining);
        if done == 0 || self.remaining == 0 || self.elapsed_ms == 0 {
            return None;
        }
        Some((self.elapsed_ms as f64 * self.remaining as f64 / done as f64 / 1000.0).round() as u64)
    }

    /// `42% · 1.2/4.0 GiB · ~35s left` (only the parts that are known).
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(f) = self.fraction() {
            parts.push(format!("{:.0}%", f * 100.0));
            parts.push(pair(self.total.saturating_sub(self.remaining), self.total));
        } else if self.processed > 0 {
            parts.push(human(self.processed));
        }
        if let Some(eta) = self.eta_secs() {
            parts.push(if eta >= 120 {
                format!("~{}m left", eta.div_ceil(60))
            } else {
                format!("~{eta}s left")
            });
        }
        parts.join(" · ")
    }
}

/// `1.0/4.0 GiB`: both in the unit of `total`.
fn pair(done: u64, total: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut unit = 0;
    let mut scale = 1.0;
    while total as f64 / scale >= 1024.0 && unit + 1 < UNITS.len() {
        scale *= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{done}/{total} B")
    } else {
        format!(
            "{:.1}/{:.1} {}",
            done as f64 / scale,
            total as f64 / scale,
            UNITS[unit]
        )
    }
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit + 1 < UNITS.len() {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// `500.123 MiB` → bytes.
fn bytes(value: &str) -> Option<u64> {
    let mut it = value.split_whitespace();
    let n: f64 = it.next()?.parse().ok()?;
    let mult = match it.next().unwrap_or("B") {
        "B" | "bytes" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}

/// Parse the output; `None` when no job runs (`Job type: None`).
pub fn parse(text: &str) -> Option<DomainJob> {
    let mut job = DomainJob {
        operation: String::new(),
        elapsed_ms: 0,
        processed: 0,
        remaining: 0,
        total: 0,
    };
    let mut active = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Job type" => active = !matches!(value, "None" | "Completed" | "Failed" | "Cancelled"),
            "Operation" => job.operation = value.to_string(),
            "Time elapsed" => {
                job.elapsed_ms = value
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0)
            }
            "Data processed" => job.processed = bytes(value).unwrap_or(0),
            "Data remaining" => job.remaining = bytes(value).unwrap_or(0),
            "Data total" => job.total = bytes(value).unwrap_or(0),
            _ => {}
        }
    }
    active.then_some(job)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_progress() {
        let text = "Job type:         Unbounded
Operation:        Outgoing migration
Time elapsed:     10000        ms
Data processed:   1.000 GiB
Data remaining:   3.000 GiB
Data total:       4.000 GiB
Memory processed: 1.000 GiB
";
        let job = parse(text).unwrap();
        assert_eq!(job.operation, "Outgoing migration");
        assert_eq!(job.fraction(), Some(0.25));
        assert_eq!(job.eta_secs(), Some(30));
        assert_eq!(job.summary(), "25% · 1.0/4.0 GiB · ~30s left");
    }

    #[test]
    fn no_job_and_unknown_total() {
        assert_eq!(parse("Job type:         None\n"), None);
        let job = parse("Job type: Unbounded\nTime elapsed: 500 ms\nData processed: 12.5 MiB\n").unwrap();
        assert_eq!(job.fraction(), None);
        assert_eq!(job.summary(), "12.5 MiB");
    }
}
