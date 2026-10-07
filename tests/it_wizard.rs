//! Integration: create a `vt-test-vm` via the wizard model (no ISO), then undefine.
//!
//! Run with: `cargo test --features it -- --test-threads=1`.

#![cfg(feature = "it")]

const URI: &str = "qemu:///session";
const NAME: &str = "vt-test-vm";

fn virsh(args: &[&str]) -> std::process::Output {
    std::process::Command::new("virsh")
        .arg("-c")
        .arg(URI)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .expect("virsh runs")
}

#[test]
fn wizard_model_define_undefine() {
    let _ = virsh(&["destroy", NAME]);
    let _ = virsh(&["undefine", NAME]);
    // Wizard model: no ISO, --boot hd, no disk, 64 MiB (defined directly;
    // virt-install needs install media on unprivileged sessions).
    let dir = std::env::temp_dir();
    let xml = dir.join("vt-test-vm.xml");
    std::fs::write(
        &xml,
        format!(
            "<domain type='kvm'>\n  <name>{NAME}</name>\n  <memory unit='KiB'>65536</memory>\n  <currentMemory unit='KiB'>65536</currentMemory>\n  <vcpu placement='static'>1</vcpu>\n  <os><type arch='x86_64' machine='pc'>hvm</type><boot dev='hd'/></os>\n  <devices><emulator>/usr/bin/qemu-system-x86_64</emulator></devices>\n</domain>\n"
        ),
    )
    .unwrap();
    let out = virsh(&["define", xml.to_str().unwrap(), "--validate"]);
    assert!(
        out.status.success(),
        "define failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let list = virsh(&["list", "--all", "--name"]);
    assert!(
        String::from_utf8_lossy(&list.stdout)
            .lines()
            .any(|l| l.trim() == NAME)
    );
    let out = virsh(&["undefine", NAME]);
    assert!(out.status.success());
    let _ = std::fs::remove_file(&xml);
    let list = virsh(&["list", "--all", "--name"]);
    assert!(
        !String::from_utf8_lossy(&list.stdout)
            .lines()
            .any(|l| l.trim() == NAME)
    );
}
