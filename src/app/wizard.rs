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
pub(super) async fn handle_wizard_key(
    key: KeyEvent,
    wizard: &mut Option<ui::overlays::wizard::WizardState>,
    uri: &str,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
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
                wz.error = edit_wizard_xml(uri, wz, dry_run, demo, history, messages).await;
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
                match run_wizard(uri, wz, dry_run, demo, history, messages).await {
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

/// Create the domain (Review › Enter). Returns the success message.
pub(super) async fn run_wizard(
    uri: &str,
    wz: &ui::overlays::wizard::WizardState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) -> Result<String, String> {
    let mut plan = crate::command::builders::install::virt_install(wz);
    // virt-install targets the URI explicitly (shown as `--connect URI`).
    plan.steps[0].argv.insert(0, uri.to_string());
    plan.steps[0].argv.insert(0, String::from("--connect"));
    let shown = crate::command::display::display(&plan.steps[0]);
    history.push(&shown);
    if demo || dry_run {
        let msg = format!("[dry-run] Created {}  ── {shown}", wz.name);
        messages.push(msg.clone());
        return Ok(msg);
    }
    if !wz.start_after {
        // Define without booting: render the XML with virt-install, then define it.
        return define_from_print_xml(uri, wz, history, messages).await;
    }
    match crate::command::exec::execute(uri, &plan, false).await {
        Ok(_) => {
            let msg = format!("✓ Created {}  ── {shown}", wz.name);
            messages.push(msg.clone());
            if wz.open_console {
                open_viewer(uri, &wz.name).await;
            }
            Ok(msg)
        }
        Err(e) => {
            messages.push(e.to_string());
            Err(e.to_string())
        }
    }
}

/// `virt-install … --print-xml` (stdout) or the failure reason (stderr).
async fn print_xml(uri: &str, wz: &ui::overlays::wizard::WizardState) -> Result<String, String> {
    let plan = crate::command::builders::install::virt_install(wz);
    let mut argv: Vec<String> = plan.steps[0].argv.clone();
    argv.push(String::from("--print-xml"));
    let out = tokio::process::Command::new("virt-install")
        .arg("--connect")
        .arg(uri)
        .args(&argv)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("cannot run virt-install: {e}"))?;
    let xml = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() && !xml.trim().is_empty() {
        return Ok(xml);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let reason = err
        .lines()
        .map(|l| l.trim_start_matches("ERROR").trim())
        .find(|l| !l.is_empty())
        .unwrap_or("no output");
    Err(format!(
        "virt-install --print-xml failed: {}",
        crate::model::sanitize(reason)
    ))
}

/// Create without starting: `--print-xml` → `virsh define`.
async fn define_from_print_xml(
    uri: &str,
    wz: &ui::overlays::wizard::WizardState,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) -> Result<String, String> {
    let xml = print_xml(uri, wz).await?;
    let tmp =
        crate::command::tempxml::write(&wz.name, &xml).map_err(|_| String::from("cannot write temp XML"))?;
    let path = crate::command::tempxml::path_arg(&tmp);
    let define = crate::command::plan::CommandPlan::single(
        "virsh",
        vec!["define", &path, "--validate"],
        &format!("Defined {} (not started)", wz.name),
    );
    history.push(&crate::command::display::display(&define.steps[0]));
    let msg = crate::command::exec::execute(uri, &define, false)
        .await
        .map_err(|e| e.to_string())?;
    messages.push(msg.clone());
    Ok(msg)
}

/// C-e: `--print-xml` → $EDITOR → `virsh define`. Returns the status text.
pub(super) async fn edit_wizard_xml(
    uri: &str,
    wz: &ui::overlays::wizard::WizardState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) -> String {
    if demo || dry_run {
        let msg = String::from("[dry-run] edit as XML  ── virt-install --print-xml");
        messages.push(msg.clone());
        return msg;
    }
    let xml = match print_xml(uri, wz).await {
        Ok(x) => x,
        Err(e) => {
            messages.push(format!("✗ {e}  ── virt-install --print-xml"));
            return e;
        }
    };
    let Ok(tmp) = crate::command::tempxml::write(&wz.name, &xml) else {
        return String::from("cannot write temp XML");
    };
    if !external::edit_file(tmp.path()) {
        return String::from("editor failed");
    }
    let path = crate::command::tempxml::path_arg(&tmp);
    let define = crate::command::plan::CommandPlan::single(
        "virsh",
        vec!["define", &path, "--validate"],
        &format!("Defined {}", wz.name),
    );
    history.push(&crate::command::display::display(&define.steps[0]));
    match crate::command::exec::execute(uri, &define, false).await {
        Ok(msg) => {
            messages.push(msg.clone());
            msg
        }
        Err(e) => {
            messages.push(e.to_string());
            e.to_string()
        }
    }
}
