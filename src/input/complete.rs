//! Tab completion for the `:` command line.
//!
//! Pure logic: given the line and what the app knows (domains, themes, …),
//! return the byte offset where the completed token starts and the candidates.
//! Commands, their arguments (domains, themes, URIs, sort keys, `:set` keys),
//! virsh flags (`--…`, from `virsh help <cmd>`) and file paths are completed.

/// One completion candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Text that replaces the token.
    pub value: String,
    /// Short dim description shown in the popup.
    pub hint: String,
}

/// What the app can complete against.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// (name, state label)
    pub domains: Vec<(String, String)>,
    pub networks: Vec<String>,
    pub pools: Vec<String>,
    pub themes: Vec<String>,
    pub uris: Vec<String>,
    /// virsh subcommands with their one-line summary.
    pub virsh: Vec<(String, String)>,
}

/// virsh-tui's own `:` commands: (name, hint).
pub const APP_COMMANDS: &[(&str, &str)] = &[
    ("start", "<domain>"),
    ("shutdown", "<domain> [--mode agent]"),
    ("destroy", "<domain>"),
    ("rename", "<domain> <new-name>"),
    ("desc", "<domain> <text>"),
    ("attach-disk", "<domain> <source> <target>"),
    ("detach-disk", "<domain> <target>"),
    ("attach-nic", "<domain> <source> <model>"),
    ("block-resize", "<domain> <target> <size>"),
    ("attach-iso", "<domain> [target] <iso>"),
    ("attach-hostdev", "<domain> <nodedev>"),
    ("connect", "<uri>"),
    ("theme", "<name>"),
    ("sort", "name | state | cpu | mem | uptime"),
    ("set", "<option>=<value>"),
    ("help", "[topic]"),
    ("messages", "message log"),
    ("w", "apply hardware changes"),
    ("q", "close / quit"),
    ("q!", "discard changes"),
    ("qa", "quit"),
    ("!virsh", "raw virsh passthrough"),
];

const SET_KEYS: &[&str] = &[
    "theme=",
    "borders=",
    "icons=",
    "graphs=",
    "gradient=",
    "transparent=",
    "dim_modals=",
    "default_uri=",
    "start_view=",
    "viewer=",
    "escape=",
    "editor=",
    "refresh_interval_secs=",
];

/// App commands whose first argument is a domain.
const DOMAIN_FIRST: &[&str] = &[
    "start",
    "shutdown",
    "destroy",
    "rename",
    "desc",
    "attach-disk",
    "detach-disk",
    "attach-nic",
    "block-resize",
    "attach-iso",
    "attach-hostdev",
];

fn item(value: impl Into<String>, hint: impl Into<String>) -> Item {
    Item {
        value: value.into(),
        hint: hint.into(),
    }
}

/// Keep candidates starting with `prefix` (case-insensitive), sorted, unique.
fn filter(prefix: &str, mut items: Vec<Item>) -> Vec<Item> {
    let p = prefix.to_lowercase();
    items.retain(|i| i.value.to_lowercase().starts_with(&p));
    items.sort_by(|a, b| a.value.cmp(&b.value));
    items.dedup_by(|a, b| a.value == b.value);
    items
}

/// Filesystem completion for `prefix` (supports `~/`); directories end in `/`.
pub fn paths(prefix: &str) -> Vec<Item> {
    let home = std::env::var("HOME").unwrap_or_default();
    let expanded = match prefix.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None => prefix.to_string(),
    };
    let (dir, stem) = match expanded.rfind('/') {
        Some(i) => (&expanded[..=i], &expanded[i + 1..]),
        None => ("./", expanded.as_str()),
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let shown_dir = &prefix[..prefix.rfind('/').map_or(0, |i| i + 1)];
    let mut out: Vec<Item> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with(stem) || (name.starts_with('.') && !stem.starts_with('.')) {
                return None;
            }
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            let suffix = if is_dir { "/" } else { "" };
            Some(item(
                format!("{shown_dir}{name}{suffix}"),
                if is_dir { "dir" } else { "" },
            ))
        })
        .collect();
    out.sort_by(|a, b| a.value.cmp(&b.value));
    out.truncate(200);
    out
}

/// Complete the token at the end of `line`.
///
/// `flags(cmd)` returns the `--options` of a virsh subcommand (from its help).
/// Returns `(start, items)`: the token to replace is `line[start..]`.
pub fn complete(
    line: &str,
    ctx: &Context,
    flags: &dyn Fn(&str) -> Vec<(String, String)>,
) -> (usize, Vec<Item>) {
    let ends_with_space = line.ends_with(char::is_whitespace);
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let current = if ends_with_space {
        ""
    } else {
        tokens.last().copied().unwrap_or("")
    };
    let start = line.len() - current.len();
    let arg_index = if ends_with_space {
        tokens.len()
    } else {
        tokens.len().saturating_sub(1)
    };

    // The command itself.
    if arg_index == 0 {
        let mut items: Vec<Item> = APP_COMMANDS.iter().map(|(n, h)| item(*n, *h)).collect();
        items.extend(ctx.virsh.iter().map(|(n, h)| item(n.clone(), h.clone())));
        return (start, filter(current, items));
    }
    let cmd = tokens[0];
    let arg = arg_index - 1; // 0 = first argument

    // virsh flags of any virsh command.
    if current.starts_with('-') {
        let raw = cmd.trim_start_matches('!').trim_start_matches("virsh");
        let sub = if raw.is_empty() {
            tokens.get(1).copied().unwrap_or("")
        } else {
            cmd
        };
        let items = flags(sub).into_iter().map(|(f, d)| item(f, d)).collect();
        return (start, filter(current, items));
    }
    // Paths anywhere a path-looking token is typed.
    if current.starts_with('/') || current.starts_with("~/") || current.starts_with("./") {
        return (start, paths(current));
    }

    let domains = || {
        ctx.domains
            .iter()
            .map(|(n, s)| item(n.clone(), s.clone()))
            .collect::<Vec<_>>()
    };
    let items: Vec<Item> = match (cmd, arg) {
        ("theme", 0) => ctx.themes.iter().map(|t| item(t.clone(), "")).collect(),
        ("connect", 0) => ctx.uris.iter().map(|u| item(u.clone(), "")).collect(),
        ("sort", 0) => ["name", "state", "cpu", "mem", "uptime"]
            .iter()
            .map(|s| item(*s, ""))
            .collect(),
        ("set", 0) => SET_KEYS.iter().map(|k| item(*k, "")).collect(),
        ("help", 0) => APP_COMMANDS.iter().map(|(n, h)| item(*n, *h)).collect(),
        (c, 0) if DOMAIN_FIRST.contains(&c) => domains(),
        ("attach-iso", 1) | ("attach-disk", 1) => paths(current),
        (c, 0) if c.starts_with("net-") => ctx.networks.iter().map(|n| item(n.clone(), "network")).collect(),
        (c, 0) if c.starts_with("pool-") => ctx.pools.iter().map(|p| item(p.clone(), "pool")).collect(),
        (c, 0) if ctx.virsh.iter().any(|(n, _)| n == c) => domains(),
        // `:!virsh <sub> <domain>`
        (c, 0) if c.starts_with('!') => ctx
            .virsh
            .iter()
            .map(|(n, h)| item(n.clone(), h.clone()))
            .collect(),
        (c, 1) if c.starts_with('!') => domains(),
        _ => Vec::new(),
    };
    (start, filter(current, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context {
            domains: vec![
                ("arch-dev".into(), "running".into()),
                ("alpine".into(), "shut off".into()),
            ],
            networks: vec!["default".into()],
            pools: vec!["isos".into()],
            themes: vec!["tokyo-night".into(), "tokyo-night-day".into(), "nord".into()],
            uris: vec!["qemu:///system".into()],
            virsh: vec![("snapshot-create-as".into(), "create a snapshot".into())],
        }
    }

    fn no_flags(_: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    #[test]
    fn completes_commands_and_theme_names() {
        let (start, items) = complete("th", &ctx(), &no_flags);
        assert_eq!(start, 0);
        assert_eq!(items[0].value, "theme");
        let (start, items) = complete("theme ", &ctx(), &no_flags);
        assert_eq!(start, 6);
        assert_eq!(items.len(), 3);
        let (_, items) = complete("theme tok", &ctx(), &no_flags);
        assert_eq!(
            items.iter().map(|i| i.value.as_str()).collect::<Vec<_>>(),
            ["tokyo-night", "tokyo-night-day"]
        );
    }

    #[test]
    fn completes_domains_networks_and_flags() {
        let (_, items) = complete("start a", &ctx(), &no_flags);
        assert_eq!(items.len(), 2);
        let (_, items) = complete("net-start ", &ctx(), &no_flags);
        assert_eq!(items[0].value, "default");
        let (_, items) = complete("snapshot-create-as arch-dev --", &ctx(), &|c| {
            assert_eq!(c, "snapshot-create-as");
            vec![("--name".into(), "name".into()), ("--atomic".into(), "".into())]
        });
        assert_eq!(items.len(), 2);
    }
}
