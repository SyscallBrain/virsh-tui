//! App configuration: load/save ~/.config/virsh-tui/config.toml.

use color_eyre::Result;
use serde::{Deserialize, Serialize};

/// Top-level config.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub appearance: Appearance,
    pub connections: Connections,
    pub monitoring: Monitoring,
    pub confirmations: Confirmations,
    pub console: Console,
    pub wizard: Wizard,
    /// File this config was loaded from (`--config` or the default path);
    /// `save` writes back to it.
    #[serde(skip)]
    pub source: Option<std::path::PathBuf>,
    /// Parse error of the file at `source`: the app runs on defaults and
    /// never overwrites that file (the user's settings would be lost).
    #[serde(skip)]
    pub load_error: Option<String>,
}

/// General options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct General {
    pub default_uri: String,
    pub refresh_interval_secs: u64,
    pub editor: String,
    pub start_view: String,
    /// Key hints in status bars, panels and dialogs. Off once the keys are
    /// known; `?` still lists them and dialog buttons stay.
    pub key_hints: bool,
    /// Domain list order at start: state, name, cpu, mem, uptime.
    pub default_sort: String,
    /// Domain state chip at start: all, running, paused, crashed, off.
    pub default_filter: String,
    /// `:` lines kept across sessions (0: not saved).
    pub history_size: usize,
    /// Seconds after an ACPI shutdown before the guest is reported as
    /// ignoring it.
    pub shutdown_timeout_secs: u64,
    /// Log level of the log file: error, warn, info, debug, trace
    /// (`RUST_LOG` overrides it).
    pub log_level: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            default_uri: String::from("qemu:///system"),
            refresh_interval_secs: 1,
            editor: String::from("vi"),
            start_view: String::from("domains"),
            key_hints: true,
            default_sort: String::from("state"),
            default_filter: String::from("all"),
            history_size: 500,
            shutdown_timeout_secs: crate::ui::views::dashboard::SHUTDOWN_TIMEOUT_SECS,
            log_level: String::from("info"),
        }
    }
}

/// Appearance options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    pub theme: String,
    pub transparent: bool,
    pub borders: String,
    pub icons: String,
    pub graphs: String,
    pub gradient: bool,
    pub dim_modals: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: String::from("tokyo-night"),
            transparent: false,
            borders: String::from("rounded"),
            icons: String::from("unicode"),
            graphs: String::from("braille"),
            gradient: true,
            dim_modals: true,
        }
    }
}

/// Saved connections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Connections {
    pub uris: Vec<String>,
    pub default: String,
}

impl Default for Connections {
    fn default() -> Self {
        Self {
            // Only working local URIs: a placeholder like
            // `qemu+ssh://user@host/system` would just fail when picked.
            uris: vec![String::from("qemu:///system"), String::from("qemu:///session")],
            default: String::from("qemu:///system"),
        }
    }
}

/// Monitoring options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Monitoring {
    pub balloon_period_secs: u64,
    pub balloon_on: bool,
    pub thread_sampling: bool,
    /// Graph history window at start: 60s, 5m, 1h.
    pub history_window: String,
}

impl Default for Monitoring {
    fn default() -> Self {
        Self {
            balloon_period_secs: 2,
            // Opt-in: enabling it changes running guests (dommemstat --period --live).
            balloon_on: false,
            thread_sampling: true,
            history_window: String::from("60s"),
        }
    }
}

/// New-domain wizard options.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Wizard {
    /// Folders scanned (one level, `*.iso`) for the ISO picker. Empty: the
    /// XDG download folder. `~/` is expanded.
    pub iso_dirs: Vec<String>,
}

static RUNTIME: std::sync::RwLock<Option<Config>> = std::sync::RwLock::new(None);

/// Refresh interval of the runtime config (read every frame: no clone).
static REFRESH_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Status-bar key hints on (read every frame: no clone).
static KEY_HINTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

// Unit tests run in parallel threads of one process: each test thread gets
// its own runtime config so `:set` / Settings tests cannot see each other's
// values. The binary uses the process-wide statics above.
#[cfg(test)]
thread_local! {
    static TEST_RUNTIME: std::cell::RefCell<Option<Config>> = const { std::cell::RefCell::new(None) };
}

/// Whether the status bars show key hints.
pub fn key_hints() -> bool {
    #[cfg(test)]
    if let Some(c) = TEST_RUNTIME.with(|r| r.borrow().as_ref().map(|c| c.general.key_hints)) {
        return c;
    }
    KEY_HINTS.load(std::sync::atomic::Ordering::Relaxed)
}

/// The active refresh interval in seconds (≥ 1).
pub fn refresh_interval_secs() -> u64 {
    #[cfg(test)]
    if let Some(n) = TEST_RUNTIME.with(|r| r.borrow().as_ref().map(|c| c.general.refresh_interval_secs)) {
        return n.max(1);
    }
    REFRESH_SECS.load(std::sync::atomic::Ordering::Relaxed).max(1)
}

/// Publish the active config (the one loaded from `--config` or the default
/// path, plus Settings changes) for code that has no handle to it.
pub fn set_runtime(c: &Config) {
    #[cfg(test)]
    {
        TEST_RUNTIME.with(|r| *r.borrow_mut() = Some(c.clone()));
        return;
    }
    #[allow(unreachable_code)]
    {
        REFRESH_SECS.store(
            c.general.refresh_interval_secs.max(1),
            std::sync::atomic::Ordering::Relaxed,
        );
        KEY_HINTS.store(c.general.key_hints, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut g) = RUNTIME.write() {
            *g = Some(c.clone());
        }
    }
}

/// The active config (defaults until published).
pub fn runtime() -> Config {
    #[cfg(test)]
    if let Some(c) = TEST_RUNTIME.with(|r| r.borrow().clone()) {
        return c;
    }
    RUNTIME.read().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Current confirmation settings.
pub fn confirmations() -> Confirmations {
    runtime().confirmations
}

/// Per-action confirmation toggles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Confirmations {
    pub destroy: bool,
    pub reset: bool,
    pub undefine: bool,
    pub revert: bool,
    pub delete_volume: bool,
    pub wipe: bool,
    pub type_name_for_undefine: bool,
    /// Ask before a destructive action on this many marked domains or more,
    /// even when its own confirmation is off (0: never).
    pub bulk_threshold: usize,
}

impl Default for Confirmations {
    fn default() -> Self {
        Self {
            destroy: true,
            reset: true,
            undefine: true,
            revert: true,
            delete_volume: true,
            wipe: true,
            type_name_for_undefine: true,
            bulk_threshold: 0,
        }
    }
}

/// Console and viewer options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Console {
    pub viewer: String,
    pub escape: String,
}

impl Default for Console {
    fn default() -> Self {
        Self {
            viewer: String::from("virt-viewer"),
            escape: String::from("C-]"),
        }
    }
}

impl Config {
    /// Config file path.
    pub fn path() -> Option<std::path::PathBuf> {
        directories::ProjectDirs::from("", "", "virsh-tui").map(|d| d.config_dir().join("config.toml"))
    }

    /// Load from disk, falling back to defaults.
    pub fn load() -> Self {
        Self::load_from(Self::path())
    }

    /// Load an explicit config file (`--config`).
    pub fn load_file(path: &std::path::Path) -> Self {
        Self::load_from(Some(path.to_path_buf()))
    }

    fn load_from(path: Option<std::path::PathBuf>) -> Self {
        let Some(path) = path else { return Self::default() };
        let mut cfg: Self = match std::fs::read_to_string(&path) {
            Err(_) => Self::default(),
            Ok(text) => match toml::from_str(&text) {
                Ok(cfg) => cfg,
                Err(e) => {
                    let mut cfg = Self::default();
                    let first = e.to_string().lines().next().unwrap_or_default().to_string();
                    cfg.load_error = Some(format!("{}: {first}", path.display()));
                    cfg
                }
            },
        };
        cfg.source = Some(path);
        cfg
    }

    /// Save to disk atomically (temporary file + rename in the same
    /// directory). Refused while the file on disk could not be parsed.
    pub fn save(&self) -> Result<()> {
        if let Some(error) = &self.load_error {
            return Err(color_eyre::eyre::eyre!(
                "not saved: fix the config file first ({error})"
            ));
        }
        let Some(path) = self.source.clone().or_else(Self::path) else {
            return Ok(());
        };
        let dir = path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        std::fs::create_dir_all(&dir)?;
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        std::io::Write::write_all(&mut file, toml::to_string_pretty(self)?.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&path).map_err(|e| e.error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn defaults_match_spec() {
        let cfg = Config::default();
        assert_eq!(cfg.general.default_uri, "qemu:///system");
        assert_eq!(cfg.appearance.theme, "tokyo-night");
    }

    #[test]
    fn missing_file_gives_defaults() {
        let path = std::path::PathBuf::from("/nonexistent-virsh-tui.toml");
        let cfg = Config::load_from(Some(path.clone()));
        assert_eq!(cfg.general, Config::default().general);
        assert_eq!(
            cfg.source,
            Some(path),
            "save() writes back to the file it came from"
        );
    }

    #[test]
    fn runtime_config_is_isolated_between_test_threads() {
        use std::sync::{Arc, Barrier};
        let set = Arc::new(Barrier::new(2));
        let reset = Arc::new(Barrier::new(2));
        let (s1, r1) = (set.clone(), reset.clone());
        let a = std::thread::spawn(move || {
            let mut cfg = Config::default();
            cfg.console.viewer = String::from("remote-viewer");
            cfg.general.key_hints = false;
            super::set_runtime(&cfg);
            s1.wait();
            r1.wait();
            (super::runtime().console.viewer, super::key_hints())
        });
        set.wait();
        super::set_runtime(&Config::default());
        reset.wait();
        assert_eq!(a.join().unwrap(), (String::from("remote-viewer"), false));
    }
}
