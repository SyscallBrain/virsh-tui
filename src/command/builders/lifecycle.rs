//! Lifecycle builders (Appendix A): exact argv per feature.

use super::super::plan::{CommandPlan, CommandStep};

/// virsh start d
pub fn start(domain: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["start", domain], &format!("Started {domain}"))
}

/// virsh shutdown d
pub fn shutdown(domain: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["shutdown", domain],
        &format!("Shutdown requested for {domain} (ACPI)"),
    )
}

/// virsh destroy d (confirm)
pub fn destroy(domain: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["destroy", domain], &format!("Destroyed {domain}"))
}

/// virsh reboot d
pub fn reboot(domain: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["reboot", domain], &format!("Rebooted {domain}"))
}

/// virsh reset d (confirm)
pub fn reset(domain: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["reset", domain], &format!("Reset {domain}"))
}

/// virsh suspend d
pub fn pause(domain: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["suspend", domain], &format!("Paused {domain}"))
}

/// virsh resume d
pub fn resume(domain: &str) -> CommandPlan {
    CommandPlan::single("virsh", vec!["resume", domain], &format!("Resumed {domain}"))
}

/// virsh managedsave d
pub fn managed_save(domain: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["managedsave", domain],
        &format!("Managed-save {domain}"),
    )
}

/// virsh managedsave-remove d
pub fn managed_save_remove(domain: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["managedsave-remove", domain],
        &format!("Removed managed save {domain}"),
    )
}

/// virsh autostart d [--disable]
pub fn autostart(domain: &str, enable: bool) -> CommandPlan {
    if enable {
        CommandPlan::single(
            "virsh",
            vec!["autostart", domain],
            &format!("Autostart on {domain}"),
        )
    } else {
        CommandPlan::single(
            "virsh",
            vec!["autostart", domain, "--disable"],
            &format!("Autostart off {domain}"),
        )
    }
}

/// virsh domrename d new (shut off only; UI enforces)
pub fn rename(domain: &str, new_name: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["domrename", domain, new_name],
        &format!("Renamed {domain} to {new_name}"),
    )
}

/// virt-clone --original d --name n --auto-clone
pub fn clone_domain(domain: &str, new_name: &str) -> CommandPlan {
    CommandPlan::single(
        "virt-clone",
        vec!["--original", domain, "--name", new_name, "--auto-clone"],
        &format!("Cloned {domain} to {new_name}"),
    )
}

/// virsh undefine d [options]
pub fn undefine(domain: &str, remove_storage: bool, nvram: bool, snapshots: bool) -> CommandPlan {
    let mut argv = vec!["undefine".to_string(), domain.to_string()];
    if remove_storage {
        argv.push("--remove-all-storage".to_string());
    }
    if nvram {
        argv.push("--nvram".to_string());
    }
    if snapshots {
        argv.push("--snapshots-metadata".to_string());
    }
    CommandPlan {
        steps: vec![CommandStep {
            program: String::from("virsh"),
            argv,
            stdin: None,
        }],
        summary: format!("Undefined {domain}"),
    }
}

/// virsh send-key d LeftCtrl LeftAlt Delete
pub fn send_key(domain: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["send-key", domain, "KEY_LEFTCTRL", "KEY_LEFTALT", "KEY_DELETE"],
        &format!("Sent Ctrl-Alt-Del to {domain}"),
    )
}

/// virsh migrate d qemu+ssh://host/system --live --persistent [--undefinesource]
pub fn migrate(domain: &str, dest_uri: &str, undefinesource: bool) -> CommandPlan {
    let mut argv = vec![
        "migrate".to_string(),
        domain.to_string(),
        dest_uri.to_string(),
        "--live".to_string(),
        "--persistent".to_string(),
    ];
    if undefinesource {
        argv.push("--undefinesource".to_string());
    }
    CommandPlan {
        steps: vec![CommandStep {
            program: String::from("virsh"),
            argv,
            stdin: None,
        }],
        summary: format!("Migrated {domain}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{autostart, clone_domain, destroy, migrate, rename, send_key, shutdown, start, undefine};

    #[test]
    fn exact_argv() {
        assert_eq!(start("d").steps[0].argv, vec!["start", "d"]);
        assert_eq!(shutdown("d").steps[0].argv, vec!["shutdown", "d"]);
        assert_eq!(destroy("d").steps[0].argv, vec!["destroy", "d"]);
        assert_eq!(
            send_key("d").steps[0].argv,
            vec!["send-key", "d", "KEY_LEFTCTRL", "KEY_LEFTALT", "KEY_DELETE"]
        );
        assert_eq!(
            clone_domain("d", "n").steps[0].argv,
            vec!["--original", "d", "--name", "n", "--auto-clone"]
        );
        assert_eq!(rename("d", "n").steps[0].argv, vec!["domrename", "d", "n"]);
        assert!(
            undefine("d", true, false, false).steps[0]
                .argv
                .contains(&"--remove-all-storage".to_string())
        );
        assert_eq!(
            autostart("d", false).steps[0].argv,
            vec!["autostart", "d", "--disable"]
        );
        assert!(
            migrate("d", "qemu+ssh://h/system", true).steps[0]
                .argv
                .contains(&"--undefinesource".to_string())
        );
    }
}
