//! Demo backend data transcribed from docs/mockups/Main.dc.html renderVals().

use crate::model::DomainState;

/// Demo domain row: name, state, vcpus, mem label, cpu fraction, uptime, autostart, marked.
pub type DemoDomainRow = (
    &'static str,
    DomainState,
    u32,
    &'static str,
    f64,
    &'static str,
    bool,
    bool,
);
#[allow(clippy::type_complexity)]
pub const DOMAINS: [DemoDomainRow; 15] = [
    (
        "alpine-edge",
        DomainState::Running,
        1,
        "0.3/0.5G",
        0.02,
        "9d 02h",
        true,
        false,
    ),
    (
        "arch-dev",
        DomainState::Running,
        8,
        "12.4/16G",
        0.42,
        "3d 04h",
        true,
        false,
    ),
    (
        "haos",
        DomainState::Running,
        2,
        "2.8/4G",
        0.08,
        "47d 11h",
        true,
        false,
    ),
    (
        "k8s-cp-01",
        DomainState::Running,
        4,
        "5.1/8G",
        0.23,
        "12d 06h",
        true,
        true,
    ),
    (
        "k8s-worker-01",
        DomainState::Running,
        4,
        "6.7/8G",
        0.55,
        "12d 06h",
        true,
        true,
    ),
    (
        "k8s-worker-02",
        DomainState::Running,
        4,
        "1.9/8G",
        0.61,
        "0d 00h",
        true,
        true,
    ),
    (
        "openbsd-fw",
        DomainState::Running,
        2,
        "0.4/1G",
        0.04,
        "47d 11h",
        true,
        false,
    ),
    (
        "truenas-scale",
        DomainState::Running,
        4,
        "14.2/16G",
        0.12,
        "47d 11h",
        true,
        false,
    ),
    (
        "win11-gaming",
        DomainState::Running,
        12,
        "28.6/32G",
        0.87,
        "5h 12m",
        false,
        false,
    ),
    (
        "nixos-lab",
        DomainState::Paused,
        4,
        "3.2/4G",
        0.0,
        "\u{2014}",
        false,
        false,
    ),
    (
        "ubuntu-24-ci",
        DomainState::Crashed,
        2,
        "\u{2014}/4G",
        0.0,
        "\u{2014}",
        false,
        false,
    ),
    (
        "debian-bookworm",
        DomainState::ShutOff,
        2,
        "\u{2014}/2G",
        0.0,
        "\u{2014}",
        false,
        false,
    ),
    (
        "fedora-41-test",
        DomainState::ShutOff,
        4,
        "\u{2014}/4G",
        0.0,
        "\u{2014}",
        false,
        false,
    ),
    (
        "kali-pentest",
        DomainState::ShutOff,
        4,
        "\u{2014}/8G",
        0.0,
        "\u{2014}",
        false,
        false,
    ),
    (
        "windows-srv-22",
        DomainState::ShutOff,
        4,
        "\u{2014}/8G",
        0.0,
        "\u{2014}",
        true,
        false,
    ),
];

/// Demo events: time, glyph-domain, event, detail.
pub const EVENTS: [(&str, &str, &str, &str); 4] = [
    ("14:31:55", "k8s-worker-02", "Started", "Booted"),
    ("14:30:12", "arch-dev", "Device added", "vda resize 120G"),
    ("14:28:44", "win11-gaming", "Suspended", "Paused by user"),
    ("14:25:01", "nixos-lab", "Suspended", "Paused"),
];

/// Demo host gauges: label, fraction, value.
pub const HOST_GAUGES: [(&str, f64, &str); 6] = [
    ("CPU   ", 0.38, "38%"),
    ("Pool  ", 0.66, "612/931G"),
    ("RAM   ", 0.64, "41.2/64G"),
    ("NVMe  ", 0.67, "1.2/1.8T"),
    ("Swap  ", 0.05, "0.4/8G"),
    ("Huge  ", 0.5, "8/16 1G"),
];

/// Demo `arch-dev` domain XML (matches docs/mockups/Main.dc.html and Detail.dc.html).
///
/// Embedded in the binary: runtime code must never read files from `tests/`.
pub const ARCH_DEV_XML: &str = include_str!("arch-dev.xml");
