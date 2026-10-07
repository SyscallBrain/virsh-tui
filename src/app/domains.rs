//! Domain table actions: lifecycle plans, confirmations, clipboard.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Targets: marks when non-empty (bulk), else the selection.
pub(super) fn targets(dash: &crate::ui::views::dashboard::DashboardState) -> Vec<String> {
    if dash.rows.is_empty() {
        return vec![];
    }
    if dash.marks.is_empty() {
        vec![dash.selected().name.clone()]
    } else {
        let mut v: Vec<String> = dash.marks.iter().cloned().collect();
        v.sort();
        v
    }
}

/// Run a plan for each target sequentially; update message, history, demo state.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_plan_on_targets(
    uri: &str,
    targets: &[String],
    build: impl Fn(&str) -> crate::command::plan::CommandPlan,
    dry_run: bool,
    demo: bool,
    dash: &mut crate::ui::views::dashboard::DashboardState,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) {
    for target in targets {
        let plan = build(target);
        let first = plan
            .steps
            .first()
            .map(crate::command::display::display)
            .unwrap_or_default();
        history.push(&first);
        match crate::command::exec::execute(uri, &plan, dry_run).await {
            Ok(msg) => {
                dash.message = msg.clone();
                messages.push(msg);
                if demo {
                    apply_demo_transition(dash, target, &plan.summary);
                }
            }
            Err(e) => {
                dash.message = e.to_string();
                messages.push(e.to_string());
                break;
            }
        }
    }
    if !demo {
        refresh_live(uri, dash).await;
    }
}

/// Demo state transitions (mockup simulation).
pub(super) fn apply_demo_transition(
    dash: &mut crate::ui::views::dashboard::DashboardState,
    target: &str,
    summary: &str,
) {
    let new_state = if summary.starts_with("Started") || summary.starts_with("Resumed") {
        Some(crate::model::DomainState::Running)
    } else if summary.starts_with("Paused") || summary.starts_with("Suspended") {
        Some(crate::model::DomainState::Paused)
    } else if summary.starts_with("Shutdown requested")
        || summary.starts_with("Destroyed")
        || summary.starts_with("Managed-save")
    {
        Some(crate::model::DomainState::ShutOff)
    } else {
        None
    };
    if let Some(s) = new_state {
        for row in &mut dash.rows {
            if row.name == target {
                row.state = s;
                row.state_label = match s {
                    crate::model::DomainState::Running => "running",
                    crate::model::DomainState::Paused => "paused",
                    crate::model::DomainState::Crashed => "crashed",
                    _ => "shut off",
                };
            }
        }
    }
}

/// Re-fetch live domains after a mutation.
pub(super) async fn refresh_live(uri: &str, dash: &mut crate::ui::views::dashboard::DashboardState) {
    if let Ok((summaries, _)) = crate::backend::virsh::VirshBackend::new(uri)
        .domains_with_stats()
        .await
    {
        dash.apply_summaries(&summaries);
        dash.apply_metrics();
    }
}

pub(super) fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}

pub(super) fn copy_text(text: &str) {
    if arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .is_err()
    {
        use std::io::Write;
        let seq = format!("\x1b]52;c;{}\x07", base64_clone(text));
        let _ = write!(std::io::stdout(), "{seq}");
    }
}

pub(super) fn base64_clone(_text: &str) -> String {
    // Minimal base64 (no extra dependency): hand-rolled for OSC 52 fallback.
    const ALPH: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = _text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as u32;
        let b1 = if i + 1 < bytes.len() {
            bytes[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < bytes.len() {
            bytes[i + 2] as u32
        } else {
            0
        };
        let n = b0 << 16 | b1 << 8 | b2;
        out.push(ALPH[((n >> 18) & 63) as usize] as char);
        out.push(ALPH[((n >> 12) & 63) as usize] as char);
        out.push(if i + 1 < bytes.len() {
            ALPH[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if i + 2 < bytes.len() {
            ALPH[(n & 63) as usize] as char
        } else {
            '='
        });
        i += 3;
    }
    out
}

#[allow(clippy::too_many_arguments, clippy::ptr_arg)]
pub(super) async fn dispatch_action(
    action: crate::input::engine::KeyAction,
    dash: &mut crate::ui::views::dashboard::DashboardState,
    uri: &mut String,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
    ex_line: &mut Option<crate::input::textinput::TextInput>,
    dry_run: bool,
    demo: bool,
) -> bool {
    use crate::command::builders::lifecycle;
    use crate::input::engine::KeyAction as A;
    match action {
        A::MoveDown => dash.move_selection(1),
        A::MoveUp => dash.move_selection(-1),
        A::GoTop => dash.selection = 0,
        A::GoBottom => dash.selection = dash.visible_rows().len().saturating_sub(1),
        A::HalfDown => dash.move_selection(10),
        A::HalfUp => dash.move_selection(-10),
        A::SortCycle => dash.cycle_sort(),
        A::ChipCycle => dash.cycle_chip(),
        A::ChipCycleBack => {
            for _ in 0..4 {
                dash.cycle_chip();
            }
        }
        A::Mark => dash.toggle_mark(),
        A::Visual => {
            dash.toggle_visual();
            dash.message = if dash.visual_anchor.is_some() {
                String::from("-- VISUAL --  j/k extend · V or ⎋ to finish · actions apply to the marked rows")
            } else {
                format!("{} marked", dash.marks.len())
            };
        }
        A::NextMatch | A::PrevMatch => {
            if dash.filter.is_empty() {
                dash.message = String::from("✗ no filter: use / first");
            } else {
                dash.next_match(if action == A::NextMatch { 1 } else { -1 });
            }
        }
        A::Redraw => dash.redraw_requested = true,
        A::WindowCycle => {
            let label = crate::metrics::store::cycle_window();
            dash.message = format!("history window: {label}");
        }
        A::Filter => {
            *ex_line = Some(crate::input::textinput::TextInput::filter(&dash.filter));
        }
        A::Ex => *ex_line = Some(crate::input::textinput::TextInput::new()),
        A::Start => {
            let t = targets(dash);
            run_plan_on_targets(uri, &t, lifecycle::start, dry_run, demo, dash, history, messages).await;
        }
        A::Shutdown => {
            // Only running guests can honour an ACPI request.
            let (t, skipped): (Vec<String>, Vec<String>) = targets(dash).into_iter().partition(|n| {
                dash.rows
                    .iter()
                    .any(|r| &r.name == n && r.state == crate::model::DomainState::Running)
            });
            if t.is_empty() {
                dash.message = format!("✗ {} is not running", skipped.join(", "));
                messages.push(dash.message.clone());
            } else {
                run_plan_on_targets(
                    uri,
                    &t,
                    lifecycle::shutdown,
                    dry_run,
                    demo,
                    dash,
                    history,
                    messages,
                )
                .await;
                if !demo && !dry_run && dash.message.starts_with('✓') {
                    for n in &t {
                        dash.track_shutdown(n);
                    }
                    dash.message = format!(
                        "⏻ Shutdown requested for {} (ACPI) — waiting for the guest…  ── virsh shutdown {}",
                        t.join(", "),
                        t.first().map_or("", String::as_str)
                    );
                }
            }
        }
        A::Destroy => ask_confirm(confirm, "⚠ Destroy domain", "destroy", &targets(dash), |d| {
            lifecycle::destroy(d)
        }),
        A::Reboot => {
            let t = targets(dash);
            run_plan_on_targets(uri, &t, lifecycle::reboot, dry_run, demo, dash, history, messages).await;
        }
        A::Reset => ask_confirm(
            confirm,
            "⚠ Reset domain",
            "reset",
            &targets(dash),
            lifecycle::reset,
        ),
        A::Pause => {
            let t = targets(dash);
            if dash.selected().state == crate::model::DomainState::Paused {
                run_plan_on_targets(uri, &t, lifecycle::resume, dry_run, demo, dash, history, messages).await;
            } else {
                run_plan_on_targets(uri, &t, lifecycle::pause, dry_run, demo, dash, history, messages).await;
            }
        }
        A::ManagedSave => {
            let t = targets(dash);
            run_plan_on_targets(
                uri,
                &t,
                lifecycle::managed_save,
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        A::Autostart => {
            let t = targets(dash);
            let enable = !dash.selected().autostart;
            run_plan_on_targets(
                uri,
                &t,
                move |d| lifecycle::autostart(d, enable),
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        A::SendKey => {
            let t = targets(dash);
            run_plan_on_targets(
                uri,
                &t,
                lifecycle::send_key,
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        A::Undefine => {
            let t = targets(dash);
            ask_confirm(confirm, "⚠ Undefine domain", "undefine", &t, |d| {
                lifecycle::undefine(d, false, false, false)
            });
            // Undefine is irreversible: require typing the name(s) (DESIGN.md §1.4).
            if let Some((c, _, _)) = confirm.as_mut() {
                c.type_name = Some(t.join(", "));
            }
        }
        A::YankName => {
            let name = dash.selected().name.clone();
            copy_text(&name);
            dash.message = format!("✓ Copied name {name}");
        }
        A::YankUuid => {
            let name = dash.selected().name.clone();
            dash.message = match dash
                .configs
                .get(&name)
                .map(|c| c.uuid.clone())
                .filter(|u| !u.is_empty())
            {
                Some(uuid) => {
                    copy_text(&uuid);
                    format!("✓ Copied UUID {uuid}")
                }
                None if demo => {
                    copy_text("6f1d2c3b-4a5e-4f60-9b7a-0d1e2f3a4b5c");
                    String::from("✓ Copied UUID 6f1d2c3b-4a5e-4f60-9b7a-0d1e2f3a4b5c")
                }
                None => format!("✗ UUID of {name} not loaded yet"),
            };
        }
        A::YankIp => {
            let name = dash.selected().name.clone();
            let ip = if demo {
                Some(String::from("192.168.122.48"))
            } else {
                dash.ips.get(&name).cloned().filter(|ip| ip != "—")
            };
            dash.message = match ip {
                Some(ip) => {
                    copy_text(&ip);
                    format!("✓ Copied IP {ip}")
                }
                None => format!("✗ no IP known for {name} (needs a DHCP lease or qemu-ga)"),
            };
        }
        A::YankCmd => {
            if let Some(last) = history.last() {
                copy_text(last);
            }
        }
        A::ExportXml => {
            let name = dash.selected().name.clone();
            dash.message = export_xml(uri, &name, dry_run || demo).await;
            messages.push(dash.message.clone());
        }
        A::Rename => {
            let sel = dash.selected().name.clone();
            *ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                "rename {sel} "
            )));
        }
        A::Clone => {
            let sel = dash.selected().name.clone();
            let build: crate::command::plan::PlanBuilder = std::sync::Arc::new(move |d: &str| {
                crate::command::builders::lifecycle::clone_domain(d, &format!("{d}-clone"))
            });
            let commands = build(&sel)
                .steps
                .iter()
                .map(crate::command::display::display)
                .collect();
            *confirm = Some((
                crate::ui::overlays::confirm::Confirm {
                    title: String::from("Clone domain"),
                    question: format!("clone {sel} to {sel}-clone?"),
                    commands,
                    confirm_label: String::from("y clone"),
                    type_name: None,
                    typed: String::new(),
                },
                build,
                vec![sel],
            ));
        }
        A::Console => external::console(uri, &dash.selected().name),
        A::Viewer => open_viewer(uri, &dash.selected().name.clone()).await,
        A::Refresh if !demo => refresh_live(uri, dash).await,
        A::Refresh => {}
        _ => {}
    }
    false
}

/// Stage a confirm modal for a destructive plan builder.
pub(super) fn ask_confirm(
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
    title: &str,
    verb: &str,
    targets: &[String],
    build: impl Fn(&str) -> crate::command::plan::CommandPlan + Send + Sync + 'static,
) {
    if targets.is_empty() {
        return;
    }
    let plan = build(targets.first().map(|s| s.as_str()).unwrap_or(""));
    let commands = plan.steps.iter().map(crate::command::display::display).collect();
    *confirm = Some((
        crate::ui::overlays::confirm::Confirm {
            title: title.to_string(),
            question: format!("{} {}?", capitalize(verb), targets.join(", ")),
            commands,
            confirm_label: format!("y {verb}"),
            type_name: None,
            typed: String::new(),
        },
        std::sync::Arc::new(build),
        targets.to_vec(),
    ));
}

pub(super) fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Open the configured graphical viewer (Settings › Console & viewer).
///
/// `virt-viewer` connects by domain name; `remote-viewer` needs the display
/// URI, which `virsh domdisplay` provides.
pub(super) async fn open_viewer(uri: &str, domain: &str) {
    let viewer = crate::config::runtime().console.viewer;
    if viewer == "remote-viewer"
        && let Ok(display) = crate::backend::virsh::exec::run(uri, &["domdisplay", domain]).await
    {
        let display = display.trim().to_string();
        if !display.is_empty() {
            detached_run(&["remote-viewer", &display]);
            return;
        }
    }
    detached_run(&["virt-viewer", "-c", uri, domain]);
}

/// Pre-filled `:` command for hardware/migration shortcuts (`␣da`, `␣dr`,
/// `␣ia`, `␣il`, `␣M`): the exact virsh command, ready to complete and run.
pub(super) fn prefill_command(
    action: crate::input::engine::KeyAction,
    domain: &str,
    cfg: Option<&crate::model::DomainConfig>,
    running: bool,
) -> Option<String> {
    use crate::input::engine::KeyAction as A;
    let disks: Vec<&crate::model::DiskInfo> = cfg
        .map(|c| c.disks.iter().filter(|d| d.device != "cdrom").collect())
        .unwrap_or_default();
    let live = if running { " --live" } else { "" };
    Some(match action {
        A::DiskAttach => {
            // Next free virtio target (vdb, vdc, …).
            let used: Vec<&str> = cfg
                .map(|c| c.disks.iter().map(|d| d.target.as_str()).collect())
                .unwrap_or_default();
            let target = ('b'..='z')
                .map(|c| format!("vd{c}"))
                .find(|t| !used.contains(&t.as_str()))
                .unwrap_or_else(|| String::from("vdz"));
            format!(
                "attach-disk {domain} /var/lib/libvirt/images/{domain}-{target}.qcow2 {target} \
                 --driver qemu --subdriver qcow2 --targetbus virtio --config{live}"
            )
        }
        A::DiskResize => match disks.first() {
            Some(d) if running => format!("blockresize {domain} {} ", d.target),
            Some(d) => format!("vol-resize {} ", d.source),
            None => return None,
        },
        A::NicAttach => format!("attach-interface {domain} network default --model virtio --config{live}"),
        A::NicLink => {
            let nic = cfg.and_then(|c| c.nics.first())?;
            let iface = if nic.target.is_empty() {
                nic.mac.clone()
            } else {
                nic.target.clone()
            };
            format!("domif-setlink {domain} {iface} down")
        }
        A::Migrate => format!("migrate {domain} qemu+ssh://HOST/system --live --persistent --verbose"),
        _ => return None,
    })
}

/// `␣x`: `virsh dumpxml <name>` into `./<name>.xml` (never overwrites).
pub(super) async fn export_xml(uri: &str, name: &str, dry_run: bool) -> String {
    let shown = format!("virsh dumpxml {name} > ./{name}.xml");
    if dry_run {
        return format!("[dry-run] export {name}  ── {shown}");
    }
    let path = std::path::PathBuf::from(format!("{name}.xml"));
    let xml = match crate::backend::virsh::exec::run(uri, &["dumpxml", name]).await {
        Ok(x) => x,
        Err(e) => return format!("✗ {e}  ── virsh dumpxml {name}"),
    };
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path);
    match file.and_then(|mut f| std::io::Write::write_all(&mut f, xml.as_bytes())) {
        Ok(()) => {
            let shown_path = std::fs::canonicalize(&path).unwrap_or(path);
            format!("✓ Exported {name} to {}  ── {shown}", shown_path.display())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            format!("✗ {} already exists, not overwritten", path.display())
        }
        Err(e) => format!("✗ cannot write {}: {e}", path.display()),
    }
}
