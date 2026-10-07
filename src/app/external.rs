//! External programs that take over the terminal ($EDITOR, console) or run detached.

use std::io::Write;

use std::sync::atomic::{AtomicBool, Ordering};

/// Set while an external program owns the terminal: the input thread stops
/// reading stdin so no keystroke meant for the editor/console is stolen.
static INPUT_PAUSED: AtomicBool = AtomicBool::new(false);
/// Set by the input thread when it is idle (not inside `poll`/`read`).
static INPUT_IDLE: AtomicBool = AtomicBool::new(true);
/// Set after the terminal was handed back: the next frame must be a full
/// repaint (ratatui's diff would otherwise keep the editor's leftovers).
static NEEDS_FULL_REDRAW: AtomicBool = AtomicBool::new(false);

/// Take (and reset) the full-redraw request.
pub fn take_full_redraw() -> bool {
    NEEDS_FULL_REDRAW.swap(false, Ordering::SeqCst)
}

/// Keyboard/terminal input thread. Replaces crossterm's `EventStream`, whose
/// background reader keeps consuming stdin while `$EDITOR` runs.
pub fn spawn_input_thread() -> tokio::sync::mpsc::UnboundedReceiver<crossterm::event::Event> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        loop {
            if INPUT_PAUSED.load(Ordering::SeqCst) {
                INPUT_IDLE.store(true, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            INPUT_IDLE.store(false, Ordering::SeqCst);
            match crossterm::event::poll(std::time::Duration::from_millis(50)) {
                Ok(true) => {
                    if let Ok(ev) = crossterm::event::read()
                        && tx.send(ev).is_err()
                    {
                        break;
                    }
                }
                Ok(false) => {}
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
            INPUT_IDLE.store(true, Ordering::SeqCst);
            if tx.is_closed() {
                break;
            }
        }
    });
    rx
}

/// Leave the alternate screen/raw mode, run `f`, then restore the TUI and
/// request a full repaint.
pub fn with_suspended_terminal<T>(f: impl FnOnce() -> T) -> T {
    // Stop reading stdin and wait (≤ ~100 ms) for the reader to go idle.
    INPUT_PAUSED.store(true, Ordering::SeqCst);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
    while !INPUT_IDLE.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    );
    let _ = crossterm::terminal::disable_raw_mode();
    let out = f();
    let _ = crossterm::terminal::enable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::Hide
    );
    let _ = std::io::stdout().flush();
    NEEDS_FULL_REDRAW.store(true, Ordering::SeqCst);
    INPUT_PAUSED.store(false, Ordering::SeqCst);
    out
}

/// Editor argv from `$VISUAL`, `$EDITOR`, then `general.editor` in the config
/// (supports arguments, e.g. `code --wait`).
pub fn editor_argv() -> Vec<String> {
    let raw = std::env::var("VISUAL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()))
        .or_else(|| Some(crate::config::runtime().general.editor).filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| String::from("vi"));
    shlex::split(&raw)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec![String::from("vi")])
}

/// Open `path` in the editor with the TUI suspended. Returns false when the
/// editor could not be started or exited with an error.
pub fn edit_file(path: &std::path::Path) -> bool {
    let argv = editor_argv();
    with_suspended_terminal(|| {
        std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .arg(path)
            .status()
            .is_ok_and(|s| s.success())
    })
}

/// Run a foreground program with the TUI suspended.
pub fn suspend_run(argv: &[&str]) {
    let Some((prog, args)) = argv.split_first() else {
        return;
    };
    with_suspended_terminal(|| {
        let _ = std::process::Command::new(prog).args(args).status();
    });
}

/// Serial console with the configured escape key (`C-]` → `virsh -e ^]`).
pub fn console(uri: &str, domain: &str) {
    let escape = escape_arg(&crate::config::runtime().console.escape);
    suspend_run(&["virsh", "-e", &escape, "-c", uri, "console", domain]);
}

/// `C-x` / `^x` → `^x` (virsh's notation); anything else falls back to `^]`.
fn escape_arg(key: &str) -> String {
    let c = key
        .strip_prefix("C-")
        .or_else(|| key.strip_prefix('^'))
        .filter(|c| c.chars().count() == 1);
    match c {
        Some(c) => format!("^{c}"),
        None => String::from("^]"),
    }
}

/// Spawn a detached program (viewer) with all stdio closed.
pub fn detached_run(argv: &[&str]) {
    let Some((prog, args)) = argv.split_first() else {
        return;
    };
    let _ = std::process::Command::new(prog)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    #[test]
    fn console_escape_notation() {
        assert_eq!(super::escape_arg("C-]"), "^]");
        assert_eq!(super::escape_arg("^a"), "^a");
        assert_eq!(super::escape_arg("bogus"), "^]");
    }

    #[test]
    fn editor_with_args() {
        // SAFETY: test-only env mutation, single-threaded within this test.
        unsafe {
            std::env::remove_var("VISUAL");
            std::env::set_var("EDITOR", "code --wait");
        }
        assert_eq!(super::editor_argv(), vec!["code", "--wait"]);
    }
}
