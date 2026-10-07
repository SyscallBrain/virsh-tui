//! In-memory deterministic demo backend (PLAN.md section 3.2 and 10).

pub mod fixtures;
pub mod sim;

use color_eyre::Result;

use super::{Backend, RawDomainStats};
use crate::model::{DhcpLease, DomainSummary, HostInfo, NetworkSummary, PoolSummary, Snapshot};

/// Fully in-memory backend reproducing the mockup data exactly.
pub struct DemoBackend;

impl DemoBackend {
    /// Create a demo backend.
    pub fn new() -> Self {
        Self
    }

    fn parse_mem_mib(label: &str) -> (u64, u64) {
        // Labels like "12.4/16G" or "—/4G".
        let (used, max) = label.split_once('/').unwrap_or(("0", "0G"));
        (gib_to_kib(used), gib_to_kib(max))
    }
}

impl Default for DemoBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn gib_to_kib(s: &str) -> u64 {
    let s = s.trim().trim_end_matches(['G', 'M', 'T']);
    if s.starts_with('\u{2014}') || s == "-" {
        return 0;
    }
    let v: f64 = s.parse().unwrap_or(0.0);
    (v * 1024.0 * 1024.0).round() as u64
}

#[async_trait::async_trait]
impl Backend for DemoBackend {
    async fn list_domains(&self) -> Result<Vec<DomainSummary>> {
        Ok(fixtures::DOMAINS
            .iter()
            .map(|(name, state, vcpus, mem, _cpu, _up, auto, _mark)| {
                let (mem_kib, max_kib) = Self::parse_mem_mib(mem);
                DomainSummary {
                    name: name.to_string(),
                    uuid: format!("demo-uuid-{name}"),
                    id: (*state == crate::model::DomainState::Running).then_some(1),
                    state: *state,
                    autostart: *auto,
                    persistent: true,
                    vcpus: *vcpus,
                    max_vcpus: *vcpus,
                    mem_kib,
                    max_mem_kib: max_kib,
                    has_managed_save: false,
                }
            })
            .collect())
    }

    async fn all_domain_stats(&self) -> Result<Vec<RawDomainStats>> {
        Ok(vec![])
    }

    async fn host_info(&self) -> Result<HostInfo> {
        Ok(HostInfo {
            hostname: String::from("forge"),
            cpu_model: String::from("Ryzen 9 7950X"),
            threads: 32,
            mem_kib: 64 * 1024 * 1024,
        })
    }

    async fn networks(&self) -> Result<Vec<NetworkSummary>> {
        Ok(vec![NetworkSummary {
            name: String::from("default"),
            state: String::from("active"),
            autostart: true,
            persistent: true,
        }])
    }

    async fn dhcp_leases(&self, _net: &str) -> Result<Vec<DhcpLease>> {
        Ok(vec![DhcpLease {
            mac: String::from("52:54:00:a3:1f:7c"),
            ip: String::from("192.168.122.48"),
            hostname: Some(String::from("arch-dev")),
            expiry: String::from("2026-10-06 15:31:00"),
        }])
    }

    async fn pools(&self) -> Result<Vec<PoolSummary>> {
        Ok(vec![PoolSummary {
            name: String::from("default"),
            state: String::from("running"),
            autostart: true,
            persistent: true,
            capacity_mib: 931 * 1024,
            allocation_mib: 612 * 1024,
            available_mib: 319 * 1024,
        }])
    }

    async fn snapshots(&self, _domain: &str) -> Result<Vec<Snapshot>> {
        Ok(vec![])
    }
}
