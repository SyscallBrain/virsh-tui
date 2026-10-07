//! Ex command parser (`:` line).

/// Parsed ex command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExCommand {
    Start(String),
    Shutdown(String),
    Destroy(String),
    Rename(String, String),
    Desc(String, String),
    AttachDisk(String, String, String),
    DetachDisk(String, String),
    AttachNic(String, String, String),
    Resize(String, String, String),
    Media(String, String, Option<String>),
    AttachHostdev(String, String),
    Quit,
    QuitAll,
    Write,
    Discard,
    Messages,
    Set(String, String),
    Help(String),
    Connect(String),
    Theme(String),
    Sort(String),
    Jump(u32),
    RawVirsh(String),
    Shell(String),
    Unknown(String),
}

/// Parse an ex line (leading `:` optional).
pub fn parse(line: &str) -> Result<ExCommand, String> {
    let line = line.strip_prefix(':').unwrap_or(line).trim();
    if line.is_empty() {
        return Err(String::from("empty command"));
    }
    if let Ok(n) = line.parse::<u32>() {
        return Ok(ExCommand::Jump(n));
    }
    let mut parts = line.splitn(2, char::is_whitespace);
    let word = parts.next().unwrap_or("");
    let args = parts.next().unwrap_or("").trim();
    match word {
        "q" => Ok(ExCommand::Quit),
        "q!" => Ok(ExCommand::Discard),
        "qa" | "qall" => Ok(ExCommand::QuitAll),
        "w" => Ok(ExCommand::Write),
        "messages" => Ok(ExCommand::Messages),
        "start" => req_arg(word, args, ExCommand::Start),
        // With extra flags (`--mode agent`) these go straight to virsh.
        "shutdown" | "destroy" if args.split_whitespace().count() > 1 => {
            Ok(ExCommand::RawVirsh(line.to_string()))
        }
        "shutdown" => req_arg(word, args, ExCommand::Shutdown),
        "destroy" => req_arg(word, args, ExCommand::Destroy),
        "rename" => {
            let mut parts = args.split_whitespace();
            match (parts.next(), parts.next()) {
                (Some(a), Some(b)) => Ok(ExCommand::Rename(a.to_string(), b.to_string())),
                _ => Err(String::from(":rename needs <domain> <new-name>")),
            }
        }
        "desc" => {
            let mut parts = args.splitn(2, char::is_whitespace);
            match (parts.next(), parts.next()) {
                (Some(a), Some(b)) if !b.trim().is_empty() => {
                    Ok(ExCommand::Desc(a.to_string(), b.trim().to_string()))
                }
                _ => Err(String::from(":desc needs <domain> <text>")),
            }
        }
        "attach-disk" => {
            let p: Vec<&str> = args.split_whitespace().collect();
            match p.as_slice() {
                [d, src, target] => Ok(ExCommand::AttachDisk(
                    d.to_string(),
                    src.to_string(),
                    target.to_string(),
                )),
                // Full virsh syntax (flags, more args) goes straight to virsh.
                _ => Ok(ExCommand::RawVirsh(line.to_string())),
            }
        }
        "detach-disk" => {
            let p: Vec<&str> = args.split_whitespace().collect();
            match p.as_slice() {
                [d, target] => Ok(ExCommand::DetachDisk(d.to_string(), target.to_string())),
                _ => Ok(ExCommand::RawVirsh(line.to_string())),
            }
        }
        "attach-nic" => {
            let p: Vec<&str> = args.split_whitespace().collect();
            match p.as_slice() {
                [d, source, model] => Ok(ExCommand::AttachNic(
                    d.to_string(),
                    source.to_string(),
                    model.to_string(),
                )),
                _ => Err(String::from(":attach-nic needs <domain> <source> <model>")),
            }
        }
        "block-resize" => {
            let p: Vec<&str> = args.split_whitespace().collect();
            match p.as_slice() {
                [d, target, size] => Ok(ExCommand::Resize(
                    d.to_string(),
                    target.to_string(),
                    size.to_string(),
                )),
                _ => Err(String::from(":block-resize needs <domain> <target> <size>")),
            }
        }
        "attach-iso" | "media" => {
            let p: Vec<&str> = args.split_whitespace().collect();
            match p.as_slice() {
                [d, iso] => Ok(ExCommand::Media(
                    d.to_string(),
                    String::new(),
                    Some(iso.to_string()),
                )),
                [d, target, iso] => Ok(ExCommand::Media(
                    d.to_string(),
                    target.to_string(),
                    Some(iso.to_string()),
                )),
                _ => Err(String::from(":attach-iso needs <domain> [target] <iso path>")),
            }
        }
        "attach-hostdev" => {
            let p: Vec<&str> = args.split_whitespace().collect();
            match p.as_slice() {
                [d, node] => Ok(ExCommand::AttachHostdev(d.to_string(), node.to_string())),
                _ => Err(String::from(":attach-hostdev needs <domain> <nodedev>")),
            }
        }
        "connect" => req_arg(word, args, ExCommand::Connect),
        "theme" => req_arg(word, args, ExCommand::Theme),
        "sort" => req_arg(word, args, ExCommand::Sort),
        "help" => Ok(ExCommand::Help(args.to_string())),
        "set" => {
            let (k, v) = args.split_once('=').unwrap_or((args, ""));
            Ok(ExCommand::Set(k.trim().to_string(), v.trim().to_string()))
        }
        w if w.starts_with('!') => {
            let cmd = format!("{} {args}", &w[1..]);
            if cmd.trim_start().starts_with("virsh") {
                Ok(ExCommand::Shell(cmd.trim().to_string()))
            } else {
                Err(String::from(":! only runs virsh (e.g. :!virsh dominfo arch-dev)"))
            }
        }
        _ => Ok(ExCommand::RawVirsh(line.to_string())),
    }
}

fn req_arg(word: &str, args: &str, f: impl Fn(String) -> ExCommand) -> Result<ExCommand, String> {
    if args.is_empty() {
        Err(format!(":{word} needs an argument"))
    } else {
        Ok(f(args.split_whitespace().next().unwrap_or("").to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{ExCommand, parse};

    #[test]
    fn commands() {
        assert_eq!(parse(":qa").unwrap(), ExCommand::QuitAll);
        assert_eq!(parse(":42").unwrap(), ExCommand::Jump(42));
        assert_eq!(
            parse(":connect qemu:///session").unwrap(),
            ExCommand::Connect("qemu:///session".to_string())
        );
    }
}

#[cfg(test)]
mod passthrough_tests {
    use super::{ExCommand, parse};

    #[test]
    fn full_virsh_syntax_is_not_rejected() {
        let line = "attach-disk vm /x.qcow2 vdb --driver qemu --config";
        assert_eq!(parse(line), Ok(ExCommand::RawVirsh(line.to_string())));
        assert!(matches!(
            parse("attach-disk vm /x.qcow2 vdb"),
            Ok(ExCommand::AttachDisk(..))
        ));
        assert!(matches!(
            parse("attach-iso vm /iso/a.iso"),
            Ok(ExCommand::Media(..))
        ));
    }
}
