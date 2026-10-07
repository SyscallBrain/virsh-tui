//! CommandPlan: steps shown to the user before anything runs, then executed as-is.

/// One process step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandStep {
    /// Program: `virsh`, `virt-install`, `virt-clone`, or `$EDITOR`.
    pub program: String,
    /// Argv without the program (displayed and executed).
    pub argv: Vec<String>,
    /// Stdin text (e.g. XML via temp file path in argv instead; reserved).
    pub stdin: Option<String>,
}

impl CommandStep {
    /// Create a step.
    pub fn new(program: &str, argv: Vec<&str>) -> Self {
        Self {
            program: program.to_string(),
            argv: argv.into_iter().map(|s| s.to_string()).collect(),
            stdin: None,
        }
    }
}

/// A plan: sequential steps stopping at the first failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPlan {
    pub steps: Vec<CommandStep>,
    /// Past-tense summary for the message line (`Started k8s-worker-02`).
    pub summary: String,
}

/// Shareable single-target plan builder (bulk runs it once per target).
pub type PlanBuilder = std::sync::Arc<dyn Fn(&str) -> CommandPlan + Send + Sync>;

/// A staged confirmation: modal + per-target builder + targets.
pub type PendingConfirm = (crate::ui::overlays::confirm::Confirm, PlanBuilder, Vec<String>);

impl CommandPlan {
    /// Single-step plan.
    pub fn single(program: &str, argv: Vec<&str>, summary: &str) -> Self {
        Self {
            steps: vec![CommandStep::new(program, argv)],
            summary: summary.to_string(),
        }
    }
}
