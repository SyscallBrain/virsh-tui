//! Table parsers: virsh list, net-list, pool-list, snapshot-list.

use color_eyre::{Result, eyre::eyre};

use crate::model::{DomainState, NetworkSummary, PoolSummary, Snapshot};

/// Split a virsh table (header, dashes, rows) by the header's column offsets.
///
/// virsh pads every cell to its column width, so the offsets of the header
/// labels hold for each row; this keeps names with spaces (`my vol.iso`) and
/// placeholder cells (`-`) in their column. A row whose text straddles a
/// column boundary (older, unpadded output) falls back to splitting on
/// whitespace.
pub fn table_rows(text: &str) -> Vec<Vec<String>> {
    let mut lines = text.lines().skip_while(|l| l.trim().is_empty());
    let Some(header) = lines.next() else {
        return vec![];
    };
    let header: Vec<char> = header.chars().collect();
    // Column starts: the start of every header word, except words that
    // continue a multi-word label (`Creation Time`).
    let mut starts: Vec<usize> = Vec::new();
    let mut previous_word = String::new();
    let mut i = 0;
    while i < header.len() {
        if header[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < header.len() && !header[i].is_whitespace() {
            i += 1;
        }
        let word: String = header[start..i].iter().collect();
        let continues = previous_word == "Creation" && word == "Time";
        if !continues {
            starts.push(start);
        }
        previous_word = word;
    }
    let columns = starts.len();
    let mut out = Vec::new();
    for line in lines {
        let t = line.trim();
        if t.is_empty() || t.chars().all(|c| c == '-') {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let straddles = starts
            .iter()
            .skip(1)
            .any(|&s| s < chars.len() && s > 0 && !chars[s - 1].is_whitespace() && !chars[s].is_whitespace());
        let cells: Vec<String> = if straddles || columns == 0 {
            t.split_whitespace().map(str::to_string).collect()
        } else {
            starts
                .iter()
                .enumerate()
                .map(|(i, &s)| {
                    let e = starts.get(i + 1).copied().unwrap_or(chars.len()).min(chars.len());
                    chars
                        .get(s.min(chars.len())..e)
                        .map_or(String::new(), |c| c.iter().collect::<String>().trim().to_string())
                })
                .collect()
        };
        out.push(cells);
    }
    out
}

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
    for cols in table_rows(text) {
        if cols.len() < 3 {
            return Err(eyre!("short list row: {cols:?}"));
        }
        let id_raw = &cols[0];
        let name = cols[1].clone();
        let state_text = cols[2..].join(" ");
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
    for cols in table_rows(text) {
        if cols.len() < 4 {
            continue;
        }
        out.push(NetworkSummary {
            name: cols[0].clone(),
            state: cols[1].clone(),
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

/// `236.03 GiB` (one cell) into MiB; `-` (inactive pool) is 0.
fn cell_mib(cell: &str) -> u64 {
    let mut parts = cell.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some(v), Some(unit)) => size_to_mib(v, unit),
        _ => 0,
    }
}

/// Parse `virsh pool-list --all --details` (inactive pools show `-` sizes).
pub fn parse_pool_list(text: &str) -> Result<Vec<PoolSummary>> {
    let mut out = Vec::new();
    for cols in table_rows(text) {
        if cols.len() < 4 {
            continue;
        }
        let cell = |i: usize| cols.get(i).map_or("", String::as_str);
        out.push(PoolSummary {
            name: cols[0].clone(),
            state: cols[1].clone(),
            autostart: cols[2] == "yes",
            persistent: cols[3] == "yes",
            capacity_mib: cell_mib(cell(4)),
            allocation_mib: cell_mib(cell(5)),
            available_mib: cell_mib(cell(6)),
        });
    }
    Ok(out)
}

/// Parse `virsh snapshot-list <d> --parent` (tolerant: empty or header-only gives no rows).
///
/// Columns: Name | Creation Time | State | Parent.
pub fn parse_snapshot_list(text: &str) -> Vec<Snapshot> {
    let mut out = Vec::new();
    for cols in table_rows(text) {
        if cols.len() < 3 {
            continue;
        }
        // Unpadded output falls back to whitespace cells: the creation time
        // is then three cells (date, time, zone).
        let (created, state, parent) = if !cols[1].contains(char::is_whitespace) && cols.len() >= 5 {
            (cols[1..4].join(" "), cols[4].clone(), cols.get(5))
        } else {
            (cols[1].clone(), cols[2].clone(), cols.get(3))
        };
        let parent = parent.filter(|p| !p.is_empty() && *p != "-").cloned();
        out.push(Snapshot {
            name: cols[0].clone(),
            parent,
            state,
            created,
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
    fn inactive_pools_and_names_with_spaces() {
        let pools = " Name           State      Autostart   Persistent   Capacity   Allocation   Available\n---------------------------------------------------------------------------------------\n default-pool   inactive   no          yes          -          -            -\n";
        let parsed = parse_pool_list(pools).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].state, "inactive");
        let vols = " Name         Path                       Type   Capacity   Allocation\n-----------------------------------------------------------------------\n my vol.iso   /default-pool/my vol.iso   file   1.00 MiB   1.00 MiB\n";
        let rows = super::table_rows(vols);
        assert_eq!(rows[0][0], "my vol.iso");
        assert_eq!(rows[0][1], "/default-pool/my vol.iso");
    }

    #[test]
    fn snapshot_sample_parents() {
        let snaps = parse_snapshot_list(&fixture("snapshot_list_sample"));
        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[1].parent.as_deref(), Some("snap-base"));
        assert_eq!(snaps[0].created, "2026-09-01 10:00:00 +0000");
        assert_eq!(snaps[0].state, "shutoff");
    }
}
