//! Domain table actions: lifecycle plans, confirmations, clipboard.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Targets: the marked rows that are visible (bulk), else the selection.
/// Marks hidden by the filter or chip are never acted on.
pub(super) fn targets(dash: &crate::ui::views::dashboard::DashboardState) -> Vec<String> {
    if dash.rows.is_empty() {
        return vec![];
    }
    let visible = dash.visible_rows();
    let marked: Vec<String> = visible
        .iter()
        .filter(|row| dash.marks.contains(&row.name))
        .map(|row| row.name.clone())
        .collect();
    if marked.is_empty() {
        visible
            .is_empty()
            .then(Vec::new)
            .unwrap_or_else(|| vec![dash.selected().name.clone()])
    } else {
        let mut v = marked;
        v.sort();
        v
    }
}

/// State of a dashboard row by name.
pub(super) fn row_state(
    dash: &crate::ui::views::dashboard::DashboardState,
    name: &str,
) -> Option<(crate::model::DomainState, bool)> {
    dash.rows
        .iter()
        .find(|row| row.name == name)
        .map(|row| (row.state, row.autostart))
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
                row.state_label = s.label();
            }
        }
    }
}

/// Re-fetch live domains after a mutation.
pub(super) fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}

/// Copy to the clipboard. The `arboard::Clipboard` is kept for the whole
/// session: on X11/Wayland the owner must stay alive or the content is lost.
pub(super) fn copy_text(text: &str) {
    static CLIPBOARD: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
    let copied = {
        let mut guard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = arboard::Clipboard::new().ok();
        }
        guard
            .as_mut()
            .is_some_and(|clipboard| clipboard.set_text(text.to_string()).is_ok())
    };
    if !copied {
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

/// Preview target plans in demo or dry-run mode; live plans are submitted as jobs.
#[allow(clippy::too_many_arguments)]
pub(super) fn preview_targets(
    _uri: &str,
    targets: &[String],
    build: impl Fn(&str) -> crate::command::plan::CommandPlan,
    dry_run: bool,
    demo: bool,
    dash: &mut crate::ui::views::dashboard::DashboardState,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) {
    if !demo && !dry_run {
        dash.message = "✗ Domain request was not submitted".into();
        return;
    }
    for target in targets {
        let plan = build(target);
        let first = plan
            .steps
            .first()
            .map(crate::command::display::display)
            .unwrap_or_default();
        history.push(&first);
        let prefix = if demo { "✓" } else { "[dry-run]" };
        dash.message = format!("{prefix} {}  ── {first}", plan.summary);
        messages.push_action(dash.message.clone(), target, true);
        if demo {
            apply_demo_transition(dash, target, &plan.summary);
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::ptr_arg)]
pub(super) fn dispatch_action(
    action: crate::input::engine::KeyAction,
    dash: &mut crate::ui::views::dashboard::DashboardState,
    uri: &mut String,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
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
        // In VISUAL mode `gg` / `G` extend the selection, as in vim.
        A::GoTop => {
            dash.selection = 0;
            dash.update_visual();
        }
        A::GoBottom => {
            dash.selection = dash.visible_rows().len().saturating_sub(1);
            dash.update_visual();
        }
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
        A::Start => {
            let t = targets(dash);
            preview_targets(uri, &t, lifecycle::start, dry_run, demo, dash, history, messages);
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
                messages.push_action(dash.message.clone(), &skipped.join(", "), false);
            } else {
                preview_targets(
                    uri,
                    &t,
                    lifecycle::shutdown,
                    dry_run,
                    demo,
                    dash,
                    history,
                    messages,
                );
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
        A::Destroy => ask_confirm(
            confirm,
            crate::ui::overlays::confirm::ConfirmKind::DomainDestroy,
            "⚠ Destroy domain",
            "destroy",
            &targets(dash),
            lifecycle::destroy,
        ),
        A::Reboot => {
            let t = targets(dash);
            preview_targets(uri, &t, lifecycle::reboot, dry_run, demo, dash, history, messages);
        }
        A::Reset => ask_confirm(
            confirm,
            crate::ui::overlays::confirm::ConfirmKind::DomainReset,
            "⚠ Reset domain",
            "reset",
            &targets(dash),
            lifecycle::reset,
        ),
        A::Pause => {
            // Per target: paused guests resume, the others pause.
            let t = targets(dash);
            let (paused, running): (Vec<String>, Vec<String>) = t.into_iter().partition(|name| {
                row_state(dash, name).is_some_and(|(s, _)| s == crate::model::DomainState::Paused)
            });
            if !paused.is_empty() {
                preview_targets(
                    uri,
                    &paused,
                    lifecycle::resume,
                    dry_run,
                    demo,
                    dash,
                    history,
                    messages,
                );
            }
            if !running.is_empty() {
                preview_targets(
                    uri,
                    &running,
                    lifecycle::pause,
                    dry_run,
                    demo,
                    dash,
                    history,
                    messages,
                );
            }
        }
        A::ManagedSave => ask_confirm(
            confirm,
            crate::ui::overlays::confirm::ConfirmKind::Always,
            "Managed-save domain",
            "managed-save",
            &targets(dash),
            lifecycle::managed_save,
        ),
        A::Autostart => {
            // Toggle each target from its own state.
            let t = targets(dash);
            let states: std::collections::HashMap<String, bool> = t
                .iter()
                .map(|name| (name.clone(), row_state(dash, name).is_some_and(|(_, a)| a)))
                .collect();
            preview_targets(
                uri,
                &t,
                move |d| lifecycle::autostart(d, !states.get(d).copied().unwrap_or(false)),
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        A::SendKey => {
            let t = targets(dash);
            preview_targets(
                uri,
                &t,
                lifecycle::send_key,
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        A::Undefine => {
            let t = targets(dash);
            ask_confirm(
                confirm,
                crate::ui::overlays::confirm::ConfirmKind::DomainUndefine,
                "⚠ Undefine domain",
                "undefine",
                &t,
                |d| lifecycle::undefine(d, false, false, true),
            );
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
            dash.message = match history.last() {
                Some(last) => {
                    copy_text(last);
                    format!("✓ Copied command  ── {last}")
                }
                None => String::from("✗ no command run yet"),
            };
        }
        A::ExportXml => {
            let name = dash.selected().name.clone();
            dash.message = format!("[dry-run] export {name}  ── virsh dumpxml {name} > ./{name}.xml");
            messages.push_action(dash.message.clone(), &name, true);
        }
        A::Rename => {
            let sel = dash.selected().name.clone();
            *ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                "rename {sel} "
            )));
        }
        A::Clone => {
            // The name is edited on the ex line (`:clone <vm> <new-name>`).
            let sel = dash.selected().name.clone();
            if !sel.is_empty() {
                *ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                    "clone {sel} {sel}-clone"
                )));
            }
        }
        A::Console if demo || dry_run => dash.message = "[dry-run] Open console".into(),
        A::Console => {
            if let Some(message) = external::console(uri, &dash.selected().name) {
                messages.push(message.clone());
                dash.message = message;
            }
        }
        A::Viewer if demo || dry_run => dash.message = "[dry-run] Open viewer".into(),
        A::Viewer => dash.message = "✗ Viewer request was not submitted".into(),
        A::Refresh if !demo => {}
        A::Refresh => {}
        _ => {}
    }
    false
}

/// Stage a confirm modal for a destructive plan builder.
pub(super) fn ask_confirm(
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
    kind: crate::ui::overlays::confirm::ConfirmKind,
    title: &str,
    verb: &str,
    targets: &[String],
    build: impl Fn(&str) -> crate::command::plan::CommandPlan + Send + Sync + 'static,
) {
    if targets.is_empty() {
        return;
    }
    let commands: Vec<String> = targets
        .iter()
        .flat_map(|t| {
            build(t)
                .steps
                .iter()
                .map(crate::command::display::display)
                .collect::<Vec<_>>()
        })
        .collect();
    *confirm = Some((
        crate::ui::overlays::confirm::Confirm {
            kind,
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

/// `:clone` refusal reason: virt-clone needs a shut-off source (current
/// virt-clone refuses paused domains too: "Domain to clone must be shutoff."),
/// and a free, valid name.
pub(super) fn clone_problem(
    dash: &crate::ui::views::dashboard::DashboardState,
    domain: &str,
    new_name: &str,
) -> Option<String> {
    match row_state(dash, domain) {
        None => return Some(format!("✗ no domain named {domain}")),
        Some((crate::model::DomainState::ShutOff, _)) => {}
        Some((state, _)) => {
            return Some(format!(
                "✗ {domain} is {}: shut it down before cloning",
                state.label()
            ));
        }
    }
    let existing: Vec<&str> = dash.rows.iter().map(|r| r.name.as_str()).collect();
    (!crate::ui::overlays::wizard::valid_name(new_name, &existing))
        .then(|| format!("✗ `{new_name}` is not a valid new domain name (or already exists)"))
}
