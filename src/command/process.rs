//! Supervised subprocess execution for user commands.

use super::plan::CommandStep;
use std::{
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::watch,
};

const OUTPUT_LIMIT: usize = 1024 * 1024;

/// Cap for structured reads whose size grows with the number of objects
/// (`domstats` on a host with hundreds of guests easily passes 1 MiB).
pub const LARGE_OUTPUT_LIMIT: usize = 64 * 1024 * 1024;

/// Collected output; excess bytes are drained but not retained.
#[derive(Debug)]
pub struct StepOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

impl StepOutput {
    /// Return complete structured stdout, rejecting truncated data.
    pub fn structured_stdout(&self) -> Result<&[u8], RunError> {
        if self.stdout_truncated {
            Err(RunError::OutputLimit)
        } else {
            Ok(&self.stdout)
        }
    }
}

/// Failure to run a supervised step.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("process cancelled")]
    Cancelled,
    #[error("process timed out")]
    Timeout,
    #[error("process output exceeds the limit")]
    OutputLimit,
    #[error("process error: {0}")]
    Io(#[from] std::io::Error),
    #[error("output reader failed: {0}")]
    Reader(#[from] tokio::task::JoinError),
    #[error("read-only connection: {0} not run")]
    ReadOnly(String),
}

/// Program of the internal step that waits for a domain's libvirt job
/// (`backup-begin` returns at once; the job goes on in libvirt). Shown as a
/// shell comment: `# wait for the backup job of <domain>`.
pub const WAIT_JOB: &str = "#";

fn exit_status(code: i32) -> ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }
    #[cfg(not(unix))]
    {
        let _ = code;
        unreachable!("virsh-tui runs on unix")
    }
}

/// Poll `virsh domjobinfo` once a second until the domain has no active job,
/// then read how it ended (`--completed`).
async fn wait_libvirt_job(
    uri: &str,
    domain: &str,
    control: &mut watch::Receiver<bool>,
) -> Result<StepOutput, RunError> {
    let output = |code: i32, stderr: String| StepOutput {
        status: exit_status(code),
        stdout: Vec::new(),
        stderr: stderr.into_bytes(),
        stdout_truncated: false,
        stderr_truncated: false,
    };
    loop {
        tokio::select! {
            _ = control.changed() => {}
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        if *control.borrow() {
            return Err(RunError::Cancelled);
        }
        match crate::backend::virsh::exec::run(uri, &["domjobinfo", domain]).await {
            Ok(text) if crate::backend::virsh::parse_jobinfo::parse(&text).is_none() => break,
            Ok(_) => {}
            Err(e) => return Ok(output(1, format!("error: {e}"))),
        }
    }
    let done = crate::backend::virsh::exec::run(uri, &["domjobinfo", "--completed", domain])
        .await
        .unwrap_or_default();
    let kind = done
        .lines()
        .find_map(|l| l.strip_prefix("Job type:"))
        .map(str::trim)
        .unwrap_or("Completed");
    Ok(match kind {
        "Failed" | "Cancelled" => output(1, format!("error: the libvirt job of {domain} ended: {kind}")),
        _ => output(0, String::new()),
    })
}

#[cfg(unix)]
pub(crate) struct ProcessGroup {
    id: Option<u32>,
    leader_reaped: bool,
}
#[cfg(unix)]
impl ProcessGroup {
    pub(crate) fn new(id: Option<u32>) -> Self {
        Self {
            id,
            leader_reaped: false,
        }
    }
    pub(crate) fn signal(&self, signal: i32) {
        if self.leader_reaped {
            return;
        }
        if let Some(id) = self.id.and_then(|id| i32::try_from(id).ok()) {
            // The unreaped leader reserves this process-group ID.
            unsafe {
                libc::kill(-id, signal);
            }
        }
    }

    pub(crate) fn mark_reaped(&mut self) {
        self.leader_reaped = true;
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd"
))]
async fn observe_exit(id: u32) -> std::io::Result<()> {
    #[cfg(target_os = "freebsd")]
    let wait_id: libc::id_t = id.into();
    #[cfg(not(target_os = "freebsd"))]
    let wait_id: libc::id_t = id;
    // Quick commands are noticed within ~10 ms; long jobs (uploads,
    // migrations) settle at one check every 250 ms instead of 100 per second.
    let mut delay = Duration::from_millis(10);
    loop {
        let exited = {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    wait_id,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result != 0 {
                return Err(std::io::Error::last_os_error());
            }
            unsafe { info.si_pid() != 0 }
        };
        if exited {
            return Ok(());
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 3 / 2).min(Duration::from_millis(250));
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd"
)))]
async fn observe_exit(_id: u32) -> std::io::Result<()> {
    std::future::pending().await
}

#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.signal(libc::SIGKILL);
    }
}

/// Output collected so far by a [`drain`] task (kept when it is aborted).
type Sink = std::sync::Arc<std::sync::Mutex<(Vec<u8>, bool)>>;

async fn drain(mut input: impl AsyncRead + Unpin, limit: usize, sink: Sink) -> std::io::Result<()> {
    let mut buf = [0; 8192];
    loop {
        let n = input.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let mut guard = sink.lock().unwrap_or_else(|e| e.into_inner());
        let keep = n.min(limit.saturating_sub(guard.0.len()));
        guard.0.extend_from_slice(&buf[..keep]);
        guard.1 |= keep < n;
    }
    Ok(())
}

fn take_sink(sink: &Sink) -> (Vec<u8>, bool) {
    std::mem::take(&mut *sink.lock().unwrap_or_else(|e| e.into_inner()))
}

/// After the process exits, a descendant that left its process group may
/// still hold stdout/stderr open: stop waiting for EOF after this long.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Run one captured step, cancelling the owned process group when requested.
pub async fn run_step(
    uri: &str,
    step: &CommandStep,
    control: &mut watch::Receiver<bool>,
) -> Result<StepOutput, RunError> {
    let limit = crate::backend::virsh::exec::timeout_for(
        &step.program,
        step.argv.first().map(String::as_str).unwrap_or(""),
    );
    run_step_with_timeout(uri, step, control, limit).await
}

/// Run a step with an explicit deadline.
pub async fn run_step_with_timeout(
    uri: &str,
    step: &CommandStep,
    control: &mut watch::Receiver<bool>,
    limit: Duration,
) -> Result<StepOutput, RunError> {
    run_step_inner(uri, step, control, limit, OUTPUT_LIMIT, || {}).await
}

/// [`run_step`] keeping up to `out_limit` bytes of stdout.
pub async fn run_step_with_output_limit(
    uri: &str,
    step: &CommandStep,
    control: &mut watch::Receiver<bool>,
    out_limit: usize,
) -> Result<StepOutput, RunError> {
    let limit = crate::backend::virsh::exec::timeout_for(
        &step.program,
        step.argv.first().map(String::as_str).unwrap_or(""),
    );
    run_step_inner(uri, step, control, limit, out_limit, || {}).await
}

pub(super) async fn run_step_with_started(
    uri: &str,
    step: &CommandStep,
    control: &mut watch::Receiver<bool>,
    started: impl FnOnce(),
) -> Result<StepOutput, RunError> {
    let limit = crate::backend::virsh::exec::timeout_for(
        &step.program,
        step.argv.first().map(String::as_str).unwrap_or(""),
    );
    run_step_inner(uri, step, control, limit, OUTPUT_LIMIT, started).await
}

async fn run_step_inner(
    uri: &str,
    step: &CommandStep,
    control: &mut watch::Receiver<bool>,
    limit: Duration,
    out_limit: usize,
    started: impl FnOnce(),
) -> Result<StepOutput, RunError> {
    if *control.borrow() {
        return Err(RunError::Cancelled);
    }
    if step.program == WAIT_JOB {
        started();
        // Boxed: the wait runs `virsh domjobinfo` through this function again.
        return Box::pin(wait_libvirt_job(
            uri,
            step.argv.last().map_or("", String::as_str),
            control,
        ))
        .await;
    }
    let read_only = crate::config::read_only();
    // virsh is opened with `-r` below, so libvirt refuses its changes; other
    // programs (virt-install, virt-clone) are stopped here.
    if read_only && step.program != "virsh" && super::readonly::step_writes(step) {
        return Err(RunError::ReadOnly(step.program.clone()));
    }
    let mut command = Command::new(&step.program);
    if step.program == "virsh" {
        if read_only {
            command.arg("-r");
        }
        command.arg("-c").arg(uri);
    }
    command
        .args(&step.argv)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn()?;
    #[cfg(unix)]
    let mut group = ProcessGroup::new(child.id());
    started();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("missing stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("missing stderr"))?;
    let out_sink = Sink::default();
    let err_sink = Sink::default();
    let mut out = tokio::spawn(drain(stdout, out_limit, out_sink.clone()));
    let mut err = tokio::spawn(drain(stderr, OUTPUT_LIMIT, err_sink.clone()));
    // Armed (reset to now + DRAIN_GRACE) once the process has exited.
    let grace = tokio::time::sleep(Duration::from_secs(365 * 24 * 3600));
    tokio::pin!(grace);
    let mut grace_armed = false;
    let timeout = tokio::time::sleep(limit);
    tokio::pin!(timeout);
    let portable_child_id = child.id().unwrap_or_default();
    let mut control_open = true;
    let mut status = None;
    let mut stdout = None;
    let mut stderr = None;
    while status.is_none() || stdout.is_none() || stderr.is_none() {
        if status.is_some() && !grace_armed {
            grace_armed = true;
            grace.as_mut().reset(tokio::time::Instant::now() + DRAIN_GRACE);
        }
        tokio::select! {
            _ = &mut grace, if grace_armed => {
                // The leader is gone; keep what was read and stop waiting.
                out.abort();
                err.abort();
                stdout.get_or_insert(());
                stderr.get_or_insert(());
            },
            result = observe_exit(portable_child_id), if status.is_none() && cfg!(any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "netbsd")) => {
                if let Err(error) = result {
                    #[cfg(unix)] { group.leader_reaped = true; }
                    return Err(error.into());
                }
                #[cfg(unix)] group.signal(libc::SIGKILL);
                status = Some(child.wait().await?);
                #[cfg(unix)] { group.leader_reaped = true; }
            },
            result = child.wait(), if status.is_none() && !cfg!(any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "netbsd")) => {
                status = Some(result?);
                #[cfg(unix)] { group.leader_reaped = true; }
            },
            result = &mut out, if stdout.is_none() => { result??; stdout = Some(()); },
            result = &mut err, if stderr.is_none() => { result??; stderr = Some(()); },
            _ = &mut timeout => {
                #[cfg(unix)] group.signal(libc::SIGKILL);
                if status.is_none() {
                    let _ = child.kill().await;
                    #[cfg(unix)] { group.leader_reaped = true; }
                }
                out.abort(); err.abort();
                return Err(RunError::Timeout);
            }
            changed = control.changed(), if control_open => {
                if changed.is_err() { control_open = false; continue; }
                if *control.borrow() {
                    #[cfg(unix)] group.signal(libc::SIGINT);
                    #[cfg(not(unix))] let _ = child.start_kill();
                    if status.is_none() {
                        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "netbsd"))]
                        {
                            let observed = tokio::time::timeout(Duration::from_secs(3), observe_exit(group.id.unwrap_or_default())).await;
                            if matches!(observed, Ok(Err(_))) { group.leader_reaped = true; }
                            group.signal(libc::SIGKILL);
                            let _ = child.wait().await;
                            group.leader_reaped = true;
                        }
                        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd", target_os = "netbsd")))]
                        {
                            if tokio::time::timeout(Duration::from_secs(3), child.wait()).await.is_err() {
                                #[cfg(unix)] group.signal(libc::SIGKILL);
                                let _ = child.kill().await;
                            }
                            #[cfg(unix)] { group.leader_reaped = true; }
                        }
                    }
                    out.abort(); err.abort();
                    return Err(RunError::Cancelled);
                }
            }
        }
    }
    let status = status.ok_or_else(|| std::io::Error::other("missing process status"))?;
    let (stdout, stdout_truncated) = take_sink(&out_sink);
    let (stderr, stderr_truncated) = take_sink(&err_sink);
    Ok(StepOutput {
        status,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
    })
}

#[cfg(all(test, unix))]
mod tests {
    #[tokio::test]
    async fn reaped_unpinned_group_does_not_signal_reused_process_id() {
        let mut child = tokio::process::Command::new("sleep")
            .arg("30")
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        let group = super::ProcessGroup {
            id: Some(pid),
            leader_reaped: true,
        };
        drop(group);
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(
            child.try_wait().unwrap().is_none(),
            "reused process group was signalled"
        );
        child.kill().await.unwrap();
    }
}
