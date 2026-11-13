//! New-domain wizard actions.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Host facts the wizard needs (threads, free memory, names, pools, ISOs).
pub(super) fn wizard_host_facts(
    app_dash: &ui::views::dashboard::DashboardState,
    nets: &ui::views::networks::NetworksState,
    store: &ui::views::storage::StorageState,
    uri: &str,
    iso_dirs: &[String],
    demo: bool,
) -> ui::overlays::wizard::HostFacts {
    use ui::overlays::iso_picker;
    if demo {
        return ui::overlays::wizard::WizardState::demo_step3().host;
    }
    let mem = crate::backend::virsh::host_local::parse_meminfo(
        &std::fs::read_to_string("/proc/meminfo").unwrap_or_default(),
    );
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
    let local = is_local_uri(uri);
    let pool_isos = store
        .isos
        .iter()
        .map(|i| iso_picker::from_pool(&i.pool, &i.name, &i.path, &i.size, local))
        .collect();
    // Local folders only describe this machine.
    let local_isos = if local {
        iso_picker::iso_dirs(iso_dirs)
            .iter()
            .flat_map(|d| iso_picker::scan_dir(d))
            .collect()
    } else {
        vec![]
    };
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
        isos: iso_picker::merge(pool_isos, local_isos),
        local,
        system_uri: local && uri.ends_with("/system"),
        no_kvm: !std::path::Path::new("/dev/kvm").exists(),
    }
}

/// ISO picker keys (the picker is open). Typing filters; C-j/k ↑↓ move;
/// ⇥ cycles the source; C-o toggles the filesystem browser; ⏎ chooses;
/// ⎋ closes the picker only.
fn handle_iso_picker_key(
    key: KeyEvent,
    wz: &mut ui::overlays::wizard::WizardState,
    iso_dirs: &[String],
    demo: bool,
) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let start = wz.browse_start(&ui::overlays::iso_picker::iso_dirs(iso_dirs));
    let local = wz.host.local && !demo;
    let cands = &wz.host.isos;
    let Some(p) = wz.picker.as_mut() else { return };
    match key.code {
        KeyCode::Char('j') if ctrl => p.move_sel(1, cands),
        KeyCode::Char('k') if ctrl => p.move_sel(-1, cands),
        KeyCode::Char('w') if ctrl => {
            p.query.clear();
            p.selected = 0;
        }
        KeyCode::Char('o') if ctrl => {
            if local {
                p.toggle_browse(start);
            } else {
                wz.error = String::from(if demo {
                    "browsing is disabled in demo mode"
                } else {
                    "remote connection: only ISOs in the remote host's pools can be used"
                });
            }
        }
        KeyCode::Down => p.move_sel(1, cands),
        KeyCode::Up => p.move_sel(-1, cands),
        KeyCode::Tab if p.browse.is_none() => p.cycle_source(cands),
        KeyCode::Backspace => p.backspace(),
        KeyCode::Esc => wz.picker = None,
        KeyCode::Enter => wz.picker_enter(),
        KeyCode::Char(c) if !ctrl => p.type_char(c),
        _ => {}
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
    st.adapt_to_host();
    st.error.clear();
    *wizard = Some(st);
}

/// Wizard keys. Text fields take every printable key (so names can contain
/// h, l, y, …); shortcuts use Ctrl. Returns true when the key was consumed.
pub(super) fn handle_wizard_key(
    key: KeyEvent,
    wizard: &mut Option<ui::overlays::wizard::WizardState>,
    uri: &str,
    iso_dirs: &[String],
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
    dash: &mut ui::views::dashboard::DashboardState,
) -> bool {
    use ui::overlays::wizard::{MediaKind, WField, WKind, WizardState};
    let Some(wz) = wizard else { return false };
    if wz.picker.is_some() {
        handle_iso_picker_key(key, wz, iso_dirs, demo);
        wz.autosave();
        return true;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if key.code == KeyCode::Char('v') && ctrl {
        wz.command_review = !wz.command_review;
        wz.command_scroll = 0;
        return true;
    }
    if wz.command_review {
        wz.command_scroll = wz.command_scroll.min(wz.command_max_scroll.get());
        match key.code {
            KeyCode::Esc => wz.command_review = false,
            KeyCode::Down | KeyCode::Char('j') => wz.command_scroll = wz.command_scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => wz.command_scroll = wz.command_scroll.saturating_sub(1),
            KeyCode::PageDown => wz.command_scroll = wz.command_scroll.saturating_add(5),
            KeyCode::PageUp => wz.command_scroll = wz.command_scroll.saturating_sub(5),
            KeyCode::Home => wz.command_scroll = 0,
            KeyCode::End => wz.command_scroll = wz.command_max_scroll.get(),
            _ => {}
        }
        wz.command_scroll = wz.command_scroll.min(wz.command_max_scroll.get());
        return true;
    }
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
                wz.adapt_to_host();
            }
            KeyCode::Char('y') => {
                let plan = crate::command::builders::install::virt_install(wz);
                copy_text(&crate::command::display::display(&plan.steps[0]));
                wz.error = String::from("command copied");
            }
            KeyCode::Char('e') => {
                wz.error = if demo || dry_run {
                    let message = String::from("[dry-run] edit as XML  ── virt-install --print-xml=1");
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
            wz.autosave_now();
            *wizard = None;
        }
        KeyCode::Tab | KeyCode::Down => wz.move_focus(1),
        KeyCode::BackTab | KeyCode::Up => wz.move_focus(-1),
        KeyCode::Backspace => wz.backspace(),
        KeyCode::Left => wz.adjust(-1),
        KeyCode::Right => wz.adjust(1),
        KeyCode::Enter if wz.media == MediaKind::Iso && wz.focused() == WField::Iso => wz.open_iso_picker(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn command_review_captures_actions_and_scrolls_back_from_end() {
        let mut wizard = Some(ui::overlays::wizard::WizardState {
            step: 5,
            ..ui::overlays::wizard::WizardState::demo_step3()
        });
        let mut history = crate::command::history::History::new(10);
        let mut messages = MessageLog::default();
        let mut dash = ui::views::dashboard::DashboardState::demo();
        let mut press = |wizard: &mut Option<ui::overlays::wizard::WizardState>, code, modifiers| {
            assert!(handle_wizard_key(
                KeyEvent::new(code, modifiers),
                wizard,
                "qemu:///session",
                &[],
                true,
                true,
                &mut history,
                &mut messages,
                &mut dash
            ));
        };
        press(&mut wizard, KeyCode::Char('v'), KeyModifiers::CONTROL);
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal
            .draw(|frame| {
                ui::overlays::wizard::render(
                    frame,
                    &crate::theme::builtin::tokyo_night(),
                    wizard.as_ref().unwrap(),
                )
            })
            .unwrap();
        press(&mut wizard, KeyCode::End, KeyModifiers::NONE);
        let at_end = wizard.as_ref().unwrap().command_scroll;
        assert!(
            at_end > 0 && at_end < u16::MAX,
            "End uses the actual command extent"
        );
        press(&mut wizard, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(wizard.as_ref().unwrap().command_scroll, at_end - 1);
        press(&mut wizard, KeyCode::Enter, KeyModifiers::NONE);
        assert!(wizard.as_ref().unwrap().command_review);
        press(&mut wizard, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!wizard.as_ref().unwrap().command_review);
        assert!(history.all().is_empty(), "review never creates a domain");
    }
}
