//! Command palette and help overlay keys.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Open the palette (curated + live help index).
pub(super) fn open_palette(
    palette: &mut Option<ui::overlays::palette::PaletteState>,
    context: &str,
    uri: &str,
    demo: bool,
) {
    use ui::overlays::palette::PaletteState;
    if demo {
        *palette = Some(PaletteState::demo());
        return;
    }
    let index = crate::backend::virsh::parse_help::load_index(uri);
    let mut st = PaletteState::open_live(context, &index);
    if let Some(cmd) = st.needs_doc()
        && let Some(doc) = crate::backend::virsh::parse_help::load_doc(uri, &cmd)
    {
        st.docs.push(doc);
    }
    *palette = Some(st);
}

/// Palette keys. Returns true when consumed.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_palette_key(
    key: KeyEvent,
    palette: &mut Option<ui::overlays::palette::PaletteState>,
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    dash: &ui::views::dashboard::DashboardState,
    host: &ui::views::host::HostState,
    uri: &str,
    demo: bool,
    ex_line: &mut Option<crate::input::textinput::TextInput>,
) -> bool {
    let Some(pal) = palette.as_mut() else { return false };
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => {
            *palette = None;
            return true;
        }
        KeyCode::Enter => {
            run_palette_enter(palette, detail, view, dash, host, uri, demo, ex_line).await;
            return true;
        }
        KeyCode::Tab => pal.toggle_opt(),
        KeyCode::Backspace => {
            pal.query.pop();
            pal.move_selection(-(pal.selected as i32));
        }
        KeyCode::Char('y') if ctrl => {
            if let Some(cmd) = pal.command_line() {
                copy_text(&format!("virsh {cmd}"));
            }
        }
        KeyCode::Char('l') if ctrl => pal.pane_focused = !pal.pane_focused,
        KeyCode::Char('j') | KeyCode::Down if ctrl || key.code == KeyCode::Down => {
            if pal.pane_focused {
                let n = pal.doc_for_selected().map_or(0, |d| d.options.len());
                pal.opt_cursor = (pal.opt_cursor + 1).min(n.saturating_sub(1));
            } else {
                pal.move_selection(1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up if ctrl || key.code == KeyCode::Up => {
            if pal.pane_focused {
                pal.opt_cursor = pal.opt_cursor.saturating_sub(1);
            } else {
                pal.move_selection(-1);
            }
        }
        KeyCode::Char(c) if !ctrl => {
            pal.query.push(c);
            pal.move_selection(-(pal.selected as i32));
        }
        _ => return false,
    }
    // Load `virsh help <cmd>` for the highlighted action (cached in the state).
    if !demo
        && let Some(cmd) = pal.needs_doc()
        && let Some(doc) = crate::backend::virsh::parse_help::load_doc(uri, &cmd)
    {
        pal.docs.push(doc);
    }
    true
}

/// Run the highlighted palette action.
///
/// Snapshot actions open the real Snapshots tab with the matching modal;
/// everything else opens the `:` line pre-filled with the exact virsh command
/// (selected flags included) so values can be completed before running it.
/// Destructive commands still go through the CONFIRM modal from there.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_palette_enter(
    palette: &mut Option<ui::overlays::palette::PaletteState>,
    detail: &mut Option<ui::views::detail::DetailState>,
    view: &mut View,
    dash: &ui::views::dashboard::DashboardState,
    host: &ui::views::host::HostState,
    uri: &str,
    demo: bool,
    ex_line: &mut Option<crate::input::textinput::TextInput>,
) {
    let Some(pal) = palette.take() else { return };
    let Some(item) = pal.selected_item().cloned() else {
        return;
    };
    let modal = match item.virsh_cmd.as_str() {
        "snapshot-create-as" => Some(ui::views::detail::SnapModal::Create),
        "snapshot-revert" => Some(ui::views::detail::SnapModal::Revert),
        "snapshot-delete" => Some(ui::views::detail::SnapModal::Delete),
        "snapshot-list" => Some(ui::views::detail::SnapModal::None),
        _ => None,
    };
    if let Some(modal) = modal {
        if detail.is_none() {
            open_detail(detail, view, dash, host, uri, demo).await;
        }
        if let Some(d) = detail.as_mut() {
            d.tab = ui::views::detail::DetailTab::Snapshots;
            d.snap_modal = if d.snaps.rows.is_empty() && modal != ui::views::detail::SnapModal::Create {
                ui::views::detail::SnapModal::None
            } else {
                modal
            };
        }
        return;
    }
    if let Some(cmd) = pal.command_line() {
        *ex_line = Some(crate::input::textinput::TextInput::with(&format!("{cmd} ")));
    }
}

/// Help overlay keys (live `/` filter). Returns true when consumed.
pub(super) fn handle_help_key(key: KeyEvent, help_filter: &mut Option<String>) -> bool {
    let Some(filter) = help_filter else {
        return false;
    };
    match key.code {
        KeyCode::Esc => {
            *help_filter = None;
            return true;
        }
        KeyCode::Backspace => {
            filter.pop();
            return true;
        }
        KeyCode::Char(c) if key.modifiers.is_empty() => {
            filter.push(c);
            return true;
        }
        _ => {}
    }
    false
}
