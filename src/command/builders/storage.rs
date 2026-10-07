//! Storage builders: pools and volumes.

use super::super::plan::CommandPlan;

/// `pool-define-as name type [--target …] [--source-…]`.
pub fn pool_define(name: &str, kind: &str, target: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["pool-define-as", name, kind, "--target", target],
        &format!("Defined pool {name}"),
    )
}

/// `pool-build pool`.
pub fn pool_build(pool: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["pool-build", pool], &format!("Built pool {pool}"))
}

/// `pool-start pool`.
pub fn pool_start(pool: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["pool-start", pool], &format!("Started pool {pool}"))
}

/// `pool-destroy pool` (confirm).
pub fn pool_destroy(pool: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["pool-destroy", pool],
        &format!("Destroyed pool {pool}"),
    )
}

/// `pool-refresh pool`.
pub fn pool_refresh(pool: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["pool-refresh", pool],
        &format!("Refreshed pool {pool}"),
    )
}

/// `pool-autostart pool [--disable]`.
pub fn pool_autostart(pool: &str, enable: bool) -> CommandPlan {
    if enable {
        CommandPlan::single(
            "virsh",
            vec!["pool-autostart", pool],
            &format!("Autostart on {pool}"),
        )
    } else {
        CommandPlan::single(
            "virsh",
            vec!["pool-autostart", pool, "--disable"],
            &format!("Autostart off {pool}"),
        )
    }
}

/// `pool-undefine pool` / `pool-delete pool` (confirm).
pub fn pool_undefine(pool: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["pool-undefine", pool],
        &format!("Undefined pool {pool}"),
    )
}

/// `vol-create-as pool name SIZE --format fmt [--backing-vol …]`.
pub fn vol_create(pool: &str, name: &str, size: &str, format: &str, backing: Option<&str>) -> CommandPlan {
    let mut argv = vec!["vol-create-as", pool, name, size, "--format", format];
    if let Some(b) = backing {
        argv.extend(["--backing-vol", b]);
    }
    CommandPlan::single("virsh", argv, &format!("Created volume {name} in {pool}"))
}

/// `vol-resize --pool pool vol SIZE [--shrink]`.
pub fn vol_resize(pool: &str, vol: &str, size: &str, shrink: bool) -> CommandPlan {
    let mut argv = vec!["vol-resize", "--pool", pool, vol, size];
    if shrink {
        argv.push("--shrink");
    }
    CommandPlan::single("virsh", argv, &format!("Resized {vol} in {pool}"))
}

/// `vol-clone --pool pool vol new`.
pub fn vol_clone(pool: &str, vol: &str, name: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["vol-clone", "--pool", pool, vol, name],
        &format!("Cloned {vol} to {name}"),
    )
}

/// `vol-upload --pool pool vol path`.
pub fn vol_upload(pool: &str, vol: &str, path: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["vol-upload", "--pool", pool, vol, path],
        &format!("Uploaded to {vol} in {pool}"),
    )
}

/// `vol-download --pool pool vol path`.
pub fn vol_download(pool: &str, vol: &str, path: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["vol-download", "--pool", pool, vol, path],
        &format!("Downloaded {vol} from {pool}"),
    )
}

/// `vol-wipe --pool pool vol [--algorithm …]` (confirm).
pub fn vol_wipe(pool: &str, vol: &str, algorithm: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["vol-wipe", "--pool", pool, vol, "--algorithm", algorithm],
        &format!("Wiped {vol} in {pool}"),
    )
}

/// `vol-delete --pool pool vol` (confirm, warns if used).
pub fn vol_delete(pool: &str, vol: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["vol-delete", "--pool", pool, vol],
        &format!("Deleted {vol} from {pool}"),
    )
}

#[cfg(test)]
mod tests {
    use super::{pool_start, vol_create, vol_resize, vol_wipe};

    #[test]
    fn exact() {
        assert_eq!(pool_start("p").steps[0].argv, vec!["pool-start", "p"]);
        assert!(
            vol_create("p", "v", "10G", "qcow2", None).steps[0]
                .argv
                .contains(&"vol-create-as".to_string())
        );
        assert!(
            vol_resize("p", "v", "20G", false).steps[0]
                .argv
                .contains(&"vol-resize".to_string())
        );
        assert!(
            vol_wipe("p", "v", "zero").steps[0]
                .argv
                .contains(&"--algorithm".to_string())
        );
    }
}
