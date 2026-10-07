//! Integration: vol-create/resize/clone/delete in a `vt-test-pool` dir pool.
//!
//! Run with: `cargo test --features it -- --test-threads=1`.

#![cfg(feature = "it")]

const URI: &str = "qemu:///session";
const POOL: &str = "vt-test-pool";

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
fn volume_lifecycle_in_dir_pool() {
    let _ = virsh(&["pool-destroy", POOL]);
    let _ = virsh(&["pool-undefine", POOL]);
    let dir = std::env::temp_dir().join("vt-test-pool");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    check(&["pool-define-as", POOL, "dir", "--target", dir.to_str().unwrap()]);
    check(&["pool-build", POOL]);
    check(&["pool-start", POOL]);
    check(&["pool-refresh", POOL]);
    // Same argv shapes the app builders produce.
    check(&["vol-create-as", POOL, "base.qcow2", "32M", "--format", "qcow2"]);
    check(&["vol-resize", "--pool", POOL, "base.qcow2", "48M"]);
    check(&["vol-clone", "--pool", POOL, "base.qcow2", "copy.qcow2"]);
    check(&["vol-wipe", "--pool", POOL, "copy.qcow2", "--algorithm", "zero"]);
    check(&["vol-delete", "--pool", POOL, "copy.qcow2"]);
    check(&["vol-delete", "--pool", POOL, "base.qcow2"]);
    check(&["pool-destroy", POOL]);
    check(&["pool-undefine", POOL]);
    let _ = std::fs::remove_dir_all(&dir);
    let list = check(&["pool-list", "--all", "--name"]);
    assert!(!list.lines().any(|l| l.trim() == POOL));
}
