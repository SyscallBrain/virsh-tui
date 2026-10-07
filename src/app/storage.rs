//! Storage view actions and the insert-media picker.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Storage-view raw keys (picker editing, pool/vol actions). True when consumed.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_storage_key(
    key: KeyEvent,
    store: &mut ui::views::storage::StorageState,
    uri: &str,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
    ex_line: &mut Option<crate::input::textinput::TextInput>,
) -> bool {
    use crate::input::key::KeySeq;
    // Media picker captures everything.
    if store.picker.is_some() {
        match key.code {
            KeyCode::Esc => {
                store.picker = None;
                store.message = String::new();
                return true;
            }
            KeyCode::Enter if key.modifiers.is_empty() => {
                insert_picked(uri, store, dry_run, demo, history, messages).await;
                return true;
            }
            KeyCode::Tab => {
                if let Some(p) = store.picker.as_mut() {
                    p.pool_idx = (p.pool_idx + 1) % p.pools.len().max(1);
                    p.selected = 0;
                }
                return true;
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(p) = store.picker.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                }
                return true;
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(p) = store.picker.as_mut() {
                    p.selected += 1;
                }
                return true;
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                eject_media(uri, store, dry_run, demo, history, messages).await;
                return true;
            }
            KeyCode::Backspace => {
                if let Some(p) = store.picker.as_mut() {
                    p.query.pop();
                    p.selected = 0;
                }
                return true;
            }
            KeyCode::Char(c) if key.modifiers.is_empty() => {
                if let Some(p) = store.picker.as_mut() {
                    p.query.push(c);
                    p.selected = 0;
                }
                return true;
            }
            _ => return true,
        }
    }
    let tok = KeySeq::from_event(&key);
    match tok.token() {
        "j" | "k" => false,
        "h" | "l" => {
            store.focus_pools = !store.focus_pools;
            true
        }
        "n" => {
            *ex_line = Some(crate::input::textinput::TextInput::with("vol-create-as "));
            true
        }
        "b" | "s" | "r" | "a" => {
            pool_action(tok.token(), store, uri, dry_run, demo, history, messages, confirm).await;
            true
        }
        "D" | "X" | "R" | "C" | "u" | "W" => {
            vol_action(
                tok.token(),
                store,
                uri,
                dry_run,
                demo,
                history,
                messages,
                confirm,
                ex_line,
            );
            true
        }
        "Enter" => {
            // ⏎ on an ISO volume opens the picker.
            if let Some(v) = store.selected_vol()
                && v.name.ends_with(".iso")
            {
                return true;
            }
            false
        }
        _ => false,
    }
}

/// Pool actions: b build, s start, r refresh, a autostart.
#[allow(clippy::too_many_arguments)]
pub(super) async fn pool_action(
    tok: &str,
    store: &mut ui::views::storage::StorageState,
    uri: &str,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
    _confirm: &mut Option<crate::command::plan::PendingConfirm>,
) {
    use crate::command::builders::storage as sb;
    let pool = store.selected_pool().to_string();
    match tok {
        "b" => {
            run_store_plan(
                uri,
                &sb::pool_build(&pool),
                store,
                dry_run,
                demo,
                history,
                messages,
            )
            .await
        }
        "s" => {
            run_store_plan(
                uri,
                &sb::pool_start(&pool),
                store,
                dry_run,
                demo,
                history,
                messages,
            )
            .await
        }
        "r" => {
            run_store_plan(
                uri,
                &sb::pool_refresh(&pool),
                store,
                dry_run,
                demo,
                history,
                messages,
            )
            .await
        }
        "a" => {
            run_store_plan(
                uri,
                &sb::pool_autostart(&pool, true),
                store,
                dry_run,
                demo,
                history,
                messages,
            )
            .await
        }
        _ => {}
    }
}

/// Volume actions: D/C/u/W/X direct, R/C via builders, n via ex.
#[allow(clippy::too_many_arguments)]
pub(super) fn vol_action(
    tok: &str,
    store: &mut ui::views::storage::StorageState,
    _uri: &str,
    _dry_run: bool,
    _demo: bool,
    _history: &mut crate::command::history::History,
    _messages: &mut Vec<String>,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
    ex_line: &mut Option<crate::input::textinput::TextInput>,
) {
    use crate::command::builders::storage as sb;
    let pool = store.selected_pool().to_string();
    let Some(vol) = store.selected_vol().map(|v| v.name.clone()) else {
        return;
    };
    match tok {
        "D" | "X" if store.volumes.get(store.vol_selected).is_some() => {
            let (question, build, targets): (String, crate::command::plan::PlanBuilder, Vec<String>) =
                if tok == "D" {
                    let pool = pool.clone();
                    (
                        format!("destroy pool {pool}?"),
                        std::sync::Arc::new(move |p: &str| sb::pool_destroy(p)),
                        vec![pool],
                    )
                } else {
                    let (pool, vol) = (pool.clone(), vol.clone());
                    let question = format!("delete {vol} from {pool}?");
                    let targets = vec![vol.clone()];
                    (
                        question,
                        std::sync::Arc::new(move |_: &str| sb::vol_delete(&pool, &vol)),
                        targets,
                    )
                };
            let commands = build(targets.first().map(|s| s.as_str()).unwrap_or(""))
                .steps
                .iter()
                .map(crate::command::display::display)
                .collect();
            *confirm = Some((
                crate::ui::overlays::confirm::Confirm {
                    title: String::from("⚠ Delete"),
                    question,
                    commands,
                    confirm_label: String::from("y delete"),
                    type_name: None,
                    typed: String::new(),
                },
                build,
                targets,
            ));
        }
        "R" => {
            *ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                "vol-resize {pool} {vol} "
            )));
        }
        "C" => {
            *ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                "vol-clone {pool} {vol} "
            )));
        }
        "u" => {
            *ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                "vol-upload {pool} {vol} "
            )));
        }
        "W" => {
            let (pool, vol) = (pool.clone(), vol.clone());
            let question = format!("wipe {vol} in {pool}?");
            let targets = vec![vol.clone()];
            let build: crate::command::plan::PlanBuilder =
                std::sync::Arc::new(move |_: &str| sb::vol_wipe(&pool, &vol, "zero"));
            let commands = build("")
                .steps
                .iter()
                .map(crate::command::display::display)
                .collect();
            *confirm = Some((
                crate::ui::overlays::confirm::Confirm {
                    title: String::from("⚠ Wipe volume"),
                    question,
                    commands,
                    confirm_label: String::from("y wipe"),
                    type_name: None,
                    typed: String::new(),
                },
                build,
                targets,
            ));
        }
        _ => {}
    }
}

/// Storage engine-action dispatch (j/k counts + Enter on ISO).
#[allow(clippy::too_many_arguments)]
pub(super) async fn dispatch_storage(
    action: crate::input::engine::KeyAction,
    store: &mut ui::views::storage::StorageState,
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    dash: &ui::views::dashboard::DashboardState,
    uri: &str,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
    ex_line: &mut Option<crate::input::textinput::TextInput>,
) -> bool {
    use crate::input::engine::KeyAction as A;
    match action {
        A::MoveDown | A::MoveUp => {
            let down = action == A::MoveDown;
            if store.focus_pools {
                let before = store.pool_selected;
                store.pool_selected = if down {
                    (store.pool_selected + 1).min(store.pools.len().saturating_sub(1))
                } else {
                    store.pool_selected.saturating_sub(1)
                };
                if !demo && store.pool_selected != before {
                    load_pool_volumes(uri, store).await;
                }
            } else if down {
                store.vol_selected = (store.vol_selected + 1).min(store.volumes.len().saturating_sub(1));
            } else {
                store.vol_selected = store.vol_selected.saturating_sub(1);
            }
        }
        A::Open => {
            if let Some(v) = store.selected_vol()
                && v.name.ends_with(".iso")
            {
                let domain = dash.selected().name.clone();
                open_picker_for(uri, store, &domain, demo).await;
                let _ = (detail, view, dry_run, history, messages, confirm, ex_line);
            }
        }
        _ => {}
    }
    false
}

/// Open the media picker for a domain (cdrom target auto-detected).
pub(super) async fn open_picker(
    store: &mut ui::views::storage::StorageState,
    detail: &Option<ui::views::detail::DetailState>,
    dash: &ui::views::dashboard::DashboardState,
    uri: &str,
    demo: bool,
) {
    let domain = detail
        .as_ref()
        .map(|d| d.domain.clone())
        .unwrap_or_else(|| dash.selected().name.clone());
    open_picker_for(uri, store, &domain, demo).await;
}

pub(super) async fn open_picker_for(
    uri: &str,
    store: &mut ui::views::storage::StorageState,
    domain: &str,
    demo: bool,
) {
    use crate::ui::views::storage::MediaPicker;
    let (target, bus, current) = if demo {
        (
            String::from("sda"),
            String::from("sata"),
            String::from("archlinux-2026.09.01-x86_64.iso"),
        )
    } else {
        cdrom_of(uri, domain)
            .await
            .unwrap_or((String::from("sda"), String::from("sata"), String::new()))
    };
    let mut pools: Vec<String> = store.pools.iter().map(|p| p.name.clone()).collect();
    if pools.is_empty() {
        pools = vec![String::from("isos"), String::from("default")];
    }
    store.picker = Some(MediaPicker {
        domain: domain.to_string(),
        target,
        bus,
        current,
        query: String::new(),
        pool_idx: 0,
        pools,
        selected: 0,
        live: true,
        config: true,
    });
}

/// First cdrom (target, bus) plus current media of a domain.
pub(super) async fn cdrom_of(uri: &str, domain: &str) -> Option<(String, String, String)> {
    let xml = crate::backend::virsh::exec::run(uri, &["dumpxml", domain])
        .await
        .ok()?;
    let cfg = crate::backend::virsh::parse_xml::parse_domain_xml(&xml).ok()?;
    let cd = cfg.disks.iter().find(|d| d.device == "cdrom")?;
    Some((cd.target.clone(), cd.bus.clone(), cd.source.clone()))
}

/// Insert the picked ISO (Enter in the picker).
pub(super) async fn insert_picked(
    uri: &str,
    store: &mut ui::views::storage::StorageState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) {
    let Some(p) = store.picker.take() else {
        return;
    };
    let isos = state_isos(store, &p);
    let Some(iso) = isos
        .get(p.selected.min(isos.len().saturating_sub(1)))
        .map(|i| i.name.clone())
    else {
        store.message = String::from("no ISO selected");
        return;
    };
    let pool_path = p
        .pools
        .get(p.pool_idx)
        .and_then(|pool| store.pools.iter().find(|pp| &pp.name == pool))
        .map(|pp| pp.path.clone())
        .unwrap_or_else(|| String::from("/var/lib/libvirt/isos"));
    let plan = crate::command::builders::hardware::change_media(
        &p.domain,
        &p.target,
        Some(&format!("{pool_path}/{iso}")),
        p.live,
    );
    for step in &plan.steps {
        history.push(&crate::command::display::display(step));
    }
    match crate::command::exec::execute(uri, &plan, dry_run).await {
        Ok(msg) => {
            store.message = msg.clone();
            messages.push(msg);
        }
        Err(e) => {
            store.message = e.to_string();
            messages.push(e.to_string());
        }
    }
    if !demo && !dry_run {
        reload_store(uri, store).await;
    }
}

pub(super) fn state_isos(
    store: &ui::views::storage::StorageState,
    p: &ui::views::storage::MediaPicker,
) -> Vec<crate::ui::views::storage::IsoEntry> {
    let q = p.query.to_lowercase();
    store
        .isos
        .iter()
        .filter(|i| {
            p.pools.get(p.pool_idx).is_none_or(|pool| &i.pool == pool)
                && (q.is_empty() || i.name.to_lowercase().contains(&q))
        })
        .cloned()
        .collect()
}

/// Eject only (C-e in picker, ␣me anywhere).
pub(super) async fn eject_media(
    uri: &str,
    store: &mut ui::views::storage::StorageState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) {
    let (domain, target, live) = store
        .picker
        .as_ref()
        .map(|p| (p.domain.clone(), p.target.clone(), p.live))
        .unwrap_or((String::from("arch-dev"), String::from("sda"), true));
    store.picker = None;
    let plan = crate::command::builders::hardware::change_media(&domain, &target, None, live);
    for step in &plan.steps {
        history.push(&crate::command::display::display(step));
    }
    match crate::command::exec::execute(uri, &plan, dry_run).await {
        Ok(msg) => {
            store.message = msg.clone();
            messages.push(msg);
        }
        Err(e) => {
            store.message = e.to_string();
            messages.push(e.to_string());
        }
    }
    let _ = demo;
}

/// Run a storage plan and update message/history.
pub(super) async fn run_store_plan(
    uri: &str,
    plan: &crate::command::plan::CommandPlan,
    store: &mut ui::views::storage::StorageState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) {
    for step in &plan.steps {
        history.push(&crate::command::display::display(step));
    }
    match crate::command::exec::execute(uri, plan, dry_run).await {
        Ok(msg) => {
            store.message = msg.clone();
            messages.push(msg);
        }
        Err(e) => {
            store.message = e.to_string();
            messages.push(e.to_string());
        }
    }
    let _ = demo;
}
