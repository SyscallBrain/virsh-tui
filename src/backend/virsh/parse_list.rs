//! Table parsers: virsh list, net-list, pool-list, snapshot-list.

use color_eyre::{Result, eyre::eyre};

use crate::model::{DomainState, NetworkSummary, PoolSummary, Snapshot};

/// Row of `virsh list --all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    pub id: Option<u32>,
    pub name: String,
    pub state: DomainState,
}

/// Parse `virsh list --all` output.
pub fn parse_list_all(text: &str) -> Result<Vec<ListRow>> {
    let mut rows = Vec::new();
    for line in text.lines().skip(2) {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let id_raw = parts.next().ok_or_else(|| eyre!("empty list row"))?;
        let name = parts.next().ok_or_else(|| eyre!("missing name"))?.to_string();
        let state_text: String = parts.collect::<Vec<_>>().join(" ");
        let id = if id_raw == "-" { None } else { id_raw.parse().ok() };
        rows.push(ListRow {
            id,
            name,
            state: parse_state(&state_text),
        });
    }
    Ok(rows)
}

fn parse_state(s: &str) -> DomainState {
    match s.trim() {
        "running" => DomainState::Running,
        "paused" => DomainState::Paused,
        "shut off" => DomainState::ShutOff,
        "shutoff" => DomainState::ShutOff,
        "crashed" => DomainState::Crashed,
        s if s.starts_with("pmsuspended") => DomainState::PmSuspended,
        _ => DomainState::Other,
    }
}

/// Parse `virsh net-list --all`.
pub fn parse_net_list(text: &str) -> Result<Vec<NetworkSummary>> {
    let mut out = Vec::new();
    for line in text.lines().skip(2) {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        out.push(NetworkSummary {
            name: cols[0].to_string(),
            state: cols[1].to_string(),
            autostart: cols[2] == "yes",
            persistent: cols[3] == "yes",
        });
    }
    Ok(out)
}

/// Parse a size like `236.03 GiB` into MiB.
pub fn size_to_mib(s: &str, unit: &str) -> u64 {
    let v: f64 = s.parse().unwrap_or(0.0);
    let mult = match unit {
        "KiB" => 1.0 / 1024.0,
        "MiB" => 1.0,
        "GiB" => 1024.0,
        "TiB" => 1024.0 * 1024.0,
        _ => 0.0,
    };
    (v * mult).round() as u64
}

/// Parse `virsh pool-list --all --details`.
pub fn parse_pool_list(text: &str) -> Result<Vec<PoolSummary>> {
    let mut out = Vec::new();
    for line in text.lines().skip(2) {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 8 {
            continue;
        }
        out.push(PoolSummary {
            name: cols[0].to_string(),
            state: cols[1].to_string(),
            autostart: cols[2] == "yes",
            persistent: cols[3] == "yes",
            capacity_mib: size_to_mib(cols[4], cols[5]),
            allocation_mib: size_to_mib(cols[6], cols[7]),
            available_mib: if cols.len() >= 10 {
                size_to_mib(cols[8], cols[9])
            } else {
                0
            },
        });
    }
    Ok(out)
}

/// Parse `virsh snapshot-list <d> --parent` (tolerant: empty or header-only gives no rows).
pub fn parse_snapshot_list(text: &str) -> Vec<Snapshot> {
    let mut out = Vec::new();
    for line in text.lines().skip(2) {
        if line.trim().is_empty() {
            continue;
        }
        // Columns: Name | Creation Time (date, time, tz) | State | Parent | Children | Descendants.
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.is_empty() || cols[0] == "Name" || cols[0].starts_with('-') {
            continue;
        }
        if cols.len() < 6 {
            continue;
        }
        let parent = cols.get(5).filter(|p| **p != "-").map(|s| s.to_string());
        out.push(Snapshot {
            name: cols[0].to_string(),
            parent,
            state: cols.get(4).unwrap_or(&"").to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_list_all, parse_net_list, parse_pool_list, parse_snapshot_list};

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/virsh/{name}.txt")).unwrap()
    }

    #[test]
    fn list_all_has_three_shutoff() {
        let rows = parse_list_all(&fixture("list_all")).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.id.is_none()));
    }

    #[test]
    fn net_and_pool_parse() {
        assert_eq!(parse_net_list(&fixture("net_list_details")).unwrap().len(), 2);
        assert_eq!(parse_pool_list(&fixture("pool_list")).unwrap().len(), 1);
    }

    #[test]
    fn snapshot_sample_parents() {
        let snaps = parse_snapshot_list(&fixture("snapshot_list_sample"));
        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[1].parent.as_deref(), Some("snap-base"));
    }
}
