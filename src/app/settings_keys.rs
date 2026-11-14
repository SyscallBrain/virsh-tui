//! Settings › Keybindings keys: filter, capture a new sequence, confirm
//! conflicts, restore defaults; edits are saved and applied live.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::input::catalog::{info_of, seq_text};
use crate::input::engine::{KeyAction, KeyEngine};
use crate::input::key::KeySeq;
use crate::input::keymap_file::{Conflict, Edit, KeymapFile, Outcome};
use crate::input::textinput::TextInput;
use crate::ui::views::settings_keys::{Capture, KeyRow, KeysState, PendingEdit, visible_rows};

/// Longest sequence that can be captured.
const MAX_TOKENS: usize = 4;

fn label(action: KeyAction) -> &'static str {
    info_of(action).map_or(action.name(), |i| i.label)
}

/// `D is bound to destroy (force off). Replace? destroy (force off) will lose it.`
fn conflict_question(conflicts: &[Conflict]) -> String {
    let clauses: Vec<String> = conflicts
        .iter()
        .map(|c| format!("{} is bound to {}.", seq_text(&c.seq), label(c.action)))
        .collect();
    let mut losers: Vec<&str> = conflicts.iter().map(|c| label(c.action)).collect();
    losers.dedup();
    let tail = if losers.len() == 1 {
        format!("{} will lose it.", losers[0])
    } else {
        String::from("They will lose those keys.")
    };
    format!("{} Replace? {tail}", clauses.join(" "))
}

/// Preview `edit`: apply it, ask about conflicts, or report the refusal.
fn submit(edit: Edit, st: &mut KeysState, engine: &mut KeyEngine, file: &mut KeymapFile) {
    match file.preview(&edit) {
        Outcome::Rejected(m) => st.message = Some(m),
        Outcome::NeedsConfirm(conflicts) => {
            st.message = None;
            st.pending = Some(PendingEdit {
                question: conflict_question(&conflicts),
                edit,
            });
        }
        Outcome::Applied(_) => apply(&edit, st, engine, file),
    }
}

fn apply(edit: &Edit, st: &mut KeysState, engine: &mut KeyEngine, file: &mut KeymapFile) {
    match file.apply(edit) {
        Outcome::Applied(m) => {
            engine.set_map(file.map());
            st.message = Some(m);
        }
        Outcome::Rejected(m) => st.message = Some(m),
        Outcome::NeedsConfirm(_) => {}
    }
}

/// Keys of the Keybindings options pane. `false`: not handled here
/// (pane switching, `⎋` closing Settings).
pub(super) fn handle(
    key: KeyEvent,
    st: &mut KeysState,
    engine: &mut KeyEngine,
    file: &mut KeymapFile,
) -> bool {
    let plain = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
    if let Some(p) = st.pending.take() {
        match key.code {
            KeyCode::Char('y') if plain => apply(&p.edit, st, engine, file),
            KeyCode::Char('n') | KeyCode::Esc => st.message = None,
            _ => st.pending = Some(p),
        }
        return true;
    }
    if let Some(mut cap) = st.capture.take() {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => {
                let edit = if cap.add {
                    Edit::Add(cap.action, cap.tokens)
                } else {
                    Edit::Replace(cap.action, cap.tokens)
                };
                submit(edit, st, engine, file);
            }
            KeyCode::Backspace => {
                cap.tokens.pop();
                st.capture = Some(cap);
            }
            _ => {
                let token = KeySeq::from_event(&key).token().to_string();
                if token != "Ignored" && token != "C-c" && cap.tokens.len() < MAX_TOKENS {
                    cap.tokens.push(token);
                }
                st.capture = Some(cap);
            }
        }
        return true;
    }
    if let Some(mut input) = st.filter.take() {
        match key.code {
            KeyCode::Esc => st.query.clear(),
            KeyCode::Enter => st.query = input.value().to_string(),
            KeyCode::Backspace => {
                input.backspace();
                st.filter = Some(input);
            }
            KeyCode::Char(c) if plain => {
                input.insert(c);
                st.filter = Some(input);
            }
            _ => st.filter = Some(input),
        }
        st.sel = 0;
        return true;
    }
    let rows = visible_rows(&st.query, engine.map());
    st.sel = st.sel.min(rows.len().saturating_sub(1));
    let row = rows.get(st.sel).copied();
    match key.code {
        KeyCode::Esc if !st.query.is_empty() => {
            st.query.clear();
            st.sel = 0;
        }
        KeyCode::Char('/') if plain => {
            st.message = None;
            st.filter = Some(TextInput::filter(&st.query));
        }
        KeyCode::Char('j') | KeyCode::Down if plain => {
            st.sel = (st.sel + 1) % rows.len().max(1);
        }
        KeyCode::Char('k') | KeyCode::Up if plain => {
            st.sel = (st.sel + rows.len().max(1) - 1) % rows.len().max(1);
        }
        KeyCode::Enter | KeyCode::Char('a') if plain => match row {
            Some(KeyRow::Action(info)) => {
                st.message = None;
                st.capture = Some(Capture {
                    action: info.action,
                    add: key.code != KeyCode::Enter,
                    tokens: Vec::new(),
                });
            }
            Some(KeyRow::ResetAll) if key.code == KeyCode::Enter => {
                let n = file.custom_count();
                if n == 0 {
                    st.message = Some(String::from("no custom bindings"));
                } else if let Some(e) = file.error() {
                    st.message = Some(format!("✗ keymap.toml has errors, fix it first: {e}"));
                } else {
                    st.pending = Some(PendingEdit {
                        edit: Edit::ResetAll,
                        question: format!(
                            "Reset {n} custom bindings to defaults? A backup is kept in keymap.toml.bak."
                        ),
                    });
                }
            }
            _ => {}
        },
        KeyCode::Char('d') if plain => {
            if let Some(KeyRow::Action(info)) = row {
                submit(Edit::Default(info.action), st, engine, file);
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::tests::fixture_app;
    use super::super::*;
    use crate::input::catalog::keys_of;
    use crate::input::engine::KeyAction;
    use crate::input::keymap::default_map;
    use crate::ui::views::settings_keys::{KeyRow, visible_rows};

    fn k(c: char) -> KeyEvent {
        let m = if c.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        KeyEvent::new(KeyCode::Char(c), m)
    }
    fn code(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|t| t.to_string()).collect()
    }

    /// Settings open on Keybindings, options focused, `action` selected.
    async fn on_action(app: &mut App, action: KeyAction) {
        app.on_key(k(',')).await;
        let st = app.settings.as_mut().unwrap();
        st.cat = 2;
        st.pane = ui::views::settings::SettingsPane::Options;
        st.keys.sel = visible_rows("", &default_map())
            .iter()
            .position(|r| matches!(r, KeyRow::Action(i) if i.action == action))
            .unwrap();
    }

    async fn press(app: &mut App, keys: &[KeyEvent]) {
        for key in keys {
            app.on_key(*key).await;
        }
    }

    fn keys(app: &App) -> &ui::views::settings_keys::KeysState {
        &app.settings.as_ref().unwrap().keys
    }

    #[tokio::test]
    async fn rebind_from_settings_applies_live() {
        let mut app = fixture_app();
        on_action(&mut app, KeyAction::Shutdown).await;
        press(
            &mut app,
            &[code(KeyCode::Enter), k(' '), k('q'), code(KeyCode::Enter)],
        )
        .await;
        assert_eq!(
            app.engine.map().get(&s(&["Space", "q"])),
            Some(&KeyAction::Shutdown)
        );
        assert!(!app.engine.map().contains_key(&s(&["S"])));
        assert!(keys(&app).capture.is_none());
    }

    #[tokio::test]
    async fn conflict_asks_before_stealing_destroy() {
        let mut app = fixture_app();
        on_action(&mut app, KeyAction::Shutdown).await;
        press(&mut app, &[code(KeyCode::Enter), k('D'), code(KeyCode::Enter)]).await;
        let question = keys(&app).pending.as_ref().map(|p| p.question.clone()).unwrap();
        assert!(
            question.contains("D is bound to destroy (force off)"),
            "{question}"
        );
        assert_eq!(app.engine.map().get(&s(&["D"])), Some(&KeyAction::Destroy));
        press(&mut app, &[k('n')]).await;
        assert!(keys(&app).pending.is_none());
        assert_eq!(app.engine.map().get(&s(&["D"])), Some(&KeyAction::Destroy));
        press(
            &mut app,
            &[code(KeyCode::Enter), k('D'), code(KeyCode::Enter), k('y')],
        )
        .await;
        assert_eq!(app.engine.map().get(&s(&["D"])), Some(&KeyAction::Shutdown));
        assert!(keys_of(app.engine.map(), KeyAction::Destroy).is_empty());
    }

    #[tokio::test]
    async fn default_restores_shutdown() {
        let mut app = fixture_app();
        on_action(&mut app, KeyAction::Shutdown).await;
        press(
            &mut app,
            &[code(KeyCode::Enter), k(' '), k('q'), code(KeyCode::Enter)],
        )
        .await;
        press(&mut app, &[k('d')]).await;
        assert_eq!(app.engine.map().get(&s(&["S"])), Some(&KeyAction::Shutdown));
        assert!(!app.engine.map().contains_key(&s(&["Space", "q"])));
    }

    #[tokio::test]
    async fn capture_rejects_fixed_key() {
        let mut app = fixture_app();
        on_action(&mut app, KeyAction::Shutdown).await;
        press(&mut app, &[code(KeyCode::Enter), k('q'), code(KeyCode::Enter)]).await;
        assert_eq!(
            keys(&app).message.as_deref(),
            Some("✗ q is a fixed key and cannot start a binding")
        );
        assert_eq!(app.engine.map(), &default_map());
    }

    #[tokio::test]
    async fn add_key_keeps_existing() {
        let mut app = fixture_app();
        on_action(&mut app, KeyAction::Console).await;
        press(
            &mut app,
            &[
                k('a'),
                KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
                code(KeyCode::Enter),
            ],
        )
        .await;
        assert_eq!(
            keys_of(app.engine.map(), KeyAction::Console),
            vec![s(&["c"]), s(&["C-t"])]
        );
    }

    #[tokio::test]
    async fn reset_all_asks_then_restores_defaults() {
        let mut app = fixture_app();
        on_action(&mut app, KeyAction::Shutdown).await;
        press(
            &mut app,
            &[code(KeyCode::Enter), k(' '), k('q'), code(KeyCode::Enter)],
        )
        .await;
        press(
            &mut app,
            &[k('k'), code(KeyCode::Enter), k(' '), k('z'), code(KeyCode::Enter)],
        )
        .await;
        assert_ne!(app.engine.map(), &default_map());
        let last = visible_rows("", app.engine.map()).len() - 1;
        app.settings.as_mut().unwrap().keys.sel = last;
        press(&mut app, &[code(KeyCode::Enter)]).await;
        assert!(keys(&app).pending.is_some());
        assert_ne!(app.engine.map(), &default_map());
        press(&mut app, &[k('y')]).await;
        assert_eq!(app.engine.map(), &default_map());
    }

    #[tokio::test]
    async fn keybindings_filter_works_but_change_is_blocked_during_job() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        on_action(&mut app, KeyAction::Shutdown).await;
        press(&mut app, &[k('/'), k('s'), k('n'), k('a'), k('p')]).await;
        assert_eq!(keys(&app).effective_query(), "snap");
        press(&mut app, &[code(KeyCode::Enter)]).await;
        assert!(keys(&app).filter.is_none());
        assert_eq!(keys(&app).query, "snap");
        press(&mut app, &[code(KeyCode::Enter)]).await;
        assert!(keys(&app).capture.is_none());
        assert!(app.dash.message.contains("is running"), "{}", app.dash.message);
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn esc_cancels_capture_before_closing_settings() {
        let mut app = fixture_app();
        on_action(&mut app, KeyAction::Shutdown).await;
        press(&mut app, &[code(KeyCode::Enter)]).await;
        assert!(keys(&app).capture.is_some());
        press(&mut app, &[code(KeyCode::Esc)]).await;
        assert!(app.settings.is_some());
        assert!(keys(&app).capture.is_none());
        press(&mut app, &[code(KeyCode::Esc)]).await;
        assert!(app.settings.is_none());
    }
}
