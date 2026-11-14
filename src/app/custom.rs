//! `[[actions]]`: user-defined commands run from a key, the palette or
//! `:run <name> [domain]`.

use super::*;

/// Values for the placeholders of one run.
#[derive(Debug, Clone, Default)]
pub(super) struct Placeholders {
    pub name: Option<String>,
    pub uuid: Option<String>,
    pub ip: Option<String>,
    pub uri: String,
    pub host: String,
}

/// Placeholders a command may use.
const KEYS: [&str; 5] = ["{name}", "{uuid}", "{ip}", "{uri}", "{host}"];

/// Split `command` like a shell and replace the placeholders in each word, so
/// a value never becomes more than the word it was in. `Err`: the message.
pub(super) fn expand(command: &str, values: &Placeholders) -> Result<Vec<String>, String> {
    let words = shlex::split(command).ok_or_else(|| String::from("unbalanced quotes in the command"))?;
    if words.is_empty() {
        return Err(String::from("empty command"));
    }
    let mut out = Vec::with_capacity(words.len());
    for word in words {
        let mut w = word.clone();
        for key in KEYS {
            if !w.contains(key) {
                continue;
            }
            let value = match key {
                "{name}" => values.name.as_deref(),
                "{uuid}" => values.uuid.as_deref(),
                "{ip}" => values.ip.as_deref(),
                "{uri}" => Some(values.uri.as_str()),
                _ => Some(values.host.as_str()),
            };
            let Some(value) = value.filter(|v| !v.is_empty()) else {
                return Err(match (key, values.name.as_deref()) {
                    ("{name}", _) => String::from("{name} needs a selected domain"),
                    ("{ip}", Some(d)) => format!("{{ip}} unknown for {d} (needs a DHCP lease or qemu-ga)"),
                    (_, Some(d)) => format!("{key} of {d} not loaded yet"),
                    _ => format!("{key} needs a selected domain"),
                });
            };
            w = w.replace(key, value);
        }
        out.push(w);
    }
    Ok(out)
}

/// Whether a command uses a placeholder that needs a domain.
fn needs_domain(command: &str) -> bool {
    ["{name}", "{uuid}", "{ip}"].iter().any(|k| command.contains(k))
}

/// Key bindings of the configured actions (entries without a valid key are
/// skipped; the warnings say why).
pub(super) fn bindings(
    actions: &[crate::config::CustomAction],
) -> (Vec<(Vec<String>, crate::input::engine::KeyAction)>, Vec<String>) {
    let mut binds = Vec::new();
    let mut warnings = Vec::new();
    for (i, action) in actions.iter().enumerate().take(usize::from(u8::MAX) + 1) {
        let Some(key) = action.key.as_deref().filter(|k| !k.trim().is_empty()) else {
            continue;
        };
        let tokens: Vec<crate::input::key::KeySeq> = key
            .split_whitespace()
            .map(crate::input::key::KeySeq::parse)
            .collect();
        if let Some(bad) = tokens.iter().find(|t| !t.is_valid()) {
            warnings.push(format!("action `{}`: unknown key `{}`", action.name, bad.token()));
            continue;
        }
        let seq: Vec<String> = tokens.iter().map(|t| t.token().to_string()).collect();
        if matches!(seq.first().map(String::as_str), Some("q" | "." | "C-c" | ":")) {
            warnings.push(format!(
                "action `{}`: `{}` cannot start a binding",
                action.name, seq[0]
            ));
            continue;
        }
        binds.push((seq, crate::input::engine::KeyAction::Custom(i as u8)));
    }
    (binds, warnings)
}

impl App {
    /// Bind the configured actions' keys; warnings go to `:messages`.
    pub(super) fn bind_custom_actions(&mut self) {
        let (binds, mut warnings) = bindings(&self.config.actions);
        warnings.extend(self.engine.set_custom(binds));
        for w in warnings {
            self.messages.push(format!("⚠ config.toml: {w}"));
        }
    }

    /// `:run <name> [domain]`.
    pub(super) fn run_custom_by_name(&mut self, name: &str, domain: Option<String>) {
        match self.config.actions.iter().position(|a| a.name == name) {
            Some(i) => self.run_custom_action(i, domain),
            None if self.config.actions.is_empty() => {
                self.show_message(String::from("✗ no actions: add [[actions]] to config.toml"));
            }
            None => self.show_message(format!("✗ no action named {name}")),
        }
    }

    /// The domain an action applies to: the open detail, else the cursor row.
    fn custom_target(&self) -> Option<String> {
        if let Some(d) = self.current_detail() {
            return Some(d.domain.clone());
        }
        (self.view == View::Dashboard)
            .then(|| self.dash.selected().name.clone())
            .filter(|n| !n.is_empty())
    }

    pub(super) fn run_custom_action(&mut self, index: usize, domain: Option<String>) {
        let Some(action) = self.config.actions.get(index).cloned() else {
            return;
        };
        let name = if needs_domain(&action.command) {
            domain.or_else(|| self.custom_target())
        } else {
            domain
        };
        let values = Placeholders {
            uuid: name.as_ref().and_then(|n| {
                self.dash.configs.get(n).map(|c| c.uuid.clone()).or_else(|| {
                    self.demo
                        .then(|| String::from("6f1d2c3b-4a5e-4f60-9b7a-0d1e2f3a4b5c"))
                })
            }),
            ip: name.as_ref().and_then(|n| {
                if self.demo {
                    Some(String::from("192.168.122.48"))
                } else {
                    self.dash.ips.get(n).cloned().filter(|ip| ip != "—")
                }
            }),
            name,
            uri: self.uri.clone(),
            host: connection_hostname(&self.uri),
        };
        let mut argv = match expand(&action.command, &values) {
            Ok(argv) => argv,
            Err(e) => {
                self.show_message(format!("✗ {}: {e}", action.name));
                return;
            }
        };
        let program = argv.remove(0);
        let step = crate::command::plan::CommandStep {
            program,
            argv,
            stdin: None,
        };
        let shown = crate::command::display::display(&step);
        if self.demo || self.dry_run {
            self.show_message(format!("[dry-run] {}  ── {shown}", action.name));
            return;
        }
        if crate::config::read_only() && crate::command::readonly::step_writes(&step) {
            self.refuse_if_read_only(&shown);
            return;
        }
        if self.refuse_if_busy() {
            return;
        }
        let target = values_target(&step);
        if action.mode == "background" {
            let plan = crate::command::plan::CommandPlan {
                summary: format!("{} done", action.name),
                steps: vec![step],
            };
            if action.confirm {
                self.confirm_plans(&format!("Run {}", action.name), "run", vec![(target, plan)]);
                return;
            }
            if self.submit_plans(vec![(target, plan)])
                && let Some(job) = self.active_job.as_mut()
            {
                job.raw = true;
            }
            return;
        }
        // Terminal: virsh gets the current connection like the jobs do.
        let mut argv: Vec<String> = vec![step.program.clone()];
        if step.program == "virsh" && !step.argv.iter().any(|a| a == "-c" || a.starts_with("--connect")) {
            if crate::config::read_only() {
                argv.push(String::from("-r"));
            }
            argv.extend([String::from("-c"), self.uri.clone()]);
        }
        argv.extend(step.argv.iter().cloned());
        self.history.push(&shown);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        let message =
            external::suspend_run(&refs).unwrap_or_else(|| format!("✓ {}  ── {shown}", action.name));
        self.messages.push(message.clone());
        self.show_message(message);
        self.request_refresh();
    }
}

/// Target recorded for the job: the first argument that is not a flag.
fn values_target(step: &crate::command::plan::CommandStep) -> String {
    step.argv
        .iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| step.program.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Placeholders {
        Placeholders {
            name: Some(String::from("web 1")),
            uuid: Some(String::from("u-1")),
            ip: None,
            uri: String::from("qemu:///system"),
            host: String::from("forge"),
        }
    }

    #[test]
    fn placeholders_stay_inside_their_word() {
        let argv = expand("sh -c 'echo {name}' x{uuid}y --uri={uri} {host}", &values()).unwrap();
        assert_eq!(
            argv,
            ["sh", "-c", "echo web 1", "xu-1y", "--uri=qemu:///system", "forge"]
        );
        // A name with shell syntax is one argument, never parsed again.
        let mut v = values();
        v.name = Some(String::from("a; rm -rf ~"));
        assert_eq!(expand("echo {name}", &v).unwrap(), ["echo", "a; rm -rf ~"]);
    }

    #[test]
    fn missing_values_are_reported() {
        assert_eq!(
            expand("ssh root@{ip}", &values()).unwrap_err(),
            "{ip} unknown for web 1 (needs a DHCP lease or qemu-ga)"
        );
        let mut v = values();
        v.name = None;
        assert_eq!(
            expand("echo {name}", &v).unwrap_err(),
            "{name} needs a selected domain"
        );
        assert!(expand("", &v).is_err());
        assert!(expand("echo 'open", &v).is_err());
    }

    #[test]
    fn bindings_skip_bad_keys() {
        let action = |name: &str, key: &str| crate::config::CustomAction {
            name: name.into(),
            command: "true".into(),
            key: Some(key.into()),
            ..Default::default()
        };
        let (binds, warnings) =
            bindings(&[action("a", "Space u s"), action("b", "q x"), action("c", "Nope!")]);
        assert_eq!(binds.len(), 1);
        assert_eq!(binds[0].0, ["Space", "u", "s"]);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }
}
