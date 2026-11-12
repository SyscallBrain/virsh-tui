//! Storage view actions and the insert-media picker.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Storage-view raw keys (picker editing, pool/vol actions). True when consumed.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_storage_key(
    key: KeyEvent,
    store: &mut ui::views::storage::StorageState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
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
                insert_picked(store, dry_run, demo, history, messages);
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
                eject_media(store, dry_run, demo, history, messages);
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
            pool_action(tok.token(), store, dry_run, demo, history, messages);
            true
        }
        "D" | "X" | "R" | "C" | "u" | "W" => {
            vol_action(tok.token(), store, confirm, ex_line);
            true
        }
        "Enter" => false,
        _ => false,
    }
}

/// Preview pool actions; live actions are submitted as jobs.
fn pool_action(
    tok: &str,
    store: &mut ui::views::storage::StorageState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) {
    use crate::command::builders::storage as sb;
    let pool = store.selected_pool().to_string();
    let plan = match tok {
        "b" => sb::pool_build(&pool),
        "s" => sb::pool_start(&pool),
        "r" => sb::pool_refresh(&pool),
        "a" => sb::pool_autostart(&pool, true),
        _ => return,
    };
    preview_store_plan(&plan, &pool, store, dry_run, demo, history, messages);
}

/// Volume actions: D/C/u/W/X direct, R/C via builders, n via ex.
#[allow(clippy::too_many_arguments)]
pub(super) fn vol_action(
    tok: &str,
    store: &mut ui::views::storage::StorageState,
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
pub(super) fn dispatch_storage(
    action: crate::input::engine::KeyAction,
    store: &mut ui::views::storage::StorageState,
) -> bool {
    use crate::input::engine::KeyAction as A;
    match action {
        A::MoveDown | A::MoveUp => {
            let down = action == A::MoveDown;
            if store.focus_pools {
                store.pool_selected = if down {
                    (store.pool_selected + 1).min(store.pools.len().saturating_sub(1))
                } else {
                    store.pool_selected.saturating_sub(1)
                };
            } else if down {
                store.vol_selected = (store.vol_selected + 1).min(store.volumes.len().saturating_sub(1));
            } else {
                store.vol_selected = store.vol_selected.saturating_sub(1);
            }
        }
        _ => {}
    }
    false
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
fn insert_picked(
    store: &mut ui::views::storage::StorageState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
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
    preview_store_plan(&plan, &p.domain, store, dry_run, demo, history, messages);
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
pub(super) fn eject_media(
    store: &mut ui::views::storage::StorageState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) {
    let Some((domain, target, live)) = store
        .picker
        .as_ref()
        .map(|p| (p.domain.clone(), p.target.clone(), p.live))
    else {
        store.message = String::from("✗ no CD-ROM selected");
        return;
    };
    store.picker = None;
    let plan = crate::command::builders::hardware::change_media(&domain, &target, None, live);
    preview_store_plan(&plan, &domain, store, dry_run, demo, history, messages);
}

/// Preview a storage plan without executing a process.
fn preview_store_plan(
    plan: &crate::command::plan::CommandPlan,
    target: &str,
    store: &mut ui::views::storage::StorageState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) {
    if !demo && !dry_run {
        store.message = "✗ Storage request was not submitted".into();
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
    store.message = format!("{prefix} {}  ── {first}", plan.summary);
    messages.push_action(store.message.clone(), target, true);
}
