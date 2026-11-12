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
/// An OS shutdown must also interrupt a suspended-terminal wait.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Notify foreground waits that the process is shutting down.
pub fn notify_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

/// Whether shutdown was requested while a foreground process owned the terminal.
pub fn shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

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

fn run_foreground(argv: &[&str], path: Option<&std::path::Path>, shutdown: &AtomicBool) -> bool {
    let Some((program, args)) = argv.split_first() else {
        return false;
    };
    if shutdown.load(Ordering::SeqCst) {
        return false;
    }
    let mut command = std::process::Command::new(program);
    command.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    if let Some(path) = path {
        command.arg(path);
    }
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    #[cfg(unix)]
    let group = i32::try_from(child.id()).ok();
    #[cfg(unix)]
    let _terminal = ForegroundTerminal::give_to(group);
    #[cfg(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd"
    ))]
    {
        let mut stopping = None;
        loop {
            if shutdown.load(Ordering::SeqCst) && stopping.is_none() {
                signal_group(group, libc::SIGINT);
                if group.is_none() {
                    let _ = child.kill();
                }
                stopping = Some(std::time::Instant::now());
            }
            match child_exited_unreaped(child.id()) {
                Ok(true) => {
                    // The unreaped leader keeps the group ID reserved here.
                    signal_group(group, libc::SIGKILL);
                    let status = child.wait();
                    return stopping.is_none() && status.is_ok_and(|status| status.success());
                }
                Ok(false)
                    if stopping.is_some_and(|start| start.elapsed() >= std::time::Duration::from_secs(3)) =>
                {
                    signal_group(group, libc::SIGKILL);
                    if group.is_none() {
                        let _ = child.kill();
                    }
                    let _ = child.wait();
                    return false;
                }
                Ok(false) => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(_) => {
                    signal_group(group, libc::SIGKILL);
                    if group.is_none() {
                        let _ = child.kill();
                    }
                    let _ = child.wait();
                    return false;
                }
            }
        }
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd"
    )))]
    loop {
        if shutdown.load(Ordering::SeqCst) {
            #[cfg(unix)]
            signal_group(group, libc::SIGKILL);
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(unix)]
fn signal_group(group: Option<i32>, signal: i32) {
    if let Some(group) = group.filter(|group| *group > 0) {
        // The child has not been reaped, so its process-group ID is reserved.
        unsafe {
            libc::kill(-group, signal);
        }
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd"
))]
fn child_exited_unreaped(pid: u32) -> std::io::Result<bool> {
    #[cfg(target_os = "freebsd")]
    let wait_id: libc::id_t = pid.into();
    #[cfg(not(target_os = "freebsd"))]
    let wait_id: libc::id_t = pid;
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            wait_id,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == 0 {
        Ok(unsafe { info.si_pid() != 0 })
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
struct ForegroundTerminal {
    fd: i32,
    previous: i32,
}

#[cfg(unix)]
impl ForegroundTerminal {
    fn give_to(group: Option<i32>) -> Option<Self> {
        use std::os::fd::AsRawFd;
        let group = group?;
        let fd = std::io::stdin().as_raw_fd();
        let previous = unsafe { libc::tcgetpgrp(fd) };
        if previous <= 0 || previous != unsafe { libc::getpgrp() } || !set_foreground(fd, group) {
            return None;
        }
        signal_group(Some(group), libc::SIGCONT);
        Some(Self { fd, previous })
    }
}

#[cfg(unix)]
impl Drop for ForegroundTerminal {
    fn drop(&mut self) {
        let _ = set_foreground(self.fd, self.previous);
    }
}

#[cfg(unix)]
fn set_foreground(fd: i32, group: i32) -> bool {
    unsafe {
        let mut blocked: libc::sigset_t = std::mem::zeroed();
        let mut previous: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut blocked);
        libc::sigaddset(&mut blocked, libc::SIGTTOU);
        if libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, &mut previous) != 0 {
            return false;
        }
        let result = libc::tcsetpgrp(fd, group) == 0;
        libc::pthread_sigmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut());
        result
    }
}

/// Open `path` in the editor with the TUI suspended. Returns false when the
/// editor could not be started or exited with an error.
pub fn edit_file(path: &std::path::Path) -> bool {
    let argv = editor_argv();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    with_suspended_terminal(|| run_foreground(&args, Some(path), &SHUTDOWN_REQUESTED))
}

/// Run a foreground program with the TUI suspended.
pub fn suspend_run(argv: &[&str]) {
    with_suspended_terminal(|| {
        let _ = run_foreground(argv, None, &SHUTDOWN_REQUESTED);
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
    #[cfg(unix)]
    #[test]
    fn foreground_terminal_is_given_to_child_and_restored() {
        use std::sync::atomic::AtomicBool;
        if let Ok(helper) = std::env::var("VT_FOREGROUND_PTY_CHILD") {
            let marker = std::env::var("VT_FOREGROUND_PTY_MARKER").unwrap();
            assert!(super::run_foreground(
                &["python3", &helper, &marker],
                None,
                &AtomicBool::new(false)
            ));
            use std::os::fd::AsRawFd;
            let fd = std::io::stdin().as_raw_fd();
            assert_eq!(unsafe { libc::tcgetpgrp(fd) }, unsafe { libc::getpgrp() });
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join("foreground.py");
        let marker = dir.path().join("ready");
        std::fs::write(&helper, "import os,signal,sys\nfrom pathlib import Path\nsignal.alarm(5)\nassert os.tcgetpgrp(0) == os.getpgrp()\nPath(sys.argv[1]).write_text('ready')\nassert os.read(0, 1) == b'x'\n").unwrap();
        let status = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/foreground_pty.py"
            ))
            .arg(std::env::current_exe().unwrap())
            .arg(helper)
            .arg(marker)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[cfg(unix)]
    #[test]
    fn foreground_shutdown_stops_owned_descendants() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("child.py");
        let started = dir.path().join("started");
        let survived = dir.path().join("survived");
        std::fs::write(&script, "import os,signal,sys,time\nfrom pathlib import Path\nsignal.signal(signal.SIGINT, lambda *_: os._exit(0))\nif os.fork() == 0:\n time.sleep(1)\n Path(sys.argv[2]).write_text('survived')\n os._exit(0)\nPath(sys.argv[1]).write_text('started')\ntime.sleep(30)\n").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let child_stop = stop.clone();
        let script_arg = script.to_string_lossy().into_owned();
        let started_arg = started.to_string_lossy().into_owned();
        let survived_arg = survived.to_string_lossy().into_owned();
        let thread = std::thread::spawn(move || {
            super::run_foreground(
                &["python3", &script_arg, &started_arg, &survived_arg],
                None,
                &child_stop,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while !started.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let did_start = started.exists();
        stop.store(true, Ordering::SeqCst);
        assert!(!thread.join().unwrap());
        assert!(did_start);
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!survived.exists(), "a descendant survived foreground shutdown");
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd"
    ))]
    #[test]
    fn foreground_shutdown_escalates_when_interrupt_is_ignored() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("ignore.py");
        let started = dir.path().join("started");
        let survived = dir.path().join("survived");
        std::fs::write(&script, "import os,signal,sys,time\nfrom pathlib import Path\nsignal.signal(signal.SIGINT, signal.SIG_IGN)\nif os.fork() == 0:\n time.sleep(4)\n Path(sys.argv[2]).write_text('survived')\n os._exit(0)\nsignal.alarm(6)\nPath(sys.argv[1]).write_text('started')\ntime.sleep(30)\n").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let child_stop = stop.clone();
        let script_arg = script.to_string_lossy().into_owned();
        let started_arg = started.to_string_lossy().into_owned();
        let survived_arg = survived.to_string_lossy().into_owned();
        let thread = std::thread::spawn(move || {
            super::run_foreground(
                &["python3", &script_arg, &started_arg, &survived_arg],
                None,
                &child_stop,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while !started.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let did_start = started.exists();
        let stopped_at = Instant::now();
        stop.store(true, Ordering::SeqCst);
        assert!(!thread.join().unwrap());
        assert!(did_start);
        assert!(stopped_at.elapsed() >= Duration::from_millis(2900));
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!survived.exists(), "a descendant survived forced shutdown");
    }

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

    #[test]
    fn foreground_wait_observes_shutdown_request() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_signal = shutdown.clone();
        let worker =
            std::thread::spawn(move || super::run_foreground(&["sleep", "30"], None, &worker_signal));
        std::thread::sleep(std::time::Duration::from_millis(100));
        shutdown.store(true, Ordering::SeqCst);
        let started = std::time::Instant::now();
        assert!(!worker.join().unwrap());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
