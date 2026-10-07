//! Domain XML parsing with roxmltree.

use color_eyre::{Result, eyre::eyre};

use crate::model::{DiskInfo, DomainConfig, GraphicsInfo, NicInfo};

fn child_text<'a, 'input>(node: roxmltree::Node<'a, 'input>, tag: &str) -> Option<String> {
    node.children()
        .find(|n| n.tag_name().name() == tag)?
        .text()
        .map(|s| s.to_string())
}

fn find_child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    tag: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|n| n.tag_name().name() == tag)
}

/// Parse `virsh dumpxml` into a DomainConfig.
pub fn parse_domain_xml(text: &str) -> Result<DomainConfig> {
    let doc = roxmltree::Document::parse(text).map_err(|e| eyre!("bad xml: {e}"))?;
    let root = doc.root_element();
    let name = root
        .children()
        .find(|n| n.tag_name().name() == "name")
        .and_then(|n| n.text())
        .ok_or_else(|| eyre!("missing <name>"))?
        .to_string();
    let uuid = child_text(root, "uuid").unwrap_or_default();
    let title = child_text(root, "title").unwrap_or_default();
    let description = child_text(root, "description").unwrap_or_default();
    let vcpu_node = find_child(root, "vcpu");
    let vcpus = vcpu_node
        .and_then(|n| n.text())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1);
    let current_vcpus = vcpu_node
        .and_then(|n| n.attribute("current"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(vcpus);
    let os_id = find_child(root, "metadata")
        .and_then(|m| {
            m.descendants()
                .find(|n| n.tag_name().name() == "os" && n.attribute("id").is_some())
        })
        .and_then(|n| n.attribute("id"))
        .unwrap_or("")
        .to_string();
    let cpu = find_child(root, "cpu");
    let cpu_mode = cpu
        .and_then(|c| c.attribute("mode"))
        .unwrap_or("custom")
        .to_string();
    let topology = cpu.and_then(|c| find_child(c, "topology")).and_then(|t| {
        Some((
            t.attribute("sockets")?.parse().ok()?,
            t.attribute("cores")?.parse().ok()?,
            t.attribute("threads")?.parse().ok()?,
        ))
    });
    let acpi = find_child(root, "features").is_some_and(|f| find_child(f, "acpi").is_some());
    let mem_kib = root
        .children()
        .find(|n| n.tag_name().name() == "currentMemory")
        .and_then(|n| n.text())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let max_mem_kib = root
        .children()
        .find(|n| n.tag_name().name() == "memory")
        .and_then(|n| n.text())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(mem_kib);
    let os = root.children().find(|n| n.tag_name().name() == "os");
    let machine = os
        .and_then(|os| os.children().find(|n| n.tag_name().name() == "type"))
        .and_then(|t| t.attribute("machine"))
        .unwrap_or("")
        .to_string();
    let arch = os
        .and_then(|os| os.children().find(|n| n.tag_name().name() == "type"))
        .and_then(|t| t.attribute("arch"))
        .unwrap_or("")
        .to_string();
    let has_loader_efi = os
        .and_then(|os| find_child(os, "loader"))
        .and_then(|l| l.text())
        .is_some_and(|t| t.to_lowercase().contains("ovmf") || t.to_lowercase().contains("efi"));
    let firmware = os
        .and_then(|os| os.attribute("firmware"))
        .map(str::to_string)
        .unwrap_or_else(|| {
            if has_loader_efi {
                String::from("efi")
            } else {
                String::new()
            }
        });
    let mut has_agent = false;
    let devices = find_child(root, "devices");
    let mut disks = Vec::new();
    let mut nics = Vec::new();
    let mut graphics = None;
    if let Some(devices) = devices {
        for dev in devices.children().filter(|n| n.is_element()) {
            match dev.tag_name().name() {
                "disk" if dev.attribute("device") != Some("cdrom") => {
                    disks.push(DiskInfo {
                        target: find_child(dev, "target")
                            .and_then(|t| t.attribute("dev"))
                            .unwrap_or("")
                            .to_string(),
                        source: find_child(dev, "source")
                            .and_then(|s| {
                                s.attribute("file")
                                    .or_else(|| s.attribute("dev"))
                                    .or_else(|| s.attribute("name"))
                            })
                            .unwrap_or("")
                            .to_string(),
                        device: dev.attribute("device").unwrap_or("disk").to_string(),
                        bus: find_child(dev, "target")
                            .and_then(|t| t.attribute("bus"))
                            .unwrap_or("")
                            .to_string(),
                    });
                }
                "disk" => {
                    disks.push(DiskInfo {
                        target: find_child(dev, "target")
                            .and_then(|t| t.attribute("dev"))
                            .unwrap_or("")
                            .to_string(),
                        source: find_child(dev, "source")
                            .and_then(|s| s.attribute("file"))
                            .unwrap_or("")
                            .to_string(),
                        device: String::from("cdrom"),
                        bus: find_child(dev, "target")
                            .and_then(|t| t.attribute("bus"))
                            .unwrap_or("")
                            .to_string(),
                    });
                }
                "interface" => {
                    nics.push(NicInfo {
                        mac: find_child(dev, "mac")
                            .and_then(|m| m.attribute("address"))
                            .unwrap_or("")
                            .to_string(),
                        source: find_child(dev, "source")
                            .and_then(|s| {
                                s.attribute("network")
                                    .or_else(|| s.attribute("bridge"))
                                    .or_else(|| s.attribute("dev"))
                            })
                            .unwrap_or("")
                            .to_string(),
                        model: find_child(dev, "model")
                            .and_then(|m| m.attribute("type"))
                            .unwrap_or("")
                            .to_string(),
                        target: find_child(dev, "target")
                            .and_then(|t| t.attribute("dev"))
                            .unwrap_or("")
                            .to_string(),
                    });
                }
                "channel"
                    if find_child(dev, "target").and_then(|t| t.attribute("name"))
                        == Some("org.qemu.guest_agent.0") =>
                {
                    has_agent = true;
                }
                "graphics" if graphics.is_none() => {
                    graphics = Some(GraphicsInfo {
                        kind: dev.attribute("type").unwrap_or("").to_string(),
                        port: dev.attribute("port").unwrap_or("").to_string(),
                        listen: dev.attribute("listen").unwrap_or("").to_string(),
                    });
                }
                _ => {}
            }
        }
    }
    Ok(DomainConfig {
        name,
        uuid,
        machine,
        firmware,
        vcpus,
        max_mem_kib,
        mem_kib,
        title,
        description,
        disks,
        nics,
        graphics,
        os_id,
        arch,
        current_vcpus,
        cpu_mode,
        topology,
        has_agent,
        acpi,
    })
}

/// Parse `virsh net-dumpxml` into a NetworkConfig.
pub fn parse_network_xml(text: &str) -> Result<crate::model::NetworkConfig> {
    let doc = roxmltree::Document::parse(text).map_err(|e| eyre!("bad xml: {e}"))?;
    let root = doc.root_element();
    let attr = |n: Option<roxmltree::Node>, a: &str| n.and_then(|n| n.attribute(a)).unwrap_or("").to_string();
    let mut cfg = crate::model::NetworkConfig {
        name: child_text(root, "name").unwrap_or_default(),
        uuid: child_text(root, "uuid").unwrap_or_default(),
        ..Default::default()
    };
    let forward = find_child(root, "forward");
    cfg.forward_mode = match forward {
        Some(f) => f.attribute("mode").unwrap_or("nat").to_string(),
        None => String::from("isolated"),
    };
    cfg.forward_dev = attr(forward, "dev");
    cfg.nat_ports = forward
        .and_then(|f| find_child(f, "nat"))
        .and_then(|n| find_child(n, "port"))
        .map(|p| (attr(Some(p), "start"), attr(Some(p), "end")));
    let bridge = find_child(root, "bridge");
    cfg.bridge = attr(bridge, "name");
    cfg.stp = attr(bridge, "stp") != "off";
    cfg.delay = attr(bridge, "delay");
    cfg.mtu = attr(find_child(root, "mtu"), "size");
    cfg.dns_domain = attr(find_child(root, "domain"), "name");
    for ip in root.children().filter(|n| n.tag_name().name() == "ip") {
        let family = ip.attribute("family").unwrap_or("ipv4");
        let addr = ip.attribute("address").unwrap_or("");
        let prefix = ip
            .attribute("prefix")
            .map(str::to_string)
            .or_else(|| ip.attribute("netmask").map(netmask_prefix))
            .unwrap_or_default();
        let cidr = format!("{addr}/{prefix}");
        if family == "ipv6" {
            cfg.ipv6.get_or_insert(cidr);
            continue;
        }
        if cfg.ipv4.is_some() {
            continue;
        }
        cfg.ipv4 = Some(cidr);
        if let Some(dhcp) = find_child(ip, "dhcp") {
            cfg.dhcp_range =
                find_child(dhcp, "range").map(|r| (attr(Some(r), "start"), attr(Some(r), "end")));
            for h in dhcp.children().filter(|n| n.tag_name().name() == "host") {
                cfg.static_hosts
                    .push((attr(Some(h), "mac"), attr(Some(h), "name"), attr(Some(h), "ip")));
            }
        }
    }
    Ok(cfg)
}

/// Parsed `vol-dumpxml`: (format, capacity bytes, allocation bytes, backing path).
pub fn parse_volume_xml(text: &str) -> Result<(String, u64, u64, Option<String>)> {
    let doc = roxmltree::Document::parse(text).map_err(|e| eyre!("bad xml: {e}"))?;
    let root = doc.root_element();
    let bytes = |tag: &str| -> u64 {
        let Some(n) = find_child(root, tag) else { return 0 };
        let v: f64 = n.text().and_then(|t| t.trim().parse().ok()).unwrap_or(0.0);
        let mult: f64 = match n.attribute("unit").unwrap_or("bytes") {
            "KiB" | "K" | "k" => 1024.0,
            "MiB" | "M" => 1024.0 * 1024.0,
            "GiB" | "G" => 1024.0 * 1024.0 * 1024.0,
            "TiB" | "T" => 1024.0_f64.powi(4),
            "KB" => 1000.0,
            "MB" => 1e6,
            "GB" => 1e9,
            "TB" => 1e12,
            _ => 1.0,
        };
        (v * mult) as u64
    };
    let format = find_child(root, "target")
        .and_then(|t| find_child(t, "format"))
        .and_then(|f| f.attribute("type"))
        .unwrap_or("—")
        .to_string();
    let backing = find_child(root, "backingStore")
        .and_then(|b| find_child(b, "path"))
        .and_then(|p| p.text())
        .map(|p| p.trim().to_string());
    Ok((format, bytes("capacity"), bytes("allocation"), backing))
}

/// `255.255.255.0` → `24`.
fn netmask_prefix(mask: &str) -> String {
    mask.split('.')
        .map(|o| o.parse::<u8>().unwrap_or(0).count_ones())
        .sum::<u32>()
        .to_string()
}

#[cfg(test)]
mod net_tests {
    #[test]
    fn parses_default_network() {
        let xml = "<network><name>default</name><uuid>u</uuid>\
            <forward mode='nat' dev='enp6s0'><nat><port start='1024' end='65535'/></nat></forward>\
            <bridge name='virbr0' stp='on' delay='0'/><mtu size='1500'/><domain name='lab.local'/>\
            <ip address='192.168.122.1' netmask='255.255.255.0'><dhcp>\
            <range start='192.168.122.2' end='192.168.122.254'/>\
            <host mac='52:54:00:11:c0:01' name='k8s-cp-01' ip='192.168.122.10'/></dhcp></ip></network>";
        let c = super::parse_network_xml(xml).unwrap();
        assert_eq!(c.forward_mode, "nat");
        assert_eq!(c.forward_dev, "enp6s0");
        assert_eq!(c.ipv4.as_deref(), Some("192.168.122.1/24"));
        assert_eq!(c.static_hosts.len(), 1);
        assert_eq!(c.dns_domain, "lab.local");
        let iso =
            super::parse_network_xml("<network><name>x</name><bridge name='virbr1'/></network>").unwrap();
        assert_eq!(iso.forward_mode, "isolated");
    }
}

#[cfg(test)]
mod tests {
    use super::parse_domain_xml;

    #[test]
    fn parses_arch_fixture() {
        let text = std::fs::read_to_string("tests/fixtures/virsh/dumpxml_arch.txt").unwrap();
        let cfg = parse_domain_xml(&text).unwrap();
        assert_eq!(cfg.name, "archlinux-install");
        assert_eq!(cfg.vcpus, 2);
        assert_eq!(cfg.machine, "pc-q35-11.0");
        assert_eq!(cfg.os_label(), "archlinux rolling");
        assert_eq!(cfg.firmware_label(), "UEFI");
        assert_eq!(cfg.cpu_mode, "host-passthrough");
        assert!(cfg.has_agent);
        assert!(cfg.acpi);
    }
}
