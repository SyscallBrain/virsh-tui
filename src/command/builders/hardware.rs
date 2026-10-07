//! Hardware builders: native virsh commands first, define path otherwise.

use super::super::plan::{CommandPlan, CommandStep};

fn step(domain: &str, sub: &str, args: Vec<String>) -> CommandStep {
    let mut argv = vec![sub.to_string(), domain.to_string()];
    argv.extend(args);
    CommandStep {
        program: String::from("virsh"),
        argv,
        stdin: None,
    }
}

/// vCPU changes: `[setvcpus max --maximum --config]` + `[setvcpus cur --live --config]`.
///
/// Flags come from Apply-to; at least `--config` is always set.
pub fn set_vcpus(
    domain: &str,
    current: u32,
    max: u32,
    max_changed: bool,
    live: bool,
    config: bool,
) -> Vec<CommandPlan> {
    let mut out = Vec::new();
    if max_changed {
        let mut flags = vec![String::from("--maximum")];
        if config {
            flags.push(String::from("--config"));
        }
        out.push(CommandPlan {
            steps: vec![step(domain, "setvcpus", {
                let mut a = vec![max.to_string()];
                a.extend(flags);
                a
            })],
            summary: format!("Set max vCPUs {max} on {domain}"),
        });
    }
    let mut flags = Vec::new();
    if live {
        flags.push(String::from("--live"));
    }
    if config {
        flags.push(String::from("--config"));
    }
    out.push(CommandPlan {
        steps: vec![step(domain, "setvcpus", {
            let mut a = vec![current.to_string()];
            a.extend(flags);
            a
        })],
        summary: format!("Set vCPUs {current} on {domain}"),
    });
    out
}

/// Memory changes: `[setmaxmem SIZE --config]` + `[setmem SIZE --live --config]`.
pub fn set_memory(domain: &str, current: &str, max: &str, live: bool, config: bool) -> Vec<CommandPlan> {
    let mut out = Vec::new();
    let mut max_flags = Vec::new();
    if config {
        max_flags.push(String::from("--config"));
    }
    out.push(CommandPlan {
        steps: vec![step(domain, "setmaxmem", {
            let mut a = vec![max.to_string()];
            a.extend(max_flags);
            a
        })],
        summary: format!("Set max memory {max} on {domain}"),
    });
    let mut flags = Vec::new();
    if live {
        flags.push(String::from("--live"));
    }
    if config {
        flags.push(String::from("--config"));
    }
    out.push(CommandPlan {
        steps: vec![step(domain, "setmem", {
            let mut a = vec![current.to_string()];
            a.extend(flags);
            a
        })],
        summary: format!("Set memory {current} on {domain}"),
    });
    out
}

/// `virsh vcpupin d vcpu cpulist --config [--live]`.
pub fn vcpupin(domain: &str, vcpu: u32, cpulist: &str, live: bool) -> CommandPlan {
    let mut args = vec![vcpu.to_string(), cpulist.to_string(), String::from("--config")];
    if live {
        args.push(String::from("--live"));
    }
    CommandPlan {
        steps: vec![step(domain, "vcpupin", args)],
        summary: format!("Pinned vCPU {vcpu} on {domain}"),
    }
}

/// `virsh emulatorpin d cpulist`.
pub fn emulatorpin(domain: &str, cpulist: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["emulatorpin", domain, cpulist],
        &format!("Pinned emulator on {domain}"),
    )
}

/// `virsh iothreadadd d id`.
pub fn iothread_add(domain: &str, id: u32) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["iothreadadd", domain, &id.to_string()],
        &format!("Added iothread {id} on {domain}"),
    )
}

/// `virsh attach-disk …` / `detach-disk`.
pub fn attach_disk(domain: &str, src: &str, target: &str, live: bool, config: bool) -> CommandPlan {
    let mut argv = vec!["attach-disk", domain, src, target];
    if live {
        argv.push("--live");
    }
    if config {
        argv.push("--config");
    }
    CommandPlan::single("virsh", argv, &format!("Attached disk {target} on {domain}"))
}

/// `virsh detach-disk d target --config [--live]`.
pub fn detach_disk(domain: &str, target: &str, live: bool) -> CommandPlan {
    let mut argv = vec!["detach-disk", domain, target, "--config"];
    if live {
        argv.push("--live");
    }
    CommandPlan::single("virsh", argv, &format!("Detached disk {target} on {domain}"))
}

/// `virsh blockresize d target SIZE`.
pub fn block_resize(domain: &str, target: &str, size: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["blockresize", domain, target, size],
        &format!("Resized {target} on {domain}"),
    )
}

/// `virsh change-media d target iso --update --live --config` (or `--eject`).
pub fn change_media(domain: &str, target: &str, iso: Option<&str>, live: bool) -> CommandPlan {
    let summary = match iso {
        Some(_) => format!("Inserted media on {domain}"),
        None => format!("Ejected media on {domain}"),
    };
    let mut argv = vec!["change-media", domain, target];
    match iso {
        Some(path) => argv.extend([path, "--update"]),
        None => argv.push("--eject"),
    }
    if live {
        argv.push("--live");
    }
    argv.push("--config");
    CommandPlan::single("virsh", argv, &summary)
}

/// `virsh attach-interface …` / `detach-interface`.
pub fn attach_nic(domain: &str, source: &str, model: &str, mac: Option<&str>, live: bool) -> CommandPlan {
    let mut argv = vec![
        "attach-interface",
        domain,
        "network",
        source,
        "--model",
        model,
        "--config",
    ];
    if let Some(m) = mac {
        argv.extend(["--mac", m]);
    }
    if live {
        argv.push("--live");
    }
    CommandPlan::single("virsh", argv, &format!("Attached NIC on {domain}"))
}

/// `virsh domif-setlink d iface up|down`.
pub fn set_link(domain: &str, iface: &str, up: bool) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["domif-setlink", domain, iface, if up { "up" } else { "down" }],
        &format!("Set link {} on {domain}", if up { "up" } else { "down" }),
    )
}

/// `virsh domiftune d iface --inbound … --outbound …`.
pub fn tune_iface(domain: &str, iface: &str, inbound: &str, outbound: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec![
            "domiftune",
            domain,
            iface,
            "--inbound",
            inbound,
            "--outbound",
            outbound,
        ],
        &format!("Tuned {iface} on {domain}"),
    )
}

/// `virsh define /tmp/vt-<domain>.xml` (XML edit path display; real path at runtime).
pub fn define(domain: &str) -> CommandPlan {
    CommandPlan::single(
        "virsh",
        vec!["define", &format!("/tmp/vt-{domain}.xml")],
        &format!("Defined {domain}"),
    )
}

#[cfg(test)]
mod tests {
    use super::{attach_disk, change_media, define, set_memory, set_vcpus};

    #[test]
    fn native_commands() {
        let plans = set_vcpus("d", 8, 16, true, true, true);
        let argv: Vec<String> = plans
            .iter()
            .flat_map(|p| p.steps.iter().flat_map(|s| s.argv.clone()))
            .collect();
        assert!(argv.contains(&"--maximum".to_string()));
        assert!(argv.contains(&"--live".to_string()));
        assert!(
            attach_disk("d", "/x.qcow2", "vdb", true, true).steps[0]
                .argv
                .contains(&"--live".to_string())
        );
        assert!(
            change_media("d", "sda", None, true).steps[0]
                .argv
                .contains(&"--eject".to_string())
        );
        assert!(!set_memory("d", "8G", "16G", true, true).is_empty());
        assert!(define("d").steps[0].argv[1].contains("vt-d.xml"));
    }
}
