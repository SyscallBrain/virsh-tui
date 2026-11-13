//! Networks view actions.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Networks engine-action dispatch. Returns true when the app should quit.
#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_networks(
    action: crate::input::engine::KeyAction,
    nets: &mut ui::views::networks::NetworksState,
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
    confirm: &mut Option<crate::command::plan::PendingConfirm>,
) -> bool {
    use crate::command::builders::network as nb;
    use crate::input::engine::KeyAction as A;
    use crate::ui::views::networks::NetFocus;
    match action {
        A::MoveDown | A::MoveUp => {
            nets.move_selection(if action == A::MoveDown { 1 } else { -1 });
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
            preview_net_plan(&plan, nets, dry_run, demo, history, messages);
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
                    kind: crate::ui::overlays::confirm::ConfirmKind::Always,
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
            preview_net_plan(&plan, nets, dry_run, demo, history, messages);
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
                    kind: crate::ui::overlays::confirm::ConfirmKind::Always,
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
                preview_net_plan(&plan, nets, dry_run, demo, history, messages);
            }
        }
        A::LeaseUnpin => {
            if let Some(l) = nets.selected_lease() {
                let plan = nb::unpin_lease(nets.selected_name(), &l.mac.clone());
                preview_net_plan(&plan, nets, dry_run, demo, history, messages);
            }
        }
        A::YankIp => {
            nets.message = match nets.selected_lease().map(|l| l.ip.clone()) {
                Some(ip) => {
                    copy_text(&ip);
                    format!("✓ Copied IP {ip}")
                }
                None => String::from("✗ no lease selected"),
            };
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

/// Preview a network plan in demo or dry-run mode; live plans are jobs.
fn preview_net_plan(
    plan: &crate::command::plan::CommandPlan,
    nets: &mut ui::views::networks::NetworksState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) {
    if !demo && !dry_run {
        nets.message = "✗ Network request was not submitted".into();
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
    nets.message = format!("{prefix} {}  ── {first}", plan.summary);
    messages.push_action(nets.message.clone(), nets.selected_name(), true);
}

/// Networks-view keys. Returns true when consumed (form editing + raw overrides).
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_networks_key(
    key: KeyEvent,
    nets: &mut ui::views::networks::NetworksState,
    dry_run: bool,
    demo: bool,
    history: &mut crate::command::history::History,
    messages: &mut MessageLog,
) -> bool {
    use crate::input::key::KeySeq;
    if nets.new_form.is_some() {
        match key.code {
            KeyCode::Esc => {
                nets.new_form = None;
                return true;
            }
            KeyCode::Enter if key.modifiers.is_empty() => {
                if demo || dry_run {
                    match prepare_network_create(nets) {
                        Ok((name, _)) => {
                            nets.new_form = None;
                            nets.message = format!("[dry-run] Defined network {name}  ── virsh net-define");
                            messages.push_action(nets.message.clone(), &name, true);
                        }
                        Err(message) => nets.message = message,
                    }
                } else {
                    nets.message = "✗ Network request was not submitted".into();
                }
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
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
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
            nets.message = if demo || dry_run {
                format!("[dry-run] Edited network {net}  ── virsh net-define /tmp/vt-net.xml")
            } else {
                "✗ Network editor request was not submitted".into()
            };
            messages.push_action(nets.message.clone(), &net, demo || dry_run);
            history.push(&format!("virsh net-define <edited {net}.xml>"));
            true
        }
        _ => false,
    }
}

/// Validate the form and capture the XML without changing the draft.
pub(super) fn prepare_network_create(
    nets: &ui::views::networks::NetworksState,
) -> Result<(String, String), String> {
    use crate::command::builders::network as nb;
    let form = nets
        .new_form
        .as_ref()
        .ok_or_else(|| String::from("✗ no network form"))?;
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
        return Err(String::from(
            "✗ network name must be non-empty and use only letters, digits, - _ .",
        ));
    }
    if nets.networks.iter().any(|n| n.name == name) {
        return Err(format!("✗ a network named {name} already exists"));
    }
    // Linux interface names: at most 15 bytes, no `/` or whitespace.
    if bridge.is_empty()
        || bridge.len() > 15
        || !bridge
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return Err(format!(
            "✗ invalid bridge name `{bridge}` (1-15 characters: letters, digits, - _ .)"
        ));
    }
    // Bridge mode attaches to an existing host bridge: no addressing.
    if mode == "bridge" {
        let xml = nb::define_xml(&name, &mode, &bridge, &cidr, "", "", &dns);
        return Ok((name, xml));
    }
    let Some((network, broadcast)) = nb::subnet_bounds(&cidr) else {
        return Err(format!(
            "✗ invalid IPv4 CIDR `{cidr}` (e.g. 192.168.100.0/24, prefix 8-30)"
        ));
    };
    let gateway = nb::gateway_for(&cidr).map(u32::from).unwrap_or(network + 1);
    let typed = get("dhcp");
    let (start, end) = if typed.trim().is_empty() {
        nb::default_dhcp_range(&cidr).ok_or_else(|| format!("✗ {cidr} has no room for DHCP"))?
    } else {
        let (a, b) = typed
            .split_once('–')
            .or_else(|| typed.split_once('-'))
            .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
            .ok_or_else(|| format!("✗ invalid DHCP range `{typed}` (start – end)"))?;
        let (Ok(sa), Ok(sb)) = (a.parse::<std::net::Ipv4Addr>(), b.parse::<std::net::Ipv4Addr>()) else {
            return Err(format!("✗ invalid DHCP range `{typed}`"));
        };
        let (sa, sb) = (u32::from(sa), u32::from(sb));
        if sa > sb || sa <= network || sb >= broadcast || (sa..=sb).contains(&gateway) {
            return Err(format!(
                "✗ DHCP range {a} – {b} must lie inside {cidr}, without the gateway {}",
                std::net::Ipv4Addr::from(gateway)
            ));
        }
        (a, b)
    };
    let xml = nb::define_xml(&name, &mode, &bridge, &cidr, &start, &end, &dns);
    Ok((name, xml))
}
