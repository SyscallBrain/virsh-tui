//! Mouse (`appearance.mouse`): wheel scrolls, clicks pick tabs and domains.

use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

/// Two clicks on the same row within this time open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

impl App {
    /// A text field or form has the keys: `j`/`k` would be typed there.
    fn typing(&self) -> bool {
        self.ex_line.is_some()
            || self.wizard.is_some()
            || self.migrate.is_some()
            || self.pool_form.is_some()
            || self.palette.is_some()
            || self.nets.new_form.is_some()
            || self.store.picker.is_some()
            || self
                .confirm
                .as_ref()
                .is_some_and(|(c, _, _)| c.type_name.is_some())
            || self.detail.as_ref().is_some_and(|d| d.hw_insert)
    }

    fn overlay_open(&self) -> bool {
        self.typing()
            || self.confirm.is_some()
            || self.settings.is_some()
            || self.help_filter.is_some()
            || self.messages_view.is_some()
    }

    /// Returns true when the app should quit (never, for the mouse).
    pub(super) fn on_mouse(&mut self, m: MouseEvent) -> bool {
        let key = |code: KeyCode| KeyEvent::new(code, KeyModifiers::NONE);
        match m.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let down = m.kind == MouseEventKind::ScrollDown;
                let code = match (self.typing(), down) {
                    (true, true) => KeyCode::Down,
                    (true, false) => KeyCode::Up,
                    (false, true) => KeyCode::Char('j'),
                    (false, false) => KeyCode::Char('k'),
                };
                self.handle_key_event(key(code))
            }
            MouseEventKind::Down(MouseButton::Left) if !self.overlay_open() => {
                if m.row == 0 {
                    if let Some(tab) = crate::ui::chrome::tab_at(m.column) {
                        use crate::input::engine::KeyAction as A;
                        let action = [
                            A::ViewDomains,
                            A::ViewHost,
                            A::ViewNetworks,
                            A::ViewStorage,
                            A::ViewEvents,
                        ][tab];
                        if !self.blocks_leaving_detail() {
                            switch_view(action, &mut self.view);
                            self.enter_view_if_changed();
                        }
                    }
                    return false;
                }
                if self.view != View::Dashboard {
                    return false;
                }
                let Some(row) = self.dash.row_at(m.column, m.row) else {
                    return false;
                };
                let again = self
                    .last_click
                    .is_some_and(|(at, r)| r == row && at.elapsed() < DOUBLE_CLICK);
                self.dash.selection = row;
                self.dash.update_visual();
                if again {
                    self.last_click = None;
                    self.request_detail();
                } else {
                    self.last_click = Some((std::time::Instant::now(), row));
                }
                false
            }
            _ => false,
        }
    }
}
