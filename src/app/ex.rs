//! Ex command line (`:`) dispatcher.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Run an ex line. Returns true when the app should quit.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_ex_line(
    line: &str,
    uri: &mut String,
    dash: &mut crate::ui::views::dashboard::DashboardState,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
    dry_run: bool,
    demo: bool,
    help_filter: &mut Option<String>,
    config: &mut Config,
    theme: &mut Theme,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
) -> bool {
    use crate::command::builders::hardware as hardware_ext;
    use crate::command::builders::lifecycle;
    match crate::input::ex::parse(line) {
        Ok(crate::input::ex::ExCommand::Quit | crate::input::ex::ExCommand::QuitAll) => return true,
        Ok(crate::input::ex::ExCommand::Help(topic)) => {
            *help_filter = Some(topic);
        }
        Ok(crate::input::ex::ExCommand::Set(key, value)) => {
            dash.message = apply_setting(uri, config, theme, &key, &value);
        }
        Ok(crate::input::ex::ExCommand::Start(d)) => {
            run_plan_on_targets(
                uri,
                &[d],
                lifecycle::start,
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        Ok(crate::input::ex::ExCommand::Rename(d, n)) => {
            run_plan_on_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| lifecycle::rename(t, &n),
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        Ok(crate::input::ex::ExCommand::Desc(d, text)) => {
            dash.message = set_description(uri, &d, &text, dry_run, demo).await;
        }
        Ok(crate::input::ex::ExCommand::AttachDisk(d, src, target)) => {
            run_plan_on_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| hardware_ext::attach_disk(t, &src, &target, true, true),
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        Ok(crate::input::ex::ExCommand::DetachDisk(d, target)) => {
            run_plan_on_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| hardware_ext::detach_disk(t, &target, true),
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        Ok(crate::input::ex::ExCommand::AttachNic(d, source, model)) => {
            run_plan_on_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| hardware_ext::attach_nic(t, &source, &model, None, true),
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        Ok(crate::input::ex::ExCommand::Resize(d, target, size)) => {
            run_plan_on_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| hardware_ext::block_resize(t, &target, &size),
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        Ok(crate::input::ex::ExCommand::Media(d, target, iso)) => {
            // `:attach-iso <vm> <iso>` without a target: use the VM's CDROM.
            let target = if target.is_empty() {
                match cdrom_of(uri, &d).await {
                    Some((t, _, _)) => t,
                    None if demo => String::from("sda"),
                    None => {
                        dash.message = format!("✗ {d} has no CDROM drive: use ␣mi to add one");
                        return false;
                    }
                }
            } else {
                target
            };
            run_plan_on_targets(
                uri,
                std::slice::from_ref(&d),
                move |t| hardware_ext::change_media(t, &target, iso.as_deref(), true),
                dry_run,
                demo,
                dash,
                history,
                messages,
            )
            .await;
        }
        Ok(crate::input::ex::ExCommand::AttachHostdev(d, node)) => {
            dash.message = attach_hostdev(uri, &d, &node, dry_run, demo).await;
        }
        Ok(crate::input::ex::ExCommand::Messages) => {
            dash.message = messages.last().cloned().unwrap_or_default();
        }
        Ok(crate::input::ex::ExCommand::Connect(u)) => {
            *uri = u;
            if !demo {
                refresh_live(uri, dash).await;
            }
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
                )
                .await;
                dash.marks = marks;
            } else {
                dash.message = format!("✗ no domain named {d}");
            }
            dash.select_name(&prev);
        }
        Ok(crate::input::ex::ExCommand::Destroy(d)) => {
            ask_confirm(
                confirm,
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
            if crate::ui::overlays::confirm::Confirm::destructive(&argv[0]) {
                let plan = crate::command::plan::CommandPlan {
                    steps: vec![step],
                    summary: format!("Ran {}", argv[0]),
                };
                let title = format!("⚠ virsh {}", argv[0]);
                ask_confirm(
                    confirm,
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
            let args: Vec<&str> = argv.iter().map(String::as_str).collect();
            match crate::backend::virsh::exec::run(uri, &args).await {
                Ok(out) => {
                    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
                    for l in &lines {
                        messages.push(crate::model::sanitize(l));
                    }
                    dash.message = match lines.first() {
                        Some(first) if lines.len() > 1 => format!(
                            "✓ {}  ── {shown} ({} more lines in :messages)",
                            crate::model::sanitize(first.trim()),
                            lines.len() - 1
                        ),
                        Some(first) => format!("✓ {}  ── {shown}", crate::model::sanitize(first.trim())),
                        None => format!("✓ done  ── {shown}"),
                    };
                    refresh_live(uri, dash).await;
                }
                Err(e) => dash.message = format!("✗ {}", first_line(&e.to_string())),
            }
        }
        Ok(other) => {
            dash.message = format!("✗ :{other:?} is not available here");
        }
        Err(e) => dash.message = format!("✗ {e}"),
    }
    false
}
