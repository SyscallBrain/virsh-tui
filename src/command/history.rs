//! Executed command history (`yc` copies the last one, `:messages` shows the log).

/// Bounded history of display strings.
#[derive(Debug, Clone, Default)]
pub struct History {
    entries: Vec<String>,
    capacity: usize,
}

impl History {
    /// Create with capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    /// Append a display string.
    pub fn push(&mut self, cmd: &str) {
        if self.entries.len() == self.capacity {
            self.entries.remove(0);
        }
        self.entries.push(cmd.to_string());
    }

    /// Most recent command.
    pub fn last(&self) -> Option<&str> {
        self.entries.last().map(|s| s.as_str())
    }

    /// All entries oldest to newest.
    pub fn all(&self) -> &[String] {
        &self.entries
    }
}
