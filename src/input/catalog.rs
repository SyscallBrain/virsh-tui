//! Remappable actions for Settings › Keybindings: section, label, and the
//! rules a new key sequence must follow.

use super::engine::KeyAction;
use super::keymap::Keymap;

/// One remappable action as listed in Settings › Keybindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionInfo {
    pub action: KeyAction,
    pub section: &'static str,
    pub label: &'static str,
    /// Painted red, as in `?`.
    pub destructive: bool,
}

const fn info(action: KeyAction, section: &'static str, label: &'static str) -> ActionInfo {
    ActionInfo {
        action,
        section,
        label,
        destructive: false,
    }
}

const fn danger(action: KeyAction, section: &'static str, label: &'static str) -> ActionInfo {
    ActionInfo {
        action,
        section,
        label,
        destructive: true,
    }
}

use KeyAction as A;

/// Every remappable action, in display order.
pub const CATALOG: &[ActionInfo] = &[
    info(A::MoveDown, "Navigation", "down"),
    info(A::MoveUp, "Navigation", "up"),
    info(A::GoTop, "Navigation", "top"),
    info(A::GoBottom, "Navigation", "bottom"),
    info(A::HalfDown, "Navigation", "half page down"),
    info(A::HalfUp, "Navigation", "half page up"),
    info(A::Open, "Navigation", "open"),
    info(A::Back, "Navigation", "back"),
    info(A::Filter, "Navigation", "filter"),
    info(A::NextMatch, "Navigation", "next match"),
    info(A::PrevMatch, "Navigation", "previous match"),
    info(A::PrevTab, "Navigation", "previous tab"),
    info(A::NextTab, "Navigation", "next tab"),
    info(A::ViewDomains, "Navigation", "view domains"),
    info(A::ViewHost, "Navigation", "view host"),
    info(A::ViewNetworks, "Navigation", "view networks"),
    info(A::ViewStorage, "Navigation", "view storage"),
    info(A::ViewEvents, "Navigation", "view events"),
    info(A::Visual, "Modes", "visual select"),
    info(A::Mark, "Modes", "mark / unmark"),
    info(A::Ex, "Modes", "ex command"),
    info(A::Palette, "Modes", "command palette"),
    info(A::Start, "Domain", "start"),
    info(A::Shutdown, "Domain", "shutdown (ACPI)"),
    danger(A::Destroy, "Domain", "destroy (force off)"),
    info(A::Reboot, "Domain", "reboot"),
    danger(A::Reset, "Domain", "reset"),
    info(A::Pause, "Domain", "pause / resume"),
    info(A::ManagedSave, "Domain", "suspend to disk"),
    info(A::ManagedSaveRemove, "Domain", "discard saved state"),
    info(A::Autostart, "Domain", "toggle autostart"),
    info(A::Console, "Domain", "serial console"),
    info(A::Viewer, "Domain", "graphical viewer"),
    info(A::EditXml, "Domain", "edit XML ($EDITOR)"),
    danger(A::Undefine, "Domain", "undefine"),
    info(A::SendKey, "Domain", "send key"),
    info(A::YankName, "Yank", "copy name"),
    info(A::YankUuid, "Yank", "copy uuid"),
    info(A::YankIp, "Yank", "copy ip address"),
    info(A::YankCmd, "Yank", "copy last virsh command"),
    info(A::NewDomain, "Leader", "new domain"),
    info(A::Clone, "Leader", "clone"),
    info(A::Rename, "Leader", "rename"),
    info(A::Migrate, "Leader", "migrate"),
    info(A::SnapshotNew, "Leader", "new snapshot"),
    info(A::SnapshotRevert, "Leader", "revert snapshot"),
    info(A::SnapshotDelete, "Leader", "delete snapshot"),
    info(A::InsertMedia, "Leader", "insert ISO"),
    info(A::EjectMedia, "Leader", "eject ISO"),
    info(A::DiskAttach, "Leader", "attach disk"),
    info(A::DiskResize, "Leader", "resize disk"),
    info(A::NicAttach, "Leader", "attach NIC"),
    info(A::NicLink, "Leader", "toggle NIC link"),
    info(A::ExportXml, "Leader", "export XML"),
    info(A::BootOrder, "Leader", "boot order"),
    info(A::WindowCycle, "Monitor", "chart window 60s · 5m · 1h"),
    info(A::SortCycle, "Monitor", "cycle sort"),
    info(A::ChipCycle, "Monitor", "next filter chip"),
    info(A::ChipCycleBack, "Monitor", "previous filter chip"),
    info(A::LeaseFocus, "Monitor", "focus leases"),
    info(A::LeasePin, "Monitor", "pin lease"),
    info(A::LeaseUnpin, "Monitor", "unpin lease"),
    info(A::UndoField, "Monitor", "undo field"),
    info(A::GotoXml, "Monitor", "go to XML tab"),
    info(A::Help, "App", "this help"),
    info(A::Settings, "App", "settings"),
    info(A::Refresh, "App", "force refresh"),
    info(A::Redraw, "App", "redraw"),
];

/// Catalogue entry of `action`.
pub fn info_of(action: KeyAction) -> Option<&'static ActionInfo> {
    CATALOG.iter().find(|i| i.action == action)
}

/// Check a captured sequence before it is bound.
pub fn validate(seq: &[String]) -> Result<(), String> {
    let Some(first) = seq.first() else {
        return Err(String::from("✗ no keys pressed"));
    };
    if let Some(bad) = seq.iter().find(|t| !super::key::KeySeq::parse(t).is_valid()) {
        return Err(format!("✗ unknown key {bad}"));
    }
    if matches!(first.as_str(), "q" | "." | "C-c") {
        return Err(format!("✗ {first} is a fixed key and cannot start a binding"));
    }
    if seq.len() > 1 && first == "Z" {
        return Err(String::from("✗ sequences cannot start with Z (ZZ quits)"));
    }
    if seq.len() > 1 && first.chars().all(|c| c.is_ascii_digit()) {
        return Err(String::from(
            "✗ a digit followed by keys is a count; use a single digit",
        ));
    }
    Ok(())
}

/// `Space s c`: tokens joined by one space (the keymap.toml form).
pub fn seq_text(seq: &[String]) -> String {
    seq.join(" ")
}

/// Sequences bound to `action`, shortest first.
pub fn keys_of(map: &Keymap, action: KeyAction) -> Vec<Vec<String>> {
    let mut keys: Vec<Vec<String>> = map
        .iter()
        .filter(|(_, a)| **a == action)
        .map(|(k, _)| k.clone())
        .collect();
    keys.sort_by_key(|k| (k.len(), seq_text(k).chars().count(), seq_text(k)));
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::keymap::default_map;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn catalog_covers_every_remappable_action() {
        let excluded = [
            KeyAction::Noop,
            KeyAction::Quit,
            KeyAction::QuitAll,
            KeyAction::ConfirmYes,
            KeyAction::ConfirmNo,
        ];
        for (action, name) in KeyAction::NAMES {
            let n = CATALOG.iter().filter(|i| i.action == *action).count();
            if excluded.contains(action) {
                assert_eq!(n, 0, "{name} must not be listed");
            } else {
                assert_eq!(n, 1, "{name} must be listed once");
            }
        }
    }

    #[test]
    fn validate_rejects_fixed_and_count_sequences() {
        assert_eq!(validate(&[]), Err(String::from("✗ no keys pressed")));
        assert_eq!(
            validate(&s(&["q"])),
            Err(String::from("✗ q is a fixed key and cannot start a binding"))
        );
        assert_eq!(
            validate(&s(&["C-c", "x"])),
            Err(String::from("✗ C-c is a fixed key and cannot start a binding"))
        );
        assert!(validate(&s(&["."])).is_err());
        assert_eq!(
            validate(&s(&["Z", "x"])),
            Err(String::from("✗ sequences cannot start with Z (ZZ quits)"))
        );
        assert_eq!(
            validate(&s(&["1", "2"])),
            Err(String::from(
                "✗ a digit followed by keys is a count; use a single digit"
            ))
        );
        assert_eq!(
            validate(&s(&["Ignored"])),
            Err(String::from("✗ unknown key Ignored"))
        );
        for ok in [
            &["Z"][..],
            &["Space", "Z"],
            &["2"],
            &["C-t"],
            &["Space", "s", "n"],
        ] {
            assert_eq!(validate(&s(ok)), Ok(()), "{ok:?}");
        }
    }

    #[test]
    fn keys_of_sorts_shortest_first() {
        let map = default_map();
        assert_eq!(keys_of(&map, KeyAction::PrevTab), vec![s(&["H"]), s(&["["])]);
        assert_eq!(keys_of(&map, KeyAction::GoTop), vec![s(&["g", "g"])]);
        assert_eq!(seq_text(&s(&["Space", "s", "c"])), "Space s c");
    }
}
