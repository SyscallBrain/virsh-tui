//! Install builder: `virt-install` from wizard state.
//!
//! The wizard's "Equivalent command" panel renders this exact argv, so what
//! is shown is what runs.

use super::super::plan::CommandPlan;
use crate::ui::overlays::wizard::{MediaKind, WizardState};

/// `--osinfo` value: the id the user typed, else virt-install's own detection
/// from the media (a guessed id that osinfo does not know fails the install).
pub fn osinfo(st: &WizardState) -> String {
    if !st.os_id.trim().is_empty() {
        return st.os_id.trim().to_string();
    }
    String::from("detect=on,require=off")
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

/// `virt-install --print-xml` for a domain that is defined but not started
/// ("start after creation" off, or `C-e` edit as XML).
///
/// `--cdrom` and `--pxe` are multi-phase installs: plain `--print-xml` prints
/// two `<domain>` documents, and the install phase alone ends in
/// `<on_reboot>destroy</on_reboot>` (issue #5). Ask for a single persistent
/// phase instead (`--install no_install=yes`) that keeps the media attached
/// and boots the disk first: an empty disk falls through to the installer,
/// an installed one boots normally.
pub fn define_xml(st: &WizardState) -> CommandPlan {
    let mut plan = virt_install(st);
    let argv = &mut plan.steps[0].argv;
    let fallback = match st.media {
        MediaKind::Iso => Some("cdrom"),
        MediaKind::Pxe => Some("network"),
        MediaKind::Url | MediaKind::Import | MediaKind::None => None,
    };
    if let Some(fallback) = fallback {
        argv.push(String::from("--install=no_install=yes"));
        let order = format!("hd,{fallback}");
        match argv.iter_mut().find(|arg| arg.starts_with("--boot=")) {
            Some(boot) => {
                boot.push(',');
                boot.push_str(&order);
            }
            None => argv.push(format!("--boot={order}")),
        }
    }
    argv.push(String::from("--print-xml"));
    plan
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
        assert!(argv.contains("--osinfo=detect=on,require=off"), "{argv}");
        assert!(argv.contains("--memory=4096,maxmemory=8192"));
        st.media = MediaKind::None;
        st.firmware = String::from("bios");
        st.net_source = String::from("none");
        let argv = virt_install(&st).steps[0].argv.join(" ");
        assert!(argv.contains("--import") && argv.contains("--network=none") && !argv.contains("--boot"));
    }

    fn define_argv(st: &WizardState) -> String {
        super::define_xml(st).steps[0].argv.join(" ")
    }

    fn iso_state() -> WizardState {
        WizardState {
            name: String::from("vm"),
            iso_path: String::from("/isos/archlinux-2026.10.01-x86_64.iso"),
            firmware: String::from("bios"),
            ..Default::default()
        }
    }

    // Issue #5: `--print-xml` of a `--cdrom` install is two documents, and the
    // install phase alone ends in `<on_reboot>destroy</on_reboot>`.
    #[test]
    fn define_only_iso_is_one_persistent_phase() {
        let argv = define_argv(&iso_state());
        assert!(
            argv.contains("--cdrom=/isos/archlinux-2026.10.01-x86_64.iso"),
            "{argv}"
        );
        assert!(argv.contains("--install=no_install=yes"), "{argv}");
        assert!(argv.contains("--boot=hd,cdrom"), "{argv}");
        assert!(argv.ends_with("--print-xml"), "{argv}");
    }

    #[test]
    fn define_only_uefi_keeps_one_boot_option() {
        let mut st = iso_state();
        st.firmware = String::from("uefi");
        st.secure_boot = true;
        let argv = super::define_xml(&st).steps[0].argv.clone();
        let boots: Vec<_> = argv.iter().filter(|arg| arg.starts_with("--boot=")).collect();
        assert_eq!(
            boots,
            ["--boot=uefi,firmware.feature0.name=secure-boot,firmware.feature0.enabled=yes,hd,cdrom"]
        );
    }

    #[test]
    fn define_only_pxe_falls_back_to_the_network() {
        let mut st = iso_state();
        st.media = MediaKind::Pxe;
        let argv = define_argv(&st);
        assert!(
            argv.contains("--pxe") && argv.contains("--install=no_install=yes"),
            "{argv}"
        );
        assert!(argv.contains("--boot=hd,network"), "{argv}");
    }

    #[test]
    fn define_only_import_is_unchanged_apart_from_print_xml() {
        let mut st = iso_state();
        st.media = MediaKind::Import;
        st.disk_new = false;
        st.disk_path = String::from("/var/lib/libvirt/images/vm.qcow2");
        let mut expected = virt_install(&st).steps[0].argv.clone();
        expected.push(String::from("--print-xml"));
        assert_eq!(super::define_xml(&st).steps[0].argv, expected);
    }

    // A `--location` install boots the downloaded kernel directly: there is no
    // persistent single-phase domain to define without starting it.
    #[test]
    fn url_install_must_start_after_creation() {
        let mut st = iso_state();
        st.media = MediaKind::Url;
        st.url = String::from("https://example.org/fedora/os/");
        assert!(st.problems().is_empty(), "{:?}", st.problems());
        st.start_after = false;
        assert!(
            st.problems()
                .iter()
                .any(|(step, text)| *step == 5 && text.contains("start after creation")),
            "{:?}",
            st.problems()
        );
    }
}
