//! Root commands to copy (never run) and the package names per distro.

use super::runner::Runner;

/// Distribution family, from `/etc/os-release`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    Arch,
    Debian,
    Fedora,
    Suse,
}

/// Package the doctor may suggest installing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pkg {
    Virsh,
    VirtInstall,
    VirtClone,
    Ovmf,
    Viewer,
}

/// `ID`, then each word of `ID_LIKE`.
pub fn distro(os_release: &str) -> Option<Distro> {
    let id = super::os_release_value(os_release, "ID").unwrap_or_default();
    let like = super::os_release_value(os_release, "ID_LIKE").unwrap_or_default();
    std::iter::once(id.as_str())
        .chain(like.split_whitespace())
        .find_map(|word| match word {
            "arch" => Some(Distro::Arch),
            "debian" | "ubuntu" => Some(Distro::Debian),
            "fedora" | "rhel" | "centos" => Some(Distro::Fedora),
            w if w == "suse" || w.starts_with("opensuse") => Some(Distro::Suse),
            _ => None,
        })
}

/// `sudo <package manager> <package>`.
pub fn install_command(d: Distro, p: Pkg) -> String {
    use Distro::*;
    use Pkg::*;
    let manager = match d {
        Arch => "pacman -S",
        Debian => "apt install",
        Fedora => "dnf install",
        Suse => "zypper install",
    };
    let package = match (d, p) {
        (Arch, Virsh) => "libvirt",
        (Debian, Virsh) => "libvirt-clients",
        (Fedora | Suse, Virsh) => "libvirt-client",
        (Debian, VirtInstall | VirtClone) => "virtinst",
        (_, VirtInstall | VirtClone) => "virt-install",
        (Arch | Fedora, Ovmf) => "edk2-ovmf",
        (Debian, Ovmf) => "ovmf",
        (Suse, Ovmf) => "qemu-ovmf-x86_64",
        (_, Viewer) => "virt-viewer",
    };
    format!("sudo {manager} {package}")
}

/// `sudo systemctl enable --now virtqemud.socket` when the modular daemon is
/// installed, else `libvirtd`.
pub fn daemon_command<R: Runner>(runner: &R) -> String {
    let modular = std::path::Path::new("/usr/lib/systemd/system/virtqemud.socket");
    let unit = if runner.exists(modular) {
        "virtqemud.socket"
    } else {
        "libvirtd"
    };
    format!("sudo systemctl enable --now {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distro_detection() {
        assert_eq!(distro("ID=arch\n"), Some(Distro::Arch));
        assert_eq!(distro("ID=ubuntu\nID_LIKE=debian\n"), Some(Distro::Debian));
        assert_eq!(
            distro("ID=rocky\nID_LIKE=\"rhel centos fedora\"\n"),
            Some(Distro::Fedora)
        );
        assert_eq!(
            distro("ID=opensuse-tumbleweed\nID_LIKE=\"opensuse suse\"\n"),
            Some(Distro::Suse)
        );
        assert_eq!(distro("ID=nixos\n"), None);
    }

    #[test]
    fn install_commands() {
        assert_eq!(
            install_command(Distro::Debian, Pkg::VirtClone),
            "sudo apt install virtinst"
        );
        assert_eq!(
            install_command(Distro::Fedora, Pkg::Ovmf),
            "sudo dnf install edk2-ovmf"
        );
        assert_eq!(
            install_command(Distro::Suse, Pkg::Ovmf),
            "sudo zypper install qemu-ovmf-x86_64"
        );
        assert_eq!(
            install_command(Distro::Arch, Pkg::Virsh),
            "sudo pacman -S libvirt"
        );
        assert_eq!(
            install_command(Distro::Arch, Pkg::Viewer),
            "sudo pacman -S virt-viewer"
        );
    }

    #[test]
    fn daemon_unit() {
        let mut r = super::super::runner::FakeRunner::default();
        assert_eq!(daemon_command(&r), "sudo systemctl enable --now libvirtd");
        r.file("/usr/lib/systemd/system/virtqemud.socket", "");
        assert_eq!(daemon_command(&r), "sudo systemctl enable --now virtqemud.socket");
    }
}
