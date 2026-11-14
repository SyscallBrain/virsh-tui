//! Domain tags, kept in the domain's own XML as libvirt metadata
//! (`<vt:tags xmlns:vt="…">dev web</vt:tags>`), so they travel with it.

use super::*;

/// Namespace of the metadata element.
pub(crate) const NS: &str = "https://syscallbrain.github.io/virsh-tui/xmlns/tags/1";

/// Tags: letters, digits, `-`, `_`, `.`; 1–32 characters.
pub(super) fn valid_tag(tag: &str) -> bool {
    (1..=32).contains(&tag.len())
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

/// `<tags>dev web</tags>` → `["dev", "web"]` (empty output: no tags).
pub(super) fn parse(xml: &str) -> Vec<String> {
    let Some(start) = xml.find('>') else {
        return Vec::new();
    };
    let end = xml.rfind("</").unwrap_or(xml.len());
    if end <= start {
        return Vec::new();
    }
    xml[start + 1..end]
        .split_whitespace()
        .filter(|t| valid_tag(t))
        .map(String::from)
        .collect()
}

/// New tag list from `:tag` words: `+a -b` edit the current list, plain words
/// replace it, `--clear` empties it. `Err`: the message.
pub(super) fn edit(current: &[String], words: &[String]) -> Result<Vec<String>, String> {
    if words.iter().any(|w| w == "--clear") {
        return if words.len() == 1 {
            Ok(Vec::new())
        } else {
            Err(String::from("--clear takes no tags"))
        };
    }
    let relative = words.iter().all(|w| w.starts_with('+') || w.starts_with('-'));
    let mut tags: Vec<String> = if relative { current.to_vec() } else { Vec::new() };
    for word in words {
        let (op, tag) = match word.chars().next() {
            Some(c @ ('+' | '-')) => (c, &word[1..]),
            _ => ('+', word.as_str()),
        };
        if !valid_tag(tag) {
            return Err(format!("bad tag `{tag}`: use letters, digits, - _ . (up to 32)"));
        }
        if op == '-' {
            tags.retain(|t| t != tag);
        } else if !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_string());
        }
    }
    Ok(tags)
}

/// The commands that store `tags` on `domain` (`--live` too when it runs).
pub(super) fn plan(domain: &str, tags: &[String], active: bool) -> crate::command::plan::CommandPlan {
    let mut argv: Vec<String> = ["metadata", domain, "--uri", NS].map(String::from).to_vec();
    if tags.is_empty() {
        argv.push(String::from("--remove"));
    } else {
        argv.extend([
            String::from("--key"),
            String::from("vt"),
            String::from("--set"),
            format!("<tags>{}</tags>", tags.join(" ")),
        ]);
    }
    argv.push(String::from("--config"));
    if active {
        argv.push(String::from("--live"));
    }
    crate::command::plan::CommandPlan {
        summary: if tags.is_empty() {
            format!("Removed the tags of {domain}")
        } else {
            format!("Tagged {domain}: {}", tags.join(" "))
        },
        steps: vec![crate::command::plan::CommandStep {
            program: String::from("virsh"),
            argv,
            stdin: None,
        }],
    }
}

/// Read the tags of every domain in `names` (one `virsh metadata` each).
async fn fetch(uri: String, names: Vec<String>) -> std::collections::HashMap<String, Vec<String>> {
    let mut out = std::collections::HashMap::new();
    for name in names {
        if let Ok(xml) = crate::backend::virsh::exec::run(&uri, &["metadata", &name, "--uri", NS]).await {
            let tags = parse(&xml);
            if !tags.is_empty() {
                out.insert(name, tags);
            }
        }
    }
    out
}

/// A tags read in flight: (uri, epoch, result).
pub(super) type TagsRx =
    tokio::sync::oneshot::Receiver<(String, u64, std::collections::HashMap<String, Vec<String>>)>;

impl App {
    /// Once a second: take a finished read; start one when the domain names
    /// changed since the last read.
    pub(super) fn poll_tags(&mut self) {
        if self.demo {
            return;
        }
        if let Some(rx) = self.tags_rx.as_mut() {
            match rx.try_recv() {
                Ok((uri, epoch, tags)) => {
                    self.tags_rx = None;
                    if uri == self.uri && epoch == self.connection_epoch {
                        self.dash.tags = tags;
                    }
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => self.tags_rx = None,
            }
        }
        let mut names: Vec<String> = self.dash.rows.iter().map(|r| r.name.clone()).collect();
        names.sort();
        if names.is_empty() || names == self.tags_names {
            return;
        }
        self.tags_names.clone_from(&names);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (uri, epoch) = (self.uri.clone(), self.connection_epoch);
        tokio::spawn(async move {
            let tags = fetch(uri.clone(), names).await;
            let _ = tx.send((uri, epoch, tags));
        });
        self.tags_rx = Some(rx);
    }

    /// `:tag <domain> [+a -b | a b | --clear]`.
    pub(super) fn run_tag(&mut self, domain: String, words: Vec<String>) {
        if !self.dash.rows.iter().any(|r| r.name == domain) {
            self.show_message(format!("✗ no domain named {domain}"));
            return;
        }
        let current = self.dash.tags.get(&domain).cloned().unwrap_or_default();
        if words.is_empty() {
            self.show_message(if current.is_empty() {
                format!("{domain} has no tags  ·  :tag {domain} +name adds one")
            } else {
                format!("{domain}: #{}", current.join(" #"))
            });
            return;
        }
        let tags = match edit(&current, &words) {
            Ok(tags) => tags,
            Err(e) => {
                self.show_message(format!("✗ {e}"));
                return;
            }
        };
        if tags == current {
            self.show_message(format!("{domain}: tags unchanged"));
            return;
        }
        let plan = plan(&domain, &tags, self.domain_is_active(&domain));
        if self.demo || self.dry_run {
            let shown = plan
                .steps
                .first()
                .map(crate::command::display::display)
                .unwrap_or_default();
            if self.demo {
                self.dash.tags.insert(domain, tags);
            }
            let prefix = if self.demo { "✓" } else { "[dry-run]" };
            self.show_message(format!("{prefix} {}  ── {shown}", plan.summary));
            return;
        }
        if self.submit_plans(vec![(domain.clone(), plan)])
            && let Some(job) = self.active_job.as_mut()
        {
            job.tags_after = Some((domain, tags));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn edits_add_remove_replace_and_clear() {
        let cur = s(&["dev", "web"]);
        assert_eq!(edit(&cur, &s(&["+prod", "-dev"])).unwrap(), s(&["web", "prod"]));
        assert_eq!(edit(&cur, &s(&["db"])).unwrap(), s(&["db"]));
        assert_eq!(edit(&cur, &s(&["--clear"])).unwrap(), Vec::<String>::new());
        assert!(edit(&cur, &s(&["+no spaces"])).is_err());
        assert!(edit(&cur, &s(&["+<x>"])).is_err());
    }

    #[test]
    fn metadata_round_trip() {
        assert_eq!(parse("<tags>dev web</tags>\n"), s(&["dev", "web"]));
        assert_eq!(parse(""), Vec::<String>::new());
        let p = plan("vm", &s(&["dev", "web"]), true);
        assert_eq!(
            p.steps[0].argv,
            s(&[
                "metadata",
                "vm",
                "--uri",
                NS,
                "--key",
                "vt",
                "--set",
                "<tags>dev web</tags>",
                "--config",
                "--live"
            ])
        );
        let p = plan("vm", &[], false);
        assert_eq!(
            p.steps[0].argv,
            s(&["metadata", "vm", "--uri", NS, "--remove", "--config"])
        );
    }
}
