//! Doctor: read-only environment checks with hints and fixes
//! (`virsh-tui --doctor`, Settings › Doctor).

mod checks;
pub mod fixes;
pub mod hints;
mod report;
pub mod runner;

use std::path::PathBuf;

pub use report::demo_report;

/// Outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Info,
    Skip,
}

impl Status {
    /// Glyph shown before the label.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Ok => "✓",
            Self::Warn => "⚠",
            Self::Fail => "✗",
            Self::Info => "·",
            Self::Skip => "–",
        }
    }
}

/// Group a check belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Connection,
    Tools,
    Files,
    Terminal,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Self::Connection => "Connection",
            Self::Tools => "Tools",
            Self::Files => "Files",
            Self::Terminal => "Terminal",
        }
    }
}

/// Something virsh-tui can do about a problem without root.
#[derive(Debug, Clone, PartialEq)]
pub enum Fix {
    /// A `:set` key and value (validated and saved like `:set`).
    Set { key: &'static str, value: String },
    /// A virsh plan run through the usual confirmation and job.
    Virsh(crate::command::plan::CommandPlan),
    /// Open a file in the editor.
    Edit(std::path::PathBuf),
    /// Switch to another connection.
    Connect(String),
}

impl Fix {
    /// One-line description (`fix: …`).
    pub fn describe(&self) -> String {
        match self {
            Self::Set { key, value } => format!("set {key}={value}"),
            Self::Virsh(plan) => crate::command::display::display_plan(&plan.steps),
            Self::Edit(path) => format!("open {} in the editor", path.display()),
            Self::Connect(uri) => format!("connect to {uri}"),
        }
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub section: Section,
    pub label: &'static str,
    pub status: Status,
    pub value: String,
    /// What to do about it (warnings and failures).
    pub hint: Option<String>,
    /// Applied in-app with `f`.
    pub fix: Option<Fix>,
    /// Root command the user can copy (`y`); never executed.
    pub command: Option<String>,
}

impl Check {
    pub fn new(section: Section, label: &'static str, status: Status, value: impl Into<String>) -> Self {
        Self {
            section,
            label,
            status,
            value: value.into(),
            hint: None,
            fix: None,
            command: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    pub fn command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    pub fn command_opt(mut self, command: Option<String>) -> Self {
        self.command = command.or(self.command);
        self
    }

    pub fn fix_opt(mut self, fix: Option<Fix>) -> Self {
        self.fix = fix.or(self.fix);
        self
    }
}

/// Where the URI came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UriSource {
    Flag,
    Config,
}

impl UriSource {
    pub fn text(self) -> &'static str {
        match self {
            Self::Flag => "from -c",
            Self::Config => "config default_uri",
        }
    }
}

/// Report header (also what a bug report needs).
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub version: String,
    pub os: String,
    pub uri: String,
    pub uri_source: UriSource,
}

/// Every check, in display order.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub header: Header,
    pub checks: Vec<Check>,
}

/// Summary counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub ok: usize,
    pub warn: usize,
    pub fail: usize,
}

/// `net-start <net>` then `net-autostart <net>` as one plan.
pub fn start_network_plan(net: &str) -> crate::command::plan::CommandPlan {
    use crate::command::builders::network;
    let mut plan = network::start(net);
    plan.steps.extend(network::autostart(net, true).steps);
    plan
}

/// What the checks need to know about this session.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub uri: String,
    pub uri_source: UriSource,
    pub config: crate::config::Config,
    pub config_path: Option<PathBuf>,
    pub keymap_path: Option<PathBuf>,
    pub themes_dir: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub term_size: Option<(u16, u16)>,
}

/// Run every check and build the report.
pub async fn run<R: runner::Runner>(ctx: &Ctx, runner: &R) -> Report {
    let os = runner
        .read(std::path::Path::new("/etc/os-release"))
        .and_then(|text| os_release_value(&text, "PRETTY_NAME"))
        .unwrap_or_else(|| std::env::consts::OS.to_string());
    Report {
        header: Header {
            version: env!("CARGO_PKG_VERSION").to_string(),
            os,
            uri: ctx.uri.clone(),
            uri_source: ctx.uri_source,
        },
        checks: checks::run_all(ctx, runner).await,
    }
}

/// `KEY=value` / `KEY="value"` from `/etc/os-release`.
pub(crate) fn os_release_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let value = line.strip_prefix(key)?.strip_prefix('=')?;
        Some(value.trim().trim_matches('"').to_string())
    })
}

/// Context for this process: `-c` or the config's default URI, the real paths.
pub fn ctx_from(
    cli: &crate::cli::Cli,
    config: &crate::config::Config,
    uri_was_flag: bool,
    term_size: Option<(u16, u16)>,
) -> Ctx {
    let dirs = directories::ProjectDirs::from("", "", "virsh-tui");
    Ctx {
        uri: cli.connect.clone(),
        uri_source: if uri_was_flag {
            UriSource::Flag
        } else {
            UriSource::Config
        },
        config: config.clone(),
        config_path: config.source.clone().or_else(crate::config::Config::path),
        keymap_path: crate::input::keymap::path(),
        themes_dir: dirs.as_ref().map(|d| d.config_dir().join("themes")),
        log_path: dirs
            .as_ref()
            .and_then(|d| d.state_dir().map(|s| s.join("virsh-tui.log"))),
        term_size,
    }
}
