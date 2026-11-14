//! `:` line history kept across sessions in
//! `$XDG_STATE_HOME/virsh-tui/history` (one line per entry, oldest first).
//!
//! Lines that start with a space are kept for the session only, like
//! `HISTCONTROL=ignorespace` in bash: a way to keep a password off the disk.

use std::path::{Path, PathBuf};

/// History file path.
pub fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "virsh-tui").and_then(|d| d.state_dir().map(|p| p.join("history")))
}

/// The last `limit` lines of the file (missing or unreadable: empty).
pub fn load(path: &Path, limit: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    let skip = lines.len().saturating_sub(limit);
    lines.into_iter().skip(skip).collect()
}

/// Whether `line` may be written to disk.
pub fn persistent(line: &str) -> bool {
    !line.starts_with(' ') && !line.contains('\n') && !line.trim().is_empty()
}

/// Write the last `limit` persistent lines atomically, readable only by the
/// user (lines can hold arguments the user would not share).
pub fn save(path: &Path, lines: &[String], limit: usize) -> std::io::Result<()> {
    let keep: Vec<&String> = lines.iter().filter(|l| persistent(l)).collect();
    let keep = &keep[keep.len().saturating_sub(limit)..];
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    let mut text = String::new();
    for line in keep {
        text.push_str(line);
        text.push('\n');
    }
    std::io::Write::write_all(&mut file, text.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_the_newest_lines_and_drops_private_ones() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/history");
        let lines: Vec<String> = ["start a", " set-user-password a root x", "start b", "start c"]
            .map(String::from)
            .to_vec();
        save(&path, &lines, 2).unwrap();
        assert_eq!(load(&path, 10), vec!["start b", "start c"]);
        assert_eq!(load(&path, 1), vec!["start c"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn missing_file_is_empty() {
        assert!(load(Path::new("/nonexistent/virsh-tui-history"), 10).is_empty());
    }
}
