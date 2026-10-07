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
    /// File this config was loaded from (`--config` or the default path);
    /// `save` writes back to it.
    #[serde(skip)]
    pub source: Option<std::path::PathBuf>,
}

/// General options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct General {
    pub default_uri: String,
    pub refresh_interval_secs: u64,
    pub editor: String,
    pub start_view: String,
    pub confirm_style: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            default_uri: String::from("qemu:///system"),
            refresh_interval_secs: 1,
            editor: String::from("vi"),
            start_view: String::from("domains"),
            confirm_style: String::from("yn"),
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
            uris: vec![
                String::from("qemu:///system"),
                String::from("qemu:///session"),
                String::from("qemu+ssh://user@host/system"),
            ],
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
}

impl Default for Monitoring {
    fn default() -> Self {
        Self {
            balloon_period_secs: 2,
            // Opt-in: enabling it changes running guests (dommemstat --period --live).
            balloon_on: false,
            thread_sampling: true,
        }
    }
}

static RUNTIME: std::sync::RwLock<Option<Config>> = std::sync::RwLock::new(None);

/// Publish the active config (the one loaded from `--config` or the default
/// path, plus Settings changes) for code that has no handle to it.
pub fn set_runtime(c: &Config) {
    if let Ok(mut g) = RUNTIME.write() {
        *g = Some(c.clone());
    }
}

/// The active config (defaults until published).
pub fn runtime() -> Config {
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
                    tracing::warn!("ignoring invalid config {}: {e}", path.display());
                    Self::default()
                }
            },
        };
        cfg.source = Some(path);
        cfg
    }

    /// Save to disk.
    pub fn save(&self) -> Result<()> {
        let Some(path) = self.source.clone().or_else(Self::path) else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
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
}
