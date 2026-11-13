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
        KeyCode::PageDown => {
            pal.pane_focused = true;
            let (min, max) = pal.reference_scroll_bounds.get();
            pal.reference_scroll = pal
                .reference_scroll
                .clamp(min, max)
                .saturating_add(3)
                .clamp(min, max);
        }
        KeyCode::PageUp => {
            pal.pane_focused = true;
            let (min, max) = pal.reference_scroll_bounds.get();
            pal.reference_scroll = pal
                .reference_scroll
                .clamp(min, max)
                .saturating_sub(3)
                .clamp(min, max);
        }
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
                pal.reference_scroll = 0;
                let n = pal.doc_for_selected().map_or(0, |d| d.options.len());
                pal.opt_cursor = (pal.opt_cursor + 1).min(n.saturating_sub(1));
            } else {
                pal.move_selection(1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up if ctrl || key.code == KeyCode::Up => {
            if pal.pane_focused {
                pal.reference_scroll = 0;
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

#[cfg(test)]
mod compact_scroll_tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(palette: &Option<ui::overlays::palette::PaletteState>, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                ui::overlays::palette::render(
                    frame,
                    &crate::theme::builtin::tokyo_night(),
                    palette.as_ref().unwrap(),
                )
            })
            .unwrap();
        (0..height)
            .map(|y| {
                let mut row = (0..width)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>();
                row.push('\n');
                row
            })
            .collect()
    }

    fn key(palette: &mut Option<ui::overlays::palette::PaletteState>, code: KeyCode) {
        assert!(handle_palette_key(
            KeyEvent::new(code, KeyModifiers::NONE),
            palette,
            &mut None
        ));
    }

    #[test]
    fn compact_palette_scroll_reverses_immediately_at_both_ends_and_after_resize() {
        let mut state = ui::overlays::palette::PaletteState::demo();
        state.pane_focused = true;
        let mut palette = Some(state);
        render(&palette, 80, 24);
        for _ in 0..100 {
            key(&mut palette, KeyCode::PageDown);
            render(&palette, 80, 24);
        }
        let bottom = render(&palette, 80, 24);
        key(&mut palette, KeyCode::PageUp);
        assert_ne!(
            render(&palette, 80, 24),
            bottom,
            "PgUp must leave the bottom immediately"
        );
        for _ in 0..100 {
            key(&mut palette, KeyCode::PageUp);
            render(&palette, 80, 24);
        }
        let top = render(&palette, 80, 24);
        key(&mut palette, KeyCode::PageDown);
        assert_ne!(
            render(&palette, 80, 24),
            top,
            "PgDn must leave the top immediately"
        );
        for _ in 0..100 {
            key(&mut palette, KeyCode::PageDown);
        }
        let resized_bottom = render(&palette, 100, 26);
        key(&mut palette, KeyCode::PageUp);
        assert_ne!(
            render(&palette, 100, 26),
            resized_bottom,
            "PgUp must leave the resized bottom immediately"
        );
    }
}
