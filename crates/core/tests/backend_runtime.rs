use agenthub_core::adapters::run_command;
use std::time::{Duration, Instant};

#[test]
fn stalled_process_is_reported_as_timeout_with_bounded_wait() {
    #[cfg(windows)]
    let (program, args) = (
        "powershell.exe",
        vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            "Start-Sleep -Seconds 60".into(),
        ],
    );
    #[cfg(not(windows))]
    let (program, args) = ("sh", vec!["-c".into(), "exec sleep 60".into()]);
    let start = Instant::now();
    let output = run_command(program, &args, None, Duration::from_millis(200)).unwrap();
    assert!(output.timed_out);
    assert!(!output.succeeded());
    assert!(start.elapsed() < Duration::from_secs(8));
}

#[test]
fn process_failure_keeps_exit_code_and_spawn_error() {
    #[cfg(windows)]
    let (program, args) = (
        "cmd.exe",
        vec![
            "/D".into(),
            "/C".into(),
            "echo offline fixture 1>&2 & exit /b 7".into(),
        ],
    );
    #[cfg(not(windows))]
    let (program, args) = (
        "sh",
        vec!["-c".into(), "echo 'offline fixture' >&2; exit 7".into()],
    );
    let output = run_command(program, &args, None, Duration::from_secs(8)).unwrap();
    assert!(!output.timed_out, "unexpected timeout: {output:?}");
    assert!(!output.succeeded());
    assert_eq!(output.exit_code(), Some(7));
    assert!(output.stderr.contains("offline fixture"));
    assert!(run_command(
        "agenthub-nonexistent-backend-648209.exe",
        &[],
        None,
        Duration::from_secs(1)
    )
    .is_err());
}
