//! Command palette and help overlay keys.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Palette keys. Returns true when consumed.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_palette_key(
    key: KeyEvent,
    palette: &mut Option<ui::overlays::palette::PaletteState>,
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
            run_palette_enter(palette, ex_line);
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
    true
}

/// Run the highlighted palette action.
///
/// Non-snapshot actions open the `:` line pre-filled with the exact virsh command
/// (selected flags included) so values can be completed before running it.
/// Destructive commands still go through the CONFIRM modal from there.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_palette_enter(
    palette: &mut Option<ui::overlays::palette::PaletteState>,
    ex_line: &mut Option<crate::input::textinput::TextInput>,
) {
    let Some(pal) = palette.take() else { return };
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
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            filter.push(c);
            return true;
        }
        _ => {}
    }
    false
}
