//! Doctor: read-only environment checks with hints and fixes
//! (`virsh-tui --doctor`, Settings › Doctor).

pub mod hints;
mod report;

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
