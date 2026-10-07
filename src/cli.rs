//! Command-line interface.

use clap::Parser;

/// virsh-tui: a vim-native TUI for libvirt/virsh.
#[derive(Debug, Clone, Parser)]
#[command(name = "virsh-tui", version, about = "vim-native TUI for libvirt/virsh")]
pub struct Cli {
    /// Libvirt connection URI (e.g. qemu:///system); defaults to the config's default URI.
    #[arg(short = 'c', long = "connect", default_value = "")]
    pub connect: String,

    /// Run against the deterministic in-memory demo backend.
    #[arg(long = "demo", default_value_t = false)]
    pub demo: bool,

    /// Print commands instead of executing them.
    #[arg(long = "dry-run", default_value_t = false)]
    pub dry_run: bool,

    /// Theme name (built-in or custom file name); overrides the config.
    #[arg(long = "theme")]
    pub theme: Option<String>,

    /// Explicit config file path.
    #[arg(long = "config")]
    pub config: Option<String>,

    /// Open straight into a screen state (dashboard, host, hardware, wizard, ...).
    #[arg(long = "screen")]
    pub screen: Option<String>,

    /// Render one frame to an ANSI file and exit.
    #[arg(long = "dump")]
    pub dump: Option<String>,

    /// Frame size for --dump, e.g. 174x43.
    #[arg(long = "size", default_value = "174x43")]
    pub size: String,

    /// Print parsed domains, host info, networks, and pools as JSON (read-only) and exit.
    #[arg(long = "probe", default_value_t = false)]
    pub probe: bool,

    /// Freeze the demo clock and pre-fill histories (used by tests).
    #[arg(long = "frozen", default_value_t = false)]
    pub frozen: bool,
}
