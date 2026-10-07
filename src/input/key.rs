//! Key chord parsing: normalized token per key press.
//!
//! Tokens: single chars (`j`, `g`, `3`, `?`, `/`, `:`), `Space` (leader),
//! `Enter`, `Esc`, `Tab`, `C-d` (ctrl), `S-x`? (shift handled via char case),
//! `Up`/`Down`/`Left`/`Right`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A normalized key token.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeySeq(pub String);

impl KeySeq {
    /// Parse a test/helper token.
    pub fn parse(s: &str) -> Self {
        Self(normalize_token(s))
    }

    /// Normalize a live crossterm event.
    pub fn from_event(key: &KeyEvent) -> Self {
        Self::parse(&event_token(key))
    }

    /// Token text.
    pub fn token(&self) -> &str {
        &self.0
    }
}

fn normalize_token(s: &str) -> String {
    match s {
        "␣" | "<Space>" | " " => "Space".to_string(),
        "<Enter>" | "⏎" => "Enter".to_string(),
        "<Esc>" | "⎋" => "Esc".to_string(),
        "<Tab>" | "⇥" => "Tab".to_string(),
        "<S-Tab>" | "⇧⇥" => "S-Tab".to_string(),
        "<BS>" | "⌫" => "Backspace".to_string(),
        _ => s.to_string(),
    }
}

fn event_token(key: &KeyEvent) -> String {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let base = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                return format!("C-{}", c.to_lowercase());
            }
            if alt {
                return format!("A-{c}");
            }
            c.to_string()
        }
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "S-Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        _ => return "Ignored".to_string(),
    };
    if base == " " { "Space".to_string() } else { base }
}

#[cfg(test)]
mod tests {
    use super::KeySeq;

    #[test]
    fn parses_leader_and_ctrl() {
        assert_eq!(KeySeq::parse("␣").token(), "Space");
        assert_eq!(KeySeq::parse("C-d").token(), "C-d");
    }
}
