//! New domain wizard (PLAN.md section 9.6, P11): 6 steps, draft autosave.
//!
//! Every step is a list of typed fields (`WField`). The same field model drives
//! rendering, key handling, validation and the `virt-install` command, and the
//! "Equivalent command" panel shows exactly the argv that will run.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::theme::Theme;

/// Wizard step index.
pub const STEP_NAMES: [&str; 6] = [
    "1  Name & OS",
    "2  Install media",
    "3  CPU & Memory",
    "4  Storage",
    "5  Network",
    "6  Review & create",
];

/// Validate a domain name: unique, `[A-Za-z0-9._-]`.
pub fn valid_name(name: &str, existing: &[&str]) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !existing.contains(&name)
}

/// Auto-detect OS variant from an ISO filename.
pub fn detect_os(iso: &str) -> Option<String> {
    let lower = iso.rsplit('/').next().unwrap_or(iso).to_lowercase();
    if lower.contains("fedora") {
        let digits: String = lower.chars().filter(|c| c.is_ascii_digit()).take(2).collect();
        return Some(format!("fedora{digits}"));
    }
    if lower.contains("arch") {
        return Some(String::from("archlinux"));
    }
    if lower.contains("ubuntu") {
        return Some(String::from("ubuntu24.04"));
    }
    if lower.contains("debian") {
        return Some(String::from("debian13"));
    }
    None
}

/// Install media kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MediaKind {
    #[default]
    Iso,
    Url,
    Pxe,
    Import,
    None,
}

impl MediaKind {
    const ALL: [Self; 5] = [Self::Iso, Self::Url, Self::Pxe, Self::Import, Self::None];

    fn label(self) -> &'static str {
        match self {
            Self::Iso => "local ISO",
            Self::Url => "URL",
            Self::Pxe => "PXE",
            Self::Import => "import disk",
            Self::None => "none",
        }
    }
}

/// Wizard state (serializable draft). Host facts are not saved.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WizardState {
    pub step: usize,
    pub focus: usize,
    pub name: String,
    pub os_id: String,
    pub title: String,
    pub desc: String,
    pub media: MediaKind,
    pub iso_path: String,
    pub url: String,
    pub kernel_args: String,
    pub vcpus: u32,
    pub max_vcpus: u32,
    pub topology_auto: bool,
    pub cpu_model: String,
    pub mem_gib: u32,
    pub max_mem_gib: u32,
    pub balloon: bool,
    pub hugepages: bool,
    pub shared: bool,
    pub disk_new: bool,
    pub pool: String,
    pub disk_size: String,
    pub disk_format: String,
    pub disk_bus: String,
    /// Existing disk image (when `disk_new` is off).
    pub disk_path: String,
    pub net_source: String,
    pub net_model: String,
    pub mac_auto: bool,
    pub mac: String,
    pub firmware: String,
    pub secure_boot: bool,
    pub tpm: bool,
    pub graphics: String,
    pub start_after: bool,
    pub open_console: bool,
    pub autostart: bool,
    /// Validation problem shown under the form.
    #[serde(skip)]
    pub error: String,
    #[serde(skip)]
    pub host: HostFacts,
}

/// What the wizard knows about the host (filled when it opens).
#[derive(Debug, Clone, Default)]
pub struct HostFacts {
    pub threads: u32,
    pub free_gib: f64,
    pub domains: Vec<String>,
    pub pools: Vec<String>,
    pub networks: Vec<String>,
    pub isos: Vec<String>,
}

impl Default for WizardState {
    fn default() -> Self {
        Self {
            step: 0,
            focus: 0,
            name: String::new(),
            os_id: String::new(),
            title: String::new(),
            desc: String::new(),
            media: MediaKind::Iso,
            iso_path: String::new(),
            url: String::new(),
            kernel_args: String::new(),
            vcpus: 2,
            max_vcpus: 4,
            topology_auto: true,
            cpu_model: String::from("host-passthrough"),
            mem_gib: 4,
            max_mem_gib: 8,
            balloon: true,
            hugepages: false,
            shared: false,
            disk_new: true,
            pool: String::from("default"),
            disk_size: String::from("20G"),
            disk_format: String::from("qcow2"),
            disk_bus: String::from("virtio"),
            disk_path: String::new(),
            net_source: String::from("default"),
            net_model: String::from("virtio"),
            mac_auto: true,
            mac: String::new(),
            firmware: String::from("uefi"),
            secure_boot: false,
            tpm: false,
            graphics: String::from("spice"),
            start_after: true,
            open_console: false,
            autostart: false,
            error: String::new(),
            host: HostFacts::default(),
        }
    }
}

/// A wizard field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WField {
    Name,
    Os,
    Title,
    Desc,
    Media,
    Iso,
    Url,
    KernelArgs,
    Vcpus,
    MaxVcpus,
    CpuModel,
    Mem,
    MaxMem,
    Balloon,
    Hugepages,
    DiskNew,
    Pool,
    DiskSize,
    DiskFormat,
    DiskBus,
    DiskPath,
    NetSource,
    NetModel,
    MacAuto,
    Mac,
    Firmware,
    SecureBoot,
    Tpm,
    Graphics,
    StartAfter,
    OpenConsole,
    Autostart,
}

/// How a field is edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WKind {
    /// Free text: printable keys type, Backspace deletes, C-w clears.
    Text,
    /// Number: digits type, Backspace deletes a digit, h/l ←/→ ±1, H/L ±10.
    Number { min: u32, max: u32 },
    /// Choice: h/l ←/→ or Space cycle.
    Select(Vec<String>),
    /// Toggle: Space (or h/l) flips.
    Check,
}

fn opts(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

impl WizardState {
    /// Demo at step 3 (acceptance snapshot).
    pub fn demo_step3() -> Self {
        Self {
            step: 2,
            name: String::from("fedora-42-ws"),
            os_id: String::from("fedora42"),
            media: MediaKind::Iso,
            iso_path: String::from("/var/lib/libvirt/isos/Fedora-WS-Live-42.iso"),
            vcpus: 4,
            max_vcpus: 8,
            cpu_model: String::from("host-passthrough"),
            mem_gib: 8,
            max_mem_gib: 16,
            pool: String::from("nvme-fast"),
            disk_size: String::from("30G"),
            graphics: String::from("spice"),
            open_console: true,
            host: HostFacts {
                threads: 32,
                free_gib: 22.8,
                domains: vec![String::from("arch-dev")],
                pools: vec![
                    String::from("default"),
                    String::from("nvme-fast"),
                    String::from("isos"),
                ],
                networks: vec![String::from("default"), String::from("lab-isolated")],
                isos: vec![String::from("/var/lib/libvirt/isos/Fedora-WS-Live-42.iso")],
            },
            ..Default::default()
        }
    }

    /// Repair values that a draft saved by an older version may have left at 0.
    pub fn sanitized(mut self) -> Self {
        let d = Self::default();
        if self.vcpus == 0 {
            self.vcpus = d.vcpus;
        }
        self.max_vcpus = self.max_vcpus.max(self.vcpus);
        if self.mem_gib == 0 {
            self.mem_gib = d.mem_gib;
        }
        self.max_mem_gib = self.max_mem_gib.max(self.mem_gib);
        for (field, default) in [
            (&mut self.cpu_model, d.cpu_model),
            (&mut self.pool, d.pool),
            (&mut self.disk_size, d.disk_size),
            (&mut self.disk_format, d.disk_format),
            (&mut self.disk_bus, d.disk_bus),
            (&mut self.net_source, d.net_source),
            (&mut self.net_model, d.net_model),
            (&mut self.firmware, d.firmware),
            (&mut self.graphics, d.graphics),
        ] {
            if field.trim().is_empty() {
                *field = default;
            }
        }
        self.step = self.step.min(5);
        self
    }

    /// Fields of the current step (conditional ones only when relevant).
    pub fn fields(&self) -> Vec<WField> {
        use WField as F;
        match self.step {
            0 => vec![F::Name, F::Os, F::Title, F::Desc],
            1 => match self.media {
                MediaKind::Iso => vec![F::Media, F::Iso],
                MediaKind::Url => vec![F::Media, F::Url, F::KernelArgs],
                _ => vec![F::Media],
            },
            2 => vec![
                F::Vcpus,
                F::MaxVcpus,
                F::CpuModel,
                F::Mem,
                F::MaxMem,
                F::Balloon,
                F::Hugepages,
            ],
            3 => {
                if self.disk_new {
                    vec![F::DiskNew, F::Pool, F::DiskSize, F::DiskFormat, F::DiskBus]
                } else {
                    vec![F::DiskNew, F::DiskPath, F::DiskBus]
                }
            }
            4 => {
                if self.net_source == "none" {
                    vec![F::NetSource]
                } else if self.mac_auto {
                    vec![F::NetSource, F::NetModel, F::MacAuto]
                } else {
                    vec![F::NetSource, F::NetModel, F::MacAuto, F::Mac]
                }
            }
            _ => vec![
                F::Firmware,
                F::SecureBoot,
                F::Tpm,
                F::Graphics,
                F::StartAfter,
                F::OpenConsole,
                F::Autostart,
            ],
        }
    }

    /// The focused field (focus is kept in range).
    pub fn focused(&self) -> WField {
        let f = self.fields();
        f[self.focus.min(f.len() - 1)]
    }

    /// Editing kind of a field.
    pub fn kind(&self, f: WField) -> WKind {
        use WField as F;
        let threads = self.host.threads.max(1);
        match f {
            F::Name
            | F::Os
            | F::Title
            | F::Desc
            | F::Iso
            | F::Url
            | F::KernelArgs
            | F::DiskSize
            | F::DiskPath
            | F::Mac => WKind::Text,
            F::Vcpus => WKind::Number {
                min: 1,
                max: threads.max(64),
            },
            F::MaxVcpus => WKind::Number {
                min: self.vcpus,
                max: threads.max(64),
            },
            F::Mem => WKind::Number { min: 1, max: 1024 },
            F::MaxMem => WKind::Number {
                min: self.mem_gib,
                max: 1024,
            },
            F::Media => WKind::Select(MediaKind::ALL.iter().map(|m| m.label().to_string()).collect()),
            F::CpuModel => WKind::Select(opts(&["host-passthrough", "host-model", "qemu64"])),
            F::Pool => {
                let mut p = self.host.pools.clone();
                if p.is_empty() {
                    p.push(self.pool.clone());
                }
                WKind::Select(p)
            }
            F::DiskFormat => WKind::Select(opts(&["qcow2", "raw"])),
            F::DiskBus => WKind::Select(opts(&["virtio", "sata", "scsi"])),
            F::NetSource => {
                let mut n = self.host.networks.clone();
                if n.is_empty() {
                    n.push(String::from("default"));
                }
                n.push(String::from("none"));
                WKind::Select(n)
            }
            F::NetModel => WKind::Select(opts(&["virtio", "e1000e"])),
            F::Firmware => WKind::Select(opts(&["uefi", "bios"])),
            F::Graphics => WKind::Select(opts(&["spice", "vnc", "none"])),
            F::Balloon
            | F::Hugepages
            | F::DiskNew
            | F::MacAuto
            | F::SecureBoot
            | F::Tpm
            | F::StartAfter
            | F::OpenConsole
            | F::Autostart => WKind::Check,
        }
    }

    /// Field label.
    pub fn label(f: WField) -> &'static str {
        use WField as F;
        match f {
            F::Name => "name",
            F::Os => "OS (osinfo)",
            F::Title => "title",
            F::Desc => "description",
            F::Media => "install from",
            F::Iso => "ISO path",
            F::Url => "tree URL",
            F::KernelArgs => "kernel args",
            F::Vcpus => "vCPUs",
            F::MaxVcpus => "maximum vCPUs",
            F::CpuModel => "CPU model",
            F::Mem => "memory (GiB)",
            F::MaxMem => "maximum (GiB)",
            F::Balloon => "virtio balloon",
            F::Hugepages => "hugepages",
            F::DiskNew => "create new disk",
            F::Pool => "storage pool",
            F::DiskSize => "size",
            F::DiskFormat => "format",
            F::DiskBus => "bus",
            F::DiskPath => "existing image",
            F::NetSource => "network",
            F::NetModel => "model",
            F::MacAuto => "automatic MAC",
            F::Mac => "MAC address",
            F::Firmware => "firmware",
            F::SecureBoot => "secure boot",
            F::Tpm => "TPM 2.0",
            F::Graphics => "graphics",
            F::StartAfter => "start after creation",
            F::OpenConsole => "open viewer",
            F::Autostart => "autostart on boot",
        }
    }

    /// One-line help for a field.
    pub fn hint(&self, f: WField) -> String {
        use WField as F;
        match f {
            F::Name => String::from("letters, digits, . _ - · must be unique"),
            F::Os => match detect_os(&self.iso_path) {
                Some(os) if self.os_id.is_empty() => {
                    format!("empty: virt-install detects it (ISO looks like {os})")
                }
                _ => String::from(
                    "e.g. fedora42, ubuntu24.04, archlinux, win11 · empty: detect from the media",
                ),
            },
            F::Iso => {
                if self.host.isos.is_empty() {
                    String::from("absolute path to the ISO")
                } else {
                    format!("known: {}", self.host.isos.join("  "))
                }
            }
            F::Url => String::from("installer tree, e.g. https://…/os/x86_64/"),
            F::Vcpus => format!("of {} host threads", self.host.threads.max(1)),
            F::MaxVcpus => String::from("hot-plug ceiling"),
            F::Mem => format!("host free {:.1} GiB", self.host.free_gib),
            F::MaxMem => String::from("balloon can grow to this"),
            F::DiskSize => String::from("e.g. 20G, 512M"),
            F::Mac => String::from("52:54:00:xx:xx:xx"),
            F::SecureBoot => String::from("UEFI only"),
            _ => String::new(),
        }
    }

    /// Displayed value of a field.
    pub fn value(&self, f: WField) -> String {
        use WField as F;
        let b = |v: bool| String::from(if v { "x" } else { "" });
        match f {
            F::Name => self.name.clone(),
            F::Os => self.os_id.clone(),
            F::Title => self.title.clone(),
            F::Desc => self.desc.clone(),
            F::Media => self.media.label().to_string(),
            F::Iso => self.iso_path.clone(),
            F::Url => self.url.clone(),
            F::KernelArgs => self.kernel_args.clone(),
            F::Vcpus => self.vcpus.to_string(),
            F::MaxVcpus => self.max_vcpus.to_string(),
            F::CpuModel => self.cpu_model.clone(),
            F::Mem => self.mem_gib.to_string(),
            F::MaxMem => self.max_mem_gib.to_string(),
            F::Balloon => b(self.balloon),
            F::Hugepages => b(self.hugepages),
            F::DiskNew => b(self.disk_new),
            F::Pool => self.pool.clone(),
            F::DiskSize => self.disk_size.clone(),
            F::DiskFormat => self.disk_format.clone(),
            F::DiskBus => self.disk_bus.clone(),
            F::DiskPath => self.disk_path.clone(),
            F::NetSource => self.net_source.clone(),
            F::NetModel => self.net_model.clone(),
            F::MacAuto => b(self.mac_auto),
            F::Mac => self.mac.clone(),
            F::Firmware => self.firmware.clone(),
            F::SecureBoot => b(self.secure_boot),
            F::Tpm => b(self.tpm),
            F::Graphics => self.graphics.clone(),
            F::StartAfter => b(self.start_after),
            F::OpenConsole => b(self.open_console),
            F::Autostart => b(self.autostart),
        }
    }

    fn text_mut(&mut self, f: WField) -> Option<&mut String> {
        use WField as F;
        Some(match f {
            F::Name => &mut self.name,
            F::Os => &mut self.os_id,
            F::Title => &mut self.title,
            F::Desc => &mut self.desc,
            F::Iso => &mut self.iso_path,
            F::Url => &mut self.url,
            F::KernelArgs => &mut self.kernel_args,
            F::DiskSize => &mut self.disk_size,
            F::DiskPath => &mut self.disk_path,
            F::Mac => &mut self.mac,
            _ => return None,
        })
    }

    fn num_mut(&mut self, f: WField) -> Option<&mut u32> {
        use WField as F;
        Some(match f {
            F::Vcpus => &mut self.vcpus,
            F::MaxVcpus => &mut self.max_vcpus,
            F::Mem => &mut self.mem_gib,
            F::MaxMem => &mut self.max_mem_gib,
            _ => return None,
        })
    }

    /// Keep maxima ≥ current values.
    fn fix_ranges(&mut self) {
        self.max_vcpus = self.max_vcpus.max(self.vcpus);
        self.max_mem_gib = self.max_mem_gib.max(self.mem_gib);
    }

    /// Type a character into the focused field. Returns false when the field
    /// does not take characters (the key may then be a shortcut).
    pub fn type_char(&mut self, c: char) -> bool {
        let f = self.focused();
        match self.kind(f) {
            WKind::Text => {
                if f == WField::Name && !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
                    return true; // swallowed: not allowed in a domain name
                }
                if let Some(s) = self.text_mut(f) {
                    s.push(c);
                }
                if f == WField::Iso && self.os_id.is_empty() {
                    // keep the OS hint current; nothing else to do
                }
                self.error.clear();
                true
            }
            WKind::Number { max, .. } => {
                let Some(d) = c.to_digit(10) else { return false };
                if let Some(n) = self.num_mut(f) {
                    *n = (n.saturating_mul(10) + d).min(max);
                }
                self.fix_ranges();
                true
            }
            _ => false,
        }
    }

    /// Backspace in the focused field.
    pub fn backspace(&mut self) {
        let f = self.focused();
        if let Some(s) = self.text_mut(f) {
            s.pop();
        } else if let Some(n) = self.num_mut(f) {
            *n /= 10;
        }
    }

    /// Clear the focused text field (C-w).
    pub fn clear_field(&mut self) {
        let f = self.focused();
        if let Some(s) = self.text_mut(f) {
            s.clear();
        }
    }

    /// Adjust the focused field: numbers ±delta, selects cycle, checks toggle.
    pub fn adjust(&mut self, delta: i32) {
        use WField as F;
        let f = self.focused();
        match self.kind(f) {
            WKind::Number { min, max } => {
                if let Some(n) = self.num_mut(f) {
                    *n = (*n as i64 + delta as i64).clamp(min as i64, max as i64) as u32;
                }
                self.fix_ranges();
            }
            WKind::Select(options) => {
                let cur = self.value(f);
                let i = options.iter().position(|o| *o == cur).unwrap_or(0) as i32;
                let n = options.len() as i32;
                let next = options[(i + delta.signum()).rem_euclid(n) as usize].clone();
                match f {
                    F::Media => {
                        self.media = MediaKind::ALL[(i + delta.signum()).rem_euclid(n) as usize];
                        self.focus = 0;
                    }
                    F::CpuModel => self.cpu_model = next,
                    F::Pool => self.pool = next,
                    F::DiskFormat => self.disk_format = next,
                    F::DiskBus => self.disk_bus = next,
                    F::NetSource => self.net_source = next,
                    F::NetModel => self.net_model = next,
                    F::Firmware => {
                        self.firmware = next;
                        if self.firmware != "uefi" {
                            self.secure_boot = false;
                        }
                    }
                    F::Graphics => self.graphics = next,
                    _ => {}
                }
            }
            WKind::Check => self.toggle(),
            WKind::Text => {}
        }
    }

    /// Toggle the focused check box.
    pub fn toggle(&mut self) {
        use WField as F;
        match self.focused() {
            F::Balloon => self.balloon = !self.balloon,
            F::Hugepages => self.hugepages = !self.hugepages,
            F::DiskNew => {
                self.disk_new = !self.disk_new;
                self.focus = 0;
            }
            F::MacAuto => self.mac_auto = !self.mac_auto,
            F::SecureBoot => self.secure_boot = !self.secure_boot && self.firmware == "uefi",
            F::Tpm => self.tpm = !self.tpm,
            F::StartAfter => self.start_after = !self.start_after,
            F::OpenConsole => self.open_console = !self.open_console,
            F::Autostart => self.autostart = !self.autostart,
            _ => {}
        }
    }

    /// Move focus within the step (wrapping).
    pub fn move_focus(&mut self, delta: i32) {
        let n = self.fields().len() as i32;
        self.focus = (self.focus.min(n as usize - 1) as i32 + delta).rem_euclid(n) as usize;
    }

    /// Go to step `s` (focus on its first field).
    pub fn goto_step(&mut self, s: usize) {
        self.step = s.min(5);
        self.focus = 0;
    }

    /// Problems that block creation, first one per step.
    pub fn problems(&self) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let existing: Vec<&str> = self.host.domains.iter().map(String::as_str).collect();
        if self.name.is_empty() {
            out.push((0, String::from("the domain needs a name")));
        } else if !valid_name(&self.name, &existing) {
            out.push((0, format!("`{}` is invalid or already exists", self.name)));
        }
        match self.media {
            MediaKind::Iso if self.iso_path.trim().is_empty() => {
                out.push((1, String::from("choose an ISO path")))
            }
            MediaKind::Iso if !self.iso_path.starts_with('/') => {
                out.push((1, String::from("the ISO path must be absolute")))
            }
            MediaKind::Url if self.url.trim().is_empty() => {
                out.push((1, String::from("enter the installer URL")))
            }
            MediaKind::Import if self.disk_new => out.push((
                3,
                String::from("import needs an existing disk image (turn off \"create new disk\")"),
            )),
            _ => {}
        }
        if !self.disk_new && self.disk_path.trim().is_empty() {
            out.push((3, String::from("enter the existing disk image path")));
        }
        if self.disk_new && crate::ui::views::detail::hardware::size_to_kib(&self.disk_size).is_none() {
            out.push((3, format!("invalid disk size `{}`", self.disk_size)));
        }
        if !self.mac_auto && self.net_source != "none" {
            let ok = self.mac.split(':').count() == 6
                && self
                    .mac
                    .split(':')
                    .all(|p| p.len() == 2 && u8::from_str_radix(p, 16).is_ok());
            if !ok {
                out.push((4, String::from("invalid MAC address")));
            }
        }
        out
    }

    /// Draft file path.
    pub fn draft_path() -> Option<std::path::PathBuf> {
        directories::ProjectDirs::from("", "", "virsh-tui")
            .and_then(|d| d.state_dir().map(|p| p.to_path_buf()))
            .map(|p| p.join("wizard-draft.toml"))
    }

    /// Save the draft.
    pub fn save_draft(&self, path: &std::path::Path) -> color_eyre::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Autosave to the default path.
    pub fn autosave(&self) {
        if let Some(path) = Self::draft_path() {
            let _ = self.save_draft(&path);
        }
    }

    /// Remove the saved draft (after a successful create).
    pub fn discard_draft() {
        if let Some(path) = Self::draft_path() {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Load a draft.
    pub fn load_draft(path: &std::path::Path) -> color_eyre::Result<Self> {
        let st: Self = toml::from_str(&std::fs::read_to_string(path)?)?;
        Ok(st.sanitized())
    }

    /// Step summaries for the side list.
    pub fn summaries(&self) -> [String; 6] {
        let or_dash = |s: &str| {
            if s.is_empty() {
                String::from("—")
            } else {
                s.to_string()
            }
        };
        [
            format!(
                "{} · {}",
                or_dash(&self.name),
                if self.os_id.is_empty() {
                    "detect OS"
                } else {
                    &self.os_id
                }
            ),
            match self.media {
                MediaKind::Iso => format!("ISO · {}", or_dash(&file_name(&self.iso_path))),
                MediaKind::Url => format!("URL · {}", or_dash(&self.url)),
                MediaKind::Pxe => String::from("PXE"),
                MediaKind::Import => String::from("import disk"),
                MediaKind::None => String::from("no media"),
            },
            format!("{} vCPU · {} GiB", self.vcpus, self.mem_gib),
            if self.disk_new {
                format!("new {} {} on {}", self.disk_size, self.disk_format, self.pool)
            } else {
                format!("existing · {}", or_dash(&file_name(&self.disk_path)))
            },
            if self.net_source == "none" {
                String::from("no network")
            } else {
                format!("{} · {}", self.net_source, self.net_model)
            },
            format!(
                "{} · {}{}",
                self.firmware,
                if self.start_after { "start" } else { "no start" },
                if self.autostart { " · autostart" } else { "" }
            ),
        ]
    }

    /// Which step a `virt-install` argument belongs to (for highlighting).
    pub fn step_of_arg(arg: &str) -> usize {
        let flag = arg.split(['=', ' ']).next().unwrap_or("");
        match flag {
            "--name" | "--osinfo" | "--metadata" => 0,
            "--cdrom" | "--location" | "--extra-args" | "--pxe" | "--import" => 1,
            "--vcpus" | "--cpu" | "--memory" | "--memballoon" | "--memorybacking" => 2,
            "--disk" => 3,
            "--network" | "--mac" => 4,
            _ => 5,
        }
    }
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Render the wizard modal over a dimmed background.
pub fn render(frame: &mut Frame, theme: &Theme, st: &WizardState) {
    let t = &theme.tokens;
    let area = frame.area();
    let w = 136u16.min(area.width.saturating_sub(4)).max(100.min(area.width));
    let h = 36u16.min(area.height.saturating_sub(4)).max(30.min(area.height));
    let modal = Rect::new(
        area.x + area.width.saturating_sub(w) / 2,
        area.y + area.height.saturating_sub(h) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    let right = Line::from(Span::styled(format!("step {} / 6", st.step + 1), theme.dim()));
    let block = crate::ui::widgets::panel::block(theme, true, "＋ New domain", Some(right), None, None)
        .style(Style::default().bg(t.bg));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(10), Constraint::Length(1)])
        .split(inner);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(32), Constraint::Min(0)])
        .spacing(2)
        .split(rows[0]);
    render_steps(frame, theme, st, top[0]);
    render_step_form(frame, theme, st, top[1]);
    render_command(frame, theme, st, rows[1]);
    let next = if st.step == 5 {
        String::from(" ⏎ create ")
    } else {
        format!(
            " ⏎ {} › ",
            STEP_NAMES[st.step + 1]
                .split_whitespace()
                .skip(1)
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let left = vec![
        Span::styled("⇥/↑↓", theme.key()),
        Span::styled(" field  ", theme.dim()),
        Span::styled("h/l ←→ ␣", theme.key()),
        Span::styled(" change  ", theme.dim()),
        Span::styled("⌫", theme.key()),
        Span::styled(" delete  ", theme.dim()),
        Span::styled("C-n/C-p", theme.key()),
        Span::styled(" step  ", theme.dim()),
        Span::styled("C-y", theme.key()),
        Span::styled(" copy  ", theme.dim()),
        Span::styled("C-e", theme.key()),
        Span::styled(" XML  ", theme.dim()),
        Span::styled("C-r", theme.key()),
        Span::styled(" reset  ", theme.dim()),
        Span::styled("⎋", theme.key()),
        Span::styled(" close", theme.dim()),
    ];
    let right = vec![Span::styled(
        next,
        Style::default().fg(t.bg2).bg(t.blue).add_modifier(Modifier::BOLD),
    )];
    crate::ui::chrome::render_split(frame, rows[2], left, right, Style::default().bg(t.bg));
}

fn render_steps(frame: &mut Frame, theme: &Theme, st: &WizardState, area: Rect) {
    let t = &theme.tokens;
    let problems = st.problems();
    let mut lines: Vec<Line> = Vec::new();
    let summaries = st.summaries();
    for (i, name) in STEP_NAMES.iter().enumerate() {
        let blocked = problems.iter().any(|(s, _)| *s == i);
        let (glyph, color) = if i == st.step {
            ("●", t.blue)
        } else if blocked {
            ("!", t.yellow)
        } else if i < st.step {
            ("✓", t.green)
        } else {
            ("○", t.comment)
        };
        let bg = if i == st.step { t.sel } else { t.bg };
        let style = if i == st.step {
            Style::default().fg(t.fg).add_modifier(Modifier::BOLD)
        } else {
            theme.text()
        };
        lines.push(
            Line::from(vec![
                Span::styled(format!("{glyph} "), Style::default().fg(color)),
                Span::styled(*name, style),
            ])
            .style(Style::default().bg(bg)),
        );
        let summary: String = summaries[i]
            .chars()
            .take(area.width.saturating_sub(2) as usize)
            .collect();
        lines.push(
            Line::from(vec![Span::raw("  "), Span::styled(summary, theme.dim())])
                .style(Style::default().bg(bg)),
        );
        lines.push(Line::from(""));
    }
    // Checklist instead of made-up osinfo requirements.
    if problems.is_empty() {
        lines.push(Line::from(Span::styled(
            "✓ ready to create",
            Style::default().fg(t.green),
        )));
    } else {
        for (_, p) in problems.iter().take(3) {
            let p: String = p.chars().take(area.width.saturating_sub(2) as usize).collect();
            lines.push(Line::from(Span::styled(
                format!("! {p}"),
                Style::default().fg(t.yellow),
            )));
        }
    }
    frame.render_widget(Paragraph::new(lines).style(theme.base()), area);
}

fn render_step_form(frame: &mut Frame, theme: &Theme, st: &WizardState, area: Rect) {
    let t = &theme.tokens;
    let section = Style::default().fg(t.blue).add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::from(Span::styled(
            STEP_NAMES[st.step]
                .split_whitespace()
                .skip(1)
                .collect::<Vec<_>>()
                .join(" "),
            section,
        )),
        Line::from(""),
    ];
    let fields = st.fields();
    let focus = st.focus.min(fields.len() - 1);
    let slider_w = 30usize;
    for (i, &f) in fields.iter().enumerate() {
        let cur = i == focus;
        let row_bg = if cur { t.sel } else { t.bg };
        let mut spans = vec![
            Span::styled(if cur { "❯ " } else { "  " }, Style::default().fg(t.blue)),
            Span::styled(format!("{:<20}", WizardState::label(f)), theme.secondary()),
        ];
        let value = st.value(f);
        match st.kind(f) {
            WKind::Text => {
                let shown = if value.is_empty() && !cur {
                    String::from("—")
                } else {
                    value
                };
                spans.push(Span::styled(
                    format!(" {shown}"),
                    Style::default().fg(t.fg).bg(t.hl),
                ));
                if cur {
                    spans.push(Span::styled(" ", Style::default().bg(t.fg)));
                }
                spans.push(Span::styled(" ", Style::default().bg(t.hl)));
            }
            WKind::Number { max, .. } => {
                if matches!(f, WField::Vcpus | WField::Mem) {
                    let cap = if f == WField::Vcpus {
                        st.host.threads.max(1)
                    } else {
                        (st.host.free_gib.ceil() as u32).max(st.mem_gib).max(1)
                    };
                    let n: u32 = value.parse().unwrap_or(0);
                    let filled = (n.min(cap) as usize * slider_w / cap.max(1) as usize).min(slider_w);
                    let col = if f == WField::Vcpus { t.blue } else { t.magenta };
                    spans.push(Span::styled("━".repeat(filled), Style::default().fg(col)));
                    spans.push(Span::styled(
                        "●",
                        Style::default().fg(col).add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(
                        "─".repeat(slider_w - filled),
                        Style::default().fg(t.gutter),
                    ));
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(
                    format!(" {value:>3} "),
                    Style::default().fg(t.fg).bg(t.hl),
                ));
                let _ = max;
            }
            WKind::Select(options) => {
                if options.len() <= 4 {
                    for o in &options {
                        let on = *o == value;
                        spans.push(Span::styled(
                            if on { "(●) " } else { "( ) " },
                            Style::default().fg(if on { t.blue } else { t.comment }),
                        ));
                        spans.push(Span::styled(
                            format!("{o}   "),
                            if on {
                                theme.text().add_modifier(Modifier::BOLD)
                            } else {
                                theme.secondary()
                            },
                        ));
                    }
                } else {
                    spans.push(Span::styled(
                        format!(" ‹ {value} › "),
                        Style::default().fg(t.fg).bg(t.hl),
                    ));
                }
            }
            WKind::Check => {
                let on = value == "x";
                spans.push(Span::styled(
                    if on { "[x]" } else { "[ ]" },
                    Style::default().fg(if on { t.green } else { t.comment }),
                ));
            }
        }
        lines.push(Line::from(spans).style(Style::default().bg(row_bg)));
        let hint = st.hint(f);
        if cur && !hint.is_empty() {
            let hint: String = hint
                .chars()
                .take(area.width.saturating_sub(24) as usize)
                .collect();
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(22)),
                Span::styled(hint, theme.dim()),
            ]));
        }
    }
    if !st.error.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("✗ {}", st.error),
            Style::default().fg(t.red),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(theme.base()),
        area,
    );
}

fn render_command(frame: &mut Frame, theme: &Theme, st: &WizardState, area: Rect) {
    let t = &theme.tokens;
    // Exactly the argv the wizard will run.
    let plan = crate::command::builders::install::virt_install(st);
    let argv = &plan.steps[0].argv;
    let mut lines = vec![Line::from(vec![
        Span::styled("virt-install ", Style::default().fg(t.green)),
        Span::styled("\\", theme.dim()),
    ])];
    let mut marked = false;
    let max = area.height.saturating_sub(3) as usize;
    let mut groups: Vec<(usize, Vec<&String>)> = Vec::new();
    for a in argv {
        let s = WizardState::step_of_arg(a);
        match groups.last_mut() {
            Some((gs, items)) if *gs == s => items.push(a),
            _ => groups.push((s, vec![a])),
        }
    }
    let last = groups.len().min(max).saturating_sub(1);
    for (gi, (step, items)) in groups.iter().take(max).enumerate() {
        let current = *step == st.step;
        let bg = if current { t.hl } else { t.bg2 };
        let text = items.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ");
        let mut spans = vec![Span::raw("  ")];
        spans.extend(flag_spans(theme, text));
        if gi < last {
            spans.push(Span::styled(" \\", theme.dim()));
        }
        if current && !marked {
            spans.push(Span::styled("   ◂ this step", Style::default().fg(t.blue)));
            marked = true;
        }
        lines.push(Line::from(spans).style(Style::default().bg(bg)));
    }
    let block = crate::ui::widgets::panel::block(
        theme,
        false,
        "Equivalent command",
        Some(crate::ui::widgets::panel::right(
            "C-y copy · C-e edit as XML",
            theme,
        )),
        None,
        None,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(t.fg2).bg(t.bg2)),
        area,
    );
}

/// Command-token spans: flags cyan, values orange, paths yellow.
fn flag_spans(theme: &Theme, text: String) -> Vec<Span<'static>> {
    let t = &theme.tokens;
    let mut out = Vec::new();
    for tok in text.split_whitespace() {
        match tok.split_once('=') {
            Some((flag, val)) if flag.starts_with("--") => {
                out.push(Span::styled(flag.to_string(), Style::default().fg(t.cyan)));
                out.push(Span::styled("=", theme.dim()));
                let style = if val.contains('/') {
                    Style::default().fg(t.yellow)
                } else {
                    Style::default().fg(t.orange)
                };
                out.push(Span::styled(val.to_string(), style));
            }
            _ => out.push(Span::styled(
                tok.to_string(),
                if tok.starts_with("--") {
                    Style::default().fg(t.cyan)
                } else {
                    Style::default().fg(t.fg)
                },
            )),
        }
        out.push(Span::raw(" "));
    }
    out
}
