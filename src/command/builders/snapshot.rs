//! Snapshot builders: create-as (all options), revert (safety), delete, edit.

use super::super::plan::CommandPlan;

/// `snapshot-create-as d --name s [--description d] [--atomic] [--quiesce] …`.
pub fn create(domain: &str, name: &str, description: &str, atomic: bool, quiesce: bool) -> CommandPlan {
    let mut argv = vec!["snapshot-create-as", domain, "--name", name];
    if !description.is_empty() {
        argv.extend(["--description", description]);
    }
    if atomic {
        argv.push("--atomic");
    }
    if quiesce {
        argv.push("--quiesce");
    }
    CommandPlan::single("virsh", argv, &format!("Created snapshot {name} on {domain}"))
}

/// Revert with auto-safety snapshot: `[create-as auto-… --atomic] + [revert …]`.
///
/// `after`: `--running` / `--paused` / none (as saved).
pub fn revert(domain: &str, snap: &str, after: Option<&str>, force: bool, safety: bool) -> CommandPlan {
    let mut steps = Vec::new();
    if safety {
        let stamp = jiff::Zoned::now().strftime("%H%M").to_string();
        steps.push(
            create(domain, &format!("auto-before-revert-{stamp}"), "", true, false)
                .steps
                .remove(0),
        );
    }
    let mut argv = vec!["snapshot-revert", domain, snap];
    // Accept both `running` and `--running` (AfterRevert::flag yields the latter).
    match after.map(|a| a.trim_start_matches("--")) {
        Some("running") => argv.push("--running"),
        Some("paused") => argv.push("--paused"),
        _ => {}
    }
    if force {
        argv.push("--force");
    }
    steps.push(crate::command::plan::CommandStep {
        program: String::from("virsh"),
        argv: argv.into_iter().map(|s| s.to_string()).collect(),
        stdin: None,
    });
    CommandPlan {
        steps,
        summary: format!("Reverted {domain} to {snap}"),
    }
}

/// `snapshot-delete d s [--children|--children-only] [--metadata]`.
pub fn delete(domain: &str, snap: &str, children: bool, children_only: bool, metadata: bool) -> CommandPlan {
    let mut argv = vec!["snapshot-delete", domain, snap];
    if children_only {
        argv.push("--children-only");
    } else if children {
        argv.push("--children");
    }
    if metadata {
        argv.push("--metadata");
    }
    CommandPlan::single("virsh", argv, &format!("Deleted snapshot {snap} on {domain}"))
}

#[cfg(test)]
mod revert_flag_tests {
    #[test]
    fn after_flag_accepts_both_spellings() {
        for after in ["running", "--running"] {
            let plan = super::revert("d", "s", Some(after), false, false);
            assert_eq!(plan.steps[0].argv, vec!["snapshot-revert", "d", "s", "--running"]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{create, delete, revert};

    #[test]
    fn exact() {
        assert!(
            create("d", "s", "", true, false).steps[0]
                .argv
                .contains(&"--atomic".to_string())
        );
        let r = revert("d", "s", Some("running"), false, true);
        assert_eq!(r.steps.len(), 2);
        assert!(r.steps[1].argv.contains(&"--running".to_string()));
        assert!(
            delete("d", "s", true, false, false).steps[0]
                .argv
                .contains(&"--children".to_string())
        );
    }
}
