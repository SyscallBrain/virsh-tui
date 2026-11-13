//! Plan execution: sequential steps, dry-run, timeouts, message formatting.

use color_eyre::{Result, eyre::eyre};

use super::display::display;
use super::plan::CommandPlan;

static DEMO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enable demo mode process-wide: plans are simulated and never spawn processes.
///
/// This is the single safety net that keeps `--demo` from touching real
/// libvirt objects that happen to share a demo domain name.
pub fn set_demo(on: bool) {
    DEMO.store(on, std::sync::atomic::Ordering::SeqCst);
}

/// True in `--demo` mode.
pub fn is_demo() -> bool {
    DEMO.load(std::sync::atomic::Ordering::SeqCst)
}

/// Execute a plan against `uri`. Returns the message-line text.
///
/// Success: `✓ <summary>  ── <first command>`.
/// Partial failure: `✗ step i/n failed: <stderr summary>` (see [`super::job::stderr_summary`]).
/// Dry-run: `[dry-run] <command>` per step, returns success.
pub async fn execute(uri: &str, plan: &CommandPlan, dry_run: bool) -> Result<String> {
    if is_demo() {
        let first = plan.steps.first().map(display).unwrap_or_default();
        return Ok(format!("✓ {}  ── {first}", plan.summary));
    }
    if dry_run {
        for step in &plan.steps {
            let line = format!("[dry-run] {}", display(step));
            tracing::info!("{line}");
        }
        let first = plan.steps.first().map(display).unwrap_or_default();
        return Ok(format!("[dry-run] {}  ── {first}", plan.summary));
    }
    let n = plan.steps.len();
    let (_cancel, mut control) = tokio::sync::watch::channel(false);
    for (i, step) in plan.steps.iter().enumerate() {
        let output =
            super::process::run_step(uri, step, &mut control)
                .await
                .map_err(|error| match error {
                    super::process::RunError::Timeout => eyre!("step {}/{} timed out", i + 1, n),
                    other => eyre!(other),
                })?;
        if !output.status.success() {
            let cause = super::job::stderr_summary(&String::from_utf8_lossy(&output.stderr));
            return Err(eyre!("✗ step {}/{} failed: {cause}", i + 1, n));
        }
    }
    let first = plan.steps.first().map(display).unwrap_or_default();
    Ok(format!("✓ {}  ── {first}", plan.summary))
}
