//! Default keymap (DESIGN.md section 6) plus TOML overrides.

use std::collections::HashMap;

use super::engine::KeyAction;

fn seq(keys: &[&str]) -> Vec<String> {
    keys.iter().map(|s| s.to_string()).collect()
}

/// Default bindings: sequence -> action.
pub fn default_map() -> HashMap<Vec<String>, KeyAction> {
    let mut m: HashMap<Vec<String>, KeyAction> = HashMap::new();
    let mut bind = |keys: &[&str], a: KeyAction| {
        m.insert(seq(keys), a);
    };
    // Navigation.
    bind(&["j"], KeyAction::MoveDown);
    bind(&["k"], KeyAction::MoveUp);
    bind(&["g", "g"], KeyAction::GoTop);
    bind(&["G"], KeyAction::GoBottom);
    bind(&["C-d"], KeyAction::HalfDown);
    bind(&["C-u"], KeyAction::HalfUp);
    bind(&["Enter"], KeyAction::Open);
    bind(&["Backspace"], KeyAction::Back);
    bind(&["q"], KeyAction::Back);
    bind(&["/"], KeyAction::Filter);
    bind(&["n"], KeyAction::NextMatch);
    bind(&["N"], KeyAction::PrevMatch);
    // Domain lifecycle.
    bind(&["s"], KeyAction::Start);
    bind(&["S"], KeyAction::Shutdown);
    bind(&["D"], KeyAction::Destroy);
    bind(&["r"], KeyAction::Reboot);
    bind(&["R"], KeyAction::Reset);
    bind(&["p"], KeyAction::Pause);
    bind(&["Z"], KeyAction::ManagedSave);
    bind(&["a"], KeyAction::Autostart);
    bind(&["c"], KeyAction::Console);
    bind(&["v"], KeyAction::Viewer);
    bind(&["e"], KeyAction::EditXml);
    bind(&["X"], KeyAction::Undefine);
    // Yank.
    bind(&["y", "y"], KeyAction::YankName);
    bind(&["y", "u"], KeyAction::YankUuid);
    bind(&["y", "i"], KeyAction::YankIp);
    bind(&["y", "c"], KeyAction::YankCmd);
    // Leader.
    bind(&["Space", "n"], KeyAction::NewDomain);
    bind(&["Space", "c"], KeyAction::Clone);
    bind(&["Space", "r"], KeyAction::Rename);
    bind(&["Space", "M"], KeyAction::Migrate);
    bind(&["Space", "s", "c"], KeyAction::SnapshotNew);
    bind(&["Space", "s", "r"], KeyAction::SnapshotRevert);
    bind(&["Space", "s", "d"], KeyAction::SnapshotDelete);
    bind(&["Space", "m", "i"], KeyAction::InsertMedia);
    bind(&["Space", "m", "e"], KeyAction::EjectMedia);
    bind(&["Space", "d", "a"], KeyAction::DiskAttach);
    bind(&["Space", "d", "r"], KeyAction::DiskResize);
    bind(&["Space", "i", "a"], KeyAction::NicAttach);
    bind(&["Space", "i", "l"], KeyAction::NicLink);
    bind(&["Space", "x"], KeyAction::ExportXml);
    bind(&["Space", "b"], KeyAction::BootOrder);
    // Monitor / views.
    bind(&["t"], KeyAction::WindowCycle);
    bind(&["o"], KeyAction::SortCycle);
    bind(&["f"], KeyAction::ChipCycle);
    bind(&["F"], KeyAction::ChipCycleBack);
    bind(&["m"], KeyAction::Mark);
    bind(&["V"], KeyAction::Visual);
    bind(&["l"], KeyAction::LeaseFocus);
    bind(&["Space", "l"], KeyAction::LeasePin);
    bind(&["Space", "L"], KeyAction::LeaseUnpin);
    bind(&["H"], KeyAction::PrevTab);
    bind(&["L"], KeyAction::NextTab);
    bind(&["["], KeyAction::PrevTab);
    bind(&["]"], KeyAction::NextTab);
    bind(&["u"], KeyAction::UndoField);
    bind(&["g", "x"], KeyAction::GotoXml);
    // View tabs: single digits resolve via KeyEngine::flush after an idle
    // timeout, so counts (15j) keep working while plain 1-5 switch views.
    bind(&["1"], KeyAction::ViewDomains);
    bind(&["2"], KeyAction::ViewHost);
    bind(&["3"], KeyAction::ViewNetworks);
    bind(&["4"], KeyAction::ViewStorage);
    bind(&["5"], KeyAction::ViewEvents);
    // Modes.
    bind(&[":"], KeyAction::Ex);
    bind(&["C-p"], KeyAction::Palette);
    bind(&["?"], KeyAction::Help);
    bind(&[","], KeyAction::Settings);
    bind(&["C-r"], KeyAction::Refresh);
    bind(&["C-l"], KeyAction::Redraw);
    // "." repeat is handled explicitly by the engine before map lookup.
    // Confirm modal keys (y/n) are handled by the modal itself, not the global map,
    // so that yank sequences (yy/yu/yi/yc) keep working.
    // Quit keys (q/ZZ/C-c) are handled at the app level for the same reason:
    // Z is managed-save and ZZ is quit.
    m
}

/// Apply `keymap.toml` overrides on top of the defaults.
///
/// ```toml
/// [normal]                       # any [normal] or [normal.<view>] table
/// shutdown = ["S", "Space q"]    # key sequences, space separated tokens
/// snapshot_new = ["Space s n"]
/// ```
///
/// An overridden action loses its default keys. Unknown actions and
/// unparsable files are reported in the returned warnings.
pub fn apply_overrides(
    mut map: HashMap<Vec<String>, KeyAction>,
    toml_text: &str,
) -> (HashMap<Vec<String>, KeyAction>, Vec<String>) {
    let mut warnings = Vec::new();
    let value: toml::Value = match toml::from_str(toml_text) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(format!("keymap.toml: {e}"));
            return (map, warnings);
        }
    };
    let mut tables: Vec<&toml::value::Table> = Vec::new();
    if let Some(normal) = value.get("normal").and_then(|v| v.as_table()) {
        tables.push(normal);
        for v in normal.values() {
            if let Some(t) = v.as_table() {
                tables.push(t);
            }
        }
    }
    for table in tables {
        for (name, keys) in table {
            let Some(keys) = keys.as_array() else { continue };
            let Some(action) = KeyAction::from_name(name) else {
                warnings.push(format!("keymap.toml: unknown action `{name}`"));
                continue;
            };
            map.retain(|_, a| *a != action);
            for k in keys.iter().filter_map(|k| k.as_str()) {
                let seq: Vec<String> = k
                    .split_whitespace()
                    .map(|t| super::key::KeySeq::parse(t).token().to_string())
                    .collect();
                if !seq.is_empty() {
                    map.insert(seq, action);
                }
            }
        }
    }
    (map, warnings)
}

/// Load `~/.config/virsh-tui/keymap.toml` (if any) over the defaults.
pub fn load() -> (HashMap<Vec<String>, KeyAction>, Vec<String>) {
    let path =
        directories::ProjectDirs::from("", "", "virsh-tui").map(|d| d.config_dir().join("keymap.toml"));
    match path.and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(text) => apply_overrides(default_map(), &text),
        None => (default_map(), Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_replace_default_keys() {
        let (map, warnings) = apply_overrides(
            default_map(),
            "[normal]\nshutdown = [\"Space q\"]\n[normal.domains]\nbogus = [\"x\"]\n",
        );
        assert_eq!(
            map.get(&vec![String::from("Space"), String::from("q")]),
            Some(&KeyAction::Shutdown)
        );
        assert!(!map.contains_key(&vec![String::from("S")]), "default S removed");
        assert_eq!(warnings.len(), 1);
        assert_eq!(KeyAction::from_name("snapshot_new"), Some(KeyAction::SnapshotNew));
    }
}
