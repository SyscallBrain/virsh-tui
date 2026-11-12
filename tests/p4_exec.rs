//! P4 execution tests: dry-run formatting and history.

#[test]
fn dry_run_formats_message() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let plan = virsh_tui::command::builders::lifecycle::start("arch-dev");
        let msg = virsh_tui::command::exec::execute("qemu:///system", &plan, true)
            .await
            .unwrap();
        assert!(msg.starts_with("[dry-run] Started arch-dev"));
        assert!(msg.contains("virsh start arch-dev"));
    });
}

#[test]
fn history_push_and_last() {
    let mut h = virsh_tui::command::history::History::new(2);
    h.push("virsh start a");
    h.push("virsh start b");
    h.push("virsh start c");
    assert_eq!(h.all().len(), 2);
    assert_eq!(h.last(), Some("virsh start c"));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn dropping_execute_stops_owned_descendants() {
    use std::time::Duration;
    use virsh_tui::command::{exec::execute, plan::CommandPlan};
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("pid");
    let script = "sleep 30 & echo $! > \"$1\"; wait";
    let plan = CommandPlan::single("sh", vec!["-c", script, "sh", marker.to_str().unwrap()], "test");
    let task = tokio::spawn(async move { execute("test:///default", &plan, false).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid: u32 = std::fs::read_to_string(marker).unwrap().trim().parse().unwrap();
    task.abort();
    let _ = task.await;
    let alive = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let status = std::fs::read_to_string(format!("/proc/{pid}/status"));
            if status.is_err() || status.unwrap().contains("State:\tZ") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    if alive.is_err() {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
    assert!(alive.is_ok(), "owned descendant survived cancellation of execute");
}
