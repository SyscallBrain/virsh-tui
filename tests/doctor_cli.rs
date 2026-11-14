//! `virsh-tui --doctor`.

use std::process::Command;

#[test]
fn doctor_demo_prints_report_and_fails_on_errors() {
    let out = Command::new(env!("CARGO_BIN_EXE_virsh-tui"))
        .args(["--doctor", "--demo"])
        .env_remove("NO_COLOR")
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    // The demo report has one ✗, so the exit code is 1.
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.starts_with("virsh-tui "), "{stdout}");
    assert!(stdout.contains("doctor ·"));
    assert!(stdout.contains("errors") || stdout.contains("error"));
    // stdout is a pipe: no colours.
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn doctor_screen_dump() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doctor.ans");
    let status = Command::new(env!("CARGO_BIN_EXE_virsh-tui"))
        .args(["--demo", "--frozen", "--screen", "doctor", "--dump"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("not in group libvirt"), "{text}");
    assert!(text.contains("sudo usermod -aG libvirt demo"));
}
