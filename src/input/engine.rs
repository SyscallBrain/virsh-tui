//! Key engine: counts, multi-key sequences, leader, dot-repeat.
//!
//! The engine keeps a pending buffer. A completed match dispatches
//! `ResolvedAction`. A partial match waits (caller shows which-key after
//! 300 ms). `Esc` clears the buffer.

use std::collections::HashMap;

use super::key::KeySeq;

/// Mutating marker: `.` repeats only these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutating {
    No,
    Yes,
}

/// A key action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    MoveUp,
    MoveDown,
    GoTop,
    GoBottom,
    HalfDown,
    HalfUp,
    Open,
    Back,
    Filter,
    NextMatch,
    PrevMatch,
    Start,
    Shutdown,
    Destroy,
    Reboot,
    Reset,
    Pause,
    ManagedSave,
    ManagedSaveRemove,
    Autostart,
    Console,
    Viewer,
    EditXml,
    Undefine,
    Rename,
    Clone,
    Migrate,
    SendKey,
    ExportXml,
    NewDomain,
    YankName,
    YankUuid,
    YankIp,
    YankCmd,
    Mark,
    Visual,
    SortCycle,
    ChipCycle,
    ChipCycleBack,
    WindowCycle,
    LeaseFocus,
    LeasePin,
    LeaseUnpin,
    Ex,
    Palette,
    Help,
    Settings,
    Refresh,
    Redraw,
    Quit,
    QuitAll,
    SnapshotNew,
    SnapshotRevert,
    SnapshotDelete,
    InsertMedia,
    EjectMedia,
    DiskAttach,
    DiskResize,
    NicAttach,
    NicLink,
    BootOrder,
    ConfirmYes,
    ConfirmNo,
    PrevTab,
    NextTab,
    UndoField,
    GotoXml,
    ViewDomains,
    ViewHost,
    ViewNetworks,
    ViewStorage,
    ViewEvents,
    Noop,
}

impl KeyAction {
    /// Every action with its keymap.toml name (snake_case).
    pub const NAMES: &'static [(Self, &'static str)] = &[
        (Self::MoveUp, "move_up"),
        (Self::MoveDown, "move_down"),
        (Self::GoTop, "go_top"),
        (Self::GoBottom, "go_bottom"),
        (Self::HalfDown, "half_down"),
        (Self::HalfUp, "half_up"),
        (Self::Open, "open"),
        (Self::Back, "back"),
        (Self::Filter, "filter"),
        (Self::NextMatch, "next_match"),
        (Self::PrevMatch, "prev_match"),
        (Self::Start, "start"),
        (Self::Shutdown, "shutdown"),
        (Self::Destroy, "destroy"),
        (Self::Reboot, "reboot"),
        (Self::Reset, "reset"),
        (Self::Pause, "pause"),
        (Self::ManagedSave, "managed_save"),
        (Self::ManagedSaveRemove, "managed_save_remove"),
        (Self::Autostart, "autostart"),
        (Self::Console, "console"),
        (Self::Viewer, "viewer"),
        (Self::EditXml, "edit_xml"),
        (Self::Undefine, "undefine"),
        (Self::Rename, "rename"),
        (Self::Clone, "clone"),
        (Self::Migrate, "migrate"),
        (Self::SendKey, "send_key"),
        (Self::ExportXml, "export_xml"),
        (Self::NewDomain, "new_domain"),
        (Self::YankName, "yank_name"),
        (Self::YankUuid, "yank_uuid"),
        (Self::YankIp, "yank_ip"),
        (Self::YankCmd, "yank_cmd"),
        (Self::Mark, "mark"),
        (Self::Visual, "visual"),
        (Self::SortCycle, "sort_cycle"),
        (Self::ChipCycle, "chip_cycle"),
        (Self::ChipCycleBack, "chip_cycle_back"),
        (Self::WindowCycle, "window_cycle"),
        (Self::LeaseFocus, "lease_focus"),
        (Self::LeasePin, "lease_pin"),
        (Self::LeaseUnpin, "lease_unpin"),
        (Self::Ex, "ex"),
        (Self::Palette, "palette"),
        (Self::Help, "help"),
        (Self::Settings, "settings"),
        (Self::Refresh, "refresh"),
        (Self::Redraw, "redraw"),
        (Self::Quit, "quit"),
        (Self::QuitAll, "quit_all"),
        (Self::SnapshotNew, "snapshot_new"),
        (Self::SnapshotRevert, "snapshot_revert"),
        (Self::SnapshotDelete, "snapshot_delete"),
        (Self::InsertMedia, "insert_media"),
        (Self::EjectMedia, "eject_media"),
        (Self::DiskAttach, "disk_attach"),
        (Self::DiskResize, "disk_resize"),
        (Self::NicAttach, "nic_attach"),
        (Self::NicLink, "nic_link"),
        (Self::BootOrder, "boot_order"),
        (Self::ConfirmYes, "confirm_yes"),
        (Self::ConfirmNo, "confirm_no"),
        (Self::PrevTab, "prev_tab"),
        (Self::NextTab, "next_tab"),
        (Self::UndoField, "undo_field"),
        (Self::GotoXml, "goto_xml"),
        (Self::ViewDomains, "view_domains"),
        (Self::ViewHost, "view_host"),
        (Self::ViewNetworks, "view_networks"),
        (Self::ViewStorage, "view_storage"),
        (Self::ViewEvents, "view_events"),
        (Self::Noop, "noop"),
    ];

    /// keymap.toml name (`shutdown`, `snapshot_new`, …).
    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(a, _)| *a == self)
            .map_or("noop", |(_, n)| n)
    }

    /// Parse a keymap.toml action name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMES.iter().find(|(_, n)| *n == name).map(|(a, _)| *a)
    }

    /// True for mutating actions (repeatable with `.`).
    pub fn mutating(self) -> bool {
        matches!(
            self,
            Self::Start
                | Self::Shutdown
                | Self::Destroy
                | Self::Reboot
                | Self::Reset
                | Self::Pause
                | Self::ManagedSave
                | Self::Autostart
                | Self::Undefine
                | Self::Rename
                | Self::Clone
                | Self::SnapshotNew
                | Self::SnapshotRevert
                | Self::SnapshotDelete
        )
    }
}

/// A resolved action with count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAction {
    pub action: KeyAction,
    pub count: u32,
}

/// Key engine.
pub struct KeyEngine {
    map: HashMap<Vec<String>, KeyAction>,
    pending: Vec<String>,
    count_buf: String,
    last_mutating: Option<(KeyAction, u32)>,
}

impl KeyEngine {
    /// The live keymap (defaults + keymap.toml).
    pub fn map(&self) -> &HashMap<Vec<String>, KeyAction> {
        &self.map
    }

    /// Create with the default keymap.
    pub fn new() -> Self {
        Self::with_map(super::keymap::default_map())
    }

    /// Create with an explicit map (tests, overrides).
    pub fn with_map(map: HashMap<Vec<String>, KeyAction>) -> Self {
        Self {
            map,
            pending: Vec::new(),
            count_buf: String::new(),
            last_mutating: None,
        }
    }

    /// Feed one normalized token. Returns resolved actions (0 or 1).
    ///
    /// Digits always accumulate as a count prefix (single `1`-`5` become view
    /// switches only via [`KeyEngine::flush`] after an idle timeout).
    pub fn feed(&mut self, key: &KeySeq) -> Vec<ResolvedAction> {
        let tok = key.token().to_string();
        if tok == "Esc" {
            self.pending.clear();
            self.count_buf.clear();
            return vec![];
        }
        if tok.chars().all(|c| c.is_ascii_digit()) && self.pending.is_empty() {
            self.count_buf.push_str(&tok);
            return vec![];
        }
        // Dot thirst: repeat last mutating action — but "." is also bound to repeat,
        // handle explicitly.
        if tok == "." && self.pending.is_empty() {
            if let Some((action, _)) = self.last_mutating {
                let count = self.take_count();
                self.last_mutating = Some((action, count));
                return vec![ResolvedAction { action, count }];
            }
            return vec![];
        }
        let mut candidate = self.pending.clone();
        candidate.push(tok);
        if self.is_full_binding(&candidate) {
            return self.resolve(candidate);
        }
        if self.is_prefix(&candidate) {
            self.pending = candidate;
            return vec![];
        }
        // No match: reset and try the token alone.
        self.pending.clear();
        self.count_buf.clear();
        if self.is_full_binding(&[key.token().to_string()]) {
            return self.resolve(vec![key.token().to_string()]);
        }
        vec![]
    }

    /// Pending prefix for which-key display.
    pub fn pending_prefix(&self) -> Option<String> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.join(" "))
        }
    }

    /// True while digits wait for a command or the idle timeout.
    pub fn has_pending_digits(&self) -> bool {
        !self.count_buf.is_empty() && self.pending.is_empty()
    }

    /// Idle-timeout flush: a lone digit resolves through the keymap (by default
    /// `1`-`5` switch views); anything else (multi-digit counts with no command)
    /// is cancelled.
    pub fn flush(&mut self) -> Vec<ResolvedAction> {
        if !self.pending.is_empty() {
            return vec![];
        }
        if self.count_buf.len() == 1 {
            let seq = vec![self.count_buf.clone()];
            if self.is_full_binding(&seq) {
                return self.resolve(seq);
            }
        }
        self.count_buf.clear();
        vec![]
    }

    fn is_full_binding(&self, seq: &[String]) -> bool {
        self.map.contains_key(seq)
    }

    fn is_prefix(&self, seq: &[String]) -> bool {
        self.map
            .keys()
            .any(|k| k.len() > seq.len() && k[..seq.len()] == *seq)
    }

    fn take_count(&mut self) -> u32 {
        if self.count_buf.is_empty() {
            1
        } else {
            self.count_buf.parse().unwrap_or(1)
        }
    }

    fn resolve(&mut self, seq: Vec<String>) -> Vec<ResolvedAction> {
        let action = self.map[&seq];
        self.pending.clear();
        let count = self.take_count();
        self.count_buf.clear();
        if action.mutating() {
            self.last_mutating = Some((action, count));
        }
        vec![ResolvedAction { action, count }]
    }
}

impl Default for KeyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyAction, KeyEngine};
    use crate::input::key::KeySeq;

    #[test]
    fn counts_and_repeat() {
        let mut eng = KeyEngine::new();
        assert!(eng.feed(&KeySeq::parse("3")).is_empty());
        let a = eng.feed(&KeySeq::parse("j"));
        assert_eq!(a[0].count, 3);
        assert_eq!(a[0].action, KeyAction::MoveDown);
    }

    #[test]
    fn leader_sequence() {
        let mut eng = KeyEngine::new();
        assert!(eng.feed(&KeySeq::parse("Space")).is_empty());
        assert!(eng.feed(&KeySeq::parse("s")).is_empty());
        let a = eng.feed(&KeySeq::parse("c"));
        assert_eq!(a[0].action, KeyAction::SnapshotNew);
    }
}
