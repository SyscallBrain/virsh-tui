//! Single-line editable text input (INSERT fields, palette, ex line).

/// Editable line with cursor.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    text: String,
    cursor: usize,
    /// `/` filter input (live filtering) instead of a `:` command.
    pub filter: bool,
}

impl TextInput {
    /// Create empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with initial text (cursor at end).
    pub fn with(text: &str) -> Self {
        Self {
            cursor: text.len(),
            text: text.to_string(),
            filter: false,
        }
    }

    /// `/` filter input pre-filled with the current filter.
    pub fn filter(text: &str) -> Self {
        Self {
            filter: true,
            ..Self::with(text)
        }
    }

    /// Insert a char at the cursor.
    pub fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Backspace.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.text[..self.cursor]
            .chars()
            .next_back()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        self.cursor -= prev;
        self.text.remove(self.cursor);
    }

    /// Move the cursor one character left (char boundaries, not bytes).
    pub fn move_left(&mut self) {
        if let Some(c) = self.text[..self.cursor].chars().next_back() {
            self.cursor -= c.len_utf8();
        }
    }

    /// Move the cursor one character right.
    pub fn move_right(&mut self) {
        if let Some(c) = self.text[self.cursor..].chars().next() {
            self.cursor += c.len_utf8();
        }
    }

    /// Delete the character under the cursor.
    pub fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    /// Cursor to the start (Home, `C-a`).
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Cursor to the end (End, `C-e`).
    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    /// `C-u`: delete everything before the cursor.
    pub fn kill_to_start(&mut self) {
        self.text.drain(..self.cursor);
        self.cursor = 0;
    }

    /// `C-w`: delete the word before the cursor (and the spaces after it).
    pub fn kill_word(&mut self) {
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end_matches(' ');
        let start = trimmed.rfind(' ').map_or(0, |i| i + 1);
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }

    /// Replace the whole value (cursor at the end).
    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
    }

    /// Current value.
    pub fn value(&self) -> &str {
        &self.text
    }

    /// Cursor byte offset.
    pub fn cursor(&self) -> usize {
        self.cursor
    }
}

#[cfg(test)]
mod utf8_tests {
    use super::TextInput;

    #[test]
    fn cursor_moves_by_chars() {
        let mut t = TextInput::with("é␣");
        t.move_left();
        t.insert('x');
        assert_eq!(t.value(), "éx␣");
        t.move_left();
        t.move_left();
        t.move_right();
        t.insert('y');
        assert_eq!(t.value(), "éyx␣");
    }
}
