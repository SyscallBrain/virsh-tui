//! Report text and the fixed demo report.

use super::*;

/// Column where values start (`  ✓ ` + 18-cell label).
const LABEL_WIDTH: usize = 18;
/// Indent of hint / run / fix lines.
const DETAIL_INDENT: usize = 22;

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn paint(color: bool, code: &str, text: &str) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn status_color(status: Status) -> &'static str {
    match status {
        Status::Ok => "32",
        Status::Warn => "33",
        Status::Fail => "31",
        Status::Info | Status::Skip => "2",
    }
}

impl Counts {
    /// `16 ok · 2 warnings · 0 errors`.
    pub fn summary(&self) -> String {
        format!(
            "{} ok · {} · {}",
            self.ok,
            plural(self.warn, "warning"),
            plural(self.fail, "error")
        )
    }
}

impl Report {
    pub fn counts(&self) -> Counts {
        let count = |s: Status| self.checks.iter().filter(|c| c.status == s).count();
        Counts {
            ok: count(Status::Ok),
            warn: count(Status::Warn),
            fail: count(Status::Fail),
        }
    }

    pub fn has_failures(&self) -> bool {
        self.checks.iter().any(|c| c.status == Status::Fail)
    }

    /// Plain report (ANSI colours when `color`), as printed by `--doctor`
    /// and copied by `Y`.
    pub fn to_text(&self, color: bool) -> String {
        let h = &self.header;
        let mut out = format!(
            "virsh-tui {} doctor · {} · {} ({})\n",
            h.version,
            h.os,
            h.uri,
            h.uri_source.text()
        );
        let pad = " ".repeat(DETAIL_INDENT);
        let mut section = None;
        for check in &self.checks {
            if section != Some(check.section) {
                section = Some(check.section);
                out.push('\n');
                out.push_str(check.section.title());
                out.push('\n');
            }
            let glyph = paint(color, status_color(check.status), check.status.glyph());
            out.push_str(&format!(
                "  {glyph} {:<width$}{}\n",
                check.label,
                check.value,
                width = LABEL_WIDTH
            ));
            if let Some(hint) = &check.hint {
                out.push_str(&format!("{pad}{}\n", paint(color, "2", &format!("→ {hint}"))));
            }
            if let Some(command) = &check.command {
                out.push_str(&format!("{pad}run: {command}\n"));
            }
            if let Some(fix) = &check.fix {
                out.push_str(&format!("{pad}fix: Settings › Doctor, f ({})\n", fix.describe()));
            }
        }
        out.push('\n');
        out.push_str(&self.counts().summary());
        out.push('\n');
        out
    }
}

/// Fixed report for `--demo` (one of each status; nothing is run).
pub fn demo_report() -> Report {
    use Section::*;
    use Status::*;
    let checks = vec![
        Check::new(Connection, "virsh", Ok, "11.6.0 (/usr/bin/virsh)"),
        Check::new(Connection, "URI", Info, "qemu:///system"),
        Check::new(Connection, "connection", Ok, "libvirt 11.6.0 · QEMU 10.1.0"),
        Check::new(Connection, "permissions", Warn, "not in group libvirt")
            .hint(super::hints::GROUP)
            .command("sudo usermod -aG libvirt demo")
            .fix(Fix::Connect(String::from("qemu:///session"))),
        Check::new(Connection, "events", Ok, "event stream available"),
        Check::new(Connection, "networks", Warn, "none active")
            .hint(super::hints::NETWORKS)
            .fix(Fix::Virsh(super::start_network_plan("default"))),
        Check::new(Connection, "UEFI firmware", Ok, "OVMF found"),
        Check::new(Tools, "graphical viewer", Ok, "/usr/bin/virt-viewer"),
        Check::new(Tools, "virt-install", Ok, "/usr/bin/virt-install"),
        Check::new(Tools, "virt-clone", Fail, "not found")
            .hint(super::hints::VIRT_CLONE)
            .command("sudo pacman -S virt-install"),
        Check::new(Tools, "editor", Ok, "nvim"),
        Check::new(Tools, "clipboard", Ok, "available"),
        Check::new(Files, "config.toml", Ok, "~/.config/virsh-tui/config.toml"),
        Check::new(Files, "keymap.toml", Info, "defaults (no file)"),
        Check::new(Files, "theme", Ok, "tokyo-night"),
        Check::new(Files, "log", Ok, "~/.local/state/virsh-tui/virsh-tui.log"),
        Check::new(Terminal, "size", Ok, "174×43"),
        Check::new(Terminal, "colours", Ok, "truecolor"),
        Check::new(Terminal, "glyphs", Info, "icons unicode · graphs braille").hint(super::hints::GLYPHS),
    ];
    Report {
        header: Header {
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: String::from("Linux"),
            uri: String::from("qemu:///system"),
            uri_source: UriSource::Config,
        },
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_report_text() {
        let text = demo_report()
            .to_text(false)
            .replacen(env!("CARGO_PKG_VERSION"), "[version]", 1);
        insta::assert_snapshot!(text);
    }

    #[test]
    fn counts_and_summary() {
        let report = demo_report();
        for status in [Status::Ok, Status::Warn, Status::Fail, Status::Info] {
            assert!(report.checks.iter().any(|c| c.status == status), "{status:?}");
        }
        let c = report.counts();
        let text = report.to_text(false);
        let plural = |n: usize, word: &str| {
            if n == 1 {
                format!("{n} {word}")
            } else {
                format!("{n} {word}s")
            }
        };
        assert_eq!(
            text.lines().last().unwrap(),
            format!(
                "{} ok · {} · {}",
                c.ok,
                plural(c.warn, "warning"),
                plural(c.fail, "error")
            )
        );
        assert!(report.has_failures());
    }

    #[test]
    fn text_layout() {
        let text = demo_report().to_text(false);
        let first = text.lines().next().unwrap();
        assert!(first.starts_with(&format!("virsh-tui {} doctor · ", env!("CARGO_PKG_VERSION"))));
        assert!(first.ends_with("· qemu:///system (config default_uri)"));
        assert!(text.lines().any(|l| l == "Connection"));
        assert!(
            text.lines()
                .any(|l| l == "  ✓ virsh             11.6.0 (/usr/bin/virsh)")
        );
        let lines: Vec<&str> = text.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with("  ⚠ permissions"))
            .unwrap();
        let pad = " ".repeat(22);
        assert!(lines[at + 1].starts_with(&format!("{pad}→ ")));
        assert_eq!(lines[at + 2], format!("{pad}run: sudo usermod -aG libvirt demo"));
        // The report only describes fixes: they are applied from the app.
        assert_eq!(
            lines[at + 3],
            format!("{pad}fix: Settings › Doctor, f (connect to qemu:///session)")
        );
    }

    #[test]
    fn color_only_when_asked() {
        let report = demo_report();
        assert!(!report.to_text(false).contains('\x1b'));
        assert!(report.to_text(true).contains("\x1b[32m✓"));
    }
}
