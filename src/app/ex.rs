//! Ex command line (`:`) dispatcher.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Run an ex line. Returns true when the app should quit.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_ex_line(
    line: &str,
    uri: &mut String,
    dash: &mut crate::ui::views::dashboard::DashboardState,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
    dry_run: bool,
    demo: bool,
    help_filter: &mut Option<String>,
    config: &mut Config,
    theme: &mut Theme,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
) -> bool {
    use crate::command::builders::hardware as hardware_ext;
    use crate::command::builders::lifecycle;
    let active = |dash: &crate::ui::views::dashboard::DashboardState, d: &str| {
        dash.rows.iter().any(|r| {
            r.name == d
                && matches!(
                    r.state,
                    crate::model::DomainState::Running | crate::model::DomainState::Paused
                )
        })
    };
    match crate::input::ex::parse(line) {
        Ok(crate::input::ex::ExCommand::Quit | crate::input::ex::ExCommand::QuitAll) => return true,
        Ok(crate::input::ex::ExCommand::Help(topic)) => {
            *help_filter = Some(topic);
        }
        Ok(crate::input::ex::ExCommand::Set(key, value)) => {
            dash.message = apply_setting(uri, config, theme, &key, &value);
        }
        Ok(crate::input::ex::ExCommand::Start(d)) => {
            preview_targets(
                uri,
                &[d],
                lifecycle::start,
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        Ok(crate::input::ex::ExCommand::Rename(d, n)) => {
            preview_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| lifecycle::rename(t, &n),
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        Ok(crate::input::ex::ExCommand::Clone(d, n)) => {
            if let Some(problem) = clone_problem(dash, &d, &n) {
                dash.message = problem;
                return false;
            }
            let on = uri.clone();
            preview_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| lifecycle::clone_on(&on, t, &n),
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        Ok(crate::input::ex::ExCommand::Desc(d, _text)) => {
            dash.message = if demo || dry_run {
                format!("[dry-run] Described {d}  ── virsh define /tmp/vt-{d}.xml")
            } else {
                "✗ Description request was not submitted".into()
            };
        }
        Ok(crate::input::ex::ExCommand::AttachDisk(d, src, target)) => {
            let Some(format) = hardware_ext::image_format(&src) else {
                dash.message = format!(
                    "✗ cannot tell the image format of {src}: run :attach-disk {d} {src} {target} --driver qemu --subdriver <qcow2|raw> --config"
                );
                return false;
            };
            let live = active(dash, &d);
            preview_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| hardware_ext::attach_disk_formatted(t, &src, &target, &format, live, true),
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        Ok(crate::input::ex::ExCommand::DetachDisk(d, target)) => {
            preview_targets(
                uri,
                std::slice::from_ref(&d),
                {
                    let live = active(dash, &d);
                    move |t| hardware_ext::detach_disk(t, &target, live)
                },
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        Ok(crate::input::ex::ExCommand::AttachNic(d, source, model)) => {
            preview_targets(
                uri,
                std::slice::from_ref(&d),
                {
                    let live = active(dash, &d);
                    move |t| hardware_ext::attach_nic(t, &source, &model, None, live)
                },
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        Ok(crate::input::ex::ExCommand::Resize(d, target, size)) => {
            let Some(kib) = crate::ui::views::detail::hardware::size_to_kib(&size) else {
                dash.message = format!("✗ invalid size `{size}`: use a unit (512M, 8G, 1.5T)");
                return false;
            };
            let size = crate::ui::views::detail::hardware::kib_arg(kib);
            preview_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| hardware_ext::block_resize(t, &target, &size),
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        Ok(crate::input::ex::ExCommand::Media(d, target, iso)) => {
            // `:attach-iso <vm> <iso>` without a target: use the VM's CDROM.
            let target = if target.is_empty() {
                if demo {
                    String::from("sda")
                } else {
                    dash.message = format!("✗ CD-ROM details for {d} are not loaded");
                    return false;
                }
            } else {
                target
            };
            preview_targets(
                uri,
                std::slice::from_ref(&d),
                {
                    let live = active(dash, &d);
                    move |t| hardware_ext::change_media(t, &target, iso.as_deref(), live)
                },
                dry_run,
                demo,
                dash,
                history,
                messages,
            );
        }
        Ok(crate::input::ex::ExCommand::AttachHostdev(d, node)) => {
            dash.message = if demo || dry_run {
                format!("[dry-run] Attached {node} on {d}  ── virsh attach-device {d} /tmp/vt-hostdev.xml")
            } else {
                "✗ Host device request was not submitted".into()
            };
        }
        Ok(crate::input::ex::ExCommand::Messages) => {
            dash.message = messages.last().cloned().unwrap_or_default();
        }
        Ok(crate::input::ex::ExCommand::Connect(u)) => {
            *uri = u;
        }
        Ok(crate::input::ex::ExCommand::Jump(n)) => {
            dash.selection = (n.saturating_sub(1) as usize).min(dash.visible_rows().len().saturating_sub(1));
        }
        Ok(crate::input::ex::ExCommand::Sort(col)) => {
            dash.sort = match col.as_str() {
                "name" => crate::ui::views::dashboard::Sort::Name,
                "cpu" => crate::ui::views::dashboard::Sort::Cpu,
                "mem" => crate::ui::views::dashboard::Sort::Mem,
                "uptime" => crate::ui::views::dashboard::Sort::Uptime,
                _ => crate::ui::views::dashboard::Sort::State,
            };
        }
        Ok(crate::input::ex::ExCommand::Theme(name)) => {
            dash.message = apply_setting(uri, config, theme, "theme", &name);
        }
        Ok(crate::input::ex::ExCommand::Shutdown(d)) => {
            let prev = dash.selected().name.clone();
            // `:shutdown <vm>` names a domain: ignore the filter/chip for it.
            let (filter, chip) = (std::mem::take(&mut dash.filter), dash.chip);
            dash.chip = crate::ui::views::dashboard::Chip::All;
            dash.select_name(&d);
            if dash.selected().name == d {
                let marks = std::mem::take(&mut dash.marks);
                dispatch_action(
                    crate::input::engine::KeyAction::Shutdown,
                    dash,
                    uri,
                    history,
                    messages,
                    confirm,
                    &mut None,
                    dry_run,
                    demo,
                );
                dash.marks = marks;
            } else {
                dash.message = format!("✗ no domain named {d}");
            }
            dash.filter = filter;
            dash.chip = chip;
            dash.select_name(&prev);
        }
        Ok(crate::input::ex::ExCommand::Destroy(d)) => {
            ask_confirm(
                confirm,
                crate::ui::overlays::confirm::ConfirmKind::DomainDestroy,
                "⚠ Destroy domain",
                "destroy",
                &[d],
                crate::command::builders::lifecycle::destroy,
            );
        }
        Ok(crate::input::ex::ExCommand::RawVirsh(raw) | crate::input::ex::ExCommand::Shell(raw)) => {
            // `:<virsh-subcommand> args` and `:!virsh args` run virsh directly (no shell).
            let raw = raw.trim();
            let raw = raw.strip_prefix("virsh ").unwrap_or(raw);
            let Some(argv) = shlex::split(raw).filter(|a| !a.is_empty()) else {
                dash.message = String::from("✗ cannot parse the command (unbalanced quotes?)");
                return false;
            };
            if argv[0].starts_with('-') || argv[0] == "-c" {
                dash.message = String::from("✗ the connection is set with :connect, not -c");
                return false;
            }
            let step = crate::command::plan::CommandStep {
                program: String::from("virsh"),
                argv: argv.clone(),
                stdin: None,
            };
            let shown = crate::command::display::display(&step);
            if crate::ui::overlays::confirm::Confirm::destructive_argv(&argv) {
                let plan = crate::command::plan::CommandPlan {
                    steps: vec![step],
                    summary: format!("Ran {}", argv[0]),
                };
                let title = format!("⚠ virsh {}", argv[0]);
                ask_confirm(
                    confirm,
                    crate::ui::overlays::confirm::ConfirmKind::Always,
                    &title,
                    &argv[0],
                    &[argv.get(1).cloned().unwrap_or_default()],
                    move |_| plan.clone(),
                );
                return false;
            }
            history.push(&shown);
            if dry_run || demo {
                dash.message = format!("[dry-run] {shown}");
                return false;
            }
            dash.message = "✗ Raw command request was not submitted".into();
        }
        Ok(other) => {
            dash.message = format!("✗ :{other:?} is not available here");
        }
        Err(e) => dash.message = format!("✗ {e}"),
    }
    false
}
