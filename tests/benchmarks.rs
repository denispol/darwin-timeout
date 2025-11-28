/*
 * Performance benchmarks for the timeout command.
 *
 * These tests ensure we don't regress catastrophically on performance.
 * They use wide tolerances to avoid flaky failures on CI systems under
 * load. For precise measurements, use scripts/benchmark.sh on an idle
 * system with the release binary.
 *
 * NOTE: The assert_cmd framework adds overhead (~100-300ms) for locating
 * and invoking the binary. Real-world performance is significantly better.
 *
 * Run with: cargo test --release --test benchmarks
 * The --release flag is important for realistic numbers.
 */

#![allow(
    clippy::uninlined_format_args,
    clippy::cast_possible_truncation,
    clippy::redundant_closure_for_method_calls
)]

use assert_cmd::Command;
use std::time::{Duration, Instant};

#[allow(deprecated)]
fn timeout_cmd() -> Command {
    Command::cargo_bin("timeout").unwrap()
}

/* =========================================================================
 * STARTUP OVERHEAD - How long does timeout itself take to start?
 * ========================================================================= */

#[test]
fn bench_startup_overhead() {
    /*
     * Run 'true' (does nothing, exits immediately) through timeout.
     * This measures our startup + teardown overhead.
     * Target: <500ms per invocation (generous for CI systems).
     */
    let iterations = 10;
    let start = Instant::now();

    for _ in 0..iterations {
        timeout_cmd().args(["60s", "true"]).assert().success();
    }

    let total = start.elapsed();
    let per_run = total / iterations;

    println!(
        "Startup overhead: {:?} per invocation ({} runs)",
        per_run, iterations
    );

    assert!(
        per_run < Duration::from_millis(500),
        "startup overhead too high: {:?}",
        per_run
    );
}

#[test]
fn bench_echo_throughput() {
    /*
     * Echo a string through timeout. This is a common use case.
     * We should add minimal latency to what echo itself takes.
     */
    let iterations = 10;
    let start = Instant::now();

    for i in 0..iterations {
        timeout_cmd()
            .args(["60s", "echo", &format!("message {}", i)])
            .assert()
            .success();
    }

    let total = start.elapsed();
    let per_run = total / iterations;

    println!(
        "Echo throughput: {:?} per invocation ({} runs)",
        per_run, iterations
    );

    assert!(
        per_run < Duration::from_millis(500),
        "echo too slow: {:?}",
        per_run
    );
}

/* =========================================================================
 * TIMEOUT PRECISION - How accurately do we hit the timeout?
 * ========================================================================= */

#[test]
fn bench_timeout_precision_100ms() {
    /*
     * Test 100ms timeout precision. kqueue provides good precision,
     * but framework overhead and CI variability can add significant time.
     */
    let target = Duration::from_millis(100);
    let max_allowed = Duration::from_millis(600); /* generous for CI */

    let start = Instant::now();
    timeout_cmd()
        .args(["0.1s", "sleep", "60"])
        .assert()
        .code(124);
    let elapsed = start.elapsed();

    println!("100ms timeout: actual {:?}, target {:?}", elapsed, target);

    assert!(
        elapsed < max_allowed,
        "100ms timeout too slow: {:?} (max {:?})",
        elapsed,
        max_allowed
    );

    /* Should be at least the timeout duration */
    assert!(elapsed >= target, "100ms timeout too fast: {:?}", elapsed);
}

#[test]
fn bench_timeout_precision_500ms() {
    let target = Duration::from_millis(500);
    let max_allowed = Duration::from_millis(1000);

    let start = Instant::now();
    timeout_cmd()
        .args(["0.5s", "sleep", "60"])
        .assert()
        .code(124);
    let elapsed = start.elapsed();

    println!("500ms timeout: actual {:?}, target {:?}", elapsed, target);

    assert!(
        elapsed < max_allowed,
        "500ms timeout too slow: {:?}",
        elapsed
    );
    assert!(elapsed >= target, "500ms timeout too fast: {:?}", elapsed);
}

#[test]
fn bench_timeout_precision_1s() {
    let target = Duration::from_secs(1);
    let max_allowed = Duration::from_millis(1500);

    let start = Instant::now();
    timeout_cmd().args(["1s", "sleep", "60"]).assert().code(124);
    let elapsed = start.elapsed();

    println!("1s timeout: actual {:?}, target {:?}", elapsed, target);

    assert!(elapsed < max_allowed, "1s timeout too slow: {:?}", elapsed);
    assert!(elapsed >= target, "1s timeout too fast: {:?}", elapsed);
}

/* =========================================================================
 * RAPID INVOCATIONS - Stress test for resource leaks
 * ========================================================================= */

#[test]
fn bench_rapid_invocations() {
    /*
     * Run many short timeouts rapidly to check for:
     * - Resource leaks (file descriptors, memory)
     * - Cumulative slowdown
     * - Process cleanup issues
     */
    let iterations = 20;
    let mut times = Vec::with_capacity(iterations);

    for i in 0..iterations {
        let start = Instant::now();
        timeout_cmd()
            .args(["0.05s", "sleep", "10"])
            .assert()
            .code(124);
        times.push(start.elapsed());

        /* Sanity check - no single iteration should be extremely slow */
        assert!(
            times[i] < Duration::from_millis(1000),
            "iteration {} too slow: {:?}",
            i,
            times[i]
        );
    }

    /* Check that later iterations aren't significantly slower (would indicate leak) */
    let first_half_avg: Duration =
        times[..iterations / 2].iter().sum::<Duration>() / (iterations / 2) as u32;
    let second_half_avg: Duration =
        times[iterations / 2..].iter().sum::<Duration>() / (iterations / 2) as u32;

    println!(
        "Rapid invocations: first half avg {:?}, second half avg {:?}",
        first_half_avg, second_half_avg
    );

    /* Second half shouldn't be more than 3x slower */
    assert!(
        second_half_avg < first_half_avg * 3,
        "performance degraded over time: first {:?}, second {:?}",
        first_half_avg,
        second_half_avg
    );
}

/* =========================================================================
 * KILL-AFTER TIMING - Verify escalation timing
 * ========================================================================= */

#[test]
fn bench_kill_after_timing() {
    /*
     * Test that --kill-after triggers at the right time.
     * With 200ms timeout + 200ms kill-after, should complete around 400ms.
     * Add generous framework overhead tolerance.
     */
    let target = Duration::from_millis(400);
    let max_allowed = Duration::from_millis(1000);

    let start = Instant::now();
    timeout_cmd()
        .args([
            "-k",
            "0.2s",
            "0.2s",
            "sh",
            "--",
            "-c",
            "trap '' TERM; sleep 60",
        ])
        .assert()
        .code(124);
    let elapsed = start.elapsed();

    println!(
        "kill-after timing: actual {:?}, target {:?}",
        elapsed, target
    );

    assert!(
        elapsed < max_allowed,
        "kill-after timing too slow: {:?}",
        elapsed
    );
    assert!(
        elapsed >= target,
        "kill-after timing too fast: {:?}",
        elapsed
    );
}

/* =========================================================================
 * CPU USAGE - kqueue should mean minimal CPU usage while waiting
 * ========================================================================= */

#[test]
fn bench_long_timeout_cpu() {
    /*
     * Run a 2-second timeout. kqueue means zero CPU while waiting -
     * the kernel wakes us only when the timer fires or process exits.
     */
    let target = Duration::from_secs(2);
    let max_allowed = Duration::from_millis(2800);

    let start = Instant::now();
    timeout_cmd().args(["2s", "sleep", "60"]).assert().code(124);
    let elapsed = start.elapsed();

    println!("2s timeout: actual {:?}, target {:?}", elapsed, target);

    assert!(elapsed < max_allowed, "2s timeout too slow: {:?}", elapsed);
    assert!(elapsed >= target, "2s timeout too fast: {:?}", elapsed);
}

/* =========================================================================
 * ARGUMENT PASSING OVERHEAD
 * ========================================================================= */

#[test]
fn bench_many_arguments() {
    /*
     * Pass many arguments to see if argument handling is efficient.
     */
    let mut args = vec!["60s", "echo"];
    let extra_args: Vec<String> = (0..100).map(|i| format!("arg{}", i)).collect();
    let extra_refs: Vec<&str> = extra_args.iter().map(|s| s.as_str()).collect();
    args.extend(extra_refs);

    let start = Instant::now();
    timeout_cmd().args(&args).assert().success();
    let elapsed = start.elapsed();

    println!("100 args: {:?}", elapsed);

    /* Allow generous overhead for CI */
    assert!(
        elapsed < Duration::from_millis(800),
        "many arguments too slow: {:?}",
        elapsed
    );
}

/* =========================================================================
 * SIGNAL DELIVERY SPEED
 * ========================================================================= */

#[test]
fn bench_signal_delivery() {
    /*
     * After timeout expires, signal should be sent immediately.
     * The process (sleep) should die quickly after receiving SIGTERM.
     */
    let timeout_duration = Duration::from_millis(100);
    let max_total = Duration::from_millis(600); /* timeout + signal + cleanup + overhead */

    let start = Instant::now();
    timeout_cmd()
        .args(["0.1s", "sleep", "60"])
        .assert()
        .code(124);
    let elapsed = start.elapsed();

    println!(
        "Signal delivery: {:?} total (timeout was {:?})",
        elapsed, timeout_duration
    );

    assert!(
        elapsed < max_total,
        "signal delivery slow: {:?} (expected < {:?})",
        elapsed,
        max_total
    );
}

/* =========================================================================
 * COMPARISON WITH NATIVE COMMAND (baseline)
 * ========================================================================= */

#[test]
fn bench_baseline_echo() {
    /*
     * Baseline: run echo directly without timeout.
     * This tells us what overhead timeout adds.
     */
    use std::process::Command as StdCommand;

    let iterations = 10;

    /* Native echo */
    let native_start = Instant::now();
    for _ in 0..iterations {
        StdCommand::new("echo")
            .arg("hello")
            .output()
            .expect("failed to run echo");
    }
    let native_time = native_start.elapsed() / iterations;

    /* Through timeout */
    let timeout_start = Instant::now();
    for _ in 0..iterations {
        timeout_cmd()
            .args(["60s", "echo", "hello"])
            .assert()
            .success();
    }
    let timeout_time = timeout_start.elapsed() / iterations;

    let overhead = timeout_time.saturating_sub(native_time);

    println!(
        "Native echo: {:?}, Through timeout: {:?}, Overhead: {:?}",
        native_time, timeout_time, overhead
    );

    /* Overhead should be reasonable (generous for CI systems) */
    assert!(
        overhead < Duration::from_millis(400),
        "timeout overhead too high: {:?}",
        overhead
    );
}
