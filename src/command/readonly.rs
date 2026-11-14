//! Read-only connections: which commands only read.
//!
//! The UI refuses a command that is not known to only read before it runs;
//! libvirt is the backstop (virsh runs with `-r`, see `process.rs`).

use super::plan::{CommandPlan, CommandStep};

/// virsh subcommands that never change anything. Anything else counts as a
/// change (unknown commands included), so the list errs on the safe side.
const READS: &[&str] = &[
    "capabilities",
    "checkpoint-dumpxml",
    "checkpoint-info",
    "checkpoint-list",
    "checkpoint-parent",
    "cpu-models",
    "domblkerror",
    "domblkinfo",
    "domblklist",
    "domblkstat",
    "domcapabilities",
    "domcontrol",
    "domdisplay",
    "domfsinfo",
    "domhostname",
    "domid",
    "domifaddr",
    "domiflist",
    "domifstat",
    "dominfo",
    "domjobinfo",
    "domname",
    "domstate",
    "domstats",
    "domuuid",
    "dumpxml",
    "echo",
    "event",
    "freecell",
    "guestinfo",
    "help",
    "hostname",
    "iface-dumpxml",
    "iface-list",
    "iothreadinfo",
    "list",
    "maxvcpus",
    "net-dhcp-leases",
    "net-dumpxml",
    "net-event",
    "net-info",
    "net-list",
    "net-name",
    "net-uuid",
    "nodecpustats",
    "nodedev-dumpxml",
    "nodedev-event",
    "nodedev-list",
    "nodeinfo",
    "nodememstats",
    "nwfilter-dumpxml",
    "nwfilter-list",
    "pool-dumpxml",
    "pool-event",
    "pool-info",
    "pool-list",
    "pool-name",
    "pool-uuid",
    "secret-dumpxml",
    "secret-list",
    "snapshot-dumpxml",
    "snapshot-info",
    "snapshot-list",
    "snapshot-parent",
    "sysinfo",
    "uri",
    "vcpucount",
    "vcpuinfo",
    "version",
    "vncdisplay",
    "vol-dumpxml",
    "vol-info",
    "vol-key",
    "vol-list",
    "vol-name",
    "vol-path",
    "vol-pool",
];

/// Whether `step` may change a libvirt object.
pub fn step_writes(step: &CommandStep) -> bool {
    match step.program.as_str() {
        "virsh" => {
            let mut words = step
                .argv
                .iter()
                .map(String::as_str)
                .filter(|a| !a.starts_with('-'));
            let Some(sub) = words.next() else { return false };
            if sub == "dommemstat" {
                // `--period` sets the balloon stats period on the guest.
                return step.argv.iter().any(|a| a.starts_with("--period"));
            }
            if sub == "metadata" {
                return step
                    .argv
                    .iter()
                    .any(|a| matches!(a.as_str(), "--set" | "--remove" | "--edit"));
            }
            if sub == "snapshot-current" {
                // With `--name` it prints; otherwise it may set the current one.
                return !step.argv.iter().any(|a| a == "--name");
            }
            !READS.contains(&sub)
        }
        "virt-clone" => true,
        "virt-install" => !step.argv.iter().any(|a| a == "--print-xml" || a == "--dry-run"),
        _ => false,
    }
}

/// The first step of `plan` that may change something.
pub fn first_write(plan: &CommandPlan) -> Option<&CommandStep> {
    plan.steps.iter().find(|s| step_writes(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn virsh(args: &[&str]) -> CommandStep {
        CommandStep::new("virsh", args.to_vec())
    }

    #[test]
    fn reads_and_writes_are_told_apart() {
        for args in [
            &["list", "--all"][..],
            &["dominfo", "vm"],
            &["dumpxml", "--inactive", "vm"],
            &["snapshot-current", "vm", "--name"],
            &["dommemstat", "vm"],
            &["net-dhcp-leases", "default"],
        ] {
            assert!(!step_writes(&virsh(args)), "{args:?} reads");
        }
        for args in [
            &["start", "vm"][..],
            &["destroy", "vm"],
            &["dommemstat", "vm", "--period", "2", "--live"],
            &["snapshot-current", "vm", "snap1"],
            &["some-future-command", "vm"],
            &["define", "/tmp/x.xml"],
        ] {
            assert!(step_writes(&virsh(args)), "{args:?} writes");
        }
        assert!(step_writes(&CommandStep::new(
            "virt-clone",
            vec!["--original", "a"]
        )));
        assert!(step_writes(&CommandStep::new(
            "virt-install",
            vec!["--name", "a"]
        )));
        assert!(!step_writes(&CommandStep::new(
            "virt-install",
            vec!["--print-xml"]
        )));
        assert!(!step_writes(&CommandStep::new("ip", vec!["-j", "addr"])));
    }
}
