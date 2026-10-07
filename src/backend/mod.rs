//! Backend trait and shared types.

pub mod demo;
pub mod virsh;

use color_eyre::Result;

use crate::model::{DhcpLease, DomainSummary, HostInfo, NetworkSummary, PoolSummary, Snapshot};

/// Raw per-domain stats from `virsh domstats --raw`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawDomainStats {
    pub domain: String,
    pub values: std::collections::HashMap<String, String>,
}

impl RawDomainStats {
    /// Get a u64 field.
    pub fn u64(&self, key: &str) -> Option<u64> {
        self.values.get(key)?.parse().ok()
    }
}

/// Read-only backend interface (mutations arrive in P4 via CommandPlan).
#[async_trait::async_trait]
pub trait Backend: Send + Sync {
    /// List domain summaries.
    async fn list_domains(&self) -> Result<Vec<DomainSummary>>;
    /// All raw domain stats in one call.
    async fn all_domain_stats(&self) -> Result<Vec<RawDomainStats>>;
    /// Host static info.
    async fn host_info(&self) -> Result<HostInfo>;
    /// Virtual networks.
    async fn networks(&self) -> Result<Vec<NetworkSummary>>;
    /// DHCP leases for a network.
    async fn dhcp_leases(&self, net: &str) -> Result<Vec<DhcpLease>>;
    /// Storage pools.
    async fn pools(&self) -> Result<Vec<PoolSummary>>;
    /// Snapshots for a domain (parent links included).
    async fn snapshots(&self, domain: &str) -> Result<Vec<Snapshot>>;
}
