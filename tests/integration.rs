/*
 * Integration tests for the timeout CLI.
 *
 * These tests validate GNU coreutils compatibility - we must behave exactly
 * like Linux timeout for scripts to be portable. Each test documents the
 * expected behavior with references to GNU behavior where relevant.
 */

use assert_cmd::Command;
use predicates::prelude::*;
use std::time::{Duration, Instant};

#[allow(deprecated)]
fn timeout_cmd() -> Command {
    Command::cargo_bin("timeout").unwrap()
}

/* =========================================================================
 * BASIC FUNCTIONALITY - Core timeout behavior
 * ========================================================================= */

#[test]
fn test_command_completes_before_timeout() {
    /*
     * When command finishes before timeout, we should exit immediately
     * with the command's exit status. No waiting around.
     */
    let start = Instant::now();

    timeout_cmd()
        .args(["5s", "echo", "hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));

    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn test_command_completes_with_exit_code() {
    /* Pass through the command's exit code unchanged */
    timeout_cmd()
        .args(["5s", "sh", "--", "-c", "exit 42"])
        .assert()
        .code(42);
}

#[test]
fn test_timeout_triggers_exit_124() {
    /*
     * GNU spec: exit 124 when command times out (unless --preserve-status).
     * This is the canonical "timed out" indicator that scripts rely on.
     */
    let start = Instant::now();

    timeout_cmd()
        .args(["0.5s", "sleep", "10"])
        .assert()
        .code(124);

    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(400), "timed out too early");
    assert!(elapsed < Duration::from_secs(2), "took too long to timeout");
}

#[test]
fn test_zero_duration_disables_timeout() {
    /*
     * GNU behavior: duration of 0 means "no timeout" - run forever.
     * Useful for conditionally disabling timeout in scripts.
     */
    timeout_cmd()
        .args(["0", "echo", "no timeout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no timeout"));
}

/* =========================================================================
 * DURATION PARSING - All the formats GNU supports
 * ========================================================================= */

#[test]
fn test_duration_seconds() {
    timeout_cmd().args(["1s", "echo", "ok"]).assert().success();
}

#[test]
fn test_duration_seconds_implicit() {
    /* No suffix means seconds - GNU default */
    timeout_cmd().args(["1", "echo", "ok"]).assert().success();
}

#[test]
fn test_duration_minutes() {
    timeout_cmd().args(["1m", "echo", "ok"]).assert().success();
}

#[test]
fn test_duration_hours() {
    timeout_cmd().args(["1h", "echo", "ok"]).assert().success();
}

#[test]
fn test_duration_days() {
    timeout_cmd().args(["1d", "echo", "ok"]).assert().success();
}

#[test]
fn test_duration_fractional() {
    /*
     * GNU supports floating point durations. 0.3s = 300ms.
     * Critical for responsive short timeouts.
     */
    let start = Instant::now();

    timeout_cmd()
        .args(["0.3s", "sleep", "10"])
        .assert()
        .code(124);

    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(250));
    assert!(elapsed < Duration::from_secs(1));
}

#[test]
fn test_duration_fractional_no_suffix() {
    /* 0.5 = 0.5 seconds */
    let start = Instant::now();

    timeout_cmd()
        .args(["0.5", "sleep", "10"])
        .assert()
        .code(124);

    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(400));
    assert!(elapsed < Duration::from_secs(1));
}

#[test]
fn test_duration_case_insensitive() {
    /* GNU accepts uppercase suffixes */
    timeout_cmd().args(["1S", "echo", "ok"]).assert().success();
    timeout_cmd().args(["1M", "echo", "ok"]).assert().success();
}

#[test]
fn test_invalid_duration() {
    /* Exit 125 for timeout's own errors */
    timeout_cmd()
        .args(["abc", "echo", "test"])
        .assert()
        .code(125)
        .stderr(predicate::str::contains("invalid"));
}

#[test]
fn test_negative_duration() {
    timeout_cmd()
        .args(["-5", "echo", "test"])
        .assert()
        .failure();
}

#[test]
fn test_invalid_suffix() {
    /* We don't support milliseconds (ms) like some tools do */
    timeout_cmd()
        .args(["100ms", "echo", "test"])
        .assert()
        .code(125);
}

/* =========================================================================
 * SIGNAL HANDLING - Various ways to specify signals
 * ========================================================================= */

#[test]
fn test_signal_by_name() {
    timeout_cmd()
        .args(["-s", "TERM", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_signal_by_number() {
    /* Signal 15 = SIGTERM */
    timeout_cmd()
        .args(["-s", "15", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_signal_with_sig_prefix() {
    /* Accept SIGTERM as well as TERM */
    timeout_cmd()
        .args(["-s", "SIGTERM", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_signal_case_insensitive() {
    /* Be nice - accept lowercase */
    timeout_cmd()
        .args(["-s", "term", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_signal_kill() {
    /* SIGKILL (9) - the unkillable killer */
    timeout_cmd()
        .args(["-s", "KILL", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_signal_hup() {
    /* SIGHUP (1) - hangup */
    timeout_cmd()
        .args(["-s", "HUP", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_signal_int() {
    /* SIGINT (2) - interrupt (like Ctrl+C) */
    timeout_cmd()
        .args(["-s", "INT", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_invalid_signal() {
    timeout_cmd()
        .args(["-s", "INVALID", "5s", "echo", "test"])
        .assert()
        .code(125)
        .stderr(predicate::str::contains("invalid signal"));
}

#[test]
fn test_invalid_signal_number() {
    /* Signal 0 is invalid for killing */
    timeout_cmd()
        .args(["-s", "0", "5s", "echo", "test"])
        .assert()
        .code(125);
}

/* =========================================================================
 * --preserve-status - Return command's exit status even on timeout
 * ========================================================================= */

#[test]
fn test_preserve_status_on_timeout() {
    /*
     * With --preserve-status, exit with 128+signal instead of 124.
     * SIGTERM=15, so expect 143. SIGKILL=9, so 137.
     */
    timeout_cmd()
        .args(["--preserve-status", "0.3s", "sleep", "10"])
        .assert()
        .code(predicate::in_iter([128 + 15, 128 + 9]));
}

#[test]
fn test_preserve_status_on_normal_exit() {
    /* Normal completion: same behavior with or without --preserve-status */
    timeout_cmd()
        .args(["--preserve-status", "5s", "sh", "--", "-c", "exit 7"])
        .assert()
        .code(7);
}

#[test]
fn test_preserve_status_with_sigkill() {
    /*
     * If we send SIGKILL directly, --preserve-status should return 137 (128+9)
     */
    timeout_cmd()
        .args(["--preserve-status", "-s", "KILL", "0.3s", "sleep", "10"])
        .assert()
        .code(137);
}

#[test]
fn test_preserve_status_short_flag() {
    /* -p is the short form */
    timeout_cmd()
        .args(["-p", "0.3s", "sleep", "10"])
        .assert()
        .code(predicate::in_iter([128 + 15, 128 + 9]));
}

/* =========================================================================
 * --kill-after - Escalate to SIGKILL if process ignores first signal
 * ========================================================================= */

#[test]
fn test_kill_after_escalation() {
    /*
     * Process traps SIGTERM (ignores it). After --kill-after duration,
     * we send SIGKILL which cannot be ignored.
     */
    let start = Instant::now();

    timeout_cmd()
        .args([
            "-s",
            "TERM",
            "-k",
            "0.3s",
            "0.3s",
            "sh",
            "--",
            "-c",
            "trap '' TERM; sleep 10",
        ])
        .assert()
        .code(124);

    let elapsed = start.elapsed();
    /* Should take ~0.6s (0.3s timeout + 0.3s kill-after) */
    assert!(elapsed >= Duration::from_millis(500), "killed too early");
    assert!(elapsed < Duration::from_secs(3), "took too long");
}

#[test]
fn test_kill_after_not_needed() {
    /*
     * If process dies from first signal, kill-after never triggers.
     * Should complete quickly at the timeout, not timeout+kill-after.
     */
    let start = Instant::now();

    timeout_cmd()
        .args(["-k", "5s", "0.3s", "sleep", "10"])
        .assert()
        .code(124);

    let elapsed = start.elapsed();
    /* Should be ~0.3s, not 5.3s */
    assert!(elapsed < Duration::from_secs(1));
}

#[test]
fn test_kill_after_with_preserve_status() {
    /*
     * With both flags: process ignores TERM, gets KILL.
     * --preserve-status means exit 137 (128+9) not 124.
     */
    timeout_cmd()
        .args([
            "--preserve-status",
            "-k",
            "0.2s",
            "0.2s",
            "sh",
            "--",
            "-c",
            "trap '' TERM; sleep 10",
        ])
        .assert()
        .code(137);
}

/* =========================================================================
 * --verbose - Print diagnostics about signals sent
 * ========================================================================= */

#[test]
fn test_verbose_output() {
    timeout_cmd()
        .args(["--verbose", "0.2s", "sleep", "10"])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("sending signal"));
}

#[test]
fn test_verbose_shows_signal_name() {
    timeout_cmd()
        .args(["--verbose", "-s", "HUP", "0.2s", "sleep", "10"])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("SIGHUP"));
}

#[test]
fn test_verbose_with_kill_after() {
    /* Should see both SIGTERM and SIGKILL messages */
    timeout_cmd()
        .args([
            "--verbose",
            "-k",
            "0.2s",
            "0.2s",
            "sh",
            "--",
            "-c",
            "trap '' TERM; sleep 10",
        ])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("SIGTERM"))
        .stderr(predicate::str::contains("SIGKILL"));
}

#[test]
fn test_verbose_short_flag() {
    /* -v is the short form */
    timeout_cmd()
        .args(["-v", "0.2s", "sleep", "10"])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("sending signal"));
}

/* =========================================================================
 * --foreground - Run in same process group for TTY access
 * ========================================================================= */

#[test]
fn test_foreground_mode() {
    timeout_cmd()
        .args(["--foreground", "5s", "echo", "foreground"])
        .assert()
        .success()
        .stdout(predicate::str::contains("foreground"));
}

#[test]
fn test_foreground_timeout() {
    /* Timeout still works in foreground mode */
    timeout_cmd()
        .args(["--foreground", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_foreground_short_flag() {
    /* -f is the short form */
    timeout_cmd()
        .args(["-f", "5s", "echo", "ok"])
        .assert()
        .success();
}

/* =========================================================================
 * EXIT CODES - GNU coreutils compatibility
 * ========================================================================= */

#[test]
fn test_exit_124_on_timeout() {
    /* The canonical timeout exit code */
    timeout_cmd()
        .args(["0.1s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_exit_125_on_internal_error() {
    /* Timeout's own errors */
    timeout_cmd()
        .args(["invalid", "echo", "test"])
        .assert()
        .code(125);
}

#[test]
fn test_exit_126_permission_denied() {
    /* Command found but not executable */
    timeout_cmd()
        .args(["5s", "/dev/null"])
        .assert()
        .code(126)
        .stderr(
            predicate::str::contains("permission denied")
                .or(predicate::str::contains("Permission denied")),
        );
}

#[test]
fn test_exit_127_command_not_found() {
    /* Command doesn't exist */
    timeout_cmd()
        .args(["5s", "nonexistent_command_xyz_12345"])
        .assert()
        .code(127)
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_exit_137_sigkill() {
    /*
     * 137 = 128 + 9 (SIGKILL)
     * This happens with --preserve-status when killed by SIGKILL
     */
    timeout_cmd()
        .args(["--preserve-status", "-s", "KILL", "0.2s", "sleep", "10"])
        .assert()
        .code(137);
}

#[test]
fn test_exit_143_sigterm() {
    /*
     * 143 = 128 + 15 (SIGTERM)
     * With --preserve-status when killed by SIGTERM
     */
    timeout_cmd()
        .args(["--preserve-status", "-s", "TERM", "0.2s", "sleep", "10"])
        .assert()
        .code(predicate::in_iter([143, 137])); /* might escalate to KILL */
}

/* =========================================================================
 * COMMAND LINE PARSING - Edge cases
 * ========================================================================= */

#[test]
fn test_command_with_arguments() {
    timeout_cmd()
        .args(["5s", "echo", "arg1", "arg2", "arg3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("arg1 arg2 arg3"));
}

#[test]
fn test_command_with_dash_args() {
    /* Need -- separator for commands starting with - */
    timeout_cmd()
        .args(["5s", "--", "echo", "-n", "hello"])
        .assert()
        .success();
}

#[test]
fn test_combined_short_options() {
    /* Multiple short flags */
    timeout_cmd()
        .args(["-v", "-p", "-f", "5s", "echo", "ok"])
        .assert()
        .success();
}

#[test]
fn test_long_options_with_equals() {
    timeout_cmd()
        .args(["--signal=TERM", "--kill-after=5s", "5s", "echo", "ok"])
        .assert()
        .success();
}

#[test]
fn test_help() {
    timeout_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("DURATION"))
        .stdout(predicate::str::contains("COMMAND"))
        .stdout(predicate::str::contains("--signal"))
        .stdout(predicate::str::contains("--kill-after"));
}

#[test]
fn test_version() {
    timeout_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("timeout"));
}

/* =========================================================================
 * PROCESS GROUP HANDLING - Kill children too
 * ========================================================================= */

#[test]
fn test_kills_child_processes() {
    /*
     * When we timeout, child processes should also be killed.
     * This script spawns a background sleep - it should die with parent.
     */
    let start = Instant::now();

    timeout_cmd()
        .args(["0.3s", "sh", "--", "-c", "sleep 100 & wait"])
        .assert()
        .code(124);

    /* Should complete around 0.3s, not wait for the sleep 100 */
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn test_foreground_does_not_kill_children() {
    /*
     * In foreground mode, only the main process is killed.
     * This is a known limitation matching GNU behavior.
     * We just verify foreground mode works - can't easily test the
     * "children survive" part without more infrastructure.
     */
    timeout_cmd()
        .args(["--foreground", "0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

/* =========================================================================
 * TIMING PRECISION - Make sure we're accurate
 * ========================================================================= */

#[test]
fn test_timing_precision_100ms() {
    let start = Instant::now();

    timeout_cmd()
        .args(["0.1s", "sleep", "10"])
        .assert()
        .code(124);

    let elapsed = start.elapsed();
    /* Should be ~100ms. Upper bound relaxed for x86_64 emulation on CI (ARM runners)
     * where process spawn overhead can exceed 500ms. */
    assert!(
        elapsed >= Duration::from_millis(50),
        "too fast: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(1000),
        "too slow: {elapsed:?}"
    );
}

#[test]
fn test_timing_precision_500ms() {
    let start = Instant::now();

    timeout_cmd()
        .args(["0.5s", "sleep", "10"])
        .assert()
        .code(124);

    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(450),
        "too fast: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(700),
        "too slow: {elapsed:?}"
    );
}

#[test]
fn test_timing_precision_1s() {
    let start = Instant::now();

    timeout_cmd().args(["1s", "sleep", "10"]).assert().code(124);

    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(950),
        "too fast: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(1200),
        "too slow: {elapsed:?}"
    );
}

/* =========================================================================
 * PERFORMANCE - No unnecessary overhead
 * ========================================================================= */

#[test]
fn test_fast_command_no_delay() {
    /*
     * Running a fast command shouldn't add significant overhead.
     * echo should complete in <100ms even with a long timeout.
     */
    let start = Instant::now();

    timeout_cmd()
        .args(["60s", "echo", "fast"])
        .assert()
        .success();

    assert!(
        start.elapsed() < Duration::from_millis(500),
        "simple echo took too long"
    );
}

#[test]
fn test_overhead_multiple_runs() {
    /*
     * Run several fast commands to check for consistent performance.
     * Each should complete quickly - no cumulative slowdown.
     */
    for i in 0..5 {
        let start = Instant::now();

        timeout_cmd()
            .args(["10s", "echo", &format!("run {i}")])
            .assert()
            .success();

        assert!(
            start.elapsed() < Duration::from_millis(200),
            "run {i} was slow"
        );
    }
}

/* =========================================================================
 * STDOUT/STDERR HANDLING - Pass through correctly
 * ========================================================================= */

#[test]
fn test_stdout_passthrough() {
    timeout_cmd()
        .args(["5s", "echo", "to stdout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("to stdout"));
}

#[test]
fn test_stderr_passthrough() {
    timeout_cmd()
        .args(["5s", "sh", "--", "-c", "echo to stderr >&2"])
        .assert()
        .success()
        .stderr(predicate::str::contains("to stderr"));
}

#[test]
fn test_both_streams() {
    timeout_cmd()
        .args(["5s", "sh", "--", "-c", "echo out; echo err >&2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("out"))
        .stderr(predicate::str::contains("err"));
}

/* =========================================================================
 * REAL-WORLD SCENARIOS - Common use cases
 * ========================================================================= */

#[test]
fn test_curl_simulation() {
    /*
     * Common pattern: timeout a network request.
     * We simulate with sleep since we can't rely on network.
     */
    timeout_cmd()
        .args(["0.3s", "sleep", "10"])
        .assert()
        .code(124);
}

#[test]
fn test_script_with_exit_code() {
    /* Run a script that exits with specific code */
    timeout_cmd()
        .args(["5s", "sh", "--", "-c", "exit 0"])
        .assert()
        .code(0);

    timeout_cmd()
        .args(["5s", "sh", "--", "-c", "exit 1"])
        .assert()
        .code(1);

    timeout_cmd()
        .args(["5s", "sh", "--", "-c", "exit 255"])
        .assert()
        .code(255);
}

#[test]
fn test_pipeline_exit_code() {
    /*
     * When running a pipeline, we get the exit code of the last command.
     * This is handled by the shell, but we should pass it through.
     */
    timeout_cmd()
        .args(["5s", "sh", "--", "-c", "true | false"])
        .assert()
        .code(1);

    timeout_cmd()
        .args(["5s", "sh", "--", "-c", "false | true"])
        .assert()
        .code(0);
}

#[test]
fn test_command_with_spaces_in_args() {
    timeout_cmd()
        .args(["5s", "echo", "hello world", "foo bar"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello world"))
        .stdout(predicate::str::contains("foo bar"));
}

#[test]
fn test_empty_output_command() {
    /* true produces no output */
    timeout_cmd()
        .args(["5s", "true"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

/* =========================================================================
 * STRESS TESTS - Edge conditions
 * ========================================================================= */

#[test]
fn test_very_short_timeout() {
    /* 10ms timeout - tests short timeout precision */
    let start = Instant::now();

    timeout_cmd()
        .args(["0.01s", "sleep", "10"])
        .assert()
        .code(124);

    /* Should be quick, but allow some slack for process startup */
    assert!(start.elapsed() < Duration::from_millis(500));
}

/* =========================================================================
 * RACE CONDITION TESTS - Verify fixes for timing-sensitive bugs
 * ========================================================================= */

#[test]
fn test_race_very_short_timeouts() {
    /*
     * Stress test: many very short timeouts in succession.
     * This exercises the race between spawn() and kevent() registration
     * where the process might exit before we can register EVFILT_PROC.
     * Bug fixed: ESRCH from kevent now properly falls back to wait().
     */
    for i in 0..50 {
        timeout_cmd()
            .args(["0.001s", "sleep", "10"])
            .assert()
            .code(124);

        /* Also test with varying short durations */
        let duration = format!("0.00{}s", (i % 9) + 1);
        timeout_cmd()
            .args([&duration, "sleep", "10"])
            .assert()
            .code(124);
    }
}

#[test]
fn test_race_fast_exiting_commands() {
    /*
     * Stress test: commands that exit faster than the timeout.
     * This exercises the race where process exits between spawn()
     * and kevent(), causing ESRCH errors. The fix ensures we properly
     * reap the process with blocking wait() when try_wait() returns None.
     */
    for _ in 0..100 {
        timeout_cmd().args(["10s", "true"]).assert().success();
    }
}

#[test]
fn test_race_command_exits_immediately() {
    /*
     * Commands that exit with various codes immediately.
     * Tests that fast process termination doesn't cause spurious errors.
     */
    for code in [0, 1, 42, 127, 255] {
        for _ in 0..20 {
            timeout_cmd()
                .args(["10s", "sh", "--", "-c", &format!("exit {code}")])
                .assert()
                .code(code);
        }
    }
}

#[test]
fn test_process_group_signal_fallback() {
    /*
     * Test that signaling works correctly even when process groups
     * might not be fully established. The fix makes send_signal()
     * fall back to kill(pid) when killpg() fails with ESRCH.
     *
     * We can't directly test setpgid failure, but we verify the
     * normal path works reliably under stress.
     */
    for _ in 0..30 {
        let start = Instant::now();

        timeout_cmd()
            .args(["0.05s", "sh", "--", "-c", "sleep 100 & wait"])
            .assert()
            .code(124);

        /* Should complete around 50ms, not wait for child processes */
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "process group signal may have failed"
        );
    }
}

#[test]
fn test_foreground_signal_no_process_group() {
    /*
     * In foreground mode, we use kill() not killpg().
     * Verify this works correctly under stress.
     */
    for _ in 0..30 {
        timeout_cmd()
            .args(["--foreground", "0.05s", "sleep", "10"])
            .assert()
            .code(124);
    }
}

#[test]
fn test_rapid_succession() {
    /* Quick timeouts in rapid succession */
    for _ in 0..3 {
        timeout_cmd()
            .args(["0.1s", "sleep", "10"])
            .assert()
            .code(124);
    }
}

#[test]
fn test_long_argument_list() {
    /* Pass many arguments to the command */
    let args: Vec<String> = (0..100).map(|i| format!("arg{i}")).collect();
    let mut cmd_args = vec!["5s".to_string(), "echo".to_string()];
    cmd_args.extend(args);

    let cmd_args_str: Vec<&str> = cmd_args.iter().map(String::as_str).collect();

    timeout_cmd()
        .args(&cmd_args_str)
        .assert()
        .success()
        .stdout(predicate::str::contains("arg0"))
        .stdout(predicate::str::contains("arg99"));
}

/* =========================================================================
 * SIGNAL FORWARDING - When timeout receives signals
 * ========================================================================= */

#[test]
fn test_signal_forwarding_sigterm() {
    /*
     * When timeout itself receives SIGTERM, it should forward the signal
     * to the child process and exit. This prevents orphaned processes
     * during system shutdown or user cancellation.
     *
     * We test this by spawning timeout in the background, then sending
     * it SIGTERM and verifying the child also terminates quickly.
     */
    use std::process::{Command, Stdio};
    use std::thread;

    /* Start timeout with a long-running command */
    let mut timeout_process = Command::new(env!("CARGO_BIN_EXE_timeout"))
        .args(["60s", "sleep", "60"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn timeout");

    /* Give it time to start and set up signal handlers */
    thread::sleep(Duration::from_millis(200));

    let start = Instant::now();

    /* Send SIGTERM to the timeout process */
    // SAFETY: kill() is safe with any valid pid/signal combo
    unsafe {
        libc::kill(timeout_process.id() as i32, libc::SIGTERM);
    }

    /* Wait for timeout to exit (should be quick since we sent it SIGTERM) */
    let status = timeout_process.wait().expect("Failed to wait for timeout");

    /*
     * The key assertion: timeout should exit quickly (not wait 60s).
     * The exact exit code depends on signal handling, but it should
     * not be 124 (timeout) since we killed it early.
     */
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "timeout should exit quickly after SIGTERM, took {:?}",
        elapsed
    );

    /* Verify it didn't exit with 124 (normal timeout) */
    assert_ne!(
        status.code(),
        Some(124),
        "Should not exit with timeout code 124 since we killed it"
    );
}

#[test]
fn test_signal_forwarding_sigint() {
    /*
     * Similar test for SIGINT (Ctrl+C). Verify the process exits quickly.
     */
    use std::process::{Command, Stdio};
    use std::thread;

    let mut timeout_process = Command::new(env!("CARGO_BIN_EXE_timeout"))
        .args(["60s", "sleep", "60"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn timeout");

    thread::sleep(Duration::from_millis(200));

    let start = Instant::now();

    // SAFETY: kill() is safe with any valid pid/signal combo
    unsafe {
        libc::kill(timeout_process.id() as i32, libc::SIGINT);
    }

    let status = timeout_process.wait().expect("Failed to wait for timeout");

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "timeout should exit quickly after SIGINT, took {:?}",
        elapsed
    );

    assert_ne!(
        status.code(),
        Some(124),
        "Should not exit with timeout code 124 since we killed it"
    );
}

/* =========================================================================
 * JSON OUTPUT - Machine-readable results for CI
 * ========================================================================= */

#[test]
fn test_json_output_completed() {
    /*
     * --json flag outputs machine-readable JSON.
     * On successful completion: status, exit_code, elapsed_ms
     */
    timeout_cmd()
        .args(["--json", "5s", "true"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""status":"completed""#))
        .stdout(predicate::str::contains(r#""exit_code":0"#))
        .stdout(predicate::str::contains(r#""elapsed_ms":"#));
}

#[test]
fn test_json_output_timeout() {
    /*
     * On timeout: status, signal, signal_num, killed, exit_code, elapsed_ms
     */
    timeout_cmd()
        .args(["--json", "0.1s", "sleep", "10"])
        .assert()
        .code(124)
        .stdout(predicate::str::contains(r#""status":"timeout""#))
        .stdout(predicate::str::contains(r#""signal":"SIGTERM""#))
        .stdout(predicate::str::contains(r#""signal_num":15"#))
        .stdout(predicate::str::contains(r#""exit_code":124"#));
}

#[test]
fn test_json_output_with_kill_after() {
    /*
     * When process ignores SIGTERM and gets SIGKILL, killed should be true
     */
    timeout_cmd()
        .args([
            "--json",
            "-k",
            "0.1s",
            "0.1s",
            "sh",
            "--",
            "-c",
            "trap '' TERM; sleep 10",
        ])
        .assert()
        .code(124)
        .stdout(predicate::str::contains(r#""killed":true"#));
}

#[test]
fn test_json_output_error() {
    /*
     * On error (command not found): status error with message
     */
    timeout_cmd()
        .args(["--json", "5s", "nonexistent_command_xyz_12345"])
        .assert()
        .code(127)
        .stdout(predicate::str::contains(r#""status":"error""#))
        .stdout(predicate::str::contains(r#""exit_code":127"#));
}

#[test]
fn test_json_output_exit_code() {
    /*
     * Command exits with specific code, JSON should show it
     */
    timeout_cmd()
        .args(["--json", "5s", "sh", "--", "-c", "exit 42"])
        .assert()
        .code(42)
        .stdout(predicate::str::contains(r#""exit_code":42"#));
}

#[test]
fn test_json_valid_format() {
    /*
     * Output should contain valid JSON (proper structure)
     * Note: command stdout comes first, JSON on its own line
     */
    let output = timeout_cmd()
        .args(["--json", "5s", "true"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_str = String::from_utf8(output).expect("valid utf8");
    /* Find the JSON line (starts with {) */
    let json_line = json_str
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("should have JSON line");
    assert!(json_line.ends_with('}'), "JSON should end with }}");
    assert!(
        json_line.contains(r#""status":"#),
        "should have status field"
    );
}

/* =========================================================================
 * NEW FEATURES - quiet, timeout-exit-code, on-timeout, env vars
 * ========================================================================= */

#[test]
fn test_quiet_suppresses_errors() {
    /*
     * --quiet/-q should suppress error messages to stderr
     */
    timeout_cmd()
        .args(["-q", "5s", "nonexistent_command_xyz_12345"])
        .assert()
        .code(127)
        .stderr(predicate::str::is_empty());
}

#[test]
fn test_quiet_does_not_suppress_json() {
    /*
     * --quiet should NOT suppress JSON output
     */
    timeout_cmd()
        .args(["--quiet", "--json", "5s", "nonexistent_command_xyz_12345"])
        .assert()
        .code(127)
        .stdout(predicate::str::contains(r#""status":"error""#));
}

#[test]
fn test_quiet_short_flag() {
    /* -q is short for --quiet */
    timeout_cmd()
        .args(["-q", "5s", "nonexistent_command_xyz_12345"])
        .assert()
        .code(127)
        .stderr(predicate::str::is_empty());
}

#[test]
fn test_timeout_exit_code_custom() {
    /*
     * --timeout-exit-code changes the exit code on timeout
     */
    timeout_cmd()
        .args(["--timeout-exit-code", "42", "0.1s", "sleep", "10"])
        .assert()
        .code(42);
}

#[test]
fn test_timeout_exit_code_not_used_on_normal_exit() {
    /*
     * --timeout-exit-code should only affect timeout, not normal completion
     */
    timeout_cmd()
        .args([
            "--timeout-exit-code",
            "42",
            "5s",
            "sh",
            "--",
            "-c",
            "exit 7",
        ])
        .assert()
        .code(7);
}

#[test]
fn test_timeout_exit_code_with_json() {
    /*
     * JSON output should reflect the custom exit code
     */
    timeout_cmd()
        .args(["--timeout-exit-code", "99", "--json", "0.1s", "sleep", "10"])
        .assert()
        .code(99)
        .stdout(predicate::str::contains(r#""exit_code":99"#));
}

#[test]
fn test_env_timeout() {
    /*
     * TIMEOUT env var sets default duration
     */
    timeout_cmd()
        .env("TIMEOUT", "5s")
        .args(["echo", "from env"])
        .assert()
        .success()
        .stdout(predicate::str::contains("from env"));
}

#[test]
fn test_env_timeout_signal() {
    /*
     * TIMEOUT_SIGNAL env var sets default signal
     */
    timeout_cmd()
        .env("TIMEOUT_SIGNAL", "HUP")
        .args(["-v", "0.1s", "sleep", "10"])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("SIGHUP"));
}

#[test]
fn test_env_timeout_signal_overridden() {
    /*
     * -s flag should override TIMEOUT_SIGNAL
     */
    timeout_cmd()
        .env("TIMEOUT_SIGNAL", "HUP")
        .args(["-v", "-s", "INT", "0.1s", "sleep", "10"])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("SIGINT"));
}

#[test]
fn test_env_timeout_kill_after() {
    /*
     * TIMEOUT_KILL_AFTER env var sets default kill-after
     */
    timeout_cmd()
        .env("TIMEOUT_KILL_AFTER", "0.1s")
        .args(["-v", "0.1s", "sh", "--", "-c", "trap '' TERM; sleep 10"])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("SIGKILL"));
}

#[test]
fn test_on_timeout_runs_hook() {
    /*
     * --on-timeout should run a command when timeout occurs
     */
    let tmp_file = "/tmp/timeout_hook_test";
    std::fs::remove_file(tmp_file).ok();

    timeout_cmd()
        .args([
            "--on-timeout",
            &format!("touch {}", tmp_file),
            "0.1s",
            "sleep",
            "10",
        ])
        .assert()
        .code(124);

    assert!(
        std::path::Path::new(tmp_file).exists(),
        "hook should have created file"
    );
    std::fs::remove_file(tmp_file).ok();
}

#[test]
fn test_on_timeout_not_run_on_success() {
    /*
     * --on-timeout should NOT run when command completes before timeout
     */
    let tmp_file = "/tmp/timeout_hook_no_run_test";
    std::fs::remove_file(tmp_file).ok();

    timeout_cmd()
        .args(["--on-timeout", &format!("touch {}", tmp_file), "5s", "true"])
        .assert()
        .success();

    assert!(
        !std::path::Path::new(tmp_file).exists(),
        "hook should NOT have run"
    );
}

#[test]
fn test_on_timeout_limit() {
    /*
     * --on-timeout-limit should limit how long the hook can run
     */
    let start = Instant::now();

    timeout_cmd()
        .args([
            "--on-timeout",
            "sleep 10",
            "--on-timeout-limit",
            "0.2s",
            "0.1s",
            "sleep",
            "10",
        ])
        .assert()
        .code(124);

    /* Should not take 10s (hook limit should kick in) */
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "hook should have been limited"
    );
}

#[test]
fn test_on_timeout_verbose() {
    /*
     * --on-timeout with -v should show hook being run
     */
    timeout_cmd()
        .args(["-v", "--on-timeout", "true", "0.1s", "sleep", "10"])
        .assert()
        .code(124)
        .stderr(predicate::str::contains("on-timeout hook"));
}

/* =========================================================================
 * BUG FIXES - Tests for issues found in code audit
 * ========================================================================= */

#[test]
fn test_on_timeout_pid_substitution() {
    /*
     * Verify %p is correctly replaced with the actual PID
     */
    let tmp_file = "/tmp/timeout_pid_test";
    std::fs::remove_file(tmp_file).ok();

    timeout_cmd()
        .args([
            "--on-timeout",
            &format!("echo %p > {}", tmp_file),
            "0.1s",
            "sleep",
            "10",
        ])
        .assert()
        .code(124);

    /* Verify file was created with a valid PID */
    let content = std::fs::read_to_string(tmp_file).expect("hook should have created file");
    let pid: i32 = content.trim().parse().expect("should be a valid integer");
    assert!(pid > 0, "PID should be positive");
    std::fs::remove_file(tmp_file).ok();
}

#[test]
fn test_on_timeout_percent_escape() {
    /*
     * Verify %% is converted to literal %
     */
    let tmp_file = "/tmp/timeout_percent_test";
    std::fs::remove_file(tmp_file).ok();

    timeout_cmd()
        .args([
            "--on-timeout",
            &format!("echo '100%%' > {}", tmp_file),
            "0.1s",
            "sleep",
            "10",
        ])
        .assert()
        .code(124);

    let content = std::fs::read_to_string(tmp_file).expect("hook should have created file");
    assert!(content.contains("100%"), "should contain literal %");
    std::fs::remove_file(tmp_file).ok();
}

#[test]
fn test_quiet_passes_through_command_stderr() {
    /*
     * --quiet should suppress timeout's messages but NOT the command's stderr
     */
    timeout_cmd()
        .args(["-q", "5s", "sh", "-c", "echo error >&2"])
        .assert()
        .success()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn test_quiet_verbose_conflict() {
    /*
     * -q and -v should be mutually exclusive
     */
    timeout_cmd()
        .args(["-q", "-v", "5s", "true"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn test_json_schema_version() {
    /*
     * All JSON output should include schema_version field
     */
    /* Test completed */
    timeout_cmd()
        .args(["--json", "5s", "true"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""schema_version":2"#));

    /* Test timeout */
    timeout_cmd()
        .args(["--json", "0.1s", "sleep", "10"])
        .assert()
        .code(124)
        .stdout(predicate::str::contains(r#""schema_version":2"#));

    /* Test error */
    timeout_cmd()
        .args(["--json", "5s", "nonexistent_command_xyz_12345"])
        .assert()
        .code(127)
        .stdout(predicate::str::contains(r#""schema_version":2"#));
}

#[test]
fn test_json_hook_fields() {
    /*
     * JSON output should include hook_* fields when hook is run
     */
    timeout_cmd()
        .args(["--json", "--on-timeout", "true", "0.1s", "sleep", "10"])
        .assert()
        .code(124)
        .stdout(predicate::str::contains(r#""hook_ran":true"#))
        .stdout(predicate::str::contains(r#""hook_timed_out":false"#));
}

#[test]
fn test_json_hook_fields_with_timeout() {
    /*
     * JSON should show hook_timed_out:true when hook exceeds limit
     */
    timeout_cmd()
        .args([
            "--json",
            "--on-timeout",
            "sleep 10",
            "--on-timeout-limit",
            "0.1s",
            "0.1s",
            "sleep",
            "10",
        ])
        .assert()
        .code(124)
        .stdout(predicate::str::contains(r#""hook_ran":true"#))
        .stdout(predicate::str::contains(r#""hook_timed_out":true"#));
}

#[test]
fn test_timeout_exit_code_warning() {
    /*
     * Using a reserved exit code (125-137) should print a warning only when timeout occurs
     */
    /* No warning when command completes before timeout */
    timeout_cmd()
        .args(["--timeout-exit-code", "127", "5s", "true"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    /* Warning should appear when timeout actually occurs */
    timeout_cmd()
        .args(["--timeout-exit-code", "127", "0.1s", "sleep", "10"])
        .assert()
        .code(127)
        .stderr(predicate::str::contains("warning"))
        .stderr(predicate::str::contains("reserved"));
}

#[test]
fn test_on_timeout_limit_warning() {
    /*
     * Warning when --on-timeout-limit exceeds main timeout
     */
    timeout_cmd()
        .args([
            "--on-timeout",
            "true",
            "--on-timeout-limit",
            "60s",
            "1s",
            "true",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("warning"))
        .stderr(predicate::str::contains("exceeds"));
}

/* =========================================================================
 * CONFINE MODE - Time measurement behavior
 * ========================================================================= */

#[test]
fn test_confine_wall_mode_accepted() {
    /*
     * -c wall should be accepted and use wall-clock timing (default behavior).
     * Uses mach_continuous_time which counts through system sleep.
     */
    timeout_cmd()
        .args(["-c", "wall", "5s", "echo", "hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));
}

#[test]
fn test_confine_active_mode_accepted() {
    /*
     * -c active should be accepted and use active-time-only timing.
     * Uses CLOCK_MONOTONIC_RAW which pauses during system sleep.
     * ~28% faster internally due to no timebase conversion.
     */
    timeout_cmd()
        .args(["-c", "active", "5s", "echo", "hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));
}

#[test]
fn test_confine_long_form_accepted() {
    /*
     * --confine=wall and --confine=active should work
     */
    timeout_cmd()
        .args(["--confine=wall", "5s", "echo", "hello"])
        .assert()
        .success();

    timeout_cmd()
        .args(["--confine=active", "5s", "echo", "hello"])
        .assert()
        .success();
}

#[test]
fn test_confine_invalid_mode_rejected() {
    /*
     * Invalid confine mode should produce an error
     */
    timeout_cmd()
        .args(["-c", "invalid", "5s", "echo", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("confine"));
}

#[test]
fn test_confine_active_timeout_works() {
    /*
     * Active mode should still properly timeout commands.
     * This verifies the CLOCK_MONOTONIC_RAW timing path.
     */
    let start = std::time::Instant::now();

    timeout_cmd()
        .args(["-c", "active", "0.5s", "sleep", "10"])
        .assert()
        .code(124);

    let elapsed = start.elapsed();
    assert!(
        elapsed >= std::time::Duration::from_millis(400),
        "timed out too early"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "took too long to timeout"
    );
}

#[test]
fn test_signal_forwarding_reports_correct_signal() {
    /*
     * When we forward SIGINT, verbose output should say SIGINT not SIGTERM
     */
    use std::process::{Command, Stdio};
    use std::thread;

    let timeout_process = Command::new(env!("CARGO_BIN_EXE_timeout"))
        .args(["-v", "30s", "sleep", "100"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start timeout");

    let pid = timeout_process.id() as i32;
    thread::sleep(Duration::from_millis(200));

    /* Send SIGINT */
    // SAFETY: kill() is safe with any valid pid/signal combo
    unsafe {
        libc::kill(pid, libc::SIGINT);
    }

    let output = timeout_process.wait_with_output().expect("Failed to wait");
    let stderr = String::from_utf8_lossy(&output.stderr);

    /* Should report SIGINT, not SIGTERM */
    assert!(
        stderr.contains("SIGINT"),
        "verbose output should report SIGINT, got: {}",
        stderr
    );
}
