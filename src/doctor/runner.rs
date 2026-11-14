//! Access to processes, PATH, files and environment (faked in tests).

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Timeout for every process the doctor runs.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// Result of running a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Exited {
        success: bool,
        stdout: String,
        stderr: String,
    },
    NotFound,
    Timeout,
}

/// What the checks may look at. Read-only by contract.
#[async_trait::async_trait]
pub trait Runner: Send + Sync {
    /// Run a program (no shell) with [`TIMEOUT`].
    async fn run(&self, program: &str, args: &[&str]) -> RunOutcome;
    fn which(&self, program: &str) -> Option<PathBuf>;
    fn read(&self, path: &Path) -> Option<String>;
    fn env(&self, key: &str) -> Option<String>;
    fn exists(&self, path: &Path) -> bool;
    fn clipboard_ok(&self) -> bool;
    fn dir_writable(&self, path: &Path) -> bool;
}

/// The real system.
pub struct SystemRunner;

#[async_trait::async_trait]
impl Runner for SystemRunner {
    async fn run(&self, program: &str, args: &[&str]) -> RunOutcome {
        run_with_timeout(program, args, TIMEOUT).await
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        if program.contains('/') {
            let path = PathBuf::from(program);
            return is_executable(&path).then_some(path);
        }
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|dir| dir.join(program))
            .find(|path| is_executable(path))
    }

    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn clipboard_ok(&self) -> bool {
        arboard::Clipboard::new().is_ok()
    }

    fn dir_writable(&self, path: &Path) -> bool {
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: `c_path` is a valid NUL-terminated string for the call.
        unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Run `program args` without a shell, stdin closed, killed after `limit`.
pub(crate) async fn run_with_timeout(program: &str, args: &[&str], limit: Duration) -> RunOutcome {
    let child = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(limit, child).await {
        Err(_) => RunOutcome::Timeout,
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => RunOutcome::NotFound,
        Ok(Err(e)) => RunOutcome::Exited {
            success: false,
            stdout: String::new(),
            stderr: e.to_string(),
        },
        Ok(Ok(out)) => RunOutcome::Exited {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
    }
}

/// Scripted runner for tests: unknown programs are `NotFound`.
#[cfg(test)]
#[derive(Default)]
pub struct FakeRunner {
    pub runs: std::collections::HashMap<String, RunOutcome>,
    pub paths: std::collections::HashMap<String, PathBuf>,
    pub files: std::collections::HashMap<PathBuf, String>,
    pub envs: std::collections::HashMap<String, String>,
    pub clipboard: bool,
    pub writable: bool,
}

#[cfg(test)]
impl FakeRunner {
    pub fn ok(&mut self, cmd: &str, stdout: &str) -> &mut Self {
        self.runs.insert(
            cmd.to_string(),
            RunOutcome::Exited {
                success: true,
                stdout: stdout.to_string(),
                stderr: String::new(),
            },
        );
        self
    }

    pub fn fail(&mut self, cmd: &str, stderr: &str) -> &mut Self {
        self.runs.insert(
            cmd.to_string(),
            RunOutcome::Exited {
                success: false,
                stdout: String::new(),
                stderr: stderr.to_string(),
            },
        );
        self
    }

    pub fn path(&mut self, program: &str) -> &mut Self {
        self.paths
            .insert(program.to_string(), PathBuf::from(format!("/usr/bin/{program}")));
        self
    }

    pub fn set_env(&mut self, key: &str, value: &str) -> &mut Self {
        self.envs.insert(key.to_string(), value.to_string());
        self
    }

    pub fn file(&mut self, path: &str, text: &str) -> &mut Self {
        self.files.insert(PathBuf::from(path), text.to_string());
        self
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl Runner for FakeRunner {
    async fn run(&self, program: &str, args: &[&str]) -> RunOutcome {
        let key = std::iter::once(program)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        self.runs.get(&key).cloned().unwrap_or(RunOutcome::NotFound)
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        self.paths.get(program).cloned()
    }

    fn read(&self, path: &Path) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn env(&self, key: &str) -> Option<String> {
        self.envs.get(key).cloned()
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    fn clipboard_ok(&self) -> bool {
        self.clipboard
    }

    fn dir_writable(&self, _path: &Path) -> bool {
        self.writable
    }
}

#[cfg(test)]
mod system_tests {
    use super::*;

    #[tokio::test]
    async fn system_runner_basics() {
        let r = SystemRunner;
        assert!(r.which("sh").is_some());
        assert!(r.which("definitely-not-a-program-vt").is_none());
        assert_eq!(
            r.run("definitely-not-a-program-vt", &[]).await,
            RunOutcome::NotFound
        );
        match r.run("sh", &["-c", "echo hi; echo err >&2; exit 3"]).await {
            RunOutcome::Exited {
                success,
                stdout,
                stderr,
            } => {
                assert!(!success);
                assert_eq!(stdout, "hi\n");
                assert_eq!(stderr, "err\n");
            }
            other => panic!("{other:?}"),
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(r.dir_writable(dir.path()));
        assert!(!r.dir_writable(Path::new("/proc/1")));
    }

    #[tokio::test]
    async fn system_runner_times_out() {
        let start = std::time::Instant::now();
        assert_eq!(run_short("sh", &["-c", "sleep 10"]).await, RunOutcome::Timeout);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    async fn run_short(program: &str, args: &[&str]) -> RunOutcome {
        run_with_timeout(program, args, Duration::from_millis(200)).await
    }
}
