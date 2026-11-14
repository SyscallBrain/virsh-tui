//! Hint texts shared by the checks and the demo report.

pub const GROUP: &str = "add your user to the libvirt group and log in again, or use -c qemu:///session";
pub const NETWORKS: &str = "guests on libvirt networks get no DHCP/IP";
pub const VIRT_INSTALL: &str = "needed by the new-domain wizard";
pub const VIRT_CLONE: &str = "needed by Space c / :clone";
pub const VIRSH: &str = "install the libvirt client package";
pub const EVENTS: &str = "state updates every refresh";
pub const UEFI: &str = "install OVMF (ovmf, edk2-ovmf) or choose BIOS in the wizard";
pub const VIEWER: &str = "install virt-viewer, or set console.viewer";
pub const CLIPBOARD: &str = "yank keys will not work (no display/Wayland session?)";
pub const CONFIG: &str = "fix it; settings are not saved until it parses";
pub const SIZE: &str = "some panels collapse; 140+ columns shows every pane";
pub const COLOURS: &str = "set COLORTERM=truecolor; inside tmux also set terminal-overrides ,*:Tc";
pub const GLYPHS: &str = "if lines or graphs look wrong, set icons = \"ascii\" and graphs = \"block\"";
