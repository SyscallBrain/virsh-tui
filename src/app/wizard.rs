//! New-domain wizard actions.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Host facts the wizard needs (threads, free memory, names, pools, ISOs).
pub(super) fn wizard_host_facts(
    app_dash: &ui::views::dashboard::DashboardState,
    nets: &ui::views::networks::NetworksState,
    store: &ui::views::storage::StorageState,
    demo: bool,
) -> ui::overlays::wizard::HostFacts {
    if demo {
        return ui::overlays::wizard::WizardState::demo_step3().host;
    }
    let mem = crate::backend::virsh::host_local::parse_meminfo(
        &std::fs::read_to_string("/proc/meminfo").unwrap_or_default(),
    );
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
    ui::overlays::wizard::HostFacts {
        threads,
        free_gib: mem.available_kib as f64 / 1024.0 / 1024.0,
        domains: app_dash.rows.iter().map(|r| r.name.clone()).collect(),
        pools: store
            .pools
            .iter()
            .filter(|p| p.active)
            .map(|p| p.name.clone())
            .collect(),
        networks: nets
            .networks
            .iter()
            .filter(|n| n.active)
            .map(|n| n.name.clone())
            .collect(),
        isos: store
            .isos
            .iter()
            .filter_map(|i| {
                store
                    .pools
                    .iter()
                    .find(|p| p.name == i.pool)
                    .map(|p| format!("{}/{}", p.path.trim_end_matches('/'), i.name))
            })
            .collect(),
    }
}

/// Open the wizard: restore the draft (repaired) or start fresh, then attach host facts.
pub(super) fn open_wizard(
    wizard: &mut Option<ui::overlays::wizard::WizardState>,
    host: ui::overlays::wizard::HostFacts,
) {
    use ui::overlays::wizard::WizardState;
    let mut st = WizardState::draft_path()
        .and_then(|p| WizardState::load_draft(&p).ok())
        .unwrap_or_default();
    // Fresh drafts pick the first pool/network that exists on this host.
    if !host.pools.is_empty() && !host.pools.contains(&st.pool) {
        st.pool = host.pools[0].clone();
    }
    if !host.networks.is_empty() && st.net_source != "none" && !host.networks.contains(&st.net_source) {
        st.net_source = host.networks[0].clone();
    }
    st.host = host;
    st.error.clear();
    *wizard = Some(st);
}

/// Wizard keys. Text fields take every printable key (so names can contain
/// h, l, y, …); shortcuts use Ctrl. Returns true when the key was consumed.
pub(super) fn handle_wizard_key(
    key: KeyEvent,
    wizard: &mut Option<ui::overlays::wizard::WizardState>,
    uri: &str,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
    dash: &mut ui::views::dashboard::DashboardState,
) -> bool {
    use ui::overlays::wizard::{WKind, WizardState};
    let Some(wz) = wizard else { return false };
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl {
        match key.code {
            KeyCode::Char('n') => wz.goto_step(wz.step + 1),
            KeyCode::Char('p') => wz.goto_step(wz.step.saturating_sub(1)),
            KeyCode::Char('j') => wz.move_focus(1),
            KeyCode::Char('k') => wz.move_focus(-1),
            KeyCode::Char('w') => wz.clear_field(),
            KeyCode::Char('r') => {
                let host = std::mem::take(&mut wz.host);
                *wz = WizardState {
                    host,
                    ..Default::default()
                };
            }
            KeyCode::Char('y') => {
                let plan = crate::command::builders::install::virt_install(wz);
                copy_text(&crate::command::display::display(&plan.steps[0]));
                wz.error = String::from("command copied");
            }
            KeyCode::Char('e') => {
                wz.error = if demo || dry_run {
                    let message = String::from("[dry-run] edit as XML  ── virt-install --print-xml");
                    messages.push_action(message.clone(), &wz.name, true);
                    message
                } else {
                    "XML editor request was not submitted".into()
                };
            }
            _ => return true,
        }
        wz.autosave();
        return true;
    }
    let field_kind = wz.kind(wz.focused());
    match key.code {
        KeyCode::Esc => {
            wz.autosave();
            *wizard = None;
        }
        KeyCode::Tab | KeyCode::Down => wz.move_focus(1),
        KeyCode::BackTab | KeyCode::Up => wz.move_focus(-1),
        KeyCode::Backspace => wz.backspace(),
        KeyCode::Left => wz.adjust(-1),
        KeyCode::Right => wz.adjust(1),
        KeyCode::Enter => {
            if wz.step < 5 {
                wz.goto_step(wz.step + 1);
            } else if let Some((step, problem)) = wz.problems().first().cloned() {
                // Jump to the step that needs attention.
                wz.goto_step(step);
                wz.error = problem;
            } else {
                match preview_wizard(uri, wz, dry_run, demo, history, messages) {
                    Ok(msg) => {
                        dash.message = msg;
                        WizardState::discard_draft();
                        *wizard = None;
                        return true;
                    }
                    Err(e) => wz.error = e,
                }
            }
        }
        // Printable keys: text first, then field shortcuts.
        KeyCode::Char(c) if wz.type_char(c) => {}
        KeyCode::Char(c) => match (field_kind, c) {
            (WKind::Check, ' ') | (WKind::Select(_), ' ') => wz.adjust(1),
            (_, 'h') => wz.adjust(-1),
            (_, 'l') => wz.adjust(1),
            (WKind::Number { .. }, 'H') => wz.adjust(-10),
            (WKind::Number { .. }, 'L') => wz.adjust(10),
            (_, 'j') => wz.move_focus(1),
            (_, 'k') => wz.move_focus(-1),
            _ => {}
        },
        _ => {}
    }
    if let Some(wz) = wizard {
        wz.autosave();
    }
    true
}

/// Preview creation in demo and dry-run modes. Live submissions are jobs.
fn preview_wizard(
    uri: &str,
    wz: &ui::overlays::wizard::WizardState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) -> Result<String, String> {
    if !demo && !dry_run {
        return Err("Wizard request was not submitted".into());
    }
    let mut plan = crate::command::builders::install::virt_install(wz);
    plan.steps[0].argv.insert(0, uri.to_string());
    plan.steps[0].argv.insert(0, String::from("--connect"));
    let shown = crate::command::display::display(&plan.steps[0]);
    history.push(&shown);
    let msg = format!("[dry-run] Created {}  ── {shown}", wz.name);
    messages.push_action(msg.clone(), &wz.name, true);
    Ok(msg)
}
