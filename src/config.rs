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
    /// Never change anything: virsh opens read-only connections (`-r`) and
    /// actions that would change libvirt objects are refused.
    pub read_only: bool,
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
            read_only: false,
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
    /// Per-connection overrides, keyed by URI:
    /// `[connections.profiles."qemu+ssh://admin@hv01/system"]`.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub profiles: std::collections::BTreeMap<String, Profile>,
}

/// Settings for one connection; unset fields use the global value.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Profile {
    /// Name shown in the top bar, e.g. `prod`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Colour of the label: a theme colour (`red`, `green`, `yellow`,
    /// `blue`, `magenta`, `cyan`, `orange`, `teal`) or `#rrggbb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_interval_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_sampling: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewer: Option<String>,
}

impl Default for Connections {
    fn default() -> Self {
        Self {
            // Only working local URIs: a placeholder like
            // `qemu+ssh://user@host/system` would just fail when picked.
            uris: vec![String::from("qemu:///system"), String::from("qemu:///session")],
            default: String::from("qemu:///system"),
            profiles: std::collections::BTreeMap::new(),
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

/// Read-only for the active connection (checked before every process spawn).
static READ_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `--read-only` was given: read-only whatever the config says.
static READ_ONLY_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// URI whose profile is applied by [`set_runtime`].
static ACTIVE_URI: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());

/// Top-bar badge of the active connection: (label, colour).
static BADGE: std::sync::RwLock<Option<(String, Option<String>)>> = std::sync::RwLock::new(None);

// Unit tests run in parallel threads of one process: each test thread gets
// its own runtime config so `:set` / Settings tests cannot see each other's
// values. The binary uses the process-wide statics above.
#[cfg(test)]
thread_local! {
    static TEST_RUNTIME: std::cell::RefCell<Option<Config>> = const { std::cell::RefCell::new(None) };
    static TEST_URI: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static TEST_RO_FLAG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Select the connection whose profile [`set_runtime`] applies. Call
/// [`set_runtime`] afterwards to publish the merged values.
pub fn set_active_uri(uri: &str) {
    #[cfg(test)]
    TEST_URI.with(|u| *u.borrow_mut() = uri.to_string());
    if let Ok(mut g) = ACTIVE_URI.write() {
        *g = uri.to_string();
    }
}

fn active_uri() -> String {
    #[cfg(test)]
    {
        return TEST_URI.with(|u| u.borrow().clone());
    }
    #[allow(unreachable_code)]
    ACTIVE_URI.read().map(|g| g.clone()).unwrap_or_default()
}

/// `--read-only`: force read-only for every connection of this run.
pub fn set_read_only_flag(on: bool) {
    #[cfg(test)]
    TEST_RO_FLAG.with(|f| f.set(on));
    READ_ONLY_FLAG.store(on, std::sync::atomic::Ordering::SeqCst);
}

/// Whether the active connection is read-only (`--read-only`,
/// `general.read_only` or the connection's profile).
pub fn read_only() -> bool {
    #[cfg(test)]
    {
        return TEST_RO_FLAG.with(|f| f.get())
            || TEST_RUNTIME.with(|r| r.borrow().as_ref().is_some_and(|c| c.general.read_only));
    }
    #[allow(unreachable_code)]
    {
        READ_ONLY_FLAG.load(std::sync::atomic::Ordering::SeqCst)
            || READ_ONLY.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Why the connection is read-only, for messages (`None`: it is not).
pub fn read_only_reason() -> Option<&'static str> {
    #[cfg(test)]
    let flag = TEST_RO_FLAG.with(|f| f.get());
    #[cfg(not(test))]
    let flag = READ_ONLY_FLAG.load(std::sync::atomic::Ordering::SeqCst);
    if flag {
        Some("--read-only")
    } else if read_only() {
        Some("read_only in config.toml")
    } else {
        None
    }
}

/// Label and colour of the active connection's profile, if it has a label.
pub fn badge() -> Option<(String, Option<String>)> {
    #[cfg(test)]
    {
        let uri = active_uri();
        return TEST_RUNTIME.with(|r| {
            r.borrow().as_ref().and_then(|c| {
                let p = c.connections.profiles.get(&uri)?;
                Some((p.label.clone()?, p.color.clone()))
            })
        });
    }
    #[allow(unreachable_code)]
    BADGE.read().ok().and_then(|g| g.clone())
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
    let c = &c.effective_for(&active_uri());
    #[cfg(test)]
    {
        TEST_RUNTIME.with(|r| *r.borrow_mut() = Some(c.clone()));
        return;
    }
    #[allow(unreachable_code)]
    {
        READ_ONLY.store(c.general.read_only, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut b) = BADGE.write() {
            *b = c
                .connections
                .profiles
                .get(&active_uri())
                .and_then(|p| Some((p.label.clone()?, p.color.clone())));
        }
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
    /// This config with the profile of `uri` applied (what the app runs
    /// with; the file keeps the global values).
    pub fn effective_for(&self, uri: &str) -> Self {
        let mut c = self.clone();
        let Some(p) = self.connections.profiles.get(uri) else {
            return c;
        };
        if let Some(v) = p.read_only {
            c.general.read_only = v;
        }
        if let Some(v) = p.refresh_interval_secs {
            c.general.refresh_interval_secs = v.clamp(1, 60);
        }
        if let Some(v) = p.thread_sampling {
            c.monitoring.thread_sampling = v;
        }
        if let Some(v) = &p.viewer {
            c.console.viewer.clone_from(v);
        }
        c
    }

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
    fn profiles_override_the_active_connection_only() {
        let text = r#"
[general]
refresh_interval_secs = 1

[connections.profiles."qemu+ssh://admin@hv01/system"]
label = "prod"
color = "red"
read_only = true
refresh_interval_secs = 5
viewer = "remote-viewer"
"#;
        let cfg: Config = toml::from_str(text).unwrap();
        let remote = "qemu+ssh://admin@hv01/system";
        let eff = cfg.effective_for(remote);
        assert!(eff.general.read_only);
        assert_eq!(eff.general.refresh_interval_secs, 5);
        assert_eq!(eff.console.viewer, "remote-viewer");
        assert_eq!(cfg.effective_for("qemu:///system"), cfg, "other URIs: unchanged");

        super::set_active_uri(remote);
        super::set_runtime(&cfg);
        assert!(super::read_only());
        assert_eq!(super::read_only_reason(), Some("read_only in config.toml"));
        assert_eq!(super::refresh_interval_secs(), 5);
        assert_eq!(
            super::badge(),
            Some((String::from("prod"), Some(String::from("red"))))
        );
        super::set_active_uri("qemu:///system");
        super::set_runtime(&cfg);
        assert!(!super::read_only());
        assert_eq!(super::badge(), None);
        super::set_read_only_flag(true);
        assert_eq!(super::read_only_reason(), Some("--read-only"));
        super::set_read_only_flag(false);
        super::set_active_uri("");
        super::set_runtime(&Config::default());

        // Saving keeps the profile and writes no empty fields.
        let saved = toml::to_string_pretty(&cfg).unwrap();
        assert!(
            saved.contains("[connections.profiles.\"qemu+ssh://admin@hv01/system\"]"),
            "{saved}"
        );
        let profile = saved.split("[connections.profiles.").nth(1).unwrap();
        let profile = profile.split("\n[").next().unwrap();
        assert!(
            !profile.contains("thread_sampling"),
            "unset fields are omitted: {profile}"
        );
        assert_eq!(
            toml::from_str::<Config>(&saved).unwrap().connections,
            cfg.connections
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
