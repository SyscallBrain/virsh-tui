//! Parser for `virsh net-dhcp-leases`.

use crate::model::DhcpLease;

/// Parse the leases table (header + dashes + rows; tolerant of empty).
pub fn parse_leases(text: &str) -> Vec<DhcpLease> {
    let mut out = Vec::new();
    for line in text.lines().skip(2) {
        if line.trim().is_empty() || line.trim_start_matches('-').is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        // Columns: Expiry(2 cols: date + time) MAC Protocol IP [Hostname] [...]
        let (expiry, mac, ip, hostname) = if cols.len() >= 6 {
            let host = cols[5].to_string();
            (
                format!("{} {}", cols[0], cols[1]),
                cols[2].to_string(),
                cols[4].to_string(),
                (host != "-").then_some(host),
            )
        } else {
            (
                cols[0].to_string(),
                cols[1].to_string(),
                cols[3].to_string(),
                None,
            )
        };
        // `192.168.122.48/24` → `192.168.122.48` (net-update needs the bare address).
        let ip = ip.split('/').next().unwrap_or(&ip).to_string();
        // Hostnames come from the guest's DHCP request: never trust them.
        out.push(DhcpLease {
            mac: crate::model::sanitize(&mac),
            ip: crate::model::sanitize(&ip),
            hostname: hostname.map(|h| crate::model::sanitize(&h)),
            expiry: crate::model::sanitize(&expiry),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse_leases;

    #[test]
    fn empty_and_sample() {
        let empty = std::fs::read_to_string("tests/fixtures/virsh/net_leases_default.txt").unwrap();
        assert!(parse_leases(&empty).is_empty());
        let sample = std::fs::read_to_string("tests/fixtures/virsh/net_leases_sample.txt").unwrap();
        let leases = parse_leases(&sample);
        assert_eq!(leases.len(), 2);
        assert_eq!(leases[0].ip, "192.168.122.48");
        assert_eq!(leases[0].expiry, "2026-10-06 14:32:07");
    }
}
