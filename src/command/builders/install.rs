//! Install builder: `virt-install` from wizard state.
//!
//! The wizard's "Equivalent command" panel renders this exact argv, so what
//! is shown is what runs.

use super::super::plan::CommandPlan;
use crate::ui::overlays::wizard::{MediaKind, WizardState, detect_os};

/// `--osinfo` value: explicit, detected from the ISO name, or virt-install detection.
pub fn osinfo(st: &WizardState) -> String {
    if !st.os_id.trim().is_empty() {
        return st.os_id.trim().to_string();
    }
    match (st.media, detect_os(&st.iso_path)) {
        (MediaKind::Iso, Some(os)) => os,
        _ => String::from("detect=on,require=off"),
    }
}

/// Build the full `virt-install … --noautoconsole` argv.
pub fn virt_install(st: &WizardState) -> CommandPlan {
    let mut argv = vec![format!("--name={}", st.name), format!("--osinfo={}", osinfo(st))];
    if !st.title.is_empty() || !st.desc.is_empty() {
        let mut meta = Vec::new();
        if !st.title.is_empty() {
            meta.push(format!("title={}", st.title));
        }
        if !st.desc.is_empty() {
            meta.push(format!("description={}", st.desc));
        }
        argv.push(format!("--metadata={}", meta.join(",")));
    }
    match st.media {
        MediaKind::Iso => argv.push(format!("--cdrom={}", st.iso_path)),
        MediaKind::Url => {
            argv.push(format!("--location={}", st.url));
            if !st.kernel_args.is_empty() {
                argv.push(format!("--extra-args={}", st.kernel_args));
            }
        }
        MediaKind::Pxe => argv.push(String::from("--pxe")),
        MediaKind::Import => argv.push(String::from("--import")),
        // No install media: boot the (empty or existing) disk.
        MediaKind::None => argv.push(String::from("--import")),
    }
    argv.push(format!(
        "--vcpus={},maxvcpus={}",
        st.vcpus,
        st.max_vcpus.max(st.vcpus)
    ));
    argv.push(format!("--cpu={}", st.cpu_model));
    argv.push(format!(
        "--memory={},maxmemory={}",
        st.mem_gib * 1024,
        st.max_mem_gib.max(st.mem_gib) * 1024
    ));
    if !st.balloon {
        argv.push(String::from("--memballoon=none"));
    }
    if st.hugepages {
        argv.push(String::from("--memorybacking=hugepages=on"));
    }
    if st.disk_new {
        argv.push(format!(
            "--disk=pool={},size={},format={},bus={}",
            st.pool,
            disk_size_gib(&st.disk_size),
            st.disk_format,
            st.disk_bus
        ));
    } else {
        argv.push(format!("--disk=path={},bus={}", st.disk_path, st.disk_bus));
    }
    if st.net_source == "none" {
        argv.push(String::from("--network=none"));
    } else {
        let mut net = format!("--network=network={},model={}", st.net_source, st.net_model);
        if !st.mac_auto && !st.mac.is_empty() {
            net.push_str(&format!(",mac={}", st.mac));
        }
        argv.push(net);
    }
    if st.firmware == "uefi" {
        argv.push(if st.secure_boot {
            String::from("--boot=uefi,firmware.feature0.name=secure-boot,firmware.feature0.enabled=yes")
        } else {
            String::from("--boot=uefi")
        });
    }
    argv.push(format!("--graphics={}", st.graphics));
    if st.tpm {
        argv.push(String::from("--tpm=default"));
    }
    if st.autostart {
        argv.push(String::from("--autostart"));
    }
    argv.push(String::from("--noautoconsole"));
    CommandPlan {
        steps: vec![crate::command::plan::CommandStep {
            program: String::from("virt-install"),
            argv,
            stdin: None,
        }],
        summary: format!("Created {}", st.name),
    }
}

/// `--disk size=` is in GiB (may be fractional): `20G` → `20`, `512M` → `0.5`.
fn disk_size_gib(s: &str) -> String {
    match crate::ui::views::detail::hardware::size_to_kib(s) {
        Some(kib) => {
            let gib = kib as f64 / 1024.0 / 1024.0;
            if gib.fract() == 0.0 {
                format!("{gib:.0}")
            } else {
                format!("{gib:.2}")
            }
        }
        None => s.trim_end_matches(['G', 'g']).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::virt_install;
    use crate::ui::overlays::wizard::{MediaKind, WizardState};

    #[test]
    fn full_argv() {
        let plan = virt_install(&WizardState::demo_step3());
        let argv = plan.steps[0].argv.join(" ");
        assert!(argv.contains("--name=fedora-42-ws"));
        assert!(argv.contains("--disk=pool=nvme-fast,size=30,format=qcow2,bus=virtio"));
        assert!(argv.contains("--noautoconsole"));
    }

    #[test]
    fn defaults_are_valid() {
        let mut st = WizardState {
            name: String::from("vm"),
            iso_path: String::from("/isos/archlinux-2026.10.01-x86_64.iso"),
            ..Default::default()
        };
        let argv = virt_install(&st).steps[0].argv.join(" ");
        assert!(argv.contains("--osinfo=archlinux"), "{argv}");
        assert!(argv.contains("--memory=4096,maxmemory=8192"));
        st.media = MediaKind::None;
        st.firmware = String::from("bios");
        st.net_source = String::from("none");
        let argv = virt_install(&st).steps[0].argv.join(" ");
        assert!(argv.contains("--import") && argv.contains("--network=none") && !argv.contains("--boot"));
    }
}
