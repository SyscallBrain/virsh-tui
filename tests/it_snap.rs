//! Integration: snapshot create/revert/delete on a `vt-test-*` domain (qemu:///session only).
//!
//! Run with: `cargo test --features it -- --test-threads=1`.

#![cfg(feature = "it")]

const URI: &str = "qemu:///session";
const NAME: &str = "vt-test-p8-snap";

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
fn snapshot_create_revert_delete() {
    let _ = virsh(&["destroy", NAME]);
    let _ = virsh(&["undefine", NAME, "--snapshots-metadata"]);
    let dir = std::env::temp_dir();
    let disk = dir.join("vt-test-p8-snap.qcow2");
    let _ = std::fs::remove_file(&disk);
    assert!(
        std::process::Command::new("qemu-img")
            .args(["create", "-f", "qcow2"])
            .arg(&disk)
            .arg("32M")
            .output()
            .unwrap()
            .status
            .success()
    );
    let xml = dir.join("vt-test-p8-snap.xml");
    std::fs::write(
        &xml,
        format!(
            r"<domain type='kvm'>
  <name>{NAME}</name>
  <memory unit='KiB'>65536</memory>
  <currentMemory unit='KiB'>65536</currentMemory>
  <vcpu placement='static'>1</vcpu>
  <os><type arch='x86_64' machine='pc'>hvm</type><boot dev='hd'/></os>
  <devices>
    <emulator>/usr/bin/qemu-system-x86_64</emulator>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{}'/>
      <target dev='vda' bus='virtio'/>
    </disk>
  </devices>
</domain>",
            disk.to_str().unwrap()
        ),
    )
    .unwrap();
    check(&["define", xml.to_str().unwrap(), "--validate"]);
    check(&["start", NAME]);
    // Same argv shapes the app builders produce.
    check(&["snapshot-create-as", NAME, "--name", "it-snap", "--atomic"]);
    // Safety snapshot then revert to it-snap --running.
    check(&["snapshot-create-as", NAME, "--name", "it-safety", "--atomic"]);
    check(&["snapshot-revert", NAME, "it-snap", "--running"]);
    check(&["snapshot-delete", NAME, "it-safety", "--metadata"]);
    check(&["snapshot-delete", NAME, "it-snap", "--metadata"]);
    check(&["destroy", NAME]);
    check(&["undefine", NAME, "--snapshots-metadata"]);
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&xml);
    let list = check(&["list", "--all", "--name"]);
    assert!(!list.lines().any(|l| l.trim() == NAME));
}
