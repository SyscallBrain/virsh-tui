//! Integration: vCPU + memory changes on a `vt-test-*` domain (qemu:///session only).
//!
//! Run with: `cargo test --features it -- --test-threads=1`.

#![cfg(feature = "it")]

const URI: &str = "qemu:///session";
const NAME: &str = "vt-test-p7-hw";

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
fn vcpu_and_memory_native_commands() {
    let _ = virsh(&["destroy", NAME]);
    let _ = virsh(&["undefine", NAME]);
    let dir = std::env::temp_dir();
    let disk = dir.join("vt-test-p7-hw.qcow2");
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
    let xml = dir.join("vt-test-p7-hw.xml");
    std::fs::write(
        &xml,
        format!(
            r"<domain type='kvm'>
  <name>{NAME}</name>
  <memory unit='KiB'>131072</memory>
  <currentMemory unit='KiB'>65536</currentMemory>
  <vcpu placement='static' current='1'>2</vcpu>
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
    // vCPU hot-plug via the same builder argv the app runs.
    check(&["setvcpus", NAME, "2", "--live", "--config"]);
    // Memory balloon via the same builder argv.
    check(&["setmem", NAME, "131072", "--live", "--config"]);
    let stats = check(&["domstats", NAME, "--raw"]);
    assert!(stats.contains("vcpu.current=2") || stats.contains("balloon.current=131072"));
    check(&["destroy", NAME]);
    check(&["undefine", NAME]);
    let _ = std::fs::remove_file(&disk);
    let _ = std::fs::remove_file(&xml);
    let list = check(&["list", "--all", "--name"]);
    assert!(!list.lines().any(|l| l.trim() == NAME));
}
