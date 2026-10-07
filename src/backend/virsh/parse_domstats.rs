//! Parser for `virsh domstats --raw`.

use super::super::RawDomainStats;
use color_eyre::Result;

/// Parse `virsh domstats --raw` into per-domain key/value maps.
pub fn parse_domstats_raw(text: &str) -> Result<Vec<RawDomainStats>> {
    let mut out: Vec<RawDomainStats> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("Domain: '").and_then(|s| s.strip_suffix('\'')) {
            out.push(RawDomainStats {
                domain: name.to_string(),
                values: std::collections::HashMap::new(),
            });
        } else if let Some(current) = out.last_mut() {
            let trimmed = line.trim();
            if let Some((k, v)) = trimmed.split_once('=') {
                current.values.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::parse_domstats_raw;

    #[test]
    fn parses_fixture_and_running() {
        let text = std::fs::read_to_string("tests/fixtures/virsh/domstats_raw.txt").unwrap();
        let stats = parse_domstats_raw(&text).unwrap();
        assert_eq!(stats.len(), 3);
        assert_eq!(stats[0].u64("vcpu.current"), Some(1));
        let running = std::fs::read_to_string("tests/fixtures/virsh/domstats_running.txt").unwrap();
        let running = parse_domstats_raw(&running).unwrap();
        assert_eq!(running[0].u64("net.0.rx.bytes"), Some(125_000_000));
    }
}
