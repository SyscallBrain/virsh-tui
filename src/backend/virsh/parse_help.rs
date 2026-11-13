//! Parser for `virsh help` and `virsh help <cmd>`.

/// Index entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HelpEntry {
    pub name: String,
    pub summary: String,
}

/// Command option.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HelpOpt {
    pub flag: String,
    pub desc: String,
    pub required: bool,
}

/// Full help doc.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HelpDoc {
    pub name: String,
    pub summary: String,
    pub synopsis: String,
    pub description: String,
    pub options: Vec<HelpOpt>,
}

/// Fetch a command's help without blocking the UI thread.
pub async fn fetch_doc(uri: &str, cmd: &str) -> Option<HelpDoc> {
    if !cmd.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let text = crate::backend::virsh::exec::run(uri, &["help", cmd]).await.ok()?;
    let doc = parse_help_cmd(&text);
    (!doc.name.is_empty()).then_some(doc)
}

/// Fetch the full `virsh help` index. `None` when virsh could not be
/// asked: the caller keeps no index and asks again next time.
pub async fn fetch_index(uri: &str) -> Option<Vec<HelpEntry>> {
    let text = crate::backend::virsh::exec::run(uri, &["help"]).await.ok()?;
    let entries = parse_help_index(&text);
    (!entries.is_empty()).then_some(entries)
}

/// Parse `virsh help` index (lines of `    cmd - summary`, skipping headers).
pub fn parse_help_index(text: &str) -> Vec<HelpEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if !(line.starts_with("    ") || line.starts_with('\t')) {
            continue;
        }
        let trimmed = trimmed.trim();
        if trimmed.is_empty() || !trimmed.contains(' ') {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("").to_string();
        let summary = parts
            .next()
            .unwrap_or("")
            .trim_start_matches("- ")
            .trim()
            .to_string();
        if name.is_empty() || name.starts_with('-') {
            continue;
        }
        out.push(HelpEntry { name, summary });
    }
    out
}

/// Parse `virsh help <cmd>` NAME/SYNOPSIS/DESCRIPTION/OPTIONS layout.
pub fn parse_help_cmd(text: &str) -> HelpDoc {
    let mut name = String::new();
    let mut summary = String::new();
    let mut synopsis = String::new();
    let mut description = String::new();
    let mut options = Vec::new();
    let mut section = "";
    for line in text.lines() {
        let trimmed = line.trim();
        match trimmed {
            "NAME" | "SYNOPSIS" | "DESCRIPTION" | "OPTIONS" => section = trimmed,
            _ => match section {
                "NAME" => {
                    if trimmed.is_empty() || trimmed.starts_with("virsh #") {
                        continue;
                    }
                    let mut parts = trimmed.splitn(2, " - ");
                    name = parts.next().unwrap_or("").to_string();
                    summary = parts.next().unwrap_or("").to_string();
                }
                "SYNOPSIS" => {
                    if !trimmed.is_empty() {
                        if !synopsis.is_empty() {
                            synopsis.push(' ');
                        }
                        synopsis.push_str(trimmed);
                    }
                }
                "DESCRIPTION" => {
                    if !trimmed.is_empty() {
                        if !description.is_empty() {
                            description.push(' ');
                        }
                        description.push_str(trimmed);
                    }
                }
                "OPTIONS" => {
                    if let Some(flag) = trimmed.split_whitespace().next()
                        && (flag.starts_with("--") || flag.starts_with('-'))
                    {
                        let desc = trimmed[flag.len()..].trim().to_string();
                        options.push(HelpOpt {
                            flag: flag.to_string(),
                            desc,
                            required: false,
                        });
                    }
                }
                _ => {}
            },
        }
    }
    HelpDoc {
        name,
        summary,
        synopsis,
        description,
        options,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_help_cmd, parse_help_index};

    #[test]
    fn index_has_many_commands() {
        let text = std::fs::read_to_string("tests/fixtures/virsh/help_index.txt").unwrap();
        let entries = parse_help_index(&text);
        assert!(entries.len() > 200, "got {} entries", entries.len());
        assert!(entries.iter().any(|e| e.name == "domstats"));
    }

    #[test]
    fn domstats_doc_parses() {
        let text = std::fs::read_to_string("tests/fixtures/virsh/help_domstats.txt").unwrap();
        let doc = parse_help_cmd(&text);
        assert_eq!(doc.name, "domstats");
        assert!(doc.options.iter().any(|o| o.flag == "--raw"));
    }
}
