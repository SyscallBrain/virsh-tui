//! Integration: lifecycle on throwaway `vt-test-*` objects (qemu:///session only).
//!
//! Run with: `cargo test --features it -- --test-threads=1`.
//! Never touches objects without the `vt-test-` prefix.

#![cfg(feature = "it")]

const URI: &str = "qemu:///session";
const NAME: &str = "vt-test-p4-lifecycle";

fn virsh(args: &[&str]) -> std::process::Output {
    std::process::Command::new("virsh")
        .arg("-c")
        .arg(URI)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .expect("virsh runs")
}

fn check(args: &[&str]) {
    let out = virsh(args);
    assert!(
        out.status.success(),
        "virsh {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = virsh(&["destroy", NAME]);
        let _ = virsh(&[
            "undefine",
            NAME,
            "--snapshots-metadata",
            "--managed-save",
            "--nvram",
        ]);
    }
}

fn minimal_xml(name: &str, disk: &std::path::Path) -> String {
    let disk = disk.to_str().unwrap();
    format!(
        r"<domain type='kvm'>
  <name>{name}</name>
  <uuid>00000000-0000-4000-8000-000000000004</uuid>
  <memory unit='KiB'>65536</memory>
  <currentMemory unit='KiB'>65536</currentMemory>
  <vcpu placement='static'>1</vcpu>
  <os><type arch='x86_64' machine='pc'>hvm</type><boot dev='hd'/></os>
  <devices>
    <emulator>/usr/bin/qemu-system-x86_64</emulator>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{disk}'/>
      <target dev='vda' bus='virtio'/>
    </disk>
  </devices>
</domain>"
    )
}

#[test]
fn lifecycle_define_start_pause_destroy_undefine() {
    let _guard = Guard;
    let _ = virsh(&["destroy", NAME]);
    let _ = virsh(&[
        "undefine",
        NAME,
        "--snapshots-metadata",
        "--managed-save",
        "--nvram",
    ]);

    let dir = std::env::temp_dir();
    let disk = dir.join("vt-test-p4-lifecycle.qcow2");
    let _ = std::fs::remove_file(&disk);
    let mk = std::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(&disk)
        .arg("32M")
        .output()
        .expect("qemu-img runs");
    assert!(mk.status.success());
    let xml = dir.join("vt-test-p4-lifecycle.xml");
    std::fs::write(&xml, minimal_xml(NAME, &disk)).unwrap();

    check(&["define", xml.to_str().unwrap(), "--validate"]);
    check(&["autostart", NAME, "--disable"]);
    check(&["start", NAME]);
    check(&["suspend", NAME]);
    check(&["resume", NAME]);
    // Snapshot round-trip on the running domain.
    check(&["snapshot-create-as", NAME, "--name", "it-snap", "--atomic"]);
    check(&["snapshot-delete", NAME, "it-snap", "--metadata"]);
    check(&["destroy", NAME]);
    check(&[
        "undefine",
        NAME,
        "--snapshots-metadata",
        "--managed-save",
        "--nvram",
    ]);
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&xml);

    let out = virsh(&["list", "--all", "--name"]);
    let names = String::from_utf8_lossy(&out.stdout);
    assert!(!names.lines().any(|l| l.trim() == NAME));
}
