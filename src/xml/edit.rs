//! Typed XML edit operations on xmltree (define path).

use color_eyre::{Result, eyre::eyre};
use xmltree::{Element, XMLNode};

fn parse(xml: &str) -> Result<Element> {
    Element::parse(xml.as_bytes()).map_err(|e| eyre!("bad xml: {e}"))
}

fn emit(elem: &Element) -> String {
    pretty(elem)
}

/// Deterministic pretty printer: sorted attributes, 2-space indent.
pub fn pretty(elem: &Element) -> String {
    let mut out = String::new();
    write_elem(elem, 0, &mut out);
    out.push('\n');
    out
}

fn write_elem(elem: &Element, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let mut attrs: Vec<(String, String)> = elem
        .attributes
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(ns) = &elem.namespaces {
        for (prefix, uri) in &ns.0 {
            if prefix == "xmlns" || prefix == "xml" || (prefix.is_empty() && uri.is_empty()) {
                continue;
            }
            if prefix.is_empty() {
                attrs.push((String::from("xmlns"), uri.clone()));
            } else {
                attrs.push((format!("xmlns:{prefix}"), uri.clone()));
            }
        }
    }
    attrs.sort_by(|a, b| a.0.cmp(&b.0));
    let qname = match &elem.prefix {
        Some(prefix) => format!("{prefix}:{}", elem.name),
        None => elem.name.clone(),
    };
    let mut open = format!("<{qname}");
    for (k, v) in &attrs {
        open.push_str(&format!(" {k}=\"{v}\""));
    }
    let elements: Vec<&Element> = elem
        .children
        .iter()
        .filter_map(|c| match c {
            XMLNode::Element(e) => Some(e),
            _ => None,
        })
        .collect();
    let texts: Vec<&str> = elem
        .children
        .iter()
        .filter_map(|c| match c {
            XMLNode::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    let text = texts.concat().trim().to_string();
    if elements.is_empty() && text.is_empty() {
        out.push_str(&format!("{pad}{open}/>\n"));
    } else if elements.is_empty() {
        out.push_str(&format!("{pad}{open}>{text}</{qname}>\n"));
    } else {
        out.push_str(&format!("{pad}{open}>\n"));
        for child in elements {
            write_elem(child, depth + 1, out);
        }
        out.push_str(&format!("{pad}</{qname}>\n"));
    }
}

fn child_mut<'a>(elem: &'a mut Element, name: &str) -> Option<&'a mut Element> {
    elem.children
        .iter_mut()
        .filter_map(|c| match c {
            XMLNode::Element(e) if e.name == name => Some(e),
            _ => None,
        })
        .next()
}

fn ensure_child<'a>(elem: &'a mut Element, name: &str) -> &'a mut Element {
    if !elem
        .children
        .iter()
        .any(|c| matches!(c, XMLNode::Element(e) if e.name == name))
    {
        elem.children.push(XMLNode::Element(Element::new(name)));
    }
    child_mut(elem, name).expect("just inserted")
}

fn set_text(elem: &mut Element, text: &str) {
    elem.children.retain(|c| !matches!(c, XMLNode::Text(_)));
    elem.children.insert(0, XMLNode::Text(text.to_string()));
}

/// Set `<vcpu>` current and maximum.
///
/// Stored as `<vcpu placement='static' current='cur'>max</vcpu>` when they differ.
pub fn set_vcpus(xml: &str, current: u32, max: u32) -> Result<String> {
    let mut root = parse(xml)?;
    let vcpu = ensure_child(&mut root, "vcpu");
    vcpu.attributes
        .insert(String::from("placement"), String::from("static"));
    if current == max {
        vcpu.attributes.remove("current");
    } else {
        vcpu.attributes
            .insert(String::from("current"), current.to_string());
    }
    set_text(vcpu, &max.to_string());
    Ok(emit(&root))
}

/// Set `<cpu><topology sockets cores threads/></cpu>`, creating nodes as needed.
pub fn set_topology(xml: &str, sockets: u32, cores: u32, threads: u32) -> Result<String> {
    let mut root = parse(xml)?;
    let cpu = ensure_child(&mut root, "cpu");
    let topo = ensure_child(cpu, "topology");
    topo.attributes
        .insert(String::from("sockets"), sockets.to_string());
    topo.attributes.insert(String::from("cores"), cores.to_string());
    topo.attributes
        .insert(String::from("threads"), threads.to_string());
    Ok(emit(&root))
}

/// Auto-adjust topology to match maximum: sockets=1, threads=2 when divisible, else threads=1.
pub fn auto_topology(max: u32) -> (u32, u32, u32) {
    if max.is_multiple_of(2) {
        (1, max / 2, 2)
    } else {
        (1, max, 1)
    }
}

/// Set `<cpu mode='...'>`.
pub fn set_cpu_mode(xml: &str, mode: &str) -> Result<String> {
    let mut root = parse(xml)?;
    let cpu = ensure_child(&mut root, "cpu");
    cpu.attributes.insert(String::from("mode"), mode.to_string());
    Ok(emit(&root))
}

/// Set memory (KiB): `<memory>` max and `<currentMemory>` current.
pub fn set_memory(xml: &str, current_kib: u64, max_kib: u64) -> Result<String> {
    let mut root = parse(xml)?;
    let max = ensure_child(&mut root, "memory");
    max.attributes.insert(String::from("unit"), String::from("KiB"));
    set_text(max, &max_kib.to_string());
    let cur = ensure_child(&mut root, "currentMemory");
    cur.attributes.insert(String::from("unit"), String::from("KiB"));
    set_text(cur, &current_kib.to_string());
    Ok(emit(&root))
}

/// Set boot device order (`<os><boot dev=...>` list in order).
pub fn set_boot_order(xml: &str, devices: &[&str]) -> Result<String> {
    let mut root = parse(xml)?;
    let os = ensure_child(&mut root, "os");
    os.children
        .retain(|c| !matches!(c, XMLNode::Element(e) if e.name == "boot"));
    for dev in devices {
        let mut boot = Element::new("boot");
        boot.attributes.insert(String::from("dev"), dev.to_string());
        os.children.push(XMLNode::Element(boot));
    }
    Ok(emit(&root))
}

/// Move a boot device from index to index (J/K reorder).
pub fn move_boot_device(devices: &mut Vec<String>, from: usize, to: usize) {
    if from >= devices.len() || to >= devices.len() {
        return;
    }
    let dev = devices.remove(from);
    devices.insert(to, dev);
}

#[cfg(test)]
mod tests {
    use super::{auto_topology, move_boot_device, set_boot_order, set_memory, set_topology, set_vcpus};

    fn arch() -> String {
        std::fs::read_to_string("tests/fixtures/virsh/dumpxml_arch.txt").unwrap()
    }

    #[test]
    fn vcpus_and_topology() {
        let out = set_vcpus(&arch(), 8, 16).unwrap();
        assert!(out.contains(">16<") && out.contains("current=\"8\"") || out.contains("current='8'"));
        let (s, c, t) = auto_topology(16);
        assert_eq!((s, c, t), (1, 8, 2));
        let topo = set_topology(&arch(), 1, 8, 2).unwrap();
        assert!(topo.contains("cores=\"8\"") || topo.contains("cores='8'"));
        let mem = set_memory(&arch(), 8 * 1024 * 1024, 16 * 1024 * 1024).unwrap();
        assert!(mem.contains("16777216"));
    }

    #[test]
    fn boot_reorder() {
        let mut devs = vec![String::from("hd"), String::from("cdrom"), String::from("network")];
        move_boot_device(&mut devs, 2, 0);
        assert_eq!(devs[0], "network");
        let xml = set_boot_order(&arch(), &["hd", "cdrom"]).unwrap();
        assert!(xml.contains("dev=\"hd\"") || xml.contains("dev='hd'"));
    }
}
