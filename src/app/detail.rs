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
pub(super) async fn handle_snap_modal(
    key: KeyEvent,
    detail: &mut Option<ui::views::detail::DetailState>,
    uri: &str,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
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
                for step in &plan.steps {
                    history.push(&crate::command::display::display(step));
                }
                match crate::command::exec::execute(uri, &plan, dry_run).await {
                    Ok(msg) => {
                        d.message = msg.clone();
                        messages.push(msg);
                        if !demo && !dry_run {
                            refresh_snapshots(uri, d).await;
                        }
                    }
                    Err(e) => {
                        d.message = e.to_string();
                        messages.push(e.to_string());
                    }
                }
                let _ = demo;
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
                for step in &plan.steps {
                    history.push(&crate::command::display::display(step));
                }
                match crate::command::exec::execute(uri, &plan, dry_run).await {
                    Ok(msg) => {
                        d.message = msg.clone();
                        messages.push(msg);
                        if !demo && !dry_run {
                            refresh_snapshots(uri, d).await;
                        }
                    }
                    Err(e) => {
                        d.message = e.to_string();
                        messages.push(e.to_string());
                    }
                }
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
                for step in &plan.steps {
                    history.push(&crate::command::display::display(step));
                }
                match crate::command::exec::execute(uri, &plan, dry_run).await {
                    Ok(msg) => {
                        d.message = msg.clone();
                        messages.push(msg);
                        if !demo && !dry_run {
                            refresh_snapshots(uri, d).await;
                        }
                    }
                    Err(e) => {
                        d.message = e.to_string();
                        messages.push(e.to_string());
                    }
                }
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
pub(super) async fn open_detail(
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    dash: &ui::views::dashboard::DashboardState,
    host: &ui::views::host::HostState,
    uri: &str,
    demo: bool,
) {
    let name = if *view == View::Host {
        host.domains.first().map(|d| d.name.clone()).unwrap_or_default()
    } else {
        dash.selected().name.clone()
    };
    if name.is_empty() {
        return;
    }
    let mut st = ui::views::detail::DetailState::demo(&name);
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
        st.uptime =
            crate::backend::virsh::host_local::domain_uptime(&name).unwrap_or_else(|| String::from("—"));
        st.config = crate::model::DomainConfig {
            name: name.clone(),
            ..Default::default()
        };
        st.xml = String::new();
        st.hw = ui::views::detail::hardware::HardwareState::from_config(&st.config, "", false);
        st.snaps.rows.clear();
        use crate::backend::virsh::VirshBackend;
        let backend = VirshBackend::new(uri);
        if let Ok(xml) = crate::backend::virsh::exec::run(uri, &["dumpxml", &name, "--inactive"]).await
            && let Ok(cfg) = crate::backend::virsh::parse_xml::parse_domain_xml(&xml)
        {
            let running = st.state == crate::model::DomainState::Running;
            st.hw = ui::views::detail::hardware::HardwareState::from_config(&cfg, &xml, running);
            st.config = cfg;
            st.xml = xml;
            st.fold = ui::views::detail::XmlFold::new(st.xml.lines().count());
        }
        refresh_snapshots(uri, &mut st).await;
        let _ = backend;
    }
    *detail = Some(st);
    *view = View::Detail;
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
pub(super) async fn dispatch_detail(
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
            d.message = edit_domain_xml(uri, &d.domain, &d.xml, dry_run, demo).await;
            if !demo {
                refresh_detail_xml(uri, d).await;
            }
        }
        A::Console => external::console(uri, &d.domain),
        A::Viewer => open_viewer(uri, &d.domain.clone()).await,
        _ => {}
    }
    let _ = view;
    false
}

/// Refresh the detail XML after an edit.
pub(super) async fn refresh_detail_xml(uri: &str, d: &mut ui::views::detail::DetailState) {
    if let Ok(xml) = crate::backend::virsh::exec::run(uri, &["dumpxml", &d.domain, "--inactive"]).await
        && let Ok(cfg) = crate::backend::virsh::parse_xml::parse_domain_xml(&xml)
    {
        let running = d.state == crate::model::DomainState::Running;
        d.hw = ui::views::detail::hardware::HardwareState::from_config(&cfg, &xml, running);
        d.config = cfg;
        d.xml = xml;
        d.fold = ui::views::detail::XmlFold::new(d.xml.lines().count());
    }
}

/// `$EDITOR` loop for the XML tab: validate with `virsh define`, reopen on error
/// (like `virsh edit`). Unchanged content is not defined.
pub(super) async fn edit_domain_xml(uri: &str, domain: &str, xml: &str, dry_run: bool, demo: bool) -> String {
    if demo {
        return format!("[dry-run] Edited {domain} XML  ── virsh define /tmp/vt-{domain}.xml");
    }
    let mut content = xml.to_string();
    loop {
        let Ok(tmp) = crate::command::tempxml::write(domain, &content) else {
            return String::from("✗ cannot write temp XML");
        };
        if !external::edit_file(tmp.path()) {
            return String::from("✗ editor failed");
        }
        let edited = std::fs::read_to_string(tmp.path()).unwrap_or_default();
        if edited == content {
            // Saved without changes: nothing to define (or the user gave up after an error).
            return if content == xml {
                format!("no changes to {domain}")
            } else {
                format!("✗ edit aborted for {domain}")
            };
        }
        let path = crate::command::tempxml::path_arg(&tmp);
        if dry_run {
            return format!("[dry-run] Edited {domain} XML  ── virsh define {path} --validate");
        }
        match crate::backend::virsh::exec::run(uri, &["define", &path, "--validate"]).await {
            Ok(_) => return format!("✓ Defined {domain}  ── virsh define {path} --validate"),
            Err(e) => content = ui::views::detail::edit_error_comment(&edited, &e.to_string()),
        }
    }
}

/// Run an ex line in the detail view (`:w` apply, `:q!` discard, `:q` close).
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_detail_ex(
    line: &str,
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    uri: &mut String,
    dash: &mut ui::views::dashboard::DashboardState,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
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
                apply_hardware(uri, d, dry_run, demo, history, messages).await;
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
        _ => {
            run_ex_line(
                line, uri, dash, history, messages, dry_run, demo, &mut None, config, theme, confirm,
            )
            .await
        }
    }
}

/// Apply pending hardware changes: run native plans, then the define path.
pub(super) async fn apply_hardware(
    uri: &str,
    d: &mut ui::views::detail::DetailState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) {
    let domain = d.domain.clone();
    let unsupported = d.hw.unsupported_changes();
    if !unsupported.is_empty() {
        d.message = format!(
            "✗ cannot apply {} yet (read-only): undo with u or :q!",
            unsupported.join(", ")
        );
        messages.push(d.message.clone());
        return;
    }
    let mut plans = d.hw.will_run(&domain);
    if plans.is_empty() {
        d.message = String::from("nothing to apply");
        return;
    }
    // The define step reads the fully edited XML from a private temp file that
    // must outlive the command; the displayed `/tmp/vt-<d>.xml` is replaced by it.
    let mut _tmp_guard = None;
    if !demo
        && plans.iter().any(|p| {
            p.steps
                .iter()
                .any(|s| s.argv.first().is_some_and(|a| a == "define"))
        })
    {
        match crate::command::tempxml::write(&domain, &d.hw.edited_xml()) {
            Ok(tmp) => {
                let path = crate::command::tempxml::path_arg(&tmp);
                for step in plans.iter_mut().flat_map(|p| p.steps.iter_mut()) {
                    if step.argv.first().is_some_and(|a| a == "define") {
                        step.argv = vec![String::from("define"), path.clone(), String::from("--validate")];
                    }
                }
                _tmp_guard = Some(tmp);
            }
            Err(_) => {
                d.message = String::from("✗ cannot write temp XML");
                return;
            }
        }
    }
    for plan in &plans {
        if let Some(first) = plan.steps.first() {
            history.push(&crate::command::display::display(first));
        }
        let result = if demo {
            Ok(format!("✓ {}", plan.summary))
        } else {
            crate::command::exec::execute(uri, plan, dry_run).await
        };
        match result {
            Ok(msg) => {
                d.message = msg.clone();
                messages.push(msg);
            }
            Err(e) => {
                d.message = e.to_string();
                messages.push(e.to_string());
                // Re-read the real config so the editor reflects what was applied.
                if !demo && !dry_run {
                    refresh_detail_xml(uri, d).await;
                }
                return;
            }
        }
    }
    if dry_run || demo {
        return;
    }
    refresh_detail_xml(uri, d).await;
}

/// Set `<description>` via the XML edit path (dumpxml → xmltree → define).
pub(super) async fn set_description(
    uri: &str,
    domain: &str,
    text: &str,
    dry_run: bool,
    demo: bool,
) -> String {
    if demo || dry_run {
        return format!("[dry-run] Described {domain}  ── virsh define /tmp/vt-{domain}.xml");
    }
    let xml = match crate::backend::virsh::exec::run(uri, &["dumpxml", domain, "--inactive"]).await {
        Ok(x) => x,
        Err(e) => return format!("✗ dumpxml failed: {}", e.to_string().lines().next().unwrap_or("")),
    };
    let new_xml = set_description_text(&xml, text);
    let Ok(tmp) = crate::command::tempxml::write(domain, &new_xml) else {
        return String::from("✗ cannot write temp XML");
    };
    let path = crate::command::tempxml::path_arg(&tmp);
    match crate::backend::virsh::exec::run(uri, &["define", &path, "--validate"]).await {
        Ok(_) => format!("✓ Described {domain}  ── virsh define {path} --validate"),
        Err(e) => format!("✗ define failed: {}", e.to_string().lines().next().unwrap_or("")),
    }
}

/// Replace (or insert) `<description>` text with xmltree.
pub(super) fn set_description_text(xml: &str, text: &str) -> String {
    use xmltree::{Element, XMLNode};
    let mut elem: Element = match Element::parse(xml.as_bytes()) {
        Ok(e) => e,
        Err(_) => return xml.to_string(),
    };
    elem.children
        .retain(|c| !matches!(c, XMLNode::Element(e) if e.name == "description"));
    let mut desc = Element::new("description");
    desc.children.push(XMLNode::Text(text.to_string()));
    let pos = elem
        .children
        .iter()
        .position(|c| matches!(c, XMLNode::Element(e) if e.name == "title"))
        .map(|p| p + 1)
        .unwrap_or(0);
    elem.children.insert(pos, XMLNode::Element(desc));
    let mut out = Vec::new();
    if elem.write(&mut out).is_err() {
        return xml.to_string();
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Attach a host device: dumpxml the nodedev, wrap in <hostdev>, attach-device.
pub(super) async fn attach_hostdev(uri: &str, domain: &str, node: &str, dry_run: bool, demo: bool) -> String {
    if demo || dry_run {
        return format!(
            "[dry-run] Attached {node} on {domain}  ── virsh attach-device {domain} /tmp/vt-hostdev.xml"
        );
    }
    let dev_xml = match crate::backend::virsh::exec::run(uri, &["nodedev-dumpxml", node]).await {
        Ok(x) => x,
        Err(e) => return format!("✗ nodedev failed: {}", e.to_string().lines().next().unwrap_or("")),
    };
    let hostdev = match hostdev_xml(&dev_xml) {
        Some(x) => x,
        None => return format!("✗ {node} is not a PCI or USB device"),
    };
    let Ok(tmp) = crate::command::tempxml::write("hostdev", &hostdev) else {
        return String::from("✗ cannot write temp XML");
    };
    let path = crate::command::tempxml::path_arg(&tmp);
    match crate::backend::virsh::exec::run(uri, &["attach-device", domain, &path, "--config"]).await {
        Ok(_) => format!("✓ Attached {node} on {domain}  ── virsh attach-device {domain} {path}"),
        Err(e) => format!("✗ attach failed: {}", e.to_string().lines().next().unwrap_or("")),
    }
}

/// Build a `<hostdev>` element for a node device from its `nodedev-dumpxml`.
///
/// Uses the device's own `<capability type='pci'>` address (or USB
/// vendor/product), never addresses of other devices in its IOMMU group.
pub(super) fn hostdev_xml(dev_xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(dev_xml).ok()?;
    let cap = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "capability")?;
    let text = |name: &str| {
        cap.children()
            .find(|n| n.tag_name().name() == name)
            .and_then(|n| n.text())
            .map(str::trim)
            .and_then(|t| t.parse::<u32>().ok())
    };
    let id_attr = |name: &str| {
        cap.children()
            .find(|n| n.tag_name().name() == name)
            .and_then(|n| n.attribute("id"))
            .map(str::to_string)
    };
    match cap.attribute("type")? {
        "pci" => Some(format!(
            "<hostdev mode='subsystem' type='pci' managed='yes'>\n  <source>\n    <address domain='{:#06x}' bus='{:#04x}' slot='{:#04x}' function='{:#x}'/>\n  </source>\n</hostdev>\n",
            text("domain")?,
            text("bus")?,
            text("slot")?,
            text("function")?
        )),
        "usb_device" => Some(format!(
            "<hostdev mode='subsystem' type='usb' managed='yes'>\n  <source>\n    <vendor id='{}'/>\n    <product id='{}'/>\n  </source>\n</hostdev>\n",
            id_attr("vendor")?,
            id_attr("product")?
        )),
        _ => None,
    }
}
