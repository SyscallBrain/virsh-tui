use std::process::Stdio;
use std::time::Duration;

#[tokio::test]
async fn helper_waits_for_release_and_returns_requested_status() {
    let dir = tempfile::tempdir().unwrap();
    let started = dir.path().join("started");
    let release = dir.path().join("release");
    let mut child = tokio::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/slow_command.py"
        ))
        .arg("--started")
        .arg(&started)
        .arg("--release")
        .arg(&release)
        .arg("--exit-code")
        .arg("7")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(child.try_wait().unwrap().is_none());
    std::fs::write(&release, "release").unwrap();
    let status = tokio::time::timeout(Duration::from_secs(3), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.code(), Some(7));
}

#[tokio::test]
async fn helper_can_be_killed_and_reaped_while_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let started = dir.path().join("started");
    let mut child = tokio::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/slow_command.py"
        ))
        .arg("--started")
        .arg(&started)
        .arg("--release")
        .arg(dir.path().join("release"))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    child.kill().await.unwrap();
    assert!(!child.wait().await.unwrap().success());
}

#[tokio::test]
async fn supervised_step_drains_both_streams_and_caps_output() {
    use virsh_tui::command::{plan::CommandStep, process::run_step};
    let dir = tempfile::tempdir().unwrap();
    let started = dir.path().join("started");
    let release = dir.path().join("release");
    std::fs::write(&release, "go").unwrap();
    let step = CommandStep {
        program: "python3".into(),
        argv: vec![
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
            "--started".into(),
            started.to_string_lossy().into_owned(),
            "--release".into(),
            release.to_string_lossy().into_owned(),
            "--flood".into(),
            "1200000".into(),
        ],
        stdin: None,
    };
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        run_step("test:///default", &step, &mut rx),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 1024 * 1024);
    assert_eq!(output.stderr.len(), 1024 * 1024);
    assert!(output.stdout_truncated && output.stderr_truncated);
}

#[tokio::test]
async fn supervised_step_cancel_before_spawn_does_not_start_process() {
    use virsh_tui::command::{
        plan::CommandStep,
        process::{RunError, run_step},
    };
    let dir = tempfile::tempdir().unwrap();
    let started = dir.path().join("started");
    let step = CommandStep {
        program: "python3".into(),
        argv: vec![
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
            "--started".into(),
            started.to_string_lossy().into_owned(),
            "--release".into(),
            dir.path().join("release").to_string_lossy().into_owned(),
        ],
        stdin: None,
    };
    let (_tx, mut rx) = tokio::sync::watch::channel(true);
    assert!(matches!(
        run_step("test:///default", &step, &mut rx).await,
        Err(RunError::Cancelled)
    ));
    assert!(!started.exists());
}

#[tokio::test]
async fn dropping_cancellation_sender_does_not_prevent_completion() {
    use virsh_tui::command::{plan::CommandStep, process::run_step};
    let dir = tempfile::tempdir().unwrap();
    let release = dir.path().join("release");
    let step = CommandStep {
        program: "python3".into(),
        argv: vec![
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
            "--started".into(),
            dir.path().join("started").to_string_lossy().into_owned(),
            "--release".into(),
            release.to_string_lossy().into_owned(),
        ],
        stdin: None,
    };
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    drop(tx);
    let runner = tokio::spawn(async move { run_step("test:///default", &step, &mut rx).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    std::fs::write(release, "go").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(1), runner).await;
    assert!(
        result.is_ok(),
        "closed control channel prevented child completion"
    );
}

#[tokio::test]
async fn cancellation_stops_running_child() {
    use virsh_tui::command::{
        plan::CommandStep,
        process::{RunError, run_step},
    };
    let dir = tempfile::tempdir().unwrap();
    let started = dir.path().join("started");
    let step = CommandStep {
        program: "python3".into(),
        argv: vec![
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
            "--started".into(),
            started.to_string_lossy().into_owned(),
            "--release".into(),
            dir.path().join("release").to_string_lossy().into_owned(),
        ],
        stdin: None,
    };
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(async move { run_step("test:///default", &step, &mut rx).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid: u32 = std::fs::read_to_string(started).unwrap().parse().unwrap();
    tx.send(true).unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Err(RunError::Cancelled)
    ));
    #[cfg(unix)]
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

#[tokio::test]
async fn cancellation_racing_successful_exit_reaps_the_child() {
    use virsh_tui::command::{
        plan::CommandStep,
        process::{RunError, run_step},
    };
    for _ in 0..8 {
        let dir = tempfile::tempdir().unwrap();
        let started = dir.path().join("started");
        let release = dir.path().join("release");
        let step = CommandStep {
            program: "python3".into(),
            argv: vec![
                concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
                "--started".into(),
                started.to_string_lossy().into_owned(),
                "--release".into(),
                release.to_string_lossy().into_owned(),
            ],
            stdin: None,
        };
        let (cancel, mut control) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(async move { run_step("test:///default", &step, &mut control).await });
        tokio::time::timeout(Duration::from_secs(3), async {
            while !started.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let pid: u32 = std::fs::read_to_string(&started).unwrap().parse().unwrap();
        std::fs::write(&release, "release").unwrap();
        let _ = cancel.send(true);
        match tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
        {
            Ok(output) => assert!(output.status.success()),
            Err(RunError::Cancelled) => {}
            Err(error) => panic!("unexpected result in cancellation race: {error}"),
        }
        #[cfg(target_os = "linux")]
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        #[cfg(not(target_os = "linux"))]
        let _ = pid;
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn cancellation_stops_child_and_grandchild_in_owned_group() {
    use virsh_tui::command::{
        plan::CommandStep,
        process::{RunError, run_step},
    };
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("tree.py");
    let ready = dir.path().join("ready");
    std::fs::write(
        &script,
        "import os, sys, time\nfrom pathlib import Path\nif os.fork() == 0:\n if os.fork() == 0:\n  Path(sys.argv[1]).write_text(str(os.getppid()) + ' ' + str(os.getpid()))\n while True: time.sleep(1)\nwhile True: time.sleep(1)\n",
    )
    .unwrap();
    let step = CommandStep {
        program: "python3".into(),
        argv: vec![
            script.to_string_lossy().into_owned(),
            ready.to_string_lossy().into_owned(),
        ],
        stdin: None,
    };
    let (cancel, mut control) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(async move { run_step("test:///default", &step, &mut control).await });
    let pids = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&ready) {
                let ids: Vec<u32> = text.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                if ids.len() == 2 {
                    break ids;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancel.send(true).unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Err(RunError::Cancelled)
    ));
    tokio::time::timeout(Duration::from_secs(1), async {
        for pid in pids {
            loop {
                let state = std::fs::read_to_string(format!("/proc/{pid}/status"));
                if state.is_err() || state.unwrap().contains("State:\tZ") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    })
    .await
    .expect("owned descendant survived cancellation");
}

#[tokio::test]
async fn batch_stops_after_first_failure_and_keeps_successful_targets() {
    use virsh_tui::command::{
        job::{BatchResult, OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("should-not-exist");
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![
            ("A".into(), CommandPlan::single("sh", vec!["-c", "exit 0"], "A")),
            ("B".into(), CommandPlan::single("sh", vec!["-c", "exit 5"], "B")),
            (
                "C".into(),
                CommandPlan::single("touch", vec![marker.to_str().unwrap()], "C"),
            ),
        ],
        temporary_files: Vec::new(),
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(64);
    let result = run_batch(batch, rx, events).await;
    assert_eq!(result.completed_targets, vec!["A"]);
    assert!(matches!(result.result, BatchResult::Failed { .. }));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_second_target_keeps_first_and_never_starts_third() {
    use virsh_tui::command::{
        job::{BatchResult, OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let dir = tempfile::tempdir().unwrap();
    let second_started = dir.path().join("second-started");
    let third_started = dir.path().join("third-started");
    let first = CommandPlan::single("sh", vec!["-c", "exit 0"], "first");
    let second = CommandPlan::single(
        "sh",
        vec![
            "-c",
            "touch \"$1\"; exec sleep 30",
            "sh",
            second_started.to_str().unwrap(),
        ],
        "second",
    );
    let third = CommandPlan::single("touch", vec![third_started.to_str().unwrap()], "third");
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![("A".into(), first), ("B".into(), second), ("C".into(), third)],
        temporary_files: vec![],
    };
    let (cancel, control) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let task = tokio::spawn(run_batch(batch, control, events));
    tokio::time::timeout(Duration::from_secs(3), async {
        while !second_started.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancel.send(true).unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(outcome.result, BatchResult::Cancelled));
    assert_eq!(outcome.completed_targets, ["A"]);
    assert_eq!(outcome.completed_steps.len(), 1);
    assert_eq!(outcome.completed_steps[0].target, "A");
    assert_eq!(
        outcome
            .started_steps
            .iter()
            .map(|step| step.target.as_str())
            .collect::<Vec<_>>(),
        ["A", "B"]
    );
    assert!(!third_started.exists());
}

#[tokio::test]
async fn full_progress_channel_does_not_block_cancellation() {
    use virsh_tui::command::{
        job::{BatchResult, CommandEvent, OwnedBatch, run_batch},
        plan::{CommandPlan, CommandStep},
    };
    let dir = tempfile::tempdir().unwrap();
    let started = dir.path().join("started");
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![(
            "A".into(),
            CommandPlan {
                summary: "A".into(),
                steps: vec![CommandStep {
                    program: "python3".into(),
                    stdin: None,
                    argv: vec![
                        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
                        "--started".into(),
                        started.to_string_lossy().into_owned(),
                        "--release".into(),
                        dir.path().join("release").to_string_lossy().into_owned(),
                    ],
                }],
            },
        )],
        temporary_files: vec![],
    };
    let (events, _receiver) = tokio::sync::mpsc::channel(1);
    events
        .try_send(CommandEvent {
            target: "occupied".into(),
            step: 1,
            total: 1,
            command: "test".into(),
            program: "test".into(),
            argv: vec![],
        })
        .unwrap();
    let (cancel, control) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(run_batch(batch, control, events));
    tokio::time::timeout(Duration::from_secs(3), async {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancel.send(true).unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(outcome.result, BatchResult::Cancelled));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn completion_or_cancellation_cleans_up_when_descendant_holds_output_open() {
    use virsh_tui::command::{
        plan::CommandStep,
        process::{RunError, run_step},
    };
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("started");
    let step = CommandStep {
        program: "sh".into(),
        argv: vec![
            "-c".into(),
            "sleep 30 & echo started > \"$1\"".into(),
            "sh".into(),
            marker.to_string_lossy().into_owned(),
        ],
        stdin: None,
    };
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    let mut task = tokio::spawn(async move { run_step("test:///default", &step, &mut rx).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _ = tx.send(true);
    let result = tokio::time::timeout(Duration::from_secs(1), &mut task).await;
    if result.is_err() {
        task.abort();
    }
    assert!(matches!(result, Ok(Ok(Err(RunError::Cancelled))) | Ok(Ok(Ok(_)))));
}

#[tokio::test]
async fn batch_keeps_temporary_xml_until_child_finishes_then_removes_it() {
    use virsh_tui::command::{
        job::{BatchResult, OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let dir = tempfile::tempdir().unwrap();
    let xml = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
    std::fs::write(xml.path(), "<domain/>").unwrap();
    let xml_path = xml.path().to_path_buf();
    let marker = dir.path().join("read");
    let release = dir.path().join("release");
    let command =
        "test -f \"$1\" && cat \"$1\" > \"$2\"; while [ ! -f \"$3\" ]; do sleep 0.01; done; test -f \"$1\"";
    let plan = CommandPlan::single(
        "sh",
        vec![
            "-c",
            command,
            "sh",
            xml_path.to_str().unwrap(),
            marker.to_str().unwrap(),
            release.to_str().unwrap(),
        ],
        "test",
    );
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![("A".into(), plan)],
        temporary_files: vec![xml],
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let task = tokio::spawn(run_batch(batch, rx, events));
    tokio::time::timeout(Duration::from_secs(3), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(xml_path.exists());
    std::fs::write(release, "go").unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(outcome.result, BatchResult::Success));
    assert!(!xml_path.exists());
}

#[tokio::test]
async fn timeout_stops_child_without_waiting_for_production_deadline() {
    use virsh_tui::command::{
        plan::CommandStep,
        process::{RunError, run_step_with_timeout},
    };
    let dir = tempfile::tempdir().unwrap();
    let started = dir.path().join("started");
    let step = CommandStep {
        program: "python3".into(),
        argv: vec![
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
            "--started".into(),
            started.to_string_lossy().into_owned(),
            "--release".into(),
            dir.path().join("release").to_string_lossy().into_owned(),
        ],
        stdin: None,
    };
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    let result = run_step_with_timeout("test:///default", &step, &mut rx, Duration::from_millis(250)).await;
    assert!(matches!(result, Err(RunError::Timeout)));
    #[cfg(target_os = "linux")]
    let pid: u32 = std::fs::read_to_string(started).unwrap().parse().unwrap();
    #[cfg(target_os = "linux")]
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

#[tokio::test]
async fn structured_output_rejects_overflow() {
    use virsh_tui::command::{
        plan::CommandStep,
        process::{RunError, run_step},
    };
    let step = CommandStep::new("python3", vec!["-c", "print('x' * 1100000)"]);
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    let output = run_step("test:///default", &step, &mut rx).await.unwrap();
    assert!(matches!(output.structured_stdout(), Err(RunError::OutputLimit)));
}

#[tokio::test]
async fn spawn_failure_does_not_report_a_started_step() {
    use virsh_tui::command::{
        job::{BatchResult, OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![(
            "A".into(),
            CommandPlan::single("/this/path/cannot/exist", vec![], "A"),
        )],
        temporary_files: vec![],
    };
    let (_cancel, control) = tokio::sync::watch::channel(false);
    let (events, mut receiver) = tokio::sync::mpsc::channel(8);
    let outcome = run_batch(batch, control, events).await;
    assert!(matches!(outcome.result, BatchResult::Failed { .. }));
    assert!(receiver.try_recv().is_err());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn normal_completion_cleans_up_owned_background_descendants() {
    use virsh_tui::command::{plan::CommandStep, process::run_step};
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    let step = CommandStep {
        program: "sh".into(),
        argv: vec![
            "-c".into(),
            "sleep 30 </dev/null >/dev/null 2>&1 & echo $! > \"$1\"".into(),
            "sh".into(),
            pid_file.to_string_lossy().into_owned(),
        ],
        stdin: None,
    };
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    let output = run_step("test:///default", &step, &mut rx).await.unwrap();
    assert!(output.status.success());
    let pid: u32 = std::fs::read_to_string(pid_file).unwrap().trim().parse().unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let status = std::fs::read_to_string(format!("/proc/{pid}/status"));
            if status.is_err() || status.unwrap().contains("State:\tZ") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    if stopped.is_err() {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
    assert!(stopped.is_ok(), "owned background descendant survived its parent");
}

#[tokio::test]
async fn temporary_xml_is_removed_after_batch_failure_and_cancellation() {
    use virsh_tui::command::{
        job::{BatchResult, OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    for cancel in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let xml = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        let xml_path = xml.path().to_path_buf();
        let marker = dir.path().join("started");
        let script = if cancel {
            "test -f \"$1\" && touch \"$2\"; sleep 30"
        } else {
            "test -f \"$1\" && exit 5; exit 6"
        };
        let plan = CommandPlan::single(
            "sh",
            vec![
                "-c",
                script,
                "sh",
                xml_path.to_str().unwrap(),
                marker.to_str().unwrap(),
            ],
            "test",
        );
        let batch = OwnedBatch {
            uri: "test:///default".into(),
            demo: false,
            dry_run: false,
            plans: vec![("A".into(), plan)],
            temporary_files: vec![xml],
        };
        let (tx, rx) = tokio::sync::watch::channel(false);
        let (events, _receiver) = tokio::sync::mpsc::channel(8);
        let task = tokio::spawn(run_batch(batch, rx, events));
        if cancel {
            tokio::time::timeout(Duration::from_secs(3), async {
                while !marker.exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert!(xml_path.exists());
            tx.send(true).unwrap();
        }
        let outcome = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        if cancel {
            assert!(matches!(outcome.result, BatchResult::Cancelled));
        } else {
            assert!(matches!(outcome.result, BatchResult::Failed { .. }));
        }
        assert!(!xml_path.exists());
    }
}

#[tokio::test]
async fn batch_retains_started_steps_even_when_progress_channel_is_full() {
    use virsh_tui::command::{
        job::{CommandEvent, OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![("A".into(), CommandPlan::single("sh", vec!["-c", "exit 0"], "A"))],
        temporary_files: vec![],
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(1);
    events
        .try_send(CommandEvent {
            target: "occupied".into(),
            step: 1,
            total: 1,
            command: "test".into(),
            program: "test".into(),
            argv: vec![],
        })
        .unwrap();
    let outcome = run_batch(batch, rx, events).await;
    assert_eq!(outcome.started_steps.len(), 1);
    assert_eq!(outcome.started_steps[0].target, "A");
}

#[tokio::test]
async fn batch_failure_message_sanitizes_terminal_control_characters() {
    use virsh_tui::command::{
        job::{BatchResult, OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![(
            "A".into(),
            CommandPlan::single("sh", vec!["-c", "printf 'bad\\033[31m' >&2; exit 1"], "A"),
        )],
        temporary_files: vec![],
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let outcome = run_batch(batch, rx, events).await;
    let BatchResult::Failed { message, .. } = outcome.result else {
        panic!("expected failure")
    };
    assert!(!message.chars().any(char::is_control));
    assert!(message.starts_with("bad"));
}

#[test]
fn demo_safety_net_blocks_batches_even_if_request_mode_is_wrong() {
    const CHILD: &str = "VIRSH_TUI_TEST_DEMO_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("demo_safety_net_blocks_batches_even_if_request_mode_is_wrong")
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("must-not-exist");
    virsh_tui::command::exec::set_demo(true);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        use virsh_tui::command::{
            job::{OwnedBatch, run_batch},
            plan::CommandPlan,
        };
        let batch = OwnedBatch {
            uri: "test:///default".into(),
            demo: false,
            dry_run: false,
            plans: vec![(
                "A".into(),
                CommandPlan::single("touch", vec![marker.to_str().unwrap()], "A"),
            )],
            temporary_files: vec![],
        };
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let (events, _receiver) = tokio::sync::mpsc::channel(8);
        let _ = run_batch(batch, rx, events).await;
        assert!(!marker.exists(), "demo safety net allowed a subprocess");
    });
}

#[tokio::test]
async fn preview_batch_modes_do_not_spawn_preparation_or_mutation_steps() {
    use virsh_tui::command::{
        job::{OwnedBatch, run_batch},
        plan::{CommandPlan, CommandStep},
    };
    let dir = tempfile::tempdir().unwrap();
    for (demo, dry_run) in [(true, false), (false, true)] {
        let preparation = dir.path().join(format!("prepare-{demo}"));
        let mutation = dir.path().join(format!("mutate-{demo}"));
        let batch = OwnedBatch {
            uri: "test:///default".into(),
            demo,
            dry_run,
            plans: vec![(
                "vt-test-a".into(),
                CommandPlan {
                    summary: "preview".into(),
                    steps: vec![
                        CommandStep::new("touch", vec![preparation.to_str().unwrap()]),
                        CommandStep::new("touch", vec![mutation.to_str().unwrap()]),
                    ],
                },
            )],
            temporary_files: vec![],
        };
        let (_cancel, control) = tokio::sync::watch::channel(false);
        let (events, _receiver) = tokio::sync::mpsc::channel(8);
        let outcome = run_batch(batch, control, events).await;
        assert!(outcome.started_steps.is_empty());
        assert!(!preparation.exists());
        assert!(!mutation.exists());
    }
}

#[tokio::test]
async fn started_step_records_exact_program_and_arguments() {
    use virsh_tui::command::{
        job::{OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![(
            "A".into(),
            CommandPlan::single("sh", vec!["-c", "exit 0", "a b"], "A"),
        )],
        temporary_files: vec![],
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let result = run_batch(batch, rx, events).await;
    let started = &result.started_steps[0];
    assert_eq!(started.program, "sh");
    assert_eq!(started.argv, vec!["-c", "exit 0", "a b"]);
}

#[tokio::test]
async fn step_receives_c_locale_and_closed_stdin() {
    use virsh_tui::command::{plan::CommandStep, process::run_step};
    let step = CommandStep::new(
        "sh",
        vec![
            "-c",
            "printf '%s:' \"$LC_ALL\"; if read -r x; then printf input; else printf eof; fi",
        ],
    );
    let (_tx, mut rx) = tokio::sync::watch::channel(false);
    let result = run_step("test:///default", &step, &mut rx).await.unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"C:eof");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn cancellation_escalates_after_child_ignores_sigint() {
    use virsh_tui::command::{
        plan::CommandStep,
        process::{RunError, run_step},
    };
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("started");
    let step = CommandStep {
        program: "python3".into(),
        argv: vec![
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
            "--started".into(),
            marker.to_string_lossy().into_owned(),
            "--release".into(),
            dir.path().join("release").to_string_lossy().into_owned(),
            "--ignore-sigint".into(),
        ],
        stdin: None,
    };
    let (tx, mut rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(async move { run_step("test:///default", &step, &mut rx).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid: u32 = std::fs::read_to_string(marker).unwrap().parse().unwrap();
    let before = std::time::Instant::now();
    tx.send(true).unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(outcome, Err(RunError::Cancelled)));
    assert!(before.elapsed() >= Duration::from_secs(3));
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

#[tokio::test]
async fn aborted_batch_releases_temporary_xml() {
    use virsh_tui::command::{
        job::{OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let dir = tempfile::tempdir().unwrap();
    let xml = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
    let path = xml.path().to_path_buf();
    let marker = dir.path().join("started");
    let plan = CommandPlan::single(
        "sh",
        vec![
            "-c",
            "test -f \"$1\" && touch \"$2\"; sleep 30",
            "sh",
            path.to_str().unwrap(),
            marker.to_str().unwrap(),
        ],
        "test",
    );
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![("A".into(), plan)],
        temporary_files: vec![xml],
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let task = tokio::spawn(run_batch(batch, rx, events));
    tokio::time::timeout(Duration::from_secs(3), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(path.exists());
    task.abort();
    let _ = task.await;
    assert!(!path.exists());
}

#[tokio::test]
async fn batch_preserves_multiline_stdout_for_raw_commands() {
    use virsh_tui::command::{
        job::{OwnedBatch, run_batch},
        plan::CommandPlan,
    };
    let batch = OwnedBatch {
        uri: "test:///default".into(),
        demo: false,
        dry_run: false,
        plans: vec![(
            "A".into(),
            CommandPlan::single("sh", vec!["-c", "printf 'alpha\\nbeta\\n'"], "A"),
        )],
        temporary_files: vec![],
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let result = run_batch(batch, rx, events).await;
    assert_eq!(result.outputs[0].stdout, b"alpha\nbeta\n");
}

#[cfg(unix)]
#[test]
fn sigterm_restores_terminal_and_exits_with_signal_status() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/signal_terminal.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn signals_stop_held_job_and_read_and_restore_terminal() {
    for (mode, signal) in [
        ("job", "SIGINT"),
        ("read", "SIGINT"),
        ("job", "SIGTERM"),
        ("read", "SIGTERM"),
    ] {
        let status = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/signal_pending.py"
            ))
            .arg(env!("CARGO_BIN_EXE_virsh-tui"))
            .arg(mode)
            .arg(signal)
            .status()
            .unwrap();
        assert!(status.success(), "{signal} shutdown failed during {mode}");
    }
}

#[cfg(unix)]
#[test]
fn first_frame_does_not_wait_for_initial_domain_fetch() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/slow_startup.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[tokio::test]
async fn network_workflow_stops_after_start_failure_and_removes_xml() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_network_create};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nbase=$(dirname \"$0\")\ncase \"$1\" in net-define) test -f \"$2\" && cp \"$2\" \"$base/xml-seen\";; net-start) exit 4;; net-autostart) touch \"$base/later\";; esac\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let result = run_network_create(
        "test:///default".into(),
        "vt-test-net".into(),
        "<network/>".into(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(result.result, BatchResult::Failed { step: 2, .. }));
    assert_eq!(result.completed_steps.len(), 1);
    assert_eq!(result.completed_steps[0].argv[0], "net-define");
    assert!(dir.path().join("xml-seen").exists());
    assert!(!dir.path().join("later").exists());
    let path = &result.started_steps[0].argv[1];
    assert!(!std::path::Path::new(path).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn wizard_define_owns_xml_until_define_finishes() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::{
        job::{BatchResult, run_wizard_define},
        plan::CommandStep,
    };
    let dir = tempfile::tempdir().unwrap();
    let install = dir.path().join("fake-install");
    let define = dir.path().join("fake-define");
    std::fs::write(&install, "#!/bin/sh\nprintf '<domain/>'\n").unwrap();
    std::fs::write(
        &define,
        "#!/bin/sh\nbase=$(dirname \"$0\")\ncp \"$2\" \"$base/xml-seen\"\n",
    )
    .unwrap();
    for program in [&install, &define] {
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let step = CommandStep {
        program: install.to_string_lossy().into_owned(),
        argv: vec!["--connect".into(), "test:///default".into(), "--print-xml".into()],
        stdin: None,
    };
    let result = run_wizard_define(
        "test:///default".into(),
        "vt-test-vm".into(),
        step,
        define.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(result.result, BatchResult::Success));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("xml-seen")).unwrap(),
        "<domain/>"
    );
    assert_eq!(result.started_steps.len(), 2);
    assert_eq!(result.completed_steps.len(), 2);
    assert!(!std::path::Path::new(&result.started_steps[1].argv[1]).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn wizard_define_rejects_truncated_xml_without_defining() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::BatchResult;
    use virsh_tui::command::{job::run_wizard_define, plan::CommandStep};
    let dir = tempfile::tempdir().unwrap();
    let install = dir.path().join("fake-install");
    let define = dir.path().join("fake-define");
    std::fs::write(&install, "#!/bin/sh\nhead -c 1100000 /dev/zero\n").unwrap();
    std::fs::write(&define, "#!/bin/sh\ntouch \"$(dirname \"$0\")/defined\"\n").unwrap();
    for program in [&install, &define] {
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let step = CommandStep {
        program: install.to_string_lossy().into_owned(),
        argv: vec![],
        stdin: None,
    };
    let result = run_wizard_define(
        "test:///default".into(),
        "vt-test-vm".into(),
        step,
        define.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(result.result, BatchResult::Failed { step: 1, .. }));
    assert!(result.completed_targets.is_empty());
    assert!(!dir.path().join("defined").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn wizard_define_rejects_invalid_or_wrong_root_xml_before_define() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::{
        job::{BatchResult, run_wizard_define},
        plan::CommandStep,
    };
    for xml in ["<domain", "<network/>"] {
        let dir = tempfile::tempdir().unwrap();
        let install = dir.path().join("fake-install");
        let define = dir.path().join("fake-define");
        std::fs::write(&install, format!("#!/bin/sh\nprintf '%s' '{xml}'\n")).unwrap();
        std::fs::write(&define, "#!/bin/sh\ntouch \"$(dirname \"$0\")/defined\"\n").unwrap();
        for program in [&install, &define] {
            std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let (_cancel, control) = tokio::sync::watch::channel(false);
        let (events, _receiver) = tokio::sync::mpsc::channel(8);
        let outcome = run_wizard_define(
            "test:///default".into(),
            "vt-test-vm".into(),
            CommandStep::new(install.to_str().unwrap(), vec![]),
            define.to_string_lossy().into_owned(),
            control,
            events,
        )
        .await;
        assert!(
            matches!(outcome.result, BatchResult::Failed { step: 1, .. }),
            "{xml}"
        );
        assert!(outcome.completed_targets.is_empty());
        assert!(!dir.path().join("defined").exists());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_wizard_define_removes_xml_and_stops_child() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::{
        job::{BatchResult, run_wizard_define},
        plan::CommandStep,
    };
    let dir = tempfile::tempdir().unwrap();
    let install = dir.path().join("fake-install");
    let define = dir.path().join("fake-define");
    std::fs::write(&install, "#!/bin/sh\nprintf '<domain/>'\n").unwrap();
    std::fs::write(&define, "#!/bin/sh\nbase=$(dirname \"$0\")\nprintf '%s' \"$2\" > \"$base/started\"\nwhile :; do sleep 0.05; done\n").unwrap();
    for program in [&install, &define] {
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let (tx, control) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let step = CommandStep {
        program: install.to_string_lossy().into_owned(),
        argv: vec![],
        stdin: None,
    };
    let task = tokio::spawn(run_wizard_define(
        "test:///default".into(),
        "vt-test-vm".into(),
        step,
        define.to_string_lossy().into_owned(),
        control,
        events,
    ));
    let marker = dir.path().join("started");
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !marker.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let xml_path = std::fs::read_to_string(&marker).unwrap();
    assert!(std::path::Path::new(&xml_path).exists());
    tx.send(true).unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(4), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result.result, BatchResult::Cancelled));
    assert_eq!(result.completed_steps.len(), 1);
    assert!(result.completed_targets.is_empty());
    assert!(!std::path::Path::new(&xml_path).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn failed_snapshot_safety_step_prevents_revert() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::{
        builders::snapshot,
        job::{BatchResult, OwnedBatch, run_batch},
    };
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = snapshot-create-as ]; then exit 7; fi\ntouch \"$(dirname \"$0\")/reverted\"\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut plan = snapshot::revert("vt-test-vm", "test-snapshot", None, false, true);
    for step in &mut plan.steps {
        step.program = program.to_string_lossy().into_owned();
    }
    let (_cancel, control) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let outcome = run_batch(
        OwnedBatch {
            uri: "test:///default".into(),
            demo: false,
            dry_run: false,
            plans: vec![("vt-test-vm".into(), plan)],
            temporary_files: vec![],
        },
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { step: 1, .. }));
    assert_eq!(outcome.started_steps.len(), 1);
    assert!(!dir.path().join("reverted").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn standalone_eject_uses_detected_drive_and_captured_domain() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_media_eject};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = dumpxml ]; then printf \"<domain><name>vt-test-vm</name><devices><disk type='file' device='cdrom'><target dev='hdb' bus='ide'/></disk></devices></domain>\"; else printf '%s\\n' \"$@\" > \"$(dirname \"$0\")/args\"; fi\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_media_eject(
        "test:///default".into(),
        "vt-test-vm".into(),
        true,
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Success));
    let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
    assert!(args.contains("vt-test-vm\nhdb\n--eject\n"));
    assert_eq!(outcome.completed_steps.len(), 2);
}

#[cfg(unix)]
#[tokio::test]
async fn standalone_eject_never_runs_after_invalid_xml() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_media_eject};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = dumpxml ]; then printf '<domain'; else touch \"$(dirname \"$0\")/ejected\"; fi\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_media_eject(
        "test:///default".into(),
        "vt-test-vm".into(),
        true,
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { step: 1, .. }));
    assert!(outcome.completed_targets.is_empty());
    assert!(!dir.path().join("ejected").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn media_change_rejects_wrong_root_before_mutation() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_media_eject};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = dumpxml ]; then printf \"<network><name>vt-test-vm</name><devices><disk device='cdrom'><target dev='hdb'/></disk></devices></network>\"; else touch \"$(dirname \"$0\")/mutated\"; fi\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_cancel, control) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let outcome = run_media_eject(
        "test:///default".into(),
        "vt-test-vm".into(),
        true,
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { step: 1, .. }));
    assert!(!dir.path().join("mutated").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn media_change_without_target_uses_detected_cdrom() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_media_change};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = dumpxml ]; then printf \"<domain><name>vt-test-vm</name><devices><disk type='file' device='cdrom'><target dev='hdb' bus='ide'/></disk></devices></domain>\"; else printf '%s\\n' \"$@\" > \"$(dirname \"$0\")/args\"; fi\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_media_change(
        "test:///default".into(),
        "vt-test-vm".into(),
        Some("/tmp/test.iso".into()),
        true,
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Success));
    let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
    assert!(args.contains("vt-test-vm\nhdb\n/tmp/test.iso\n--update\n"));
}

#[cfg(unix)]
#[tokio::test]
async fn description_workflow_defines_captured_xml_and_removes_temp_file() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_description_update};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = dumpxml ]; then printf '<domain><name>vt-test-vm</name><description>old</description></domain>'; else cp \"$2\" \"$(dirname \"$0\")/xml-seen\"; fi\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_description_update(
        "test:///default".into(),
        "vt-test-vm".into(),
        "new & safe".into(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Success));
    let xml = std::fs::read_to_string(dir.path().join("xml-seen")).unwrap();
    assert!(xml.contains("<description>new &amp; safe</description>"));
    assert_eq!(outcome.started_steps.len(), 2);
    assert!(!std::path::Path::new(&outcome.started_steps[1].argv[1]).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn description_workflow_does_not_define_invalid_xml() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_description_update};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = dumpxml ]; then printf '<domain'; else touch \"$(dirname \"$0\")/defined\"; fi\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_description_update(
        "test:///default".into(),
        "vt-test-vm".into(),
        "new".into(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { step: 1, .. }));
    assert!(!dir.path().join("defined").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn hostdev_workflow_attaches_only_detected_pci_device() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_hostdev_attach};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = nodedev-dumpxml ]; then printf \"<device><capability type='pci'><domain>0</domain><bus>2</bus><slot>3</slot><function>1</function></capability></device>\"; else cp \"$3\" \"$(dirname \"$0\")/xml-seen\"; fi\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_hostdev_attach(
        "test:///default".into(),
        "vt-test-vm".into(),
        "pci_test".into(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Success));
    let xml = std::fs::read_to_string(dir.path().join("xml-seen")).unwrap();
    assert!(xml.contains("bus='0x02' slot='0x03' function='0x1'"));
    assert!(!std::path::Path::new(&outcome.started_steps[1].argv[2]).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn hostdev_workflow_rejects_invalid_usb_ids() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_hostdev_attach};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nif [ \"$1\" = nodedev-dumpxml ]; then printf \"<device><capability type='usb_device'><vendor id='0x1234&amp;apos;'/><product id='0x0001'/></capability></device>\"; else touch \"$(dirname \"$0\")/attached\"; fi\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_hostdev_attach(
        "test:///default".into(),
        "vt-test-vm".into(),
        "usb_test".into(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { step: 1, .. }));
    assert!(!dir.path().join("attached").exists());
}

#[test]
fn hostdev_xml_accepts_normal_usb_ids_and_rejects_non_numeric_values() {
    use virsh_tui::command::builders::hardware::hostdev_xml;
    let valid = "<device><capability type='usb_device'><vendor id='0X1234'/><product id='0x0001'/></capability></device>";
    let xml = hostdev_xml(valid).unwrap();
    assert!(xml.contains("<vendor id='0x1234'/>"));
    assert!(xml.contains("<product id='0x0001'/>"));
    let invalid = "<device><capability type='usb_device'><vendor id='oops'/><product id='0x0001'/></capability></device>";
    assert!(hostdev_xml(invalid).is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn hardware_apply_keeps_xml_through_define_and_records_partial_failure() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::{
        job::{BatchResult, run_hardware_apply},
        plan::{CommandPlan, CommandStep},
    };
    use virsh_tui::ui::views::detail::hardware::HardwareState;
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(
        &program,
        "#!/bin/sh\nif [ \"$1\" = define ]; then cp \"$2\" \"$(dirname \"$0\")/xml-seen\"; else exit 7; fi\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let plan = CommandPlan {
        summary: "apply".into(),
        steps: vec![
            CommandStep::new("virsh", vec!["define", "<private-xml>", "--validate"]),
            CommandStep::new("virsh", vec!["setvcpus", "vt-test-vm", "4", "--live"]),
        ],
    };
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_hardware_apply(
        "test:///default".into(),
        "vt-test-vm".into(),
        HardwareState::demo(),
        vec![plan],
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { step: 2, .. }));
    assert_eq!(outcome.completed_steps.len(), 1);
    assert!(dir.path().join("xml-seen").exists());
    assert!(!std::path::Path::new(&outcome.started_steps[0].argv[1]).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn xml_export_never_overwrites_existing_file() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_xml_export};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nprintf '<domain/>'\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = dir.path().join("existing.xml");
    std::fs::write(&path, "preserve").unwrap();
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_xml_export(
        "test:///default".into(),
        "vt-test-vm".into(),
        path.clone(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { .. }));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "preserve");
    assert!(outcome.completed_targets.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn xml_export_rejects_wrong_root_without_creating_a_file() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_xml_export};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    let path = dir.path().join("export.xml");
    std::fs::write(&program, "#!/bin/sh\nprintf '<network/>'\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_cancel, control) = tokio::sync::watch::channel(false);
    let (events, _receiver) = tokio::sync::mpsc::channel(8);
    let outcome = run_xml_export(
        "test:///default".into(),
        "vt-test-vm".into(),
        path.clone(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { .. }));
    assert!(!path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn cancelled_preparation_never_defines_attaches_or_exports() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_description_update, run_hostdev_attach, run_xml_export};
    for workflow in ["description", "hostdev", "export"] {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-virsh");
        let started = dir.path().join("started");
        let mutation = dir.path().join("mutation");
        let export = dir.path().join("export.xml");
        std::fs::write(
            &program,
            "#!/usr/bin/env python3\nimport pathlib, sys, time\nroot = pathlib.Path(__file__).parent\nif sys.argv[1] in ('dumpxml', 'nodedev-dumpxml'):\n (root / 'started').write_text('started')\n while True: time.sleep(0.05)\nelse:\n (root / 'mutation').write_text('mutation')\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, _receiver) = tokio::sync::mpsc::channel(8);
        let program = program.to_string_lossy().into_owned();
        let export_path = export.clone();
        let task = tokio::spawn(async move {
            match workflow {
                "description" => {
                    run_description_update(
                        "test:///default".into(),
                        "vt-test-vm".into(),
                        "description".into(),
                        program,
                        control,
                        events,
                    )
                    .await
                }
                "hostdev" => {
                    run_hostdev_attach(
                        "test:///default".into(),
                        "vt-test-vm".into(),
                        "vt-test-device".into(),
                        program,
                        control,
                        events,
                    )
                    .await
                }
                "export" => {
                    run_xml_export(
                        "test:///default".into(),
                        "vt-test-vm".into(),
                        export_path,
                        program,
                        control,
                        events,
                    )
                    .await
                }
                _ => unreachable!(),
            }
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            while !started.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        cancel.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result.result, BatchResult::Cancelled), "{workflow}");
        assert!(
            !mutation.exists(),
            "{workflow} started a mutation after cancellation"
        );
        assert!(
            !export.exists(),
            "{workflow} created an export after cancellation"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn oversized_preparation_never_defines_or_attaches() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_description_update, run_hostdev_attach};
    for workflow in ["description", "hostdev"] {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-virsh");
        let mutation = dir.path().join("mutation");
        std::fs::write(
            &program,
            "#!/usr/bin/env python3\nimport pathlib, sys\nif sys.argv[1] in ('dumpxml', 'nodedev-dumpxml'):\n sys.stdout.write('x' * (1024 * 1024 + 1))\nelse:\n (pathlib.Path(__file__).parent / 'mutation').write_text('mutation')\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (_cancel, control) = tokio::sync::watch::channel(false);
        let (events, _receiver) = tokio::sync::mpsc::channel(8);
        let program = program.to_string_lossy().into_owned();
        let result = match workflow {
            "description" => {
                run_description_update(
                    "test:///default".into(),
                    "vt-test-vm".into(),
                    "description".into(),
                    program,
                    control,
                    events,
                )
                .await
            }
            "hostdev" => {
                run_hostdev_attach(
                    "test:///default".into(),
                    "vt-test-vm".into(),
                    "vt-test-device".into(),
                    program,
                    control,
                    events,
                )
                .await
            }
            _ => unreachable!(),
        };
        assert!(matches!(result.result, BatchResult::Failed { .. }), "{workflow}");
        assert!(result.outputs[0].stdout_truncated, "{workflow}");
        assert!(
            !mutation.exists(),
            "{workflow} started a mutation with oversized XML"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn xml_export_writes_complete_output_and_rejects_invalid_xml() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_xml_export};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(
        &program,
        "#!/bin/sh\nprintf '<domain><name>vt-test-vm</name></domain>'\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let path = dir.path().join("export.xml");
    let outcome = run_xml_export(
        "test:///default".into(),
        "vt-test-vm".into(),
        path.clone(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Success));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "<domain><name>vt-test-vm</name></domain>"
    );
    std::fs::write(&program, "#!/bin/sh\nprintf '<domain'\n").unwrap();
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let path = dir.path().join("invalid.xml");
    let outcome = run_xml_export(
        "test:///default".into(),
        "vt-test-vm".into(),
        path.clone(),
        program.to_string_lossy().into_owned(),
        tx.subscribe(),
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { .. }));
    assert!(!path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn xml_export_rejects_truncated_output_without_creating_file() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_xml_export};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\nhead -c 1100000 /dev/zero\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = dir.path().join("export.xml");
    let (_tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_xml_export(
        "test:///default".into(),
        "vt-test-vm".into(),
        path.clone(),
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { .. }));
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn signal_during_foreground_editor_restores_terminal_and_stops_descendants() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/signal_foreground.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn held_job_keeps_real_ui_loop_responsive() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/responsive_loop.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn preview_cli_modes_do_not_execute_command_workflows() {
    for mode in ["demo", "dry-run"] {
        let status = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/preview_modes.py"
            ))
            .arg(env!("CARGO_BIN_EXE_virsh-tui"))
            .arg(mode)
            .status()
            .unwrap();
        assert!(status.success(), "preview mode failed: {mode}");
    }
}

#[cfg(unix)]
#[test]
fn command_routes_keep_real_ui_responsive_while_jobs_are_held() {
    for route in [
        "keyboard",
        "command",
        "palette",
        "repeat",
        "confirm",
        "no-confirm",
        "wizard-draft",
        "network",
        "network-create",
        "pool",
        "volume",
        "snapshot",
        "media",
        "disconnect",
        "completion",
    ] {
        let status = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/responsive_routes.py"
            ))
            .arg(env!("CARGO_BIN_EXE_virsh-tui"))
            .arg(route)
            .status()
            .unwrap();
        assert!(status.success(), "held-job route failed: {route}");
    }
}

#[cfg(unix)]
#[test]
fn delayed_reads_keep_real_ui_loop_responsive() {
    for mode in [
        "startup",
        "detail",
        "snapshots",
        "picker",
        "viewer",
        "palette",
        "completion",
        "refresh",
        "network",
        "pool",
    ] {
        let status = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/responsive_reads.py"
            ))
            .arg(env!("CARGO_BIN_EXE_virsh-tui"))
            .arg(mode)
            .status()
            .unwrap();
        assert!(status.success(), "delayed read failed: {mode}");
    }
}

#[cfg(unix)]
#[test]
fn switching_connections_cancels_a_delayed_viewer_lookup() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/responsive_reads.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .arg("viewer-switch")
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn delayed_preparations_keep_real_ui_responsive_and_cancel_cleanly() {
    for mode in ["description-prep", "hostdev-prep", "export-prep"] {
        let status = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/responsive_reads.py"
            ))
            .arg(env!("CARGO_BIN_EXE_virsh-tui"))
            .arg(mode)
            .status()
            .unwrap();
        assert!(status.success(), "delayed preparation failed: {mode}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn edited_xml_define_owns_file_and_rejects_wrong_root() {
    use std::os::unix::fs::PermissionsExt;
    use virsh_tui::command::job::{BatchResult, run_xml_define};
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fake-virsh");
    std::fs::write(&program, "#!/bin/sh\ncp \"$2\" \"$(dirname \"$0\")/xml-seen\"\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (tx, control) = tokio::sync::watch::channel(false);
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_xml_define(
        "test:///default".into(),
        "vt-test-a".into(),
        "<domain><name>vt-test-a</name></domain>".into(),
        false,
        program.to_string_lossy().into_owned(),
        control,
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Success));
    assert!(dir.path().join("xml-seen").exists());
    assert!(!std::path::Path::new(&outcome.started_steps[0].argv[1]).exists());
    std::fs::remove_file(dir.path().join("xml-seen")).unwrap();
    let (events, _rx) = tokio::sync::mpsc::channel(8);
    let outcome = run_xml_define(
        "test:///default".into(),
        "vt-test-a".into(),
        "<network/>".into(),
        false,
        program.to_string_lossy().into_owned(),
        tx.subscribe(),
        events,
    )
    .await;
    assert!(matches!(outcome.result, BatchResult::Failed { .. }));
    assert!(!dir.path().join("xml-seen").exists());
}

#[cfg(unix)]
#[test]
fn xml_editor_reopens_after_validation_error_without_leaking_files() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/editor_workflow.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn network_xml_editor_reopens_after_validation_error() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/editor_workflow.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .arg("network")
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn wizard_xml_editor_reopens_after_validation_error() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/editor_workflow.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .arg("wizard")
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn delayed_wizard_xml_keeps_draft_editable_and_does_not_open_stale_editor() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/editor_workflow.py"
        ))
        .arg(env!("CARGO_BIN_EXE_virsh-tui"))
        .arg("wizard-delay")
        .status()
        .unwrap();
    assert!(status.success());
}
