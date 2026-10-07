//! Integration: XML edit round-trip on a `vt-test-*` domain (qemu:///session only).
//!
//! Run with: `cargo test --features it -- --test-threads=1`.

#![cfg(feature = "it")]

const URI: &str = "qemu:///session";
const NAME: &str = "vt-test-p6-xml";

fn virsh(args: &[&str]) -> std::process::Output {
    std::process::Command::new("virsh")
        .arg("-c")
        .arg(URI)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .expect("virsh runs")
}

fn check(args: &[&str]) -> String {
    let out = virsh(args);
    assert!(
        out.status.success(),
        "virsh {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn xml_edit_round_trip() {
    let _ = virsh(&["destroy", NAME]);
    let _ = virsh(&["undefine", NAME]);
    let dir = std::env::temp_dir();
    let xml = dir.join("vt-test-p6-xml.xml");
    std::fs::write(
        &xml,
        format!(
            r"<domain type='kvm'>
  <name>{NAME}</name>
  <memory unit='KiB'>65536</memory>
  <currentMemory unit='KiB'>65536</currentMemory>
  <vcpu placement='static'>1</vcpu>
  <os><type arch='x86_64' machine='pc'>hvm</type><boot dev='hd'/></os>
  <devices><emulator>/usr/bin/qemu-system-x86_64</emulator></devices>
</domain>"
        ),
    )
    .unwrap();
    check(&["define", xml.to_str().unwrap(), "--validate"]);

    // Parse, set description via the app's xmltree path, define, verify.
    let dumpxml = check(&["dumpxml", NAME, "--inactive"]);
    let mut elem: xmltree::Element = xmltree::Element::parse(dumpxml.as_bytes()).unwrap();
    let mut desc = xmltree::Element::new("description");
    desc.children
        .push(xmltree::XMLNode::Text(String::from("it-round-trip")));
    elem.children.insert(0, xmltree::XMLNode::Element(desc));
    let mut out = Vec::new();
    elem.write(&mut out).unwrap();
    std::fs::write(&xml, &out).unwrap();
    check(&["define", xml.to_str().unwrap(), "--validate"]);
    let again = check(&["dumpxml", NAME, "--inactive"]);
    assert!(again.contains("it-round-trip"));

    let _ = virsh(&["undefine", NAME]);
    let _ = std::fs::remove_file(&xml);
    let list = check(&["list", "--all", "--name"]);
    assert!(!list.lines().any(|l| l.trim() == NAME));
}
