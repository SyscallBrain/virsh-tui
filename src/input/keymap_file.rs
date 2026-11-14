//! `keymap.toml` as an editable document: Settings › Keybindings changes,
//! adds, restores and resets bindings, keeping comments and other tables.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, Item, Table, Value};

use super::catalog::{keys_of, seq_text, validate};
use super::engine::KeyAction;
use super::keymap::{Keymap, apply_overrides, default_map};

/// One change requested from Settings › Keybindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The sequence becomes the action's only key.
    Replace(KeyAction, Vec<String>),
    /// The sequence is added to the action's keys.
    Add(KeyAction, Vec<String>),
    /// The action goes back to its default keys.
    Default(KeyAction),
    /// Every action goes back to its defaults.
    ResetAll,
}

/// A sequence another binding would lose (equal or prefix-related).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub action: KeyAction,
    pub seq: Vec<String>,
}

/// Result of [`KeymapFile::preview`] / [`KeymapFile::apply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Done (apply) or would be done without asking (preview).
    Applied(String),
    /// Other bindings lose these sequences: ask first.
    NeedsConfirm(Vec<Conflict>),
    /// Nothing changes.
    Rejected(String),
}

/// `keymap.toml` loaded as an editable document.
#[derive(Debug, Clone)]
pub struct KeymapFile {
    /// Where edits are saved; `None` keeps them in memory (demo, tests).
    path: Option<PathBuf>,
    doc: DocumentMut,
    /// Raw text when it did not parse.
    raw: String,
    error: Option<String>,
}

/// Final keys per action: `Some(list)` writes the entry, `None` removes it.
type Writes = Vec<(KeyAction, Option<Vec<Vec<String>>>)>;

fn related(a: &[String], b: &[String]) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

impl KeymapFile {
    /// Load the file at `read_from` (or `path`); edits are saved to `path`.
    pub fn load(path: Option<PathBuf>, read_from: Option<&Path>) -> Self {
        let source = read_from.map(Path::to_path_buf).or_else(|| path.clone());
        let raw = source
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        match raw.parse::<DocumentMut>() {
            Ok(doc) => Self {
                path,
                doc,
                raw: String::new(),
                error: None,
            },
            Err(e) => Self {
                path,
                doc: DocumentMut::new(),
                error: Some(e.to_string().lines().next().unwrap_or_default().to_string()),
                raw,
            },
        }
    }

    /// Empty, never saved.
    pub fn in_memory() -> Self {
        Self::load(None, None)
    }

    /// Parse error of the file, if any (edits are refused).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Live map: defaults with the file's overrides.
    pub fn map(&self) -> Keymap {
        let text = if self.error.is_some() {
            self.raw.clone()
        } else {
            self.doc.to_string()
        };
        apply_overrides(default_map(), &text).0
    }

    /// The file has an entry for `action`.
    pub fn is_custom(&self, action: KeyAction) -> bool {
        self.tables().iter().any(|t| t.contains_key(action.name()))
    }

    /// Actions with an entry in the file.
    pub fn custom_count(&self) -> usize {
        super::catalog::CATALOG
            .iter()
            .filter(|i| self.is_custom(i.action))
            .count()
    }

    /// What `edit` would do, without writing.
    pub fn preview(&self, edit: &Edit) -> Outcome {
        match self.plan(edit) {
            Err(m) => Outcome::Rejected(m),
            Ok((conflicts, _)) if !conflicts.is_empty() => Outcome::NeedsConfirm(conflicts),
            Ok(_) => Outcome::Applied(self.message(edit)),
        }
    }

    /// Apply `edit` (conflicting sequences are taken from their actions)
    /// and save.
    pub fn apply(&mut self, edit: &Edit) -> Outcome {
        let writes = match self.plan(edit) {
            Err(m) => return Outcome::Rejected(m),
            Ok((_, writes)) => writes,
        };
        let message = self.message(edit);
        let before = self.doc.clone();
        let before_text = before.to_string();
        for (action, keys) in &writes {
            self.write_entry(*action, keys.as_deref());
        }
        match self.save(edit, &before_text) {
            Ok(()) => Outcome::Applied(message),
            Err(e) => {
                self.doc = before;
                Outcome::Rejected(format!("✗ save failed: {e}"))
            }
        }
    }

    fn message(&self, edit: &Edit) -> String {
        let mut m = match edit {
            Edit::Replace(a, _) | Edit::Add(a, _) => format!("saved {}", a.name()),
            Edit::Default(a) => format!("restored {} to its defaults", a.name()),
            Edit::ResetAll if self.path.is_some() => format!(
                "reset {} bindings to defaults (backup: keymap.toml.bak)",
                self.custom_count()
            ),
            Edit::ResetAll => format!("reset {} bindings to defaults", self.custom_count()),
        };
        if self.path.is_none() {
            m.push_str(" · demo: not saved");
        }
        m
    }

    /// Conflicts and the entries to write for `edit`.
    fn plan(&self, edit: &Edit) -> Result<(Vec<Conflict>, Writes), String> {
        if let Some(e) = &self.error {
            return Err(format!("✗ keymap.toml has errors, fix it first: {e}"));
        }
        let map = self.map();
        let (target, new_seqs, mut writes): (Option<KeyAction>, Vec<Vec<String>>, Writes) = match edit {
            Edit::Replace(a, seq) => {
                validate(seq)?;
                (Some(*a), vec![seq.clone()], vec![(*a, Some(vec![seq.clone()]))])
            }
            Edit::Add(a, seq) => {
                validate(seq)?;
                let mut own = keys_of(&map, *a);
                if own.contains(seq) {
                    return Err(format!("✗ already bound to {}", a.name()));
                }
                own.push(seq.clone());
                (None, vec![seq.clone()], vec![(*a, Some(own))])
            }
            Edit::Default(a) => (Some(*a), keys_of(&default_map(), *a), vec![(*a, None)]),
            Edit::ResetAll => {
                let writes = super::catalog::CATALOG
                    .iter()
                    .filter(|i| self.is_custom(i.action))
                    .map(|i| (i.action, None))
                    .collect();
                return Ok((Vec::new(), writes));
            }
        };
        // Replace/Default drop the target's own keys, so those never conflict;
        // Add keeps them, so a prefix clash with its own keys counts.
        let mut conflicts: Vec<Conflict> = map
            .iter()
            .filter(|(_, b)| Some(**b) != target)
            .filter(|(k, _)| new_seqs.iter().any(|s| related(k, s)))
            .map(|(k, b)| Conflict {
                action: *b,
                seq: k.clone(),
            })
            .collect();
        conflicts.sort_by_key(|c| (seq_text(&c.seq), c.action.name()));
        let mut lost: HashMap<KeyAction, Vec<Vec<String>>> = HashMap::new();
        for c in &conflicts {
            lost.entry(c.action).or_default().push(c.seq.clone());
        }
        for (action, gone) in lost {
            let keys = match writes.iter().position(|(a, _)| *a == action) {
                Some(i) => writes[i].1.take().unwrap_or_default(),
                None => keys_of(&map, action),
            };
            let kept: Vec<Vec<String>> = keys.into_iter().filter(|k| !gone.contains(k)).collect();
            writes.retain(|(a, _)| *a != action);
            writes.push((action, Some(kept)));
        }
        Ok((conflicts, writes))
    }

    /// `[normal]` and its sub-tables.
    fn tables(&self) -> Vec<&Table> {
        let Some(normal) = self.doc.get("normal").and_then(Item::as_table) else {
            return Vec::new();
        };
        let mut out = vec![normal];
        out.extend(normal.iter().filter_map(|(_, v)| v.as_table()));
        out
    }

    /// The table holding `name`: `[normal]` itself (`None`) or a sub-table.
    fn holder(&self, name: &str) -> Option<Option<String>> {
        let normal = self.doc.get("normal").and_then(Item::as_table)?;
        if normal.contains_key(name) {
            return Some(None);
        }
        normal
            .iter()
            .find(|(_, v)| v.as_table().is_some_and(|t| t.contains_key(name)))
            .map(|(k, _)| Some(k.to_string()))
    }

    fn write_entry(&mut self, action: KeyAction, keys: Option<&[Vec<String>]>) {
        let name = action.name();
        let holder = self.holder(name);
        if !self.doc.contains_key("normal") {
            self.doc["normal"] = toml_edit::table();
        }
        let Some(normal) = self.doc["normal"].as_table_mut() else {
            return;
        };
        let table = match &holder {
            Some(Some(sub)) => match normal.get_mut(sub).and_then(Item::as_table_mut) {
                Some(t) => t,
                None => return,
            },
            _ => normal,
        };
        match keys {
            None => {
                table.remove(name);
            }
            Some(keys) => {
                let array: Array = keys.iter().map(|k| seq_text(k)).collect();
                let mut value = Value::Array(array);
                if let Some(old) = table.get(name).and_then(Item::as_value) {
                    *value.decor_mut() = old.decor().clone();
                }
                table.set_implicit(false);
                table.insert(name, Item::Value(value));
            }
        }
    }

    fn save(&self, edit: &Edit, before_text: &str) -> std::io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir)?;
        if *edit == Edit::ResetAll && path.exists() {
            std::fs::write(path.with_file_name("keymap.toml.bak"), before_text)?;
        }
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        std::io::Write::write_all(&mut file, self.doc.to_string().as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::keymap::default_map;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|t| t.to_string()).collect()
    }

    fn file_with(text: &str) -> (tempfile::TempDir, std::path::PathBuf, KeymapFile) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keymap.toml");
        if !text.is_empty() {
            std::fs::write(&path, text).unwrap();
        }
        let file = KeymapFile::load(Some(path.clone()), None);
        (dir, path, file)
    }

    #[test]
    fn replace_writes_entry_and_map_resolves_it() {
        let (_d, path, mut f) = file_with("");
        let out = f.apply(&Edit::Replace(KeyAction::Shutdown, s(&["Space", "q"])));
        assert!(matches!(out, Outcome::Applied(_)), "{out:?}");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#"shutdown = ["Space q"]"#), "{text}");
        let map = f.map();
        assert_eq!(map.get(&s(&["Space", "q"])), Some(&KeyAction::Shutdown));
        assert!(!map.contains_key(&s(&["S"])));
        assert!(f.is_custom(KeyAction::Shutdown));
    }

    #[test]
    fn exact_conflict_needs_confirm_then_strips_other() {
        let (_d, path, mut f) = file_with("");
        let edit = Edit::Replace(KeyAction::Shutdown, s(&["D"]));
        assert_eq!(
            f.preview(&edit),
            Outcome::NeedsConfirm(vec![Conflict {
                action: KeyAction::Destroy,
                seq: s(&["D"])
            }])
        );
        assert!(!path.exists(), "preview writes nothing");
        assert!(matches!(f.apply(&edit), Outcome::Applied(_)));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("destroy = []"), "{text}");
        assert_eq!(f.map().get(&s(&["D"])), Some(&KeyAction::Shutdown));
    }

    #[test]
    fn prefix_conflicts_both_ways() {
        let (_d, _p, f) = file_with("");
        let Outcome::NeedsConfirm(mut c) = f.preview(&Edit::Replace(KeyAction::Start, s(&["g"]))) else {
            panic!("expected conflicts")
        };
        c.sort_by_key(|c| seq_text(&c.seq));
        assert_eq!(
            c,
            vec![
                Conflict {
                    action: KeyAction::GoTop,
                    seq: s(&["g", "g"])
                },
                Conflict {
                    action: KeyAction::GotoXml,
                    seq: s(&["g", "x"])
                },
            ]
        );
        assert_eq!(
            f.preview(&Edit::Replace(KeyAction::Start, s(&["g", "g", "x"]))),
            Outcome::NeedsConfirm(vec![Conflict {
                action: KeyAction::GoTop,
                seq: s(&["g", "g"])
            }])
        );
    }

    #[test]
    fn adding_own_key_is_rejected() {
        let (_d, _p, f) = file_with("");
        assert_eq!(
            f.preview(&Edit::Add(KeyAction::MoveDown, s(&["j"]))),
            Outcome::Rejected(String::from("✗ already bound to move_down"))
        );
    }

    #[test]
    fn add_keeps_default_keys() {
        let (_d, path, mut f) = file_with("");
        assert!(matches!(
            f.apply(&Edit::Add(KeyAction::PrevTab, s(&["C-h"]))),
            Outcome::Applied(_)
        ));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#"prev_tab = ["H", "[", "C-h"]"#), "{text}");
    }

    #[test]
    fn invalid_sequence_is_rejected() {
        let (_d, _p, f) = file_with("");
        assert_eq!(
            f.preview(&Edit::Replace(KeyAction::Start, s(&["q"]))),
            Outcome::Rejected(String::from("✗ q is a fixed key and cannot start a binding"))
        );
    }

    #[test]
    fn default_keys_conflict_is_reported() {
        let (_d, _p, mut f) = file_with("");
        f.apply(&Edit::Replace(KeyAction::Shutdown, s(&["D"])));
        assert_eq!(
            f.preview(&Edit::Default(KeyAction::Destroy)),
            Outcome::NeedsConfirm(vec![Conflict {
                action: KeyAction::Shutdown,
                seq: s(&["D"])
            }])
        );
        assert!(matches!(
            f.apply(&Edit::Default(KeyAction::Destroy)),
            Outcome::Applied(_)
        ));
        let map = f.map();
        assert_eq!(map.get(&s(&["D"])), Some(&KeyAction::Destroy));
        assert!(!f.is_custom(KeyAction::Destroy));
    }

    #[test]
    fn edit_keeps_comments_and_other_tables() {
        let (_d, path, mut f) =
            file_with("# mine\n[normal.domains]\nconsole = [\"C-t\"] # tty\n\n[other]\nx = 1\n");
        assert!(matches!(
            f.apply(&Edit::Replace(KeyAction::Console, s(&["C-y"]))),
            Outcome::Applied(_)
        ));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mine"), "{text}");
        assert!(text.contains("[other]\nx = 1"), "{text}");
        let domains = text.split("[normal.domains]").nth(1).unwrap();
        assert!(domains.contains(r#"console = ["C-y"]"#), "{text}");
        assert!(
            !text.contains("[normal]\n"),
            "no duplicate entry in [normal]: {text}"
        );
    }

    #[test]
    fn reset_all_backs_up_and_clears() {
        let old = "# keep me\n[normal]\nshutdown = [\"Space q\"]\nconsole = [\"C-t\"]\n";
        let (_d, path, mut f) = file_with(old);
        assert_eq!(f.custom_count(), 2);
        let out = f.apply(&Edit::ResetAll);
        assert_eq!(
            out,
            Outcome::Applied(String::from(
                "reset 2 bindings to defaults (backup: keymap.toml.bak)"
            ))
        );
        let bak = path.with_file_name("keymap.toml.bak");
        assert_eq!(std::fs::read_to_string(bak).unwrap(), old);
        assert!(std::fs::read_to_string(&path).unwrap().contains("# keep me"));
        assert_eq!(f.custom_count(), 0);
        assert_eq!(f.map(), default_map());
    }

    #[test]
    fn broken_file_refuses_edits() {
        let (_d, path, mut f) = file_with("[normal\n");
        assert!(f.error().is_some());
        let Outcome::Rejected(m) = f.apply(&Edit::Replace(KeyAction::Start, s(&["C-s"]))) else {
            panic!("expected rejection")
        };
        assert!(m.starts_with("✗ keymap.toml has errors, fix it first: "), "{m}");
        assert!(matches!(f.apply(&Edit::ResetAll), Outcome::Rejected(_)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[normal\n");
        assert!(!path.with_file_name("keymap.toml.bak").exists());
        assert_eq!(f.map(), default_map());
    }

    #[test]
    fn in_memory_never_writes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("keymap.toml");
        std::fs::write(&p, "[normal]\nconsole = [\"C-t\"]\n").unwrap();
        let mut f = KeymapFile::load(None, Some(&p));
        assert_eq!(f.map().get(&s(&["C-t"])), Some(&KeyAction::Console));
        let Outcome::Applied(m) = f.apply(&Edit::Replace(KeyAction::Start, s(&["C-s"]))) else {
            panic!("expected applied")
        };
        assert!(m.ends_with(" · demo: not saved"), "{m}");
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "[normal]\nconsole = [\"C-t\"]\n"
        );
        assert_eq!(f.map().get(&s(&["C-s"])), Some(&KeyAction::Start));
        assert_eq!(KeymapFile::in_memory().map(), default_map());
    }
}
