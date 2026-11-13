//! ISO picker for the new-domain wizard (step 2 › ISO path, opened with ⏎).
//!
//! Candidates come from two places: `.iso` volumes of the connection's storage
//! pools, and a shallow scan of local folders (`[wizard] iso_dirs` in the
//! config, default: the XDG download folder). Local folders and the
//! filesystem browser are only offered for local connections: on
//! `qemu+ssh://…` the ISO must exist on the remote host.

use std::path::{Path, PathBuf};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::theme::Theme;

/// One ISO the picker can offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoCandidate {
    /// Absolute path (what `--cdrom` gets).
    pub path: String,
    /// File name.
    pub name: String,
    /// Where it was found: `pool default`, `~/Downloads`, …
    pub source: String,
    /// Human size (`1.3G`), empty when unknown.
    pub size: String,
    /// Modification time (Unix seconds), for newest-first ordering.
    pub modified: Option<u64>,
}

impl IsoCandidate {
    /// `YYYY-MM-DD` of the modification time (empty when unknown).
    pub fn date(&self) -> String {
        self.modified.map(ymd).unwrap_or_default()
    }
}

/// Civil date of a Unix timestamp (UTC), `YYYY-MM-DD`.
fn ymd(secs: u64) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Human size of a byte count (`1.3G`).
pub fn human_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    let k = 1024.0_f64;
    if b >= k.powi(4) {
        format!("{:.1}T", b / k.powi(4))
    } else if b >= k.powi(3) {
        format!("{:.1}G", b / k.powi(3))
    } else if b >= k.powi(2) {
        format!("{:.1}M", b / k.powi(2))
    } else {
        format!("{:.0}K", b / k)
    }
}

fn is_iso(name: &str) -> bool {
    name.to_lowercase().ends_with(".iso")
}

/// `~/x` for paths under the home directory (labels only).
pub fn tilde(path: &Path) -> String {
    if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() {
            String::from("~")
        } else {
            format!("~/{}", rest.display())
        };
    }
    path.display().to_string()
}

/// Expand a leading `~` / `$HOME` in a configured folder.
pub fn expand(dir: &str) -> PathBuf {
    let home = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf());
    for prefix in ["~/", "$HOME/"] {
        if let (Some(rest), Some(home)) = (dir.strip_prefix(prefix), &home) {
            return home.join(rest);
        }
    }
    if matches!(dir, "~" | "$HOME")
        && let Some(home) = home
    {
        return home;
    }
    PathBuf::from(dir)
}

/// Folders to scan: the configured ones, or the XDG download folder
/// (`$XDG_DOWNLOAD_DIR` / `user-dirs.dirs`, so localized names work), or
/// `~/Downloads` as a last resort.
pub fn iso_dirs(configured: &[String]) -> Vec<PathBuf> {
    let configured: Vec<PathBuf> = configured
        .iter()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .map(expand)
        .collect();
    if !configured.is_empty() {
        return configured;
    }
    let users = directories::UserDirs::new();
    if let Some(dl) = users.as_ref().and_then(|u| u.download_dir()) {
        return vec![dl.to_path_buf()];
    }
    users
        .map(|u| vec![u.home_dir().join("Downloads")])
        .unwrap_or_default()
}

fn stat(path: &Path) -> (String, Option<u64>) {
    match std::fs::metadata(path) {
        Ok(m) => (
            human_bytes(m.len()),
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        ),
        Err(_) => (String::new(), None),
    }
}

/// `*.iso` files directly inside `dir` (no recursion: a big home folder must
/// not stall the UI). Missing or unreadable folders give nothing.
pub fn scan_dir(dir: &Path) -> Vec<IsoCandidate> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let source = tilde(dir);
    let mut out: Vec<IsoCandidate> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file() || t.is_symlink()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !is_iso(&name) || name.starts_with('.') {
                return None;
            }
            let path = e.path();
            let (size, modified) = stat(&path);
            Some(IsoCandidate {
                path: path.display().to_string(),
                name,
                source: source.clone(),
                size,
                modified,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Pool ISOs first, then local folders; duplicates (a folder that is also a
/// pool, as virt-install creates for `--cdrom ~/Downloads/x.iso`) keep the
/// pool entry. Newest first, unknown dates last.
pub fn merge(pools: Vec<IsoCandidate>, local: Vec<IsoCandidate>) -> Vec<IsoCandidate> {
    let mut out: Vec<IsoCandidate> = Vec::new();
    for c in pools.into_iter().chain(local) {
        if !out.iter().any(|o| o.path == c.path) {
            out.push(c);
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Pool ISO as a candidate; `local` stats the file for size and date.
pub fn from_pool(pool: &str, name: &str, path: &str, size: &str, local: bool) -> IsoCandidate {
    let (st_size, modified) = if local {
        stat(Path::new(path))
    } else {
        (String::new(), None)
    };
    IsoCandidate {
        path: path.to_string(),
        name: name.to_string(),
        source: format!("pool {pool}"),
        size: if size.is_empty() || size == "—" {
            st_size
        } else {
            size.to_string()
        },
        modified,
    }
}

/// First folder on the way to `path` that the system emulator cannot search:
/// no `o+x` and no POSIX ACL entry granting search to the `qemu` /
/// `libvirt-qemu` user (virt-install adds such an entry when it fixes a
/// home folder). On `qemu:///system` the emulator runs as that user and
/// cannot open an ISO behind such a folder (typically a 0700 home).
pub fn unsearchable_ancestor(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let uids = emulator_uids();
    path.ancestors().skip(1).find_map(|dir| {
        let blocked = std::fs::metadata(dir).is_ok_and(|m| m.permissions().mode() & 0o001 == 0)
            && !read_acl(dir).is_some_and(|acl| acl_allows_search(&acl, &uids));
        blocked.then(|| dir.to_path_buf())
    })
}

/// UIDs of the users libvirt's system QEMU usually runs as.
fn emulator_uids() -> Vec<u32> {
    ["qemu", "libvirt-qemu"]
        .iter()
        .filter_map(|name| {
            let c = std::ffi::CString::new(*name).ok()?;
            // SAFETY: `c` is a valid C string; the returned record is only
            // read immediately (the UI thread is the only caller).
            let pw = unsafe { libc::getpwnam(c.as_ptr()) };
            (!pw.is_null()).then(|| unsafe { (*pw).pw_uid })
        })
        .collect()
}

/// Raw `system.posix_acl_access` of `dir`, if it has one.
fn read_acl(dir: &Path) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let name = c"system.posix_acl_access";
    let mut buf = vec![0u8; 512];
    // SAFETY: both strings are NUL-terminated and `buf` is writable for its length.
    let n = unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
    (n > 0).then(|| {
        buf.truncate(n as usize);
        buf
    })
}

/// Whether a POSIX ACL (xattr format: `u32` version 2, then 8-byte
/// entries `u16 tag, u16 perm, u32 id`, little-endian) grants search (`x`) to
/// one of `uids` through a named-user entry, within the mask.
pub fn acl_allows_search(acl: &[u8], uids: &[u32]) -> bool {
    const USER: u16 = 0x02;
    const MASK: u16 = 0x10;
    const EXEC: u16 = 0x01;
    if acl.len() < 4 || u32::from_le_bytes([acl[0], acl[1], acl[2], acl[3]]) != 2 {
        return false;
    }
    let entries: Vec<(u16, u16, u32)> = acl[4..]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|e| {
            (
                u16::from_le_bytes([e[0], e[1]]),
                u16::from_le_bytes([e[2], e[3]]),
                u32::from_le_bytes([e[4], e[5], e[6], e[7]]),
            )
        })
        .collect();
    let mask_ok = entries
        .iter()
        .find(|(tag, ..)| *tag == MASK)
        .is_none_or(|(_, perm, _)| perm & EXEC != 0);
    mask_ok
        && entries
            .iter()
            .any(|(tag, perm, id)| *tag == USER && perm & EXEC != 0 && uids.contains(id))
}

/// A filesystem browser entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseEntry {
    pub name: String,
    pub dir: bool,
}

/// Browser state: current folder and its sub-folders + ISOs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browse {
    pub dir: PathBuf,
    pub entries: Vec<BrowseEntry>,
}

impl Browse {
    /// Read `dir` (`..`, sub-folders, then `*.iso`; hidden entries skipped).
    pub fn open(dir: PathBuf) -> Self {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                // Follow symlinks: a link to a folder is browsable.
                let is_dir = std::fs::metadata(e.path()).is_ok_and(|m| m.is_dir());
                if is_dir {
                    dirs.push(name);
                } else if is_iso(&name) {
                    files.push(name);
                }
            }
        }
        dirs.sort_by_key(|d| d.to_lowercase());
        files.sort_by_key(|f| f.to_lowercase());
        let mut entries = Vec::new();
        if dir.parent().is_some() {
            entries.push(BrowseEntry {
                name: String::from(".."),
                dir: true,
            });
        }
        entries.extend(dirs.into_iter().map(|name| BrowseEntry { name, dir: true }));
        entries.extend(files.into_iter().map(|name| BrowseEntry { name, dir: false }));
        Self { dir, entries }
    }
}

/// What ⏎ did in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// An ISO was chosen (absolute path).
    Chosen(String),
    /// Entered a folder (browser); keep the picker open.
    Navigated,
    /// Nothing selected.
    Nothing,
}

/// Picker state (lives in the wizard while open).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IsoPicker {
    pub query: String,
    pub selected: usize,
    /// 0 = all sources, then one per distinct source.
    pub source_idx: usize,
    /// Filesystem browser (C-o); `None` = candidate list.
    pub browse: Option<Browse>,
}

impl IsoPicker {
    /// Distinct sources, in first-seen order.
    pub fn sources(cands: &[IsoCandidate]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in cands {
            if !out.contains(&c.source) {
                out.push(c.source.clone());
            }
        }
        out
    }

    /// Current source filter label (`all` or a source).
    pub fn source_label(&self, cands: &[IsoCandidate]) -> String {
        match self.source_idx {
            0 => String::from("all"),
            i => Self::sources(cands)
                .get(i - 1)
                .cloned()
                .unwrap_or_else(|| String::from("all")),
        }
    }

    /// Candidates matching the source filter and the query (case-insensitive
    /// substring of the file name).
    pub fn filtered<'a>(&self, cands: &'a [IsoCandidate]) -> Vec<&'a IsoCandidate> {
        let q = self.query.to_lowercase();
        let source = (self.source_idx > 0)
            .then(|| Self::sources(cands).get(self.source_idx - 1).cloned())
            .flatten();
        cands
            .iter()
            .filter(|c| source.as_ref().is_none_or(|s| &c.source == s))
            .filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q))
            .collect()
    }

    /// Browser entries matching the query (`..` always stays).
    pub fn browse_filtered(&self) -> Vec<&BrowseEntry> {
        let q = self.query.to_lowercase();
        self.browse
            .as_ref()
            .map(|b| {
                b.entries
                    .iter()
                    .filter(|e| e.name == ".." || q.is_empty() || e.name.to_lowercase().contains(&q))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn len(&self, cands: &[IsoCandidate]) -> usize {
        if self.browse.is_some() {
            self.browse_filtered().len()
        } else {
            self.filtered(cands).len()
        }
    }

    /// Type into the query.
    pub fn type_char(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
    }

    /// Delete a query character; in the browser an empty query goes up a folder.
    pub fn backspace(&mut self) {
        if self.query.pop().is_some() {
            self.selected = 0;
        } else if let Some(b) = &self.browse
            && let Some(parent) = b.dir.parent()
        {
            self.browse = Some(Browse::open(parent.to_path_buf()));
            self.selected = 0;
        }
    }

    /// Move the selection (clamped).
    pub fn move_sel(&mut self, delta: i32, cands: &[IsoCandidate]) {
        let n = self.len(cands);
        if n == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as i64 + delta as i64).clamp(0, n as i64 - 1) as usize;
    }

    /// Cycle the source filter (list mode).
    pub fn cycle_source(&mut self, cands: &[IsoCandidate]) {
        let n = Self::sources(cands).len() + 1;
        self.source_idx = (self.source_idx + 1) % n;
        self.selected = 0;
    }

    /// Switch between the list and the browser opened at `start`.
    pub fn toggle_browse(&mut self, start: PathBuf) {
        self.browse = match self.browse {
            Some(_) => None,
            None => Some(Browse::open(start)),
        };
        self.query.clear();
        self.selected = 0;
    }

    /// ⏎: choose the selected ISO or enter the selected folder.
    pub fn enter(&mut self, cands: &[IsoCandidate]) -> Pick {
        if let Some(b) = &self.browse {
            let entries = self.browse_filtered();
            let Some(e) = entries.get(self.selected.min(entries.len().saturating_sub(1))) else {
                return Pick::Nothing;
            };
            let target = if e.name == ".." {
                b.dir
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| b.dir.clone())
            } else {
                b.dir.join(&e.name)
            };
            if e.dir {
                self.browse = Some(Browse::open(target));
                self.query.clear();
                self.selected = 0;
                return Pick::Navigated;
            }
            return Pick::Chosen(target.display().to_string());
        }
        let list = self.filtered(cands);
        list.get(self.selected.min(list.len().saturating_sub(1)))
            .map_or(Pick::Nothing, |c| Pick::Chosen(c.path.clone()))
    }
}

/// Render the picker over the wizard modal `area`.
pub fn render(
    frame: &mut Frame,
    theme: &Theme,
    picker: &IsoPicker,
    cands: &[IsoCandidate],
    system_uri: bool,
    area: Rect,
) {
    let t = &theme.tokens;
    let w = 100u16.min(area.width.saturating_sub(4));
    let (count, total) = match &picker.browse {
        Some(b) => (picker.browse_filtered().len(), b.entries.len()),
        None => (picker.filtered(cands).len(), cands.len()),
    };
    // Fit the list (prompt, blank, rows, blank, warning, keys + borders).
    let h = (count as u16 + 8)
        .clamp(12, 24)
        .min(area.height.saturating_sub(2));
    let modal = Rect::new(
        area.x + area.width.saturating_sub(w) / 2,
        area.y + area.height.saturating_sub(h) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    let right = match &picker.browse {
        Some(b) => tilde(&b.dir),
        None => String::from("step 2 › ISO path"),
    };
    let block = crate::ui::widgets::panel::block(
        theme,
        true,
        "◎ Choose ISO",
        Some(Line::from(Span::styled(right, theme.dim()))),
        None,
        None,
    )
    .style(Style::default().bg(t.bg));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let inner = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), inner.height);

    let mut status = format!("{count} / {total}");
    if picker.browse.is_none() {
        status = format!("{} ▾   {status}", picker.source_label(cands));
    }
    let mut lines = vec![
        Line::from(vec![
            Span::styled("❯ ", Style::default().fg(t.magenta).add_modifier(Modifier::BOLD)),
            Span::styled(
                picker.query.clone(),
                Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(t.fg)),
            Span::raw("   "),
            Span::styled(status, theme.dim()),
        ]),
        Line::from(""),
    ];
    // Rows available for the list: total − prompt (2) − blank + warning + keys (3).
    let rows = inner.height.saturating_sub(5) as usize;
    let selected = picker.selected;
    let first = selected.saturating_sub(rows.saturating_sub(1));
    let name_w = (inner.width as usize).saturating_sub(40).max(20);
    let mut chosen_path: Option<String> = None;
    match &picker.browse {
        Some(b) => {
            let entries = picker.browse_filtered();
            if entries.is_empty() {
                lines.push(Line::from(Span::styled("  nothing here", theme.dim())));
            }
            for (i, e) in entries.iter().enumerate().skip(first).take(rows) {
                let sel = i == selected;
                let bg = if sel { t.sel } else { t.bg };
                if sel && !e.dir {
                    chosen_path = Some(b.dir.join(&e.name).display().to_string());
                }
                let (glyph, color) = if e.dir {
                    ("▸ ", t.blue)
                } else {
                    ("◎ ", t.yellow)
                };
                let name = if e.dir && e.name != ".." {
                    format!("{}/", e.name)
                } else {
                    e.name.clone()
                };
                lines.push(
                    Line::from(vec![
                        Span::styled(glyph, Style::default().fg(color).bg(bg)),
                        Span::styled(
                            name,
                            if sel {
                                Style::default().fg(t.fg).bg(bg).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(t.fg2).bg(bg)
                            },
                        ),
                    ])
                    .style(Style::default().bg(bg)),
                );
            }
        }
        None => {
            let list = picker.filtered(cands);
            if list.is_empty() {
                let msg = if cands.is_empty() {
                    "  no ISOs found in the pools or the ISO folders · C-o browse the filesystem"
                } else {
                    "  no match"
                };
                lines.push(Line::from(Span::styled(msg, theme.dim())));
            }
            for (i, c) in list.iter().enumerate().skip(first).take(rows) {
                let sel = i == selected;
                let bg = if sel { t.sel } else { t.bg };
                if sel {
                    chosen_path = Some(c.path.clone());
                }
                let (head, rest) = split_match(&c.name, &picker.query);
                let name_len = c.name.chars().count();
                let pad = name_w.saturating_sub(name_len);
                lines.push(
                    Line::from(vec![
                        Span::styled("◎ ", Style::default().fg(t.yellow).bg(bg)),
                        Span::styled(
                            head,
                            Style::default().fg(t.orange).bg(bg).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            rest,
                            if sel {
                                Style::default().fg(t.fg).bg(bg).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(t.fg2).bg(bg)
                            },
                        ),
                        Span::styled(" ".repeat(pad + 1), Style::default().bg(bg)),
                        Span::styled(format!("{:>7}", c.size), Style::default().fg(t.fg2).bg(bg)),
                        Span::styled("  ", Style::default().bg(bg)),
                        Span::styled(format!("{:10}", c.date()), Style::default().fg(t.comment).bg(bg)),
                        Span::styled("  ", Style::default().bg(bg)),
                        Span::styled(c.source.clone(), Style::default().fg(t.cyan).bg(bg)),
                    ])
                    .style(Style::default().bg(bg)),
                );
            }
        }
    }
    let used = lines.len();
    let footer_at = (inner.height as usize).saturating_sub(2);
    lines.extend((used..footer_at).map(|_| Line::from("")));
    let warning = chosen_path
        .filter(|_| system_uri)
        .and_then(|p| unsearchable_ancestor(Path::new(&p)))
        .map(|dir| permission_warning(&dir))
        .unwrap_or_default();
    lines.push(Line::from(Span::styled(warning, Style::default().fg(t.yellow))));
    let mut keys = vec![
        Span::styled(" ⏎", theme.key()),
        Span::styled(" choose  ", theme.dim()),
        Span::styled("C-j/k ↑↓", theme.key()),
        Span::styled(" move  ", theme.dim()),
    ];
    if picker.browse.is_some() {
        keys.extend([
            Span::styled("⌫", theme.key()),
            Span::styled(" up  ", theme.dim()),
            Span::styled("C-o", theme.key()),
            Span::styled(" list  ", theme.dim()),
        ]);
    } else {
        keys.extend([
            Span::styled("⇥", theme.key()),
            Span::styled(" source  ", theme.dim()),
            Span::styled("C-o", theme.key()),
            Span::styled(" browse…  ", theme.dim()),
        ]);
    }
    keys.extend([
        Span::styled("⎋", theme.key()),
        Span::styled(" cancel", theme.dim()),
    ]);
    lines.push(Line::from(keys));
    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(t.bg)), inner);
}

/// `⚠ …` line for an ISO the system emulator may not be able to read.
pub fn permission_warning(dir: &Path) -> String {
    format!(
        "⚠ {} is not searchable by other users: qemu (system) may not read this ISO · copy it into a pool",
        tilde(dir)
    )
}

/// Split `name` after the case-insensitive prefix up to the end of the first
/// match of `query` (highlight).
fn split_match(name: &str, query: &str) -> (String, String) {
    if query.is_empty() {
        return (String::new(), name.to_string());
    }
    let lower = name.to_lowercase();
    match lower.find(&query.to_lowercase()) {
        Some(i) if lower.len() == name.len() => {
            let end = i + query.len();
            (name[..end].to_string(), name[end..].to_string())
        }
        _ => (String::new(), name.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(name: &str, source: &str, modified: Option<u64>) -> IsoCandidate {
        IsoCandidate {
            path: format!("/x/{name}"),
            name: name.to_string(),
            source: source.to_string(),
            size: String::new(),
            modified,
        }
    }

    #[test]
    fn dates_are_civil_utc() {
        assert_eq!(ymd(0), "1970-01-01");
        assert_eq!(ymd(1_762_992_000), "2025-11-13");
    }

    #[test]
    fn scan_finds_only_isos_without_recursing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("arch.iso"), b"x").unwrap();
        std::fs::write(dir.path().join("Win11.ISO"), b"x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.path().join(".hidden.iso"), b"x").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/deep.iso"), b"x").unwrap();
        let found: Vec<String> = scan_dir(dir.path()).into_iter().map(|c| c.name).collect();
        assert_eq!(found, vec!["Win11.ISO", "arch.iso"]);
        assert!(scan_dir(Path::new("/nonexistent-virsh-tui")).is_empty());
    }

    #[test]
    fn merge_dedupes_keeping_pool_and_sorts_newest_first() {
        let pool = vec![cand("a.iso", "pool Downloads", Some(10))];
        let local = vec![
            cand("a.iso", "~/Downloads", Some(10)),
            cand("b.iso", "~/Downloads", Some(20)),
            cand("c.iso", "~/Downloads", None),
        ];
        let m = merge(pool, local);
        let names: Vec<(&str, &str)> = m.iter().map(|c| (c.name.as_str(), c.source.as_str())).collect();
        assert_eq!(
            names,
            vec![
                ("b.iso", "~/Downloads"),
                ("a.iso", "pool Downloads"),
                ("c.iso", "~/Downloads")
            ]
        );
    }

    #[test]
    fn filter_by_query_and_source() {
        let cands = vec![
            cand("archlinux.iso", "pool isos", None),
            cand("Fedora.iso", "~/Downloads", None),
            cand("arch-old.iso", "~/Downloads", None),
        ];
        let mut p = IsoPicker::default();
        assert_eq!(p.filtered(&cands).len(), 3);
        p.type_char('A');
        p.type_char('r');
        assert_eq!(p.filtered(&cands).len(), 2, "case-insensitive");
        p.cycle_source(&cands); // pool isos
        assert_eq!(p.source_label(&cands), "pool isos");
        assert_eq!(p.filtered(&cands).len(), 1);
        p.cycle_source(&cands); // ~/Downloads
        p.move_sel(5, &cands);
        assert_eq!(p.enter(&cands), Pick::Chosen(String::from("/x/arch-old.iso")));
        p.cycle_source(&cands); // back to all
        assert_eq!(p.source_label(&cands), "all");
    }

    #[test]
    fn browser_navigates_and_chooses() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("isos")).unwrap();
        std::fs::write(dir.path().join("isos/x.iso"), b"x").unwrap();
        std::fs::write(dir.path().join("readme.md"), b"x").unwrap();
        let mut p = IsoPicker::default();
        p.toggle_browse(dir.path().to_path_buf());
        let names: Vec<&str> = p.browse_filtered().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["..", "isos"], "only folders and ISOs");
        p.move_sel(1, &[]);
        assert_eq!(p.enter(&[]), Pick::Navigated);
        p.move_sel(1, &[]);
        let want = dir.path().join("isos/x.iso").display().to_string();
        assert_eq!(p.enter(&[]), Pick::Chosen(want));
        p.backspace(); // empty query: up one folder
        assert_eq!(p.browse.as_ref().unwrap().dir, dir.path());
        p.toggle_browse(PathBuf::new());
        assert!(p.browse.is_none());
    }

    #[test]
    fn unsearchable_folder_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let private = dir.path().join("private");
        std::fs::create_dir(&private).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
        let iso = private.join("a.iso");
        assert_eq!(unsearchable_ancestor(&iso), Some(private.clone()));
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Only ancestors outside the temp dir may remain (e.g. a 0700 $TMPDIR).
        assert_ne!(unsearchable_ancestor(&iso), Some(private));
    }

    /// The ACL virt-install leaves on a home folder (`user:libvirt-qemu:--x`).
    #[test]
    fn acl_named_user_search_counts() {
        let hex = "0200000001000700ffffffff02000100c203000004000000ffffffff10000100ffffffff20000000ffffffff";
        let acl: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert!(acl_allows_search(&acl, &[962]));
        assert!(!acl_allows_search(&acl, &[1000]));
        let mut masked = acl.clone();
        masked[30] = 0; // mask perm: ---
        assert!(!acl_allows_search(&masked, &[962]));
        assert!(!acl_allows_search(&[], &[962]));
    }

    #[test]
    fn configured_dirs_expand_home() {
        let home = directories::BaseDirs::new().unwrap().home_dir().to_path_buf();
        assert_eq!(iso_dirs(&[String::from("~/isos")]), vec![home.join("isos")]);
        assert_eq!(
            iso_dirs(&[String::from("/srv/isos")]),
            vec![PathBuf::from("/srv/isos")]
        );
        assert!(!iso_dirs(&[]).is_empty() || directories::UserDirs::new().is_none());
    }
}
