//! Network builders: net-* commands and lease pinning.

use super::super::plan::CommandPlan;

/// `virsh net-start net`.
pub fn start(net: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["net-start", net], &format!("Started network {net}"))
}

/// `virsh net-destroy net` (confirm).
pub fn destroy(net: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["net-destroy", net],
        &format!("Destroyed network {net}"),
    )
}

/// `virsh net-autostart net [--disable]`.
pub fn autostart(net: &str, enable: bool) -> CommandPlan {
    if enable {
        CommandPlan::single(
            "virsh",
            vec!["net-autostart", net],
            &format!("Autostart on {net}"),
        )
    } else {
        CommandPlan::single(
            "virsh",
            vec!["net-autostart", net, "--disable"],
            &format!("Autostart off {net}"),
        )
    }
}

/// `virsh net-undefine net` (confirm).
pub fn undefine(net: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["net-undefine", net],
        &format!("Undefined network {net}"),
    )
}

/// Pin a static lease: `net-update net add ip-dhcp-host "<host …/>" --live --config`.
pub fn pin_lease(net: &str, mac: &str, name: &str, ip: &str) -> CommandPlan {
    // The hostname comes from the guest's DHCP request: escape everything.
    let e = crate::xml::escape;
    let host = if name.is_empty() || name == "-" {
        format!("<host mac='{}' ip='{}'/>", e(mac), e(ip))
    } else {
        format!("<host mac='{}' name='{}' ip='{}'/>", e(mac), e(name), e(ip))
    };
    CommandPlan::single(
        "virsh",
        vec![
            "net-update",
            net,
            "add",
            "ip-dhcp-host",
            &host,
            "--live",
            "--config",
        ],
        &format!("Pinned {ip} on {net}"),
    )
}

/// Remove a static lease.
pub fn unpin_lease(net: &str, mac: &str) -> CommandPlan {
    let host = format!("<host mac='{}'/>", crate::xml::escape(mac));
    CommandPlan::single(
        "virsh",
        vec![
            "net-update",
            net,
            "delete",
            "ip-dhcp-host",
            &host,
            "--live",
            "--config",
        ],
        &format!("Removed static lease on {net}"),
    )
}

/// Generate `<network>` XML for the new-network form.
pub fn define_xml(
    name: &str,
    mode: &str,
    bridge: &str,
    cidr: &str,
    dhcp_start: &str,
    dhcp_end: &str,
    dns_domain: &str,
) -> String {
    let e = crate::xml::escape;
    let forward = match mode {
        "nat" => "  <forward mode='nat'/>\n",
        "route" => "  <forward mode='route'/>\n",
        "open" => "  <forward mode='open'/>\n",
        // Host bridge: libvirt only references an existing bridge (no IP/DHCP).
        "bridge" => "  <forward mode='bridge'/>\n",
        _ => "",
    };
    let ip_block = if mode == "bridge" {
        format!("  <bridge name='{}'/>\n", e(bridge))
    } else {
        let (_, prefix) = cidr.split_once('/').unwrap_or((cidr, "24"));
        let gateway = gateway_for(cidr).map_or_else(|| gateway_of(cidr), |g| g.to_string());
        format!(
            "  <bridge name='{}' stp='on' delay='0'/>\n  <ip address='{}' prefix='{}'>\n    <dhcp>\n      <range start='{}' end='{}'/>\n    </dhcp>\n  </ip>\n",
            e(bridge),
            e(&gateway),
            e(prefix),
            e(dhcp_start),
            e(dhcp_end)
        )
    };
    let dns = if dns_domain.is_empty() {
        String::new()
    } else {
        format!("  <dns><domain name='{}'/></dns>\n", e(dns_domain))
    };
    format!(
        "<network>\n  <name>{}</name>\n{forward}{ip_block}{dns}</network>\n",
        e(name)
    )
}

/// Parse `a.b.c.d/p` (prefix 8..=30, room for a gateway and a DHCP range).
pub fn parse_cidr(cidr: &str) -> Option<(std::net::Ipv4Addr, u8)> {
    let (addr, prefix) = cidr.trim().split_once('/')?;
    let addr: std::net::Ipv4Addr = addr.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    (8..=30).contains(&prefix).then_some((addr, prefix))
}

/// (network, broadcast) of a CIDR.
pub fn subnet_bounds(cidr: &str) -> Option<(u32, u32)> {
    let (addr, prefix) = parse_cidr(cidr)?;
    let mask = u32::MAX << (32 - u32::from(prefix));
    let network = u32::from(addr) & mask;
    Some((network, network | !mask))
}

/// The host's address on the network: the typed address when it is a host
/// address (`192.168.100.254/24`), else the first host of the subnet
/// (`192.168.100.128/25` → `192.168.100.129`).
pub fn gateway_for(cidr: &str) -> Option<std::net::Ipv4Addr> {
    let (addr, _) = parse_cidr(cidr)?;
    let (network, broadcast) = subnet_bounds(cidr)?;
    let a = u32::from(addr);
    Some(std::net::Ipv4Addr::from(if a == network || a == broadcast {
        network + 1
    } else {
        a
    }))
}

/// Default DHCP range: every host after the gateway (up to the broadcast).
pub fn default_dhcp_range(cidr: &str) -> Option<(String, String)> {
    let (network, broadcast) = subnet_bounds(cidr)?;
    let gateway = u32::from(gateway_for(cidr)?);
    let start = if gateway == network + 1 {
        gateway + 1
    } else {
        network + 1
    };
    let end = if gateway == broadcast - 1 {
        broadcast - 2
    } else {
        broadcast - 1
    };
    (start <= end).then(|| {
        (
            std::net::Ipv4Addr::from(start).to_string(),
            std::net::Ipv4Addr::from(end).to_string(),
        )
    })
}

/// Gateway (.1) for an IPv4 network address (legacy: no prefix known).
pub fn gateway_of(network_addr: &str) -> String {
    let mut parts: Vec<&str> = network_addr.split('.').collect();
    if parts.len() == 4 {
        parts[3] = "1";
        parts.join(".")
    } else {
        network_addr.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{define_xml, pin_lease, start};

    #[test]
    fn exact() {
        assert_eq!(start("default").steps[0].argv, vec!["net-start", "default"]);
        assert!(
            pin_lease("n", "m", "h", "1.2.3.4").steps[0]
                .argv
                .contains(&"--live".to_string())
        );
        let xml = define_xml(
            "lab",
            "isolated",
            "virbr9",
            "10.10.0.0/24",
            "10.10.0.2",
            "10.10.0.254",
            "",
        );
        assert!(xml.contains("<name>lab</name>") && xml.contains("10.10.0.1"));
    }
}
