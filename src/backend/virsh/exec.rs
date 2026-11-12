//! Process spawning: LC_ALL=C, explicit URI, timeouts, stderr capture.

use color_eyre::{Result, eyre::eyre};

/// How long a virsh subcommand may run. Long jobs (migration, cloning, wiping,
/// snapshots with memory, uploads) get hours; everything else 30 s.
pub fn timeout_for(program: &str, sub: &str) -> std::time::Duration {
    const LONG: &[&str] = &[
        "migrate",
        "vol-wipe",
        "vol-upload",
        "vol-download",
        "vol-clone",
        "vol-create-from",
        "blockcopy",
        "blockcommit",
        "blockpull",
        "snapshot-create-as",
        "snapshot-create",
        "snapshot-revert",
        "snapshot-delete",
        "managedsave",
        "save",
        "restore",
        "dump",
        "pool-build",
        "pool-delete",
    ];
    if program != "virsh" || LONG.contains(&sub) {
        std::time::Duration::from_secs(24 * 3600)
    } else {
        std::time::Duration::from_secs(30)
    }
}

/// Run `virsh -c <uri> <args>` and return stdout.
pub async fn run(uri: &str, args: &[&str]) -> Result<String> {
    if crate::command::exec::is_demo() {
        return Err(eyre!("demo mode: virsh is not executed"));
    }
    structured_result(run_output(uri, args).await?)
}

/// Run an arbitrary user command, preserving a notice if stdout is truncated.
pub async fn run_raw(uri: &str, args: &[&str]) -> Result<String> {
    raw_result(run_output(uri, args).await?)
}

fn raw_result(output: crate::command::process::StepOutput) -> Result<String> {
    if !output.status.success() {
        return Err(eyre!("virsh failed: {}", friendly_first_line(&output.stderr)));
    }
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.stdout_truncated {
        text.push_str("\n[output truncated]");
    }
    Ok(text)
}

async fn run_output(uri: &str, args: &[&str]) -> Result<crate::command::process::StepOutput> {
    if crate::command::exec::is_demo() {
        return Err(eyre!("demo mode: virsh is not executed"));
    }
    let step = crate::command::plan::CommandStep::new("virsh", args.to_vec());
    let (_cancel, mut control) = tokio::sync::watch::channel(false);
    crate::command::process::run_step(uri, &step, &mut control)
        .await
        .map_err(|error| match error {
            crate::command::process::RunError::Timeout => eyre!("virsh timed out"),
            crate::command::process::RunError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eyre!("virsh not found in PATH")
            }
            other => eyre!("failed to run virsh: {other}"),
        })
}

fn structured_result(output: crate::command::process::StepOutput) -> Result<String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(output.structured_stdout()?).into_owned())
    } else {
        Err(eyre!("virsh failed: {}", friendly_first_line(&output.stderr)))
    }
}

/// First stderr line plus actionable hints (permissions, daemon down).
fn friendly_first_line(stderr: &[u8]) -> String {
    let first = String::from_utf8_lossy(stderr)
        .lines()
        .next()
        .unwrap_or("unknown error")
        .to_string();
    let lower = first.to_lowercase();
    if lower.contains("permission denied") || lower.contains("not authorized") {
        format!("{first} (not in the libvirt group?)")
    } else if lower.contains("failed to connect") || lower.contains("no connection") {
        format!("{first} (is libvirtd running?)")
    } else {
        first
    }
}

/// Blocking variant for startup/cache paths (spawns `virsh` synchronously).
pub fn run_blocking(uri: &str, args: &[&str]) -> Result<String> {
    if crate::command::exec::is_demo() {
        return Err(eyre!("demo mode: virsh is not executed"));
    }
    let output = std::process::Command::new("virsh")
        .arg("-c")
        .arg(uri)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(eyre!("virsh failed"))
    }
}

#[cfg(test)]
mod supervised_tests {
    #[tokio::test]
    async fn raw_result_marks_truncated_output() {
        let step = crate::command::plan::CommandStep::new("python3", vec!["-c", "print('x' * 1100000)"]);
        let (_tx, mut rx) = tokio::sync::watch::channel(false);
        let output = crate::command::process::run_step("test:///default", &step, &mut rx)
            .await
            .unwrap();
        let result = super::raw_result(output).unwrap();
        assert!(result.ends_with("\n[output truncated]"));
    }

    #[tokio::test]
    async fn structured_result_rejects_truncated_output() {
        let step = crate::command::plan::CommandStep::new("python3", vec!["-c", "print('x' * 1100000)"]);
        let (_tx, mut rx) = tokio::sync::watch::channel(false);
        let output = crate::command::process::run_step("test:///default", &step, &mut rx)
            .await
            .unwrap();
        let result = super::structured_result(output);
        assert!(result.unwrap_err().to_string().contains("exceeds the limit"));
    }
}
