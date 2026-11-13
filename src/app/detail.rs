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
    // F6 never overlaps field text or device actions.
    if key.code == KeyCode::F(6) {
        d.hw.review = !d.hw.review;
        d.hw.review_scroll = (0, 0);
        return true;
    }
    if d.hw.review {
        let (max_row, max_col) = d.hw.review_limits.get();
        d.hw.review_scroll.0 = d.hw.review_scroll.0.min(max_row);
        d.hw.review_scroll.1 = d.hw.review_scroll.1.min(max_col);
        let (row, col) = &mut d.hw.review_scroll;
        match key.code {
            KeyCode::Esc => d.hw.review = false,
            KeyCode::Down | KeyCode::Char('j') => *row = row.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => *row = row.saturating_sub(1),
            KeyCode::PageDown => *row = row.saturating_add(10),
            KeyCode::PageUp => *row = row.saturating_sub(10),
            KeyCode::Right | KeyCode::Char('l') => *col = col.saturating_add(4),
            KeyCode::Left | KeyCode::Char('h') => *col = col.saturating_sub(4),
            KeyCode::Home => d.hw.review_scroll = (0, 0),
            KeyCode::End => d.hw.review_scroll = (max_row, 0),
            KeyCode::Char('y') => {
                let text =
                    d.hw.will_run(&d.domain)
                        .iter()
                        .flat_map(|p| p.steps.iter().map(crate::command::display::display))
                        .collect::<Vec<_>>()
                        .join("\n");
                copy_text(&text);
                d.message = String::from("copied will-run commands");
            }
            _ => {}
        }
        d.hw.review_scroll.0 = d.hw.review_scroll.0.min(max_row);
        d.hw.review_scroll.1 = d.hw.review_scroll.1.min(max_col);
        return true;
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
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                use crate::ui::widgets::form::FieldKind;
                let kind =
                    d.hw.forms
                        .get(dev_id.as_str())
                        .and_then(|f| f.fields.get(f.focus))
                        .map(|f| f.kind.clone());
                let is_number = matches!(kind, Some(FieldKind::Number { .. }));
                let is_size = matches!(kind, Some(FieldKind::Size));
                // Numbers take digits only; sizes digits, `.` and a binary unit.
                let accepted = match kind {
                    Some(FieldKind::Number { .. }) => c.is_ascii_digit(),
                    Some(FieldKind::Size) => {
                        c.is_ascii_digit()
                            || matches!(c, '.' | 'K' | 'M' | 'G' | 'T' | 'k' | 'm' | 'g' | 't' | 'i' | 'B')
                    }
                    _ => true,
                };
                if is_number && matches!(c, 'h' | 'l' | 'H' | 'L') {
                    let delta = match c {
                        'h' => -1,
                        'l' => 1,
                        'H' => -10,
                        'L' => 10,
                        _ => 0,
                    };
                    edit_focused(&mut d.hw, &dev_id, EditOp::Adjust(delta));
                } else if accepted {
                    edit_focused(&mut d.hw, &dev_id, EditOp::Push(c));
                } else if is_size || is_number {
                    d.message = String::from("-- INSERT --  digits only (sizes take a unit: 512M, 8G)");
                }
                return true;
            }
            // INSERT owns the keyboard until Esc.
            _ => return true,
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
            use crate::input::engine::KeyAction as A;
            let dev = d.hw.selected_device().id.to_string();
            let active = matches!(
                d.state,
                crate::model::DomainState::Running | crate::model::DomainState::Paused
            );
            let line = if dev.starts_with("disk") {
                prefill_command(A::DiskAttach, &d.domain, Some(&d.config), active)
            } else if dev.starts_with("nic") {
                prefill_command(A::NicAttach, &d.domain, Some(&d.config), active)
            } else {
                Some(format!("attach-hostdev {} ", d.domain))
            };
            if let Some(line) = line {
                *ex_line = Some(crate::input::textinput::TextInput::with(&line));
            }
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
        "l" | "h" if d.hw.selected_device().id == "boot" => {
            boot_cursor_next(d, if tok.token() == "l" { 1 } else { -1 });
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
    // Move the entry under the boot cursor one place; the cursor follows it.
    let from = d.hw.boot_cursor.min(devs.len() - 1);
    let to = if dir > 0 {
        (from + 1).min(devs.len() - 1)
    } else {
        from.saturating_sub(1)
    };
    crate::xml::edit::move_boot_device(&mut devs, from, to);
    d.hw.boot_cursor = to;
    d.hw.set_field("boot", "order", &devs.join(","));
    let shown: Vec<String> = devs
        .iter()
        .enumerate()
        .map(|(i, dev)| if i == to { format!("[{dev}]") } else { dev.clone() })
        .collect();
    d.message = format!("boot order: {}  (h/l select · J/K move)", shown.join(" › "));
}

/// Select the next/previous boot entry for `J`/`K` (`l`/`h`, wraps).
pub(super) fn boot_cursor_next(d: &mut ui::views::detail::DetailState, delta: isize) {
    let count =
        d.hw.field_value("boot", "order")
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .count();
    if count > 0 {
        d.hw.boot_cursor = (d.hw.boot_cursor as isize + delta).rem_euclid(count as isize) as usize;
        let devs: Vec<String> =
            d.hw.field_value("boot", "order")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        let shown: Vec<String> = devs
            .iter()
            .enumerate()
            .map(|(i, dev)| {
                if i == d.hw.boot_cursor {
                    format!("[{dev}]")
                } else {
                    dev.clone()
                }
            })
            .collect();
        d.message = format!("boot order: {}  (h/l select · J/K move)", shown.join(" › "));
    }
}

/// Stage detach confirmation for disk/NIC devices.
pub(super) fn stage_detach(
    d: &mut ui::views::detail::DetailState,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
) {
    use crate::command::builders::hardware;
    let dev = d.hw.selected_device();
    let domain = d.domain.clone();
    // `--live` only applies to a running (or paused) domain; virsh rejects it otherwise.
    let active = matches!(
        d.state,
        crate::model::DomainState::Running | crate::model::DomainState::Paused
    );
    let (title, build): (String, crate::command::plan::PlanBuilder) = if dev.id.starts_with("disk") {
        let target = dev.name.split_whitespace().last().unwrap_or("vda").to_string();
        (
            String::from("⚠ Detach disk"),
            std::sync::Arc::new(move |d: &str| hardware::detach_disk(d, &target, active)),
        )
    } else if dev.id.starts_with("nic") {
        let mac = d.hw.field_value(&dev.id, "mac");
        let kind = xmltree::Element::parse(d.hw.base_xml.as_bytes())
            .ok()
            .and_then(|root| root.get_child("devices").cloned())
            .and_then(|devices| {
                devices
                    .children
                    .into_iter()
                    .filter_map(|n| match n {
                        xmltree::XMLNode::Element(e) if e.name == "interface" => Some(e),
                        _ => None,
                    })
                    .find(|e| e.get_child("mac").and_then(|m| m.attributes.get("address")) == Some(&mac))
            })
            .and_then(|e| e.attributes.get("type").cloned());
        let Some(kind) = kind.filter(|_| !mac.is_empty()) else {
            d.message = String::from("✗ cannot identify selected NIC");
            return;
        };
        let live = active;
        (
            String::from("⚠ Detach NIC"),
            std::sync::Arc::new(move |d: &str| {
                let mut args = vec!["detach-interface", d, &kind, "--mac", &mac, "--config"];
                if live {
                    args.push("--live");
                }
                crate::command::plan::CommandPlan::single("virsh", args, &format!("Detached NIC on {d}"))
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
            kind: crate::ui::overlays::confirm::ConfirmKind::Always,
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
                if d.snaps.create_name.trim().is_empty() {
                    d.message = String::from("✗ snapshot name is required");
                    return true;
                }
                let domain = d.domain.clone();
                let plan = crate::command::builders::snapshot::create(
                    &domain,
                    &d.snaps.create_name.clone(),
                    &d.snaps.create_desc.clone(),
                    d.snaps.create_atomic,
                    d.snaps.create_quiesce,
                );
                d.snap_modal = SnapModal::None;
                preview_snapshot(&plan, d, dry_run, demo, history, messages);
            }
            KeyCode::Tab => d.snaps.create_focus = (d.snaps.create_focus + 1) % 4,
            KeyCode::Backspace => match d.snaps.create_focus {
                0 => {
                    d.snaps.create_name.pop();
                }
                1 => {
                    d.snaps.create_desc.pop();
                }
                _ => {}
            },
            KeyCode::Char(' ') if d.snaps.create_focus == 2 => d.snaps.create_atomic = !d.snaps.create_atomic,
            KeyCode::Char(' ') if d.snaps.create_focus == 3 => {
                d.snaps.create_quiesce = !d.snaps.create_quiesce
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                match d.snaps.create_focus {
                    0 => d.snaps.create_name.push(c),
                    1 => d.snaps.create_desc.push(c),
                    _ => {}
                }
            }
            _ => {}
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
    // The pid-file uptime only exists on this machine.
    st.uptime = if is_local_uri(uri) {
        crate::backend::virsh::host_local::domain_uptime(&st.domain)
    } else {
        None
    }
    .unwrap_or_else(|| "—".into());
    let xml = match crate::backend::virsh::exec::run(uri, &["dumpxml", &st.domain, "--inactive"]).await {
        Ok(xml) => xml,
        Err(e) => {
            st.message = format!("✗ cannot read the configuration of {}: {e}", st.domain);
            return;
        }
    };
    match crate::backend::virsh::parse_xml::parse_domain_xml(&xml) {
        Ok(cfg) => {
            let running = st.state == crate::model::DomainState::Running;
            st.hw = ui::views::detail::hardware::HardwareState::from_config(&cfg, &xml, running);
            for disk in cfg
                .disks
                .iter()
                .filter(|d| d.device != "cdrom" && !d.target.is_empty())
            {
                if let Ok(info) =
                    crate::backend::virsh::exec::run(uri, &["domblkinfo", &st.domain, &disk.target]).await
                    && let Some(bytes) = parse_domblkinfo_capacity(&info)
                {
                    st.hw.disk_capacity_kib.insert(disk.target.clone(), bytes / 1024);
                }
            }
            st.config = cfg;
            st.xml = xml;
            st.fold = ui::views::detail::XmlFold::new(st.xml.lines().count());
        }
        Err(e) => {
            st.message = format!("✗ cannot parse the configuration of {}: {e}", st.domain);
            st.xml = xml;
            st.fold = ui::views::detail::XmlFold::new(st.xml.lines().count());
            return;
        }
    }
    refresh_snapshots(uri, st).await;
}

/// `Capacity:       21474836480` from `virsh domblkinfo` (bytes).
pub(super) fn parse_domblkinfo_capacity(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("Capacity:"))
        .and_then(|v| v.trim().parse().ok())
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
    let snaps = match VirshBackend::new(uri).snapshots(&d.domain).await {
        Ok(snaps) => snaps,
        Err(e) => {
            d.message = format!("✗ cannot list snapshots of {}: {e}", d.domain);
            return;
        }
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
    // Details/description of every snapshot (bounded concurrency).
    let xmls = run_many(
        uri,
        snaps
            .iter()
            .map(|s| vec![String::from("snapshot-dumpxml"), d.domain.clone(), s.name.clone()])
            .collect(),
    )
    .await;
    d.snaps.infos = snaps
        .iter()
        .zip(xmls)
        .filter_map(|(s, xml)| {
            let info = ui::views::detail::snapshots::snap_info(&xml.ok()?, &snaps)?;
            Some((s.name.clone(), info))
        })
        .collect();
    d.snaps.details.clear();
    d.snaps.description.clear();
    d.snaps.sync_details();
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
                if !demo {
                    d.snaps.create_name = format!("snap-{}", jiff::Zoned::now().strftime("%Y%m%d-%H%M%S"));
                }
                d.snaps.create_focus = 0;
                d.snap_modal = SnapModal::Create;
                return false;
            }
            A::SnapshotRevert if d.snaps.rows.is_empty() => {
                d.message = String::from("✗ no snapshot to revert to");
                return false;
            }
            A::SnapshotDelete if d.snaps.rows.is_empty() => {
                d.message = String::from("✗ no snapshot to delete");
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
                if demo || dry_run {
                    d.message = String::from("[dry-run] Snapshot editing is disabled");
                    return false;
                }
                let snap = d.snaps.selected_name().to_string();
                if demo || dry_run {
                    d.message = format!("[dry-run] Edit snapshot {snap} XML");
                    return false;
                }
                if let Some(message) = suspend_run(&[
                    "virsh",
                    "-c",
                    uri,
                    "snapshot-edit",
                    &d.domain.clone(),
                    "--snapshotname",
                    &snap,
                ]) {
                    d.message = message;
                }
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
        A::Console => {
            if let Some(message) = external::console(uri, &d.domain) {
                d.message = message;
            }
        }
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
    let problems = detail.hw.problems();
    if !problems.is_empty() {
        detail.message = format!("✗ {}", problems.join("; "));
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

#[cfg(test)]
mod hardware_compact_tests {
    use super::*;

    #[test]
    fn review_scroll_returns_immediately_from_repeated_end_of_content() {
        let mut state = ui::views::detail::DetailState::demo("arch-dev");
        state.tab = ui::views::detail::DetailTab::Hardware;
        state.hw.review = true;
        state.hw.set_field("memory", "current", "8G");
        let mut detail = Some(state);
        let mut confirm = None;
        let mut ex = None;
        let mut engine = crate::input::engine::KeyEngine::new();
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
        terminal
            .draw(|f| {
                ui::views::detail::hardware::render(
                    f,
                    &crate::theme::builtin::tokyo_night(),
                    &detail.as_ref().unwrap().hw,
                    "arch-dev",
                    false,
                    f.area(),
                )
            })
            .unwrap();
        for _ in 0..100 {
            handle_hardware_key(
                KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
                &mut detail,
                &mut confirm,
                &mut ex,
                &mut engine,
            );
        }
        let bottom = detail.as_ref().unwrap().hw.review_scroll.0;
        handle_hardware_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut detail,
            &mut confirm,
            &mut ex,
            &mut engine,
        );
        assert!(bottom < 30, "scroll escaped content bounds: {bottom}");
        assert_eq!(
            detail.as_ref().unwrap().hw.review_scroll.0,
            bottom.saturating_sub(1)
        );
        handle_hardware_key(
            KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            &mut detail,
            &mut confirm,
            &mut ex,
            &mut engine,
        );
        assert_eq!(detail.as_ref().unwrap().hw.review_scroll, (bottom, 0));
        for _ in 0..100 {
            handle_hardware_key(
                KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
                &mut detail,
                &mut confirm,
                &mut ex,
                &mut engine,
            );
        }
        let right = detail.as_ref().unwrap().hw.review_scroll.1;
        assert_eq!(right, detail.as_ref().unwrap().hw.review_limits.get().1);
        handle_hardware_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut detail,
            &mut confirm,
            &mut ex,
            &mut engine,
        );
        assert_eq!(
            detail.as_ref().unwrap().hw.review_scroll.1,
            right.saturating_sub(4)
        );
        // A larger viewport refreshes bounds; the next key normalizes stale offsets.
        let mut large = ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 80)).unwrap();
        large
            .draw(|f| {
                ui::views::detail::hardware::render(
                    f,
                    &crate::theme::builtin::tokyo_night(),
                    &detail.as_ref().unwrap().hw,
                    "arch-dev",
                    false,
                    f.area(),
                )
            })
            .unwrap();
        handle_hardware_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut detail,
            &mut confirm,
            &mut ex,
            &mut engine,
        );
        assert_eq!(detail.as_ref().unwrap().hw.review_scroll, (0, 0));
    }
}
