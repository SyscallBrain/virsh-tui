//! Core data model.

use serde::{Deserialize, Serialize};

/// Make untrusted text safe to draw: control characters (ESC, BEL, CR, …)
/// are replaced, so guest-controlled strings such as DHCP hostnames cannot
/// inject terminal escape sequences.
pub fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}

/// Domain lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainState {
    Running,
    Paused,
    ShutOff,
    Crashed,
    PmSuspended,
    Other,
}

impl DomainState {
    /// Glyph used in tables.
    pub fn glyph(self) -> char {
        self.glyph_in(crate::theme::IconSet::Unicode)
    }

    /// Glyph for an icon set (`unicode` default, `nerd`, `ascii`).
    pub fn glyph_in(self, set: crate::theme::IconSet) -> char {
        use crate::theme::IconSet as I;
        match set {
            I::Ascii => match self {
                Self::Running => '*',
                Self::Paused => '=',
                Self::ShutOff => 'o',
                Self::Crashed => 'x',
                Self::PmSuspended => '~',
                Self::Other => '?',
            },
            I::Nerd | I::Unicode => match self {
                Self::Running => '\u{25cf}',
                Self::Paused => '\u{2016}',
                Self::ShutOff => '\u{25cb}',
                Self::Crashed => '\u{2717}',
                Self::PmSuspended => '\u{25cc}',
                Self::Other => '?',
            },
        }
    }
}

/// Summary row for the domains table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainSummary {
    pub name: String,
    pub uuid: String,
    pub id: Option<u32>,
    pub state: DomainState,
    pub autostart: bool,
    pub persistent: bool,
    pub vcpus: u32,
    pub max_vcpus: u32,
    pub mem_kib: u64,
    pub max_mem_kib: u64,
    pub has_managed_save: bool,
}

/// Parsed domain configuration (subset needed for P2; extended in P6-P7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainConfig {
    pub name: String,
    pub uuid: String,
    pub machine: String,
    pub firmware: String,
    pub vcpus: u32,
    pub max_mem_kib: u64,
    pub mem_kib: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub disks: Vec<DiskInfo>,
    #[serde(default)]
    pub nics: Vec<NicInfo>,
    #[serde(default)]
    pub graphics: Option<GraphicsInfo>,
    /// libosinfo id (`http://archlinux.org/archlinux/rolling`), when present.
    #[serde(default)]
    pub os_id: String,
    #[serde(default)]
    pub arch: String,
    /// `<vcpu current=…>` (falls back to the maximum).
    #[serde(default)]
    pub current_vcpus: u32,
    /// `<cpu mode=…>` (host-passthrough, host-model, custom).
    #[serde(default)]
    pub cpu_mode: String,
    #[serde(default)]
    pub topology: Option<(u32, u32, u32)>,
    /// A guest-agent channel is configured.
    #[serde(default)]
    pub has_agent: bool,
    /// `<on_poweroff>`/ACPI feature present.
    #[serde(default)]
    pub acpi: bool,
}

impl DomainConfig {
    /// Short OS label from the libosinfo id (`archlinux rolling`), or `—`.
    pub fn os_label(&self) -> String {
        let parts: Vec<&str> = self.os_id.trim_end_matches('/').rsplit('/').take(2).collect();
        match parts.as_slice() {
            [ver, name, ..] if !ver.is_empty() => format!("{name} {ver}"),
            [one] if !one.is_empty() => one.to_string(),
            _ => String::from("—"),
        }
    }

    /// `UEFI` / `BIOS`.
    pub fn firmware_label(&self) -> &'static str {
        if self.firmware == "efi" { "UEFI" } else { "BIOS" }
    }
}

/// Domain disk device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskInfo {
    pub target: String,
    pub source: String,
    pub device: String,
    pub bus: String,
}

/// Domain NIC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NicInfo {
    pub mac: String,
    pub source: String,
    pub model: String,
    pub target: String,
}

/// Graphics device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicsInfo {
    pub kind: String,
    pub port: String,
    pub listen: String,
}

/// Virtual network summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSummary {
    pub name: String,
    pub state: String,
    pub autostart: bool,
    pub persistent: bool,
}

/// DHCP lease row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DhcpLease {
    pub mac: String,
    /// Address without the prefix length (`192.168.122.48`).
    pub ip: String,
    pub hostname: Option<String>,
    /// `2026-10-06 15:31:00` as printed by virsh.
    #[serde(default)]
    pub expiry: String,
}

/// Parsed `<network>` definition.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub name: String,
    pub uuid: String,
    /// `nat`, `route`, `bridge`, `open`, … or `isolated` when there is no `<forward>`.
    pub forward_mode: String,
    pub forward_dev: String,
    pub nat_ports: Option<(String, String)>,
    pub bridge: String,
    pub stp: bool,
    pub delay: String,
    pub mtu: String,
    /// IPv4 `address/prefix` of the host side.
    pub ipv4: Option<String>,
    pub dhcp_range: Option<(String, String)>,
    /// Static hosts: (mac, name, ip).
    pub static_hosts: Vec<(String, String, String)>,
    pub dns_domain: String,
    pub ipv6: Option<String>,
}

/// Storage pool summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolSummary {
    pub name: String,
    pub state: String,
    pub autostart: bool,
    pub persistent: bool,
    pub capacity_mib: u64,
    pub allocation_mib: u64,
    pub available_mib: u64,
}

/// Storage volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    pub pool: String,
    pub name: String,
    pub path: String,
    pub capacity_mib: u64,
}

/// Snapshot with parent link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    pub parent: Option<String>,
    pub state: String,
}

/// Host static info (subset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInfo {
    pub hostname: String,
    pub cpu_model: String,
    pub threads: u32,
    pub mem_kib: u64,
}

/// Libvirt event (subset for P2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibvirtEvent {
    pub timestamp: String,
    pub object: String,
    pub event: String,
    pub detail: String,
    /// Event type as printed by virsh (`lifecycle`, `reboot`, `device-added`, …).
    #[serde(default)]
    pub kind: String,
    /// Object scope: `domain`, `network`, `pool`, or `app` (virsh-tui actions).
    #[serde(default)]
    pub scope: String,
}

#[cfg(test)]
mod sanitize_tests {
    #[test]
    fn strips_escape_sequences() {
        assert_eq!(
            super::sanitize("evil\x1b]52;c;AAAA\x07host"),
            "evil\u{fffd}]52;c;AAAA\u{fffd}host"
        );
    }
}
