//! The checks, in report order.

use super::runner::{RunOutcome, Runner};
use super::*;

/// Why a connection-dependent check did not run.
enum Gate {
    Open,
    NoVirsh,
    NoConnection,
}

impl Gate {
    fn skip(&self, label: &'static str) -> Option<Check> {
        let why = match self {
            Self::Open => return None,
            Self::NoVirsh => "virsh not available",
            Self::NoConnection => "no connection",
        };
        Some(Check::new(Section::Connection, label, Status::Skip, why))
    }
}

#[allow(clippy::result_large_err)] // private helper; the Err is the report row itself
/// Successful stdout, or the check to report instead.
fn stdout_or(
    outcome: RunOutcome,
    label: &'static str,
    on_fail: impl FnOnce(&str) -> Check,
) -> Result<String, Check> {
    match outcome {
        RunOutcome::Exited {
            success: true,
            stdout,
            ..
        } => Ok(stdout),
        RunOutcome::Exited { stderr, .. } => {
            let first = stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("unknown error");
            Err(on_fail(first.trim()))
        }
        RunOutcome::NotFound => Err(on_fail("not found in PATH")),
        RunOutcome::Timeout => Err(Check::new(
            Section::Connection,
            label,
            Status::Warn,
            "timed out after 3s",
        )),
    }
}

/// `/home/u/x` → `~/x`.
fn tilde<R: Runner>(runner: &R, path: &std::path::Path) -> String {
    let shown = path.display().to_string();
    match runner.env("HOME").filter(|h| !h.is_empty()) {
        Some(home) => match shown.strip_prefix(&home) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => shown,
        },
        None => shown,
    }
}

/// True for URIs without a host part (`qemu:///system`, `test:///default`).
pub fn is_local(uri: &str) -> bool {
    uri.split_once("://")
        .is_some_and(|(scheme, rest)| !scheme.contains('+') && rest.starts_with('/'))
}

/// Run every check.
pub async fn run_all<R: Runner>(ctx: &Ctx, runner: &R) -> Vec<Check> {
    use super::fixes::{Pkg, daemon_command, distro, install_command};
    use Section::*;
    let uri = ctx.uri.as_str();
    let local = is_local(uri);
    let system = local && uri.starts_with("qemu:///system");
    let distro = runner
        .read(std::path::Path::new("/etc/os-release"))
        .and_then(|text| distro(&text));
    let install = |pkg: Pkg| distro.map(|d| install_command(d, pkg));
    let usermod = runner
        .env("USER")
        .filter(|u| !u.is_empty())
        .map(|u| format!("sudo usermod -aG libvirt {u}"));
    let mut out = Vec::new();

    // 1. virsh
    let virsh = match runner.run("virsh", &["--version"]).await {
        RunOutcome::Exited {
            success: true,
            stdout,
            ..
        } => {
            let path = runner
                .which("virsh")
                .map(|p| format!(" ({})", p.display()))
                .unwrap_or_default();
            Check::new(
                Connection,
                "virsh",
                Status::Ok,
                format!("{}{path}", stdout.trim()),
            )
        }
        RunOutcome::Timeout => Check::new(Connection, "virsh", Status::Warn, "timed out after 3s"),
        RunOutcome::NotFound => Check::new(Connection, "virsh", Status::Fail, "not found in PATH")
            .hint(hints::VIRSH)
            .command_opt(install(Pkg::Virsh)),
        RunOutcome::Exited { stderr, .. } => Check::new(
            Connection,
            "virsh",
            Status::Fail,
            stderr.lines().next().unwrap_or("failed").trim().to_string(),
        )
        .hint(hints::VIRSH),
    };
    let mut gate = if virsh.status == Status::Ok {
        Gate::Open
    } else {
        Gate::NoVirsh
    };
    out.push(virsh);

    // 2. URI
    out.push(
        match runner
            .env("LIBVIRT_DEFAULT_URI")
            .filter(|e| !e.is_empty() && e != uri)
        {
            Some(env) => Check::new(Connection, "URI", Status::Warn, uri)
                .hint(format!("virsh in a shell uses {env}; virsh-tui uses {uri}"))
                .fix(Fix::Set {
                    key: "default_uri",
                    value: env,
                }),
            None => Check::new(Connection, "URI", Status::Info, uri),
        },
    );

    // 3. connection
    out.push(match gate.skip("connection") {
        Some(skip) => skip,
        None => {
            let outcome = runner.run("virsh", &["-c", uri, "version"]).await;
            match stdout_or(outcome, "connection", |line| {
                let check = Check::new(Connection, "connection", Status::Fail, line);
                match crate::backend::virsh::exec::stderr_hint(line) {
                    Some(hint @ "not in the libvirt group?") if system => check
                        .hint(hint)
                        .fix(Fix::Connect(String::from("qemu:///session")))
                        .command_opt(usermod.clone()),
                    Some(hint @ "is libvirtd running?") if local => {
                        check.hint(hint).command(daemon_command(runner))
                    }
                    Some(hint) => check.hint(hint),
                    None => check,
                }
            }) {
                Ok(stdout) => {
                    let field = |key: &str| {
                        stdout
                            .lines()
                            .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
                    };
                    let mut parts = Vec::new();
                    if let Some(lib) = field("Using library:") {
                        parts.push(lib);
                    }
                    if let Some(hv) = field("Running hypervisor:") {
                        parts.push(hv);
                    }
                    Check::new(Connection, "connection", Status::Ok, parts.join(" · "))
                }
                Err(check) => {
                    gate = Gate::NoConnection;
                    check
                }
            }
        }
    });

    // 4. permissions
    out.push(if system {
        match runner.run("id", &["-nG"]).await {
            RunOutcome::Exited {
                success: true,
                stdout,
                ..
            } => {
                if stdout.split_whitespace().any(|g| g == "libvirt") {
                    Check::new(Connection, "permissions", Status::Ok, "in group libvirt")
                } else {
                    Check::new(Connection, "permissions", Status::Warn, "not in group libvirt")
                        .hint(hints::GROUP)
                        .command_opt(usermod.clone())
                }
            }
            _ => Check::new(Connection, "permissions", Status::Skip, "cannot read groups"),
        }
    } else {
        Check::new(Connection, "permissions", Status::Skip, "not a local system URI")
    });

    // 5–7 need a connection; run concurrently.
    if let (Some(e), Some(n), Some(u)) = (
        gate.skip("events"),
        gate.skip("networks"),
        gate.skip("UEFI firmware"),
    ) {
        out.extend([e, n, u]);
    } else {
        let events_args = ["-c", uri, "event", "--all", "--timeout", "1"];
        let nets_args = ["-c", uri, "net-list", "--all"];
        let caps_args = ["-c", uri, "domcapabilities"];
        let (events, nets, caps) = tokio::join!(
            runner.run("virsh", &events_args),
            runner.run("virsh", &nets_args),
            runner.run("virsh", &caps_args),
        );
        out.push(
            match stdout_or(events, "events", |_| {
                Check::new(Connection, "events", Status::Warn, "polling only").hint(hints::EVENTS)
            }) {
                Ok(_) => Check::new(Connection, "events", Status::Ok, "event stream available"),
                Err(check) => check,
            },
        );
        out.push(
            match stdout_or(nets, "networks", |line| {
                Check::new(Connection, "networks", Status::Warn, line)
            }) {
                Ok(text) => {
                    let nets = crate::backend::virsh::parse_list::parse_net_list(&text).unwrap_or_default();
                    let active: Vec<&str> = nets
                        .iter()
                        .filter(|n| n.state == "active")
                        .map(|n| n.name.as_str())
                        .collect();
                    if active.is_empty() {
                        let check = Check::new(Connection, "networks", Status::Warn, "none active")
                            .hint(hints::NETWORKS);
                        if nets.iter().any(|n| n.name == "default") {
                            check.fix(Fix::Virsh(super::start_network_plan("default")))
                        } else {
                            check
                        }
                    } else {
                        Check::new(
                            Connection,
                            "networks",
                            Status::Ok,
                            format!("{} active ({})", active.len(), active.join(", ")),
                        )
                    }
                }
                Err(check) => check,
            },
        );
        out.push(
            match stdout_or(caps, "UEFI firmware", |line| {
                Check::new(Connection, "UEFI firmware", Status::Skip, line)
            }) {
                Ok(xml) => {
                    if has_uefi_loader(&xml) {
                        Check::new(Connection, "UEFI firmware", Status::Ok, "OVMF found")
                    } else {
                        Check::new(Connection, "UEFI firmware", Status::Warn, "no UEFI loader")
                            .hint(hints::UEFI)
                            .command_opt(install(Pkg::Ovmf).filter(|_| local))
                    }
                }
                Err(check) => check,
            },
        );
    }

    // Tools
    let viewer = ctx.config.console.viewer.as_str();
    out.push(match runner.which(viewer) {
        Some(p) => Check::new(Tools, "graphical viewer", Status::Ok, p.display().to_string()),
        None => {
            let other = if viewer == "remote-viewer" {
                "virt-viewer"
            } else {
                "remote-viewer"
            };
            let check = Check::new(
                Tools,
                "graphical viewer",
                Status::Warn,
                format!("{viewer} not found"),
            )
            .hint(hints::VIEWER)
            .command_opt(install(Pkg::Viewer));
            if runner.which(other).is_some() {
                check.fix(Fix::Set {
                    key: "viewer",
                    value: other.to_string(),
                })
            } else {
                check
            }
        }
    });
    for (label, hint, pkg) in [
        ("virt-install", hints::VIRT_INSTALL, Pkg::VirtInstall),
        ("virt-clone", hints::VIRT_CLONE, Pkg::VirtClone),
    ] {
        out.push(match runner.which(label) {
            Some(p) => Check::new(Tools, label, Status::Ok, p.display().to_string()),
            None => Check::new(Tools, label, Status::Warn, "not found")
                .hint(hint)
                .command_opt(install(pkg)),
        });
    }
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|k| runner.env(k).filter(|v| !v.trim().is_empty()))
        .or_else(|| Some(ctx.config.general.editor.clone()).filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| String::from("vi"));
    let program = shlex::split(&editor)
        .and_then(|v| v.into_iter().next())
        .unwrap_or_else(|| String::from("vi"));
    out.push(if runner.which(&program).is_some() {
        Check::new(Tools, "editor", Status::Ok, editor)
    } else {
        Check::new(Tools, "editor", Status::Warn, format!("{program} not found"))
            .hint("set $VISUAL or $EDITOR, or general.editor")
    });
    out.push(if runner.clipboard_ok() {
        Check::new(Tools, "clipboard", Status::Ok, "available")
    } else {
        Check::new(Tools, "clipboard", Status::Warn, "unavailable").hint(hints::CLIPBOARD)
    });

    // Files
    out.push(match (&ctx.config.load_error, &ctx.config_path) {
        (Some(error), path) => Check::new(Files, "config.toml", Status::Fail, error.clone())
            .hint(hints::CONFIG)
            .fix_opt(path.clone().map(Fix::Edit)),
        (None, Some(path)) if runner.exists(path) => {
            Check::new(Files, "config.toml", Status::Ok, tilde(runner, path))
        }
        _ => Check::new(Files, "config.toml", Status::Info, "defaults (no file)"),
    });
    out.push(
        match ctx
            .keymap_path
            .as_ref()
            .and_then(|p| runner.read(p).map(|t| (p, t)))
        {
            None => Check::new(Files, "keymap.toml", Status::Info, "defaults (no file)"),
            Some((path, text)) => {
                let (_, warnings) =
                    crate::input::keymap::apply_overrides(crate::input::keymap::default_map(), &text);
                match warnings.first() {
                    None => Check::new(Files, "keymap.toml", Status::Ok, tilde(runner, path)),
                    Some(first) => {
                        let n = warnings.len();
                        let value = if n == 1 {
                            String::from("1 warning")
                        } else {
                            format!("{n} warnings")
                        };
                        Check::new(Files, "keymap.toml", Status::Warn, value)
                            .hint(first.clone())
                            .fix(Fix::Edit(path.clone()))
                    }
                }
            }
        },
    );
    let theme = ctx.config.appearance.theme.as_str();
    out.push(if crate::theme::builtin::all().iter().any(|t| t.name == theme) {
        Check::new(Files, "theme", Status::Ok, theme)
    } else {
        let path = ctx.themes_dir.as_ref().map(|d| d.join(format!("{theme}.toml")));
        let error = match path.as_ref().and_then(|p| runner.read(p)) {
            None => String::from("no such theme"),
            Some(text) => match crate::theme::load::from_toml(theme, &text) {
                Ok(_) => String::new(),
                Err(e) => e.to_string().lines().next().unwrap_or_default().to_string(),
            },
        };
        if error.is_empty() {
            Check::new(Files, "theme", Status::Ok, theme)
        } else {
            Check::new(Files, "theme", Status::Warn, format!("{theme}: {error}"))
                .hint("using tokyo-night instead")
                .fix(Fix::Set {
                    key: "theme",
                    value: String::from("tokyo-night"),
                })
        }
    });
    out.push(match &ctx.log_path {
        Some(path) if path.parent().is_some_and(|d| runner.dir_writable(d)) => {
            Check::new(Files, "log", Status::Ok, tilde(runner, path))
        }
        Some(path) => Check::new(
            Files,
            "log",
            Status::Warn,
            format!("{} not writable", tilde(runner, path)),
        )
        .hint("logging is off"),
        None => Check::new(Files, "log", Status::Skip, "no state directory"),
    });

    // Terminal
    out.push(match ctx.term_size {
        None => Check::new(Terminal, "size", Status::Skip, "not a terminal"),
        Some((w, h)) => {
            let value = format!("{w}×{h}");
            if w < 40 || h < 12 {
                Check::new(Terminal, "size", Status::Fail, value).hint("virsh-tui needs at least 40×12")
            } else if w < 100 || h < 30 {
                Check::new(Terminal, "size", Status::Warn, value).hint(hints::SIZE)
            } else {
                Check::new(Terminal, "size", Status::Ok, value)
            }
        }
    });
    out.push(match runner.env("COLORTERM").as_deref() {
        Some("truecolor" | "24bit") => Check::new(Terminal, "colours", Status::Ok, "truecolor"),
        Some(other) if !other.is_empty() => {
            Check::new(Terminal, "colours", Status::Warn, format!("COLORTERM={other}")).hint(hints::COLOURS)
        }
        _ => Check::new(Terminal, "colours", Status::Warn, "COLORTERM unset").hint(hints::COLOURS),
    });
    let a = &ctx.config.appearance;
    out.push(
        Check::new(
            Terminal,
            "glyphs",
            Status::Info,
            format!("icons {} · graphs {}", a.icons, a.graphs),
        )
        .hint(hints::GLYPHS),
    );
    out
}

/// `domcapabilities` lists at least one firmware loader path.
fn has_uefi_loader(xml: &str) -> bool {
    roxmltree::Document::parse(xml).is_ok_and(|doc| {
        doc.descendants().any(|n| {
            n.has_tag_name("value")
                && n.parent().is_some_and(|p| p.has_tag_name("loader"))
                && n.text().is_some_and(|t| !t.trim().is_empty())
        })
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::runner::FakeRunner;
    use super::*;

    pub(crate) const VERSION_OUT: &str = "Compiled against library: libvirt 11.6.0\nUsing library: libvirt 11.6.0\nUsing API: QEMU 11.6.0\nRunning hypervisor: QEMU 10.1.0\n";
    pub(crate) const NET_ACTIVE: &str = " Name      State    Autostart   Persistent\n----------------------------------------------\n default   active   yes         yes\n";
    pub(crate) const NET_INACTIVE: &str = " Name      State      Autostart   Persistent\n------------------------------------------------\n default   inactive   no          yes\n";
    pub(crate) const CAPS_OVMF: &str = "<domainCapabilities><os supported='yes'><loader supported='yes'><value>/usr/share/edk2/x64/OVMF_CODE.4m.fd</value></loader></os></domainCapabilities>";
    pub(crate) const CAPS_NONE: &str =
        "<domainCapabilities><os supported='yes'><loader supported='yes'></loader></os></domainCapabilities>";

    pub(crate) fn ctx(uri: &str) -> Ctx {
        Ctx {
            uri: uri.to_string(),
            uri_source: UriSource::Config,
            config: crate::config::Config::default(),
            config_path: Some(PathBuf::from("/home/u/.config/virsh-tui/config.toml")),
            keymap_path: Some(PathBuf::from("/home/u/.config/virsh-tui/keymap.toml")),
            themes_dir: Some(PathBuf::from("/home/u/.config/virsh-tui/themes")),
            log_path: Some(PathBuf::from("/home/u/.local/state/virsh-tui/virsh-tui.log")),
            term_size: Some((174, 43)),
        }
    }

    /// Everything healthy on `qemu:///system`.
    pub(crate) fn healthy(uri: &str) -> FakeRunner {
        let mut r = FakeRunner {
            clipboard: true,
            writable: true,
            ..Default::default()
        };
        r.ok("virsh --version", "11.6.0\n")
            .path("virsh")
            .ok(&format!("virsh -c {uri} version"), VERSION_OUT)
            .ok("id -nG", "u wheel libvirt\n")
            .ok(&format!("virsh -c {uri} event --all --timeout 1"), "")
            .ok(&format!("virsh -c {uri} net-list --all"), NET_ACTIVE)
            .ok(&format!("virsh -c {uri} domcapabilities"), CAPS_OVMF)
            .path("virt-viewer")
            .path("virt-install")
            .path("virt-clone")
            .path("vi")
            .set_env("HOME", "/home/u")
            .set_env("USER", "u")
            .set_env("COLORTERM", "truecolor");
        r
    }

    pub(crate) fn run_sync<R: Runner>(ctx: &Ctx, r: &R) -> Vec<Check> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run_all(ctx, r))
    }

    pub(crate) fn get<'a>(checks: &'a [Check], label: &str) -> &'a Check {
        checks
            .iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("no check {label}"))
    }

    const SYS: &str = "qemu:///system";

    #[test]
    fn healthy_system_is_all_ok() {
        let checks = run_sync(&ctx(SYS), &healthy(SYS));
        let labels: Vec<&str> = checks.iter().map(|c| c.label).collect();
        assert_eq!(
            labels,
            [
                "virsh",
                "URI",
                "connection",
                "permissions",
                "events",
                "networks",
                "UEFI firmware",
                "graphical viewer",
                "virt-install",
                "virt-clone",
                "editor",
                "clipboard",
                "config.toml",
                "keymap.toml",
                "theme",
                "log",
                "size",
                "colours",
                "glyphs"
            ]
        );
        assert_eq!(get(&checks, "virsh").value, "11.6.0 (/usr/bin/virsh)");
        assert_eq!(get(&checks, "connection").value, "libvirt 11.6.0 · QEMU 10.1.0");
        assert_eq!(get(&checks, "permissions").value, "in group libvirt");
        assert_eq!(get(&checks, "networks").value, "1 active (default)");
        assert_eq!(
            get(&checks, "log").value,
            "~/.local/state/virsh-tui/virsh-tui.log"
        );
        for c in &checks {
            assert!(
                matches!(c.status, Status::Ok | Status::Info),
                "{} is {:?}",
                c.label,
                c.status
            );
        }
    }

    #[test]
    fn virsh_missing_skips_dependents() {
        let mut r = healthy(SYS);
        r.runs.remove("virsh --version");
        r.paths.remove("virsh");
        let checks = run_sync(&ctx(SYS), &r);
        let virsh = get(&checks, "virsh");
        assert_eq!(
            (virsh.status, virsh.value.as_str()),
            (Status::Fail, "not found in PATH")
        );
        assert_eq!(virsh.hint.as_deref(), Some(hints::VIRSH));
        for label in ["connection", "events", "networks", "UEFI firmware"] {
            assert_eq!(get(&checks, label).status, Status::Skip, "{label}");
        }
    }

    #[test]
    fn env_uri_mismatch_warns() {
        let mut r = healthy(SYS);
        r.set_env("LIBVIRT_DEFAULT_URI", "qemu:///session");
        let checks = run_sync(&ctx(SYS), &r);
        let uri = get(&checks, "URI");
        assert_eq!(uri.status, Status::Warn);
        assert!(
            uri.hint
                .as_deref()
                .unwrap()
                .contains("virsh in a shell uses qemu:///session")
        );
    }

    #[test]
    fn connection_permission_denied() {
        let mut r = healthy(SYS);
        r.fail("virsh -c qemu:///system version", "error: Permission denied\n");
        let checks = run_sync(&ctx(SYS), &r);
        let c = get(&checks, "connection");
        assert_eq!(c.status, Status::Fail);
        assert_eq!(c.value, "error: Permission denied");
        assert!(c.hint.as_deref().unwrap().contains("not in the libvirt group?"));
        for label in ["events", "networks", "UEFI firmware"] {
            assert_eq!(get(&checks, label).status, Status::Skip, "{label}");
        }
    }

    #[test]
    fn permissions_group() {
        let mut r = healthy(SYS);
        r.ok("id -nG", "tiago wheel\n");
        let c = run_sync(&ctx(SYS), &r);
        let p = get(&c, "permissions");
        assert_eq!(
            (p.status, p.value.as_str()),
            (Status::Warn, "not in group libvirt")
        );
        assert_eq!(p.hint.as_deref(), Some(hints::GROUP));
        let session = "qemu:///session";
        let c = run_sync(&ctx(session), &healthy(session));
        let p = get(&c, "permissions");
        assert_eq!(
            (p.status, p.value.as_str()),
            (Status::Skip, "not a local system URI")
        );
    }

    #[test]
    fn events_unsupported() {
        let mut r = healthy(SYS);
        r.fail(
            "virsh -c qemu:///system event --all --timeout 1",
            "error: unsupported\n",
        );
        let c = run_sync(&ctx(SYS), &r);
        let e = get(&c, "events");
        assert_eq!((e.status, e.value.as_str()), (Status::Warn, "polling only"));
        assert_eq!(e.hint.as_deref(), Some(hints::EVENTS));
    }

    #[test]
    fn networks_none_active() {
        let mut r = healthy(SYS);
        r.ok("virsh -c qemu:///system net-list --all", NET_INACTIVE);
        let c = run_sync(&ctx(SYS), &r);
        let n = get(&c, "networks");
        assert_eq!((n.status, n.value.as_str()), (Status::Warn, "none active"));
        assert_eq!(n.hint.as_deref(), Some(hints::NETWORKS));
    }

    #[test]
    fn uefi_loader() {
        let mut r = healthy(SYS);
        r.ok("virsh -c qemu:///system domcapabilities", CAPS_NONE);
        let c = run_sync(&ctx(SYS), &r);
        let u = get(&c, "UEFI firmware");
        assert_eq!((u.status, u.value.as_str()), (Status::Warn, "no UEFI loader"));
        assert_eq!(u.hint.as_deref(), Some(hints::UEFI));
        let c = run_sync(&ctx(SYS), &healthy(SYS));
        assert_eq!(get(&c, "UEFI firmware").value, "OVMF found");
    }

    #[test]
    fn tools_in_path() {
        let mut r = healthy(SYS);
        r.paths.remove("virt-clone");
        r.paths.remove("virt-viewer");
        let mut cx = ctx(SYS);
        cx.config.console.viewer = String::from("remote-viewer");
        r.path("remote-viewer");
        let c = run_sync(&cx, &r);
        let clone = get(&c, "virt-clone");
        assert_eq!((clone.status, clone.value.as_str()), (Status::Warn, "not found"));
        assert_eq!(clone.hint.as_deref(), Some(hints::VIRT_CLONE));
        assert_eq!(get(&c, "graphical viewer").value, "/usr/bin/remote-viewer");
    }

    #[test]
    fn editor_resolution() {
        let mut r = healthy(SYS);
        r.set_env("EDITOR", "code --wait").path("code");
        let c = run_sync(&ctx(SYS), &r);
        let e = get(&c, "editor");
        assert_eq!((e.status, e.value.as_str()), (Status::Ok, "code --wait"));
        r.paths.remove("code");
        let c = run_sync(&ctx(SYS), &r);
        assert_eq!(get(&c, "editor").status, Status::Warn);
    }

    #[test]
    fn files() {
        let mut cx = ctx(SYS);
        cx.config.load_error = Some(String::from("config.toml: expected `=`"));
        let mut r = healthy(SYS);
        r.file(
            "/home/u/.config/virsh-tui/keymap.toml",
            "[normal]\nbogus = [\"x\"]\n",
        );
        cx.config.appearance.theme = String::from("nope");
        let c = run_sync(&cx, &r);
        let config = get(&c, "config.toml");
        assert_eq!(
            (config.status, config.value.as_str()),
            (Status::Fail, "config.toml: expected `=`")
        );
        let keymap = get(&c, "keymap.toml");
        assert_eq!(
            (keymap.status, keymap.value.as_str()),
            (Status::Warn, "1 warning")
        );
        assert!(keymap.hint.as_deref().unwrap().contains("bogus"));
        assert_eq!(get(&c, "theme").status, Status::Warn);

        let c = run_sync(&ctx(SYS), &healthy(SYS));
        let keymap = get(&c, "keymap.toml");
        assert_eq!(
            (keymap.status, keymap.value.as_str()),
            (Status::Info, "defaults (no file)")
        );
        let config = get(&c, "config.toml");
        assert_eq!(
            (config.status, config.value.as_str()),
            (Status::Info, "defaults (no file)")
        );
        assert_eq!(get(&c, "theme").value, "tokyo-night");
    }

    #[test]
    fn custom_theme_file() {
        let mut cx = ctx(SYS);
        cx.config.appearance.theme = String::from("mine");
        let mut r = healthy(SYS);
        r.file("/home/u/.config/virsh-tui/themes/mine.toml", "not = valid = toml");
        let c = run_sync(&cx, &r);
        let t = get(&c, "theme");
        assert_eq!(t.status, Status::Warn);
        assert!(t.value.starts_with("mine: "));
    }

    #[test]
    fn terminal() {
        let mut cx = ctx(SYS);
        let mut r = healthy(SYS);
        cx.term_size = Some((90, 30));
        assert_eq!(get(&run_sync(&cx, &r), "size").status, Status::Warn);
        cx.term_size = Some((39, 12));
        assert_eq!(get(&run_sync(&cx, &r), "size").status, Status::Fail);
        cx.term_size = None;
        assert_eq!(get(&run_sync(&cx, &r), "size").status, Status::Skip);
        let c = run_sync(&cx, &r);
        assert_eq!(get(&c, "colours").value, "truecolor");
        r.envs.remove("COLORTERM");
        let c = run_sync(&cx, &r);
        let col = get(&c, "colours");
        assert_eq!(col.status, Status::Warn);
        assert_eq!(col.hint.as_deref(), Some(hints::COLOURS));
    }

    #[test]
    fn slow_process_times_out() {
        let mut r = healthy(SYS);
        r.runs.insert(
            String::from("virsh -c qemu:///system version"),
            RunOutcome::Timeout,
        );
        let c = run_sync(&ctx(SYS), &r);
        let conn = get(&c, "connection");
        assert_eq!(
            (conn.status, conn.value.as_str()),
            (Status::Warn, "timed out after 3s")
        );
    }

    #[tokio::test]
    #[cfg_attr(not(feature = "it"), ignore)]
    async fn system_runner_test_uri() {
        let cx = ctx("test:///default");
        let checks = run_all(&cx, &super::super::runner::SystemRunner).await;
        let conn = get(&checks, "connection");
        assert_eq!(conn.status, Status::Ok, "{conn:?}");
        assert_eq!(get(&checks, "permissions").status, Status::Skip);
    }

    #[test]
    fn fixes_attached() {
        // Permission denied on qemu:///system.
        let mut r = healthy(SYS);
        r.set_env("USER", "tiago")
            .fail("virsh -c qemu:///system version", "error: Permission denied\n")
            .file("/etc/os-release", "ID=arch\n");
        let c = run_sync(&ctx(SYS), &r);
        let conn = get(&c, "connection");
        assert_eq!(conn.fix, Some(Fix::Connect(String::from("qemu:///session"))));
        assert_eq!(conn.command.as_deref(), Some("sudo usermod -aG libvirt tiago"));

        // Daemon down.
        let mut r = healthy(SYS);
        r.fail(
            "virsh -c qemu:///system version",
            "error: failed to connect to socket '/run/libvirt/virtqemud-sock'",
        );
        let c = run_sync(&ctx(SYS), &r);
        assert_eq!(
            get(&c, "connection").command.as_deref(),
            Some("sudo systemctl enable --now libvirtd")
        );

        // Not in the group.
        let mut r = healthy(SYS);
        r.set_env("USER", "tiago").ok("id -nG", "tiago\n");
        let c = run_sync(&ctx(SYS), &r);
        assert_eq!(
            get(&c, "permissions").command.as_deref(),
            Some("sudo usermod -aG libvirt tiago")
        );

        // Env URI differs.
        let mut r = healthy(SYS);
        r.set_env("LIBVIRT_DEFAULT_URI", "qemu:///session");
        let c = run_sync(&ctx(SYS), &r);
        assert_eq!(
            get(&c, "URI").fix,
            Some(Fix::Set {
                key: "default_uri",
                value: String::from("qemu:///session")
            })
        );

        // default network inactive.
        let mut r = healthy(SYS);
        r.ok("virsh -c qemu:///system net-list --all", NET_INACTIVE);
        let c = run_sync(&ctx(SYS), &r);
        let Some(Fix::Virsh(plan)) = &get(&c, "networks").fix else {
            panic!("no virsh fix")
        };
        let argv: Vec<Vec<String>> = plan.steps.iter().map(|s| s.argv.clone()).collect();
        assert_eq!(
            argv,
            [vec!["net-start", "default"], vec!["net-autostart", "default"]]
        );
        assert_eq!(plan.summary, "Started network default");

        // Only remote-viewer installed.
        let mut r = healthy(SYS);
        r.paths.remove("virt-viewer");
        r.path("remote-viewer").file("/etc/os-release", "ID=fedora\n");
        let c = run_sync(&ctx(SYS), &r);
        let viewer = get(&c, "graphical viewer");
        assert_eq!(
            viewer.fix,
            Some(Fix::Set {
                key: "viewer",
                value: String::from("remote-viewer")
            })
        );
        assert_eq!(viewer.command.as_deref(), Some("sudo dnf install virt-viewer"));

        // Missing tools and firmware with a known distro.
        let mut r = healthy(SYS);
        r.paths.remove("virt-clone");
        r.file("/etc/os-release", "ID=debian\n")
            .ok("virsh -c qemu:///system domcapabilities", CAPS_NONE);
        let c = run_sync(&ctx(SYS), &r);
        assert_eq!(
            get(&c, "virt-clone").command.as_deref(),
            Some("sudo apt install virtinst")
        );
        assert_eq!(
            get(&c, "UEFI firmware").command.as_deref(),
            Some("sudo apt install ovmf")
        );

        // Files.
        let mut cx = ctx(SYS);
        cx.config.load_error = Some(String::from("bad"));
        cx.config.appearance.theme = String::from("nope");
        let mut r = healthy(SYS);
        r.file(
            "/home/u/.config/virsh-tui/keymap.toml",
            "[normal]\nbogus = [\"x\"]\n",
        );
        let c = run_sync(&cx, &r);
        assert_eq!(
            get(&c, "config.toml").fix,
            Some(Fix::Edit(PathBuf::from("/home/u/.config/virsh-tui/config.toml")))
        );
        assert_eq!(
            get(&c, "keymap.toml").fix,
            Some(Fix::Edit(PathBuf::from("/home/u/.config/virsh-tui/keymap.toml")))
        );
        assert_eq!(
            get(&c, "theme").fix,
            Some(Fix::Set {
                key: "theme",
                value: String::from("tokyo-night")
            })
        );
    }

    #[test]
    fn remote_uri_has_no_root_commands() {
        let uri = "qemu+ssh://h/system";
        let mut r = healthy(uri);
        r.file("/etc/os-release", "ID=arch\n")
            .fail(&format!("virsh -c {uri} version"), "error: Permission denied")
            .ok(&format!("virsh -c {uri} domcapabilities"), CAPS_NONE);
        let c = run_sync(&ctx(uri), &r);
        for check in &c {
            if matches!(check.label, "connection" | "permissions" | "UEFI firmware") {
                assert_eq!(check.command, None, "{}", check.label);
            }
        }
        // Remote: no session switch either (that would be the local machine).
        assert_eq!(get(&c, "connection").fix, None);
        let mut r = healthy(uri);
        r.file("/etc/os-release", "ID=arch\n")
            .ok(&format!("virsh -c {uri} domcapabilities"), CAPS_NONE);
        let c = run_sync(&ctx(uri), &r);
        assert_eq!(get(&c, "UEFI firmware").command, None);
    }

    #[test]
    fn unknown_distro_no_install_command() {
        let mut r = healthy(SYS);
        r.paths.remove("virt-clone");
        r.file("/etc/os-release", "ID=nixos\n");
        let c = run_sync(&ctx(SYS), &r);
        let clone = get(&c, "virt-clone");
        assert_eq!(clone.command, None);
        assert_eq!(clone.hint.as_deref(), Some(hints::VIRT_CLONE));
    }
}
