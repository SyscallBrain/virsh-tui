//! Integration: define/undefine an isolated `vt-test-net` network (qemu:///session only).
//!
//! Unprivileged sessions cannot create bridge interfaces (`net-start` fails
//! with Operation not permitted), so this exercises define/dumpxml/undefine.
//! Run with: `cargo test --features it -- --test-threads=1`.

#![cfg(feature = "it")]

const URI: &str = "qemu:///session";
const NAME: &str = "vt-test-net";

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
fn isolated_network_lifecycle() {
    let _ = virsh(&["net-destroy", NAME]);
    let _ = virsh(&["net-undefine", NAME]);
    let dir = std::env::temp_dir();
    let xml = dir.join("vt-test-net.xml");
    std::fs::write(
        &xml,
        format!(
            "<network>\n  <name>{NAME}</name>\n  <bridge name='virbr9' stp='on' delay='0'/>\n  <ip address='10.20.0.1' prefix='24'>\n    <dhcp>\n      <range start='10.20.0.2' end='10.20.0.254'/>\n    </dhcp>\n  </ip>\n</network>\n"
        ),
    )
    .unwrap();
    check(&["net-define", xml.to_str().unwrap()]);
    let dumpxml = check(&["net-dumpxml", NAME]);
    assert!(dumpxml.contains("10.20.0.1"));
    check(&["net-autostart", NAME]);
    check(&["net-autostart", NAME, "--disable"]);
    check(&["net-undefine", NAME]);
    let _ = std::fs::remove_file(&xml);
    let list = check(&["net-list", "--all", "--name"]);
    assert!(!list.lines().any(|l| l.trim() == NAME));
}
