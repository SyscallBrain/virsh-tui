//! Networks view actions.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Networks engine-action dispatch. Returns true when the app should quit.
#[allow(clippy::too_many_arguments)]
pub(super) async fn dispatch_networks(
    action: crate::input::engine::KeyAction,
    nets: &mut ui::views::networks::NetworksState,
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    uri: &str,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
) -> bool {
    use crate::command::builders::network as nb;
    use crate::input::engine::KeyAction as A;
    use crate::ui::views::networks::NetFocus;
    match action {
        A::MoveDown | A::MoveUp => {
            let before = nets.selected;
            nets.move_selection(if action == A::MoveDown { 1 } else { -1 });
            if !demo && nets.focus == NetFocus::Networks && nets.selected != before {
                load_net_selection(uri, nets).await;
            }
        }
        A::LeaseFocus => {
            nets.focus = if nets.focus == NetFocus::Leases {
                NetFocus::Networks
            } else {
                NetFocus::Leases
            };
        }
        A::Start => {
            let plan = nb::start(nets.selected_name());
            run_net_plan(uri, &plan, nets, dry_run, demo, history, messages).await;
        }
        A::Destroy => {
            let name = nets.selected_name().to_string();
            let build: crate::command::plan::PlanBuilder = std::sync::Arc::new(move |n: &str| nb::destroy(n));
            let commands = build(&name)
                .steps
                .iter()
                .map(crate::command::display::display)
                .collect();
            *confirm = Some((
                crate::ui::overlays::confirm::Confirm {
                    title: String::from("⚠ Destroy network"),
                    question: format!("Destroy network {name}?"),
                    commands,
                    confirm_label: String::from("y destroy"),
                    type_name: None,
                    typed: String::new(),
                },
                build,
                vec![name],
            ));
        }
        A::Autostart => {
            let name = nets.selected_name().to_string();
            let enable = !nets.networks.get(nets.selected).is_some_and(|n| n.autostart);
            let plan = nb::autostart(&name, enable);
            run_net_plan(uri, &plan, nets, dry_run, demo, history, messages).await;
        }
        A::Undefine => {
            let name = nets.selected_name().to_string();
            let build: crate::command::plan::PlanBuilder =
                std::sync::Arc::new(move |n: &str| nb::undefine(n));
            let commands = build(&name)
                .steps
                .iter()
                .map(crate::command::display::display)
                .collect();
            *confirm = Some((
                crate::ui::overlays::confirm::Confirm {
                    title: String::from("⚠ Undefine network"),
                    question: format!("Undefine network {name}?"),
                    commands,
                    confirm_label: String::from("y undefine"),
                    type_name: None,
                    typed: String::new(),
                },
                build,
                vec![name],
            ));
        }
        A::LeasePin => {
            if let Some(l) = nets.selected_lease() {
                let host = if l.hostname == "—" {
                    String::new()
                } else {
                    l.hostname.clone()
                };
                let plan = nb::pin_lease(nets.selected_name(), &l.mac.clone(), &host, &l.ip.clone());
                run_net_plan(uri, &plan, nets, dry_run, demo, history, messages).await;
            }
        }
        A::LeaseUnpin => {
            if let Some(l) = nets.selected_lease() {
                let plan = nb::unpin_lease(nets.selected_name(), &l.mac.clone());
                run_net_plan(uri, &plan, nets, dry_run, demo, history, messages).await;
            }
        }
        A::YankIp => {
            if let Some(l) = nets.selected_lease() {
                copy_text(&l.ip);
            }
        }
        A::Open if nets.focus == NetFocus::Leases => {
            // The app loop opens the real domain (needs the dashboard selection).
            if let Some(l) = nets.selected_lease() {
                if l.domain.is_empty() {
                    nets.message = format!("✗ no domain owns {}", l.mac);
                } else {
                    nets.pending_open = Some(l.domain.clone());
                }
            }
            let _ = (detail, view);
        }
        _ => {}
    }
    false
}

/// Run a network plan and update message/history.
pub(super) async fn run_net_plan(
    uri: &str,
    plan: &crate::command::plan::CommandPlan,
    nets: &mut ui::views::networks::NetworksState,
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
            nets.message = msg.clone();
            messages.push(msg);
        }
        Err(e) => {
            nets.message = e.to_string();
            messages.push(e.to_string());
        }
    }
    if !demo && !dry_run {
        reload_nets(uri, nets).await;
    }
}

/// Networks-view keys. Returns true when consumed (form editing + raw overrides).
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_networks_key(
    key: KeyEvent,
    nets: &mut ui::views::networks::NetworksState,
    uri: &str,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) -> bool {
    use crate::input::key::KeySeq;
    if nets.new_form.is_some() {
        match key.code {
            KeyCode::Esc => {
                nets.new_form = None;
                return true;
            }
            KeyCode::Enter if key.modifiers.is_empty() => {
                create_network(uri, nets, dry_run, demo, history, messages).await;
                return true;
            }
            KeyCode::Tab => {
                if let Some(form) = nets.new_form.as_mut() {
                    form.move_focus(1);
                }
                return true;
            }
            KeyCode::BackTab => {
                if let Some(form) = nets.new_form.as_mut() {
                    form.move_focus(-1);
                }
                return true;
            }
            KeyCode::Backspace => {
                if let Some(form) = nets.new_form.as_mut()
                    && let Some(f) = form.fields.get_mut(form.focus)
                {
                    f.current.pop();
                }
                return true;
            }
            KeyCode::Char(c) if key.modifiers.is_empty() => {
                if let Some(form) = nets.new_form.as_mut() {
                    if let Some(f) = form.fields.get_mut(form.focus) {
                        f.current.push(c);
                    }
                    if form.fields.get(form.focus).is_some_and(|f| f.id == "cidr") {
                        let cidr = form
                            .fields
                            .iter()
                            .find(|f| f.id == "cidr")
                            .map(|f| f.current.clone())
                            .unwrap_or_default();
                        if let Some((start, end)) = crate::ui::views::networks::dhcp_range(&cidr)
                            && let Some(f) = form.fields.iter_mut().find(|f| f.id == "dhcp")
                        {
                            f.current = format!("{start} – {end}");
                        }
                    }
                }
                return true;
            }
            _ => return true,
        }
    }
    let tok = KeySeq::from_event(&key);
    match tok.token() {
        "n" => {
            nets.open_new_form();
            true
        }
        "e" => {
            let net = nets.selected_name().to_string();
            nets.message = if demo {
                format!("[dry-run] Edited network {net}  ── virsh net-define /tmp/vt-net.xml")
            } else {
                edit_net_xml(uri, &net, dry_run).await
            };
            messages.push(nets.message.clone());
            history.push(&format!("virsh net-define <edited {net}.xml>"));
            true
        }
        _ => false,
    }
}

/// Create the network from the form: net-define + net-start + autostart.
pub(super) async fn create_network(
    uri: &str,
    nets: &mut ui::views::networks::NetworksState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut Vec<String>,
) {
    use crate::command::builders::network as nb;
    let Some(form) = nets.new_form.take() else {
        return;
    };
    let get = |id: &str| {
        form.fields
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.current.clone())
            .unwrap_or_default()
    };
    let name = get("name");
    let mode = get("mode");
    let bridge = get("bridge");
    let cidr = get("cidr");
    let dns = get("dns");
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        nets.message = String::from("✗ network name must be non-empty and use only letters, digits, - _ .");
        nets.new_form = Some(form);
        return;
    }
    if nets.networks.iter().any(|n| n.name == name) {
        nets.message = format!("✗ a network named {name} already exists");
        nets.new_form = Some(form);
        return;
    }
    // Use the DHCP range typed in the form ("a – b"), else derive it from the CIDR.
    let typed = get("dhcp");
    let parsed = typed
        .split_once('–')
        .or_else(|| typed.split_once('-'))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .filter(|(a, b)| a.parse::<std::net::Ipv4Addr>().is_ok() && b.parse::<std::net::Ipv4Addr>().is_ok());
    let Some((start, end)) = parsed.or_else(|| crate::ui::views::networks::dhcp_range(&cidr)) else {
        nets.message = format!("✗ invalid IPv4 CIDR {cidr}");
        nets.new_form = Some(form);
        return;
    };
    let xml = nb::define_xml(&name, &mode, &bridge, &cidr, &start, &end, &dns);
    if demo || dry_run {
        let msg = format!("[dry-run] Defined network {name}  ── virsh net-define");
        nets.message = msg.clone();
        messages.push(msg);
        return;
    }
    let tmp = match crate::command::tempxml::write("net", &xml) {
        Ok(t) => t,
        Err(_) => {
            nets.message = String::from("✗ cannot write temp XML");
            return;
        }
    };
    let path = crate::command::tempxml::path_arg(&tmp);
    for sub in ["net-define", "net-start", "net-autostart"] {
        let argv: Vec<String> = if sub == "net-define" {
            vec![sub.to_string(), path.clone()]
        } else {
            vec![sub.to_string(), name.clone()]
        };
        let plan = crate::command::plan::CommandPlan {
            steps: vec![crate::command::plan::CommandStep {
                program: String::from("virsh"),
                argv,
                stdin: None,
            }],
            summary: format!("{sub} {name}"),
        };
        history.push(&crate::command::display::display(&plan.steps[0]));
        match crate::command::exec::execute(uri, &plan, false).await {
            Ok(msg) => {
                nets.message = msg.clone();
                messages.push(msg);
            }
            Err(e) => {
                nets.message = e.to_string();
                messages.push(e.to_string());
                reload_nets(uri, nets).await;
                return;
            }
        }
    }
    nets.message = format!("✓ Created network {name}  ── virsh net-define {path}");
    reload_nets(uri, nets).await;
}

/// Edit network XML in $EDITOR (net-edit path): dumpxml → editor → net-define.
pub(super) async fn edit_net_xml(uri: &str, net: &str, dry_run: bool) -> String {
    let xml = match crate::backend::virsh::exec::run(uri, &["net-dumpxml", "--inactive", net]).await {
        Ok(x) => x,
        Err(e) => return format!("✗ net-dumpxml failed: {}", first_line(&e.to_string())),
    };
    let Ok(tmp) = crate::command::tempxml::write("net", &xml) else {
        return String::from("✗ cannot write temp XML");
    };
    if !external::edit_file(tmp.path()) {
        return String::from("✗ editor failed");
    }
    let edited = std::fs::read_to_string(tmp.path()).unwrap_or_default();
    if edited == xml {
        return format!("no changes to {net}");
    }
    let path = crate::command::tempxml::path_arg(&tmp);
    if dry_run {
        return format!("[dry-run] Edited network {net}  ── virsh net-define {path}");
    }
    match crate::backend::virsh::exec::run(uri, &["net-define", &path, "--validate"]).await {
        Ok(_) => format!("✓ Edited network {net}  ── virsh net-define {path} --validate"),
        Err(e) => format!("✗ net-define failed: {}", first_line(&e.to_string())),
    }
}
