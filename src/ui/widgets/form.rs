//! Form fields (PLAN.md 7.12): label, value box, validation, pending tracking.

pub fn checkbox(on: bool) -> &'static str {
    if on { "[x]" } else { "[ ]" }
}
pub fn radio(on: bool) -> &'static str {
    if on { "(●)" } else { "( )" }
}

/// Parse size strings like `512M`, `8G`, `1.5T` into MiB.
pub fn parse_size_mib(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, mult) = if let Some(n) = s.strip_suffix(['G', 'g']) {
        (n, 1024.0)
    } else if let Some(n) = s.strip_suffix(['M', 'm']) {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix(['T', 't']) {
        (n, 1024.0 * 1024.0)
    } else if let Some(n) = s.strip_suffix(['K', 'k']) {
        (n, 1.0 / 1024.0)
    } else {
        (s, 1.0 / 1024.0 / 1024.0)
    };
    let v: f64 = num.trim().parse().ok()?;
    if v < 0.0 {
        return None;
    }
    Some((v * mult).round() as u64)
}

/// Field kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Number { min: i64, max: i64, step: i64 },
    Size,
    Checkbox,
    Radio(Vec<String>),
    Select(Vec<String>),
}

/// One form field with original + current values.
#[derive(Debug, Clone)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub kind: FieldKind,
    pub original: String,
    pub current: String,
    pub warning: Option<String>,
}

impl Field {
    /// Create with an initial value.
    pub fn new(id: &str, label: &str, kind: FieldKind, value: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            kind,
            original: value.to_string(),
            current: value.to_string(),
            warning: None,
        }
    }

    /// True when edited.
    pub fn dirty(&self) -> bool {
        self.current != self.original
    }

    /// Reset to original.
    pub fn undo(&mut self) {
        self.current.clone_from(&self.original);
        self.warning = None;
    }

    /// Validate the current value.
    pub fn valid(&self) -> bool {
        match &self.kind {
            FieldKind::Number { min, max, .. } => self
                .current
                .trim()
                .parse::<i64>()
                .is_ok_and(|v| v >= *min && v <= *max),
            FieldKind::Size => parse_size_mib(&self.current).is_some(),
            _ => true,
        }
    }

    /// Adjust numeric fields (`h/l` ±1, `H/L` ×step, `C-a/C-x` ±1).
    pub fn adjust(&mut self, delta: i64) {
        if let FieldKind::Number { min, max, .. } = &self.kind {
            let v: i64 = self.current.trim().parse().unwrap_or(*min);
            self.current = v.saturating_add(delta).clamp(*min, *max).to_string();
        }
    }
}

/// A form: ordered fields with a focused index.
#[derive(Debug, Clone, Default)]
pub struct Form {
    pub fields: Vec<Field>,
    pub focus: usize,
}

impl Form {
    /// Create from fields.
    pub fn new(fields: Vec<Field>) -> Self {
        Self { fields, focus: 0 }
    }

    /// Focused field.
    pub fn focused(&self) -> Option<&Field> {
        self.fields.get(self.focus)
    }

    /// Focused field mutably.
    pub fn focused_mut(&mut self) -> Option<&mut Field> {
        self.fields.get_mut(self.focus)
    }

    /// Move focus.
    pub fn move_focus(&mut self, delta: i32) {
        if self.fields.is_empty() {
            return;
        }
        let n = self.fields.len() as i32;
        self.focus = (self.focus as i32 + delta).rem_euclid(n) as usize;
    }

    /// Dirty fields.
    pub fn dirty(&self) -> Vec<&Field> {
        self.fields.iter().filter(|f| f.dirty()).collect()
    }

    /// Undo one field by id.
    pub fn undo(&mut self, id: &str) {
        if let Some(f) = self.fields.iter_mut().find(|f| f.id == id) {
            f.undo();
        }
    }
}
