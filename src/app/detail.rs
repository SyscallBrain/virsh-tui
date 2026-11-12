//! Domain detail view: hardware editor, snapshots, XML editing.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Hardware-tab keys (NORMAL + INSERT). Returns true when consumed.
pub(super) fn handle_hardware_key(
    key: KeyEvent,
    detail: &mut Option<ui::views::detail::DetailState>,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
    ex_line: &mut Option<crate::input::textinput::TextInput>,
    engine: &mut crate::input::engine::KeyEngine,
) -> bool {
    use crate::input::key::KeySeq;
    let Some(d) = detail else { return false };
    if d.tab != crate::ui::views::detail::DetailTab::Hardware {
        return false;
    }
    // INSERT mode: raw editing, bypass the engine (except Esc).
    if d.hw_insert {
        if key.code == KeyCode::Esc {
            d.hw_insert = false;
            d.message.clear();
            engine.feed(&KeySeq::parse("Esc"));
            return true;
        }
        let dev_id = d.hw.selected_device().id.to_string();
        match key.code {
            KeyCode::Enter | KeyCode::Tab => {
                if let Some(form) = d.hw.forms.get_mut(dev_id.as_str()) {
                    form.move_focus(1);
                }
                return true;
            }
            KeyCode::BackTab => {
                if let Some(form) = d.hw.forms.get_mut(dev_id.as_str()) {
                    form.move_focus(-1);
                }
                return true;
            }
            KeyCode::Backspace => {
                edit_focused(&mut d.hw, &dev_id, EditOp::Pop);
                return true;
            }
            KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) && (c == 'a' || c == 'x') => {
                edit_focused(&mut d.hw, &dev_id, EditOp::Adjust(if c == 'a' { 1 } else { -1 }));
                return true;
            }
            KeyCode::Char(c) if key.modifiers.is_empty() => {
                let is_number =
                    d.hw.forms
                        .get(dev_id.as_str())
                        .and_then(|f| f.fields.get(f.focus))
                        .is_some_and(|f| {
                            matches!(
                                f.kind,
                                crate::ui::widgets::form::FieldKind::Number { .. }
                                    | crate::ui::widgets::form::FieldKind::Size
                            )
                        });
                if is_number && matches!(c, 'h' | 'l' | 'H' | 'L') {
                    let delta = match c {
                        'h' => -1,
                        'l' => 1,
                        'H' => -10,
                        'L' => 10,
                        _ => 0,
                    };
                    edit_focused(&mut d.hw, &dev_id, EditOp::Adjust(delta));
                } else {
                    edit_focused(&mut d.hw, &dev_id, EditOp::Push(c));
                }
                return true;
            }
            _ => return false,
        }
    }
    // NORMAL mode hardware keys.
    let tok = KeySeq::from_event(&key);
    match tok.token() {
        "j" => {
            d.hw.selected = (d.hw.selected + 1).min(d.hw.devices.len() - 1);
            true
        }
        "k" => {
            d.hw.selected = d.hw.selected.saturating_sub(1);
            true
        }
        "i" => {
            d.hw_insert = true;
            d.message = String::from("-- INSERT --");
            true
        }
        "a" => {
            let dev = d.hw.selected_device().id.to_string();
            let prefix = if dev.starts_with("disk") {
                "attach-disk <src> <target>"
            } else if dev.starts_with("nic") {
                "attach-nic <source> <model>"
            } else {
                "attach-hostdev "
            };
            *ex_line = Some(crate::input::textinput::TextInput::with(prefix));
            true
        }
        "d" => {
            stage_detach(d, confirm);
            true
        }
        "J" => {
            boot_reorder(d, 1);
            true
        }
        "K" => {
            boot_reorder(d, -1);
            true
        }
        "Tab" => {
            let dev = d.hw.selected_device().id.to_string();
            if let Some(form) = d.hw.forms.get_mut(dev.as_str()) {
                form.move_focus(1);
            }
            true
        }
        "S-Tab" => {
            let dev = d.hw.selected_device().id.to_string();
            if let Some(form) = d.hw.forms.get_mut(dev.as_str()) {
                form.move_focus(-1);
            }
            true
        }
        "u" => {
            let dev = d.hw.selected_device().id.to_string();
            undo_focused(d, &dev);
            true
        }
        "y" => {
            let text =
                d.hw.will_run(&d.domain)
                    .iter()
                    .flat_map(|p| p.steps.iter().map(crate::command::display::display))
                    .collect::<Vec<_>>()
                    .join("\n");
            copy_text(&text);
            d.message = String::from("copied will-run commands");
            true
        }
        _ => false,
    }
}

/// Focused-field edit operation (INSERT mode).
pub(super) enum EditOp {
    Pop,
    Push(char),
    Adjust(i64),
}

/// Apply an edit op to the focused field and sync pending tracking.
pub(super) fn edit_focused(
    hw: &mut crate::ui::views::detail::hardware::HardwareState,
    dev: &str,
    op: EditOp,
) {
    let dirty_id = if let Some(form) = hw.forms.get_mut(dev) {
        if let Some(f) = form.fields.get_mut(form.focus) {
            match op {
                EditOp::Pop => {
                    f.current.pop();
                }
                EditOp::Push(c) => f.current.push(c),
                EditOp::Adjust(delta) => f.adjust(delta),
            }
            Some((f.id.clone(), f.dirty()))
        } else {
            None
        }
    } else {
        None
    };
    if let Some((id, dirty)) = dirty_id {
        if dirty {
            hw.explicit_insert(dev, &id);
            if dev == "cpus" && id == "max" {
                hw.auto_topology();
            }
        } else {
            hw.explicit_remove(dev, &id);
        }
    }
}

/// Undo the focused field.
pub(super) fn undo_focused(d: &mut ui::views::detail::DetailState, dev: &str) {
    let id =
        d.hw.forms
            .get(dev)
            .and_then(|f| f.fields.get(f.focus))
            .map(|f| f.id.clone())
            .unwrap_or_default();
    if !id.is_empty() {
        d.hw.undo_field(dev, &id);
    }
}

/// Reorder boot devices (J/K).
pub(super) fn boot_reorder(d: &mut ui::views::detail::DetailState, dir: i32) {
    let order = d.hw.field_value("boot", "order");
    let mut devs: Vec<String> = order
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if devs.len() < 2 {
        return;
    }
    let from = 0;
    let to = if dir > 0 { 1 } else { devs.len() - 1 };
    crate::xml::edit::move_boot_device(&mut devs, from, to);
    d.hw.set_field("boot", "order", &devs.join(","));
}

/// Stage detach confirmation for disk/NIC devices.
pub(super) fn stage_detach(
    d: &mut ui::views::detail::DetailState,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
) {
    use crate::command::builders::hardware;
    let dev = d.hw.selected_device();
    let domain = d.domain.clone();
    let (title, build): (String, crate::command::plan::PlanBuilder) = if dev.id.starts_with("disk") {
        let target = dev.name.split_whitespace().last().unwrap_or("vda").to_string();
        (
            String::from("⚠ Detach disk"),
            std::sync::Arc::new(move |d: &str| hardware::detach_disk(d, &target, true)),
        )
    } else if dev.id.starts_with("nic") {
        let mac = String::from("52:54:00:a3:1f:7c");
        (
            String::from("⚠ Detach NIC"),
            std::sync::Arc::new(move |d: &str| {
                crate::command::plan::CommandPlan::single(
                    "virsh",
                    vec![
                        "detach-interface",
                        d,
                        "network",
                        "--mac",
                        &mac,
                        "--config",
                        "--live",
                    ],
                    &format!("Detached NIC on {d}"),
                )
            }),
        )
    } else {
        d.message = String::from("d removes disks and NICs; other devices use the define path (:w)");
        return;
    };
    let commands = build(&domain)
        .steps
        .iter()
        .map(crate::command::display::display)
        .collect();
    *confirm = Some((
        crate::ui::overlays::confirm::Confirm {
            title,
            question: format!("detach {} from {}?", dev.name, domain),
            commands,
            confirm_label: String::from("y detach"),
            type_name: None,
            typed: String::new(),
        },
        build,
        vec![domain],
    ));
}

/// Snapshot modal keys: y/n/Esc plus option toggles. Returns true when consumed.
pub(super) fn handle_snap_modal(
    key: KeyEvent,
    detail: &mut Option<ui::views::detail::DetailState>,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) -> bool {
    use crate::ui::views::detail::{SnapModal, snapshots::AfterRevert};
    let Some(d) = detail else { return false };
    match d.snap_modal {
        SnapModal::None => return false,
        SnapModal::Create => match key.code {
            KeyCode::Esc => {
                d.snap_modal = SnapModal::None;
            }
            KeyCode::Enter if key.modifiers.is_empty() => {
                let domain = d.domain.clone();
                let plan = crate::command::builders::snapshot::create(
                    &domain,
                    &d.snaps.create_name.clone(),
                    &d.snaps.create_desc.clone(),
                    true,
                    false,
                );
                d.snap_modal = SnapModal::None;
                preview_snapshot(&plan, d, dry_run, demo, history, messages);
            }
            _ => return false,
        },
        SnapModal::Revert => match key.code {
            KeyCode::Esc => d.snap_modal = SnapModal::None,
            KeyCode::Char('n') if key.modifiers.is_empty() => d.snap_modal = SnapModal::None,
            KeyCode::Char('y') if key.modifiers.is_empty() => {
                let domain = d.domain.clone();
                let snap = d.snaps.selected_name().to_string();
                let plan = crate::command::builders::snapshot::revert(
                    &domain,
                    &snap,
                    d.snaps.after.flag(),
                    d.snaps.force,
                    d.snaps.safety,
                );
                d.snap_modal = SnapModal::None;
                preview_snapshot(&plan, d, dry_run, demo, history, messages);
            }
            KeyCode::Char('1') if key.modifiers.is_empty() => d.snaps.after = AfterRevert::Running,
            KeyCode::Char('2') if key.modifiers.is_empty() => d.snaps.after = AfterRevert::Paused,
            KeyCode::Char('3') if key.modifiers.is_empty() => d.snaps.after = AfterRevert::AsSaved,
            KeyCode::Char('f') if key.modifiers.is_empty() => d.snaps.force = !d.snaps.force,
            KeyCode::Char('s') if key.modifiers.is_empty() => d.snaps.safety = !d.snaps.safety,
            _ => return false,
        },
        SnapModal::Delete => match key.code {
            KeyCode::Esc => d.snap_modal = SnapModal::None,
            KeyCode::Char('n') if key.modifiers.is_empty() => d.snap_modal = SnapModal::None,
            KeyCode::Char('y') if key.modifiers.is_empty() => {
                let domain = d.domain.clone();
                let snap = d.snaps.selected_name().to_string();
                let plan = crate::command::builders::snapshot::delete(
                    &domain,
                    &snap,
                    d.snaps.del_children,
                    d.snaps.del_children_only,
                    d.snaps.del_metadata,
                );
                d.snap_modal = SnapModal::None;
                preview_snapshot(&plan, d, dry_run, demo, history, messages);
            }
            KeyCode::Char('c') if key.modifiers.is_empty() => {
                d.snaps.del_children = !d.snaps.del_children;
            }
            KeyCode::Char('o') if key.modifiers.is_empty() => {
                d.snaps.del_children_only = !d.snaps.del_children_only;
            }
            KeyCode::Char('m') if key.modifiers.is_empty() => {
                d.snaps.del_metadata = !d.snaps.del_metadata;
            }
            _ => return false,
        },
    }
    true
}

/// Open the detail view for the dashboard selection (or host selection).
/// Preview a snapshot operation; live operations are submitted as jobs.
fn preview_snapshot(
    plan: &crate::command::plan::CommandPlan,
    detail: &mut ui::views::detail::DetailState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) {
    if !demo && !dry_run {
        detail.message = "✗ Snapshot request was not submitted".into();
        return;
    }
    for step in &plan.steps {
        history.push(&crate::command::display::display(step));
    }
    let first = plan
        .steps
        .first()
        .map(crate::command::display::display)
        .unwrap_or_default();
    let prefix = if demo { "✓" } else { "[dry-run]" };
    detail.message = format!("{prefix} {}  ── {first}", plan.summary);
    messages.push_action(detail.message.clone(), &detail.domain, true);
}

pub(super) async fn load_detail(uri: &str, st: &mut ui::views::detail::DetailState) {
    st.uptime = crate::backend::virsh::host_local::domain_uptime(&st.domain).unwrap_or_else(|| "—".into());
    if let Ok(xml) = crate::backend::virsh::exec::run(uri, &["dumpxml", &st.domain, "--inactive"]).await
        && let Ok(cfg) = crate::backend::virsh::parse_xml::parse_domain_xml(&xml)
    {
        let running = st.state == crate::model::DomainState::Running;
        st.hw = ui::views::detail::hardware::HardwareState::from_config(&cfg, &xml, running);
        st.config = cfg;
        st.xml = xml;
        st.fold = ui::views::detail::XmlFold::new(st.xml.lines().count());
    }
    refresh_snapshots(uri, st).await;
}

pub(super) fn detail_initial(
    name: &str,
    dash: &ui::views::dashboard::DashboardState,
    uri: &str,
    demo: bool,
) -> ui::views::detail::DetailState {
    let mut st = ui::views::detail::DetailState::demo(name);
    st.uri = uri.to_string();
    st.connected = true;
    if !demo {
        // Never show demo data for a real domain: start from an empty config and
        // the real state; the editor is rebuilt from the inactive XML below.
        st.state = dash
            .rows
            .iter()
            .find(|r| r.name == name)
            .map_or(crate::model::DomainState::Other, |r| r.state);
        st.uptime = String::from("—");
        st.config = crate::model::DomainConfig {
            name: name.to_string(),
            ..Default::default()
        };
        st.xml = String::new();
        st.fold = ui::views::detail::XmlFold::new(0);
        st.hw = ui::views::detail::hardware::HardwareState::from_config(&st.config, "", false);
        st.snaps.rows.clear();
        st.snaps.details.clear();
        st.snaps.description_title.clear();
        st.snaps.description.clear();
        st.snaps.current = None;
        st.snaps.selected = 0;
        st.snaps.frozen = false;
    }
    st
}

/// Reload the snapshot tree (and the current snapshot) of the detail domain.
pub(super) async fn refresh_snapshots(uri: &str, d: &mut ui::views::detail::DetailState) {
    use crate::backend::{Backend, virsh::VirshBackend};
    let Ok(snaps) = VirshBackend::new(uri).snapshots(&d.domain).await else {
        return;
    };
    let current = crate::backend::virsh::exec::run(uri, &["snapshot-current", "--name", &d.domain])
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let keep = d.snaps.selected_name().to_string();
    d.snaps.rows = ui::views::detail::snapshots::build_tree(&snaps, current.as_deref());
    d.snaps.current = current;
    d.snaps.frozen = false;
    d.snaps.selected = d.snaps.rows.iter().position(|r| r.name == keep).unwrap_or(0);
}

/// Detail-view keys. Returns true when the app should quit.
pub(super) fn dispatch_detail(
    action: crate::input::engine::KeyAction,
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    uri: &str,
    dry_run: bool,
    demo: bool,
) -> bool {
    use crate::input::engine::KeyAction as A;
    use crate::ui::views::detail::{DetailTab, SnapModal};
    let Some(d) = detail else { return false };
    if d.tab == DetailTab::Snapshots && d.snap_modal == SnapModal::None {
        match action {
            A::MoveDown => {
                d.snaps.move_selection(1);
                return false;
            }
            A::MoveUp => {
                d.snaps.move_selection(-1);
                return false;
            }
            A::SnapshotNew => {
                d.snap_modal = SnapModal::Create;
                return false;
            }
            A::SnapshotRevert => {
                d.snap_modal = SnapModal::Revert;
                return false;
            }
            A::SnapshotDelete => {
                d.snap_modal = SnapModal::Delete;
                return false;
            }
            A::EditXml => {
                let snap = d.snaps.selected_name().to_string();
                if demo || dry_run {
                    d.message = format!("[dry-run] Edit snapshot {snap} XML");
                    return false;
                }
                suspend_run(&[
                    "virsh",
                    "-c",
                    uri,
                    "snapshot-edit",
                    &d.domain.clone(),
                    "--snapshotname",
                    &snap,
                ]);
                return false;
            }
            _ => {}
        }
    }
    if d.tab == DetailTab::Xml {
        // Scroll the XML (top visible line; clamped when rendering).
        let last = d.xml.lines().count().saturating_sub(1);
        let top = &mut d.xml_cursor;
        match action {
            A::MoveDown => *top = (*top + 1).min(last),
            A::MoveUp => *top = top.saturating_sub(1),
            A::HalfDown => *top = (*top + 15).min(last),
            A::HalfUp => *top = top.saturating_sub(15),
            A::GoTop => *top = 0,
            A::GoBottom => *top = last,
            _ => {}
        }
    }
    match action {
        A::NextTab => d.next_tab(),
        A::PrevTab => d.prev_tab(),
        A::GotoXml => d.tab = ui::views::detail::DetailTab::Xml,
        A::UndoField if d.tab == ui::views::detail::DetailTab::Hardware && !d.hw_insert => {
            let dev = d.hw.selected_device().id.to_string();
            if let Some(form) = d.hw.forms.get(dev.as_str())
                && let Some(field) = form.fields.get(form.focus)
            {
                let id = field.id.clone();
                d.hw.undo_field(&dev, &id);
            }
        }
        A::EditXml if d.tab == ui::views::detail::DetailTab::Xml => {
            d.message = if demo || dry_run {
                format!(
                    "[dry-run] Edited {} XML  ── virsh define /tmp/vt-{}.xml",
                    d.domain, d.domain
                )
            } else {
                "✗ XML editor request was not submitted".into()
            };
        }
        A::Console if demo || dry_run => d.message = "[dry-run] Open console".into(),
        A::Console => external::console(uri, &d.domain),
        _ => {}
    }
    let _ = view;
    false
}

/// Run an ex line in the detail view (`:w` apply, `:q!` discard, `:q` close).
#[allow(clippy::too_many_arguments)]
pub(super) fn run_detail_ex(
    line: &str,
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    uri: &mut String,
    dash: &mut ui::views::dashboard::DashboardState,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
    dry_run: bool,
    demo: bool,
    config: &mut Config,
    theme: &mut Theme,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
) -> bool {
    use crate::input::ex::ExCommand as E;
    match crate::input::ex::parse(line) {
        Ok(E::Write) => {
            if let Some(d) = detail {
                preview_hardware(d, dry_run, demo, history, messages);
            }
            false
        }
        Ok(E::Discard) => {
            if let Some(d) = detail {
                d.hw.discard_all();
                d.hw_insert = false;
                d.message = String::from("discarded pending changes");
            }
            false
        }
        Ok(E::Quit) => {
            if detail.as_ref().is_some_and(|d| d.hw.pending_count() > 0) {
                if let Some(d) = detail {
                    d.message = String::from("unsaved changes (:w apply, :q! discard)");
                }
                false
            } else {
                *detail = None;
                *view = View::Dashboard;
                false
            }
        }
        _ => run_ex_line(
            line, uri, dash, history, messages, dry_run, demo, &mut None, config, theme, confirm,
        ),
    }
}

/// Preview pending hardware changes; live changes are submitted as jobs.
fn preview_hardware(
    detail: &mut ui::views::detail::DetailState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) {
    if !demo && !dry_run {
        detail.message = "✗ Hardware request was not submitted".into();
        return;
    }
    let unsupported = detail.hw.unsupported_changes();
    if !unsupported.is_empty() {
        detail.message = format!(
            "✗ cannot apply {} yet (read-only): undo with u or :q!",
            unsupported.join(", ")
        );
        messages.push_action(detail.message.clone(), &detail.domain, false);
        return;
    }
    let plans = detail.hw.will_run(&detail.domain);
    if plans.is_empty() {
        detail.message = "nothing to apply".into();
        return;
    }
    for plan in &plans {
        let first = plan
            .steps
            .first()
            .map(crate::command::display::display)
            .unwrap_or_default();
        history.push(&first);
        detail.message = if demo {
            format!("✓ {}", plan.summary)
        } else {
            format!("[dry-run] {}  ── {first}", plan.summary)
        };
        messages.push_action(detail.message.clone(), &detail.domain, true);
    }
}
