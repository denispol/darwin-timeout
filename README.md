# procguard

The formally verified process supervisor for macOS.

**CLI tool** — process timeouts, resource limits, lifecycle control.  
**Rust library** — embed supervision logic in your own tools.

    brew install denispol/tap/procguard          # CLI
    cargo add procguard                           # library

Works exactly like GNU timeout (it's a drop-in replacement):

    procguard 30s ./slow-command           # kill after 30 seconds
    procguard -k 5 1m ./stubborn           # SIGTERM, then SIGKILL after 5s
    procguard --preserve-status 10s ./cmd  # exit with command's status

Plus features GNU doesn't have:

    procguard --json 5m ./test-suite       # JSON output for CI
    procguard --on-timeout 'cleanup.sh' 30s ./task  # pre-timeout hook
    procguard --retry 3 30s ./flaky-test   # retry on timeout
    procguard --mem-limit 1G 1h ./build    # kill if memory exceeds 1GB
    procguard --cpu-percent 50 1h ./batch  # throttle to 50% CPU

## GNU Compatibility

**Dual binary:** `procguard` is the primary binary. A `timeout` alias is also provided for scripts expecting GNU timeout.

| Binary      | Default Behavior                  | Use Case                  |
| ----------- | --------------------------------- | ------------------------- |
| `procguard` | Wall clock (survives sleep)       | macOS-native, sleep-aware |
| `timeout`   | Active time (pauses during sleep) | GNU-compatible scripts    |

```bash
# These behave identically to GNU timeout:
timeout 30s ./command
timeout -k 5 1m ./stubborn

# procguard defaults to wall-clock (unique to macOS):
procguard 30s ./command              # survives system sleep
procguard -c active 30s ./command    # GNU-like behavior
```

## Why procguard?

Apple doesn't ship `timeout`. The alternatives have problems:

**GNU coreutils** (`brew install coreutils`):

- 15.7MB and 475 files for one command
- **Stops counting when your Mac sleeps** (uses `nanosleep`)

**uutils** (Rust rewrite of coreutils):

- Also stops counting during sleep (uses `Instant`/`mach_absolute_time`)

procguard uses `mach_continuous_time`, the only macOS clock that keeps ticking through sleep. Set 1 hour, get 1 hour, even if you close your laptop.

**Scenario:** `procguard 1h ./build` with laptop sleeping 45min in the middle

    0        15min                 1h                    1h 45min
    ├──────────┬──────────────────────────────┬──────────────────────────────┤
    Real time  │▓▓▓▓▓▓▓▓▓▓│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│
              │  awake   │            sleep             │            awake             │
              └──────────┴──────────────────────────────┴──────────────────────────────┘

    procguard       |██████████|██████████████████████████████^ fires at 1h ✓
                               (counts sleep time)

    GNU timeout     |██████████|······························|██████████████████████████████^ fires at 1h 45min ✗
                               (pauses during sleep)

    Legend: ▓ awake  ░ sleep  █ counting  · paused  ^ fire point

|                           | procguard          | GNU coreutils   |
| ------------------------- | ------------------ | --------------- |
| Works during system sleep | ✓                  | ✗               |
| Selectable time mode      | ✓ (wall/active)    | ✗ (active only) |
| **Resource limits**       | ✓ (mem/CPU)        | ✗               |
| **Formal verification**   | ✓ (19 kani proofs) | ✗               |
| JSON output               | ✓                  | ✗               |
| Retry on timeout          | ✓                  | ✗               |
| Stdin idle timeout        | ✓                  | ✗               |
| Pre-timeout hooks         | ✓                  | ✗               |
| CI heartbeat (keep-alive) | ✓                  | ✗               |
| Wait-for-file             | ✓                  | ✗               |
| Custom exit codes         | ✓                  | ✗               |
| Env var configuration     | ✓                  | ✗               |
| Binary size               | ~100KB             | 15.7MB          |
| Startup time              | 3.6ms              | 4.2ms           |
| Zero CPU while waiting    | ✓ (kqueue)         | ✓ (nanosleep)   |

_Performance data from [250 benchmark runs](#benchmarks) on Apple M4 Pro._

100% GNU-compatible. All flags work identically (`-s`, `-k`, `-p`, `-f`, `-v`). Drop-in replacement for Apple Silicon and Intel Macs.

## Quality & Verification

procguard uses a **five-layer verification approach**:

| Method                         | Coverage                   | What It Catches                                   |
| ------------------------------ | -------------------------- | ------------------------------------------------- |
| **Unit tests**                 | 154 tests                  | Logic errors, edge cases                          |
| **Integration tests**          | 185 tests                  | Real process behavior, signals, I/O               |
| **Library API tests**          | 10 tests                   | Public API usability, lifecycle                   |
| **Property-based (proptest)**  | 30 properties, ~7500 cases | Input invariants, mathematical relationships      |
| **Fuzzing (cargo-fuzz)**       | 4 targets, ~70M executions | Crashes, panics, hangs from malformed input       |
| **Formal verification (kani)** | 19 proofs                  | Mathematical proof of memory safety, no overflows |

**What this means for you:**

- Parsing code is fuzz-tested (found and fixed bugs before release)
- Unsafe code has formal proofs (mathematically verified, not just tested)
- State machines are proven correct (no race conditions in signal handling)
- Arithmetic is overflow-checked (all time calculations verified)

See [docs/VERIFICATION.md](docs/VERIFICATION.md) for methodology details.

## Install

**Homebrew** (recommended):

    brew install denispol/tap/procguard

**Binary download:** Grab the universal binary (arm64 + x86_64) from [releases](https://github.com/denispol/procguard/releases).

**From source (CLI):**

    cargo build --release
    sudo cp target/release/procguard /usr/local/bin/
    sudo ln -s procguard /usr/local/bin/timeout  # optional: GNU-compatible alias

**As a Rust library:**

    cargo add procguard

Shell completions are installed automatically with Homebrew. For manual install, see [completions/](completions/).

## Quick Start

    procguard 30 ./slow-command           # kill after 30 seconds
    procguard -k 5 30 ./stubborn          # SIGTERM, then SIGKILL after 5s
    procguard --json 1m ./build           # JSON output for CI
    procguard -v 10 ./script              # verbose: shows signals sent

## Use Cases

**CI/CD**: Stop flaky tests before they hang your pipeline.

    procguard --json 5m ./run-tests

**Overnight builds**: Timeouts that work even when your Mac sleeps.

    procguard 2h make release             # 2 hours wall-clock, guaranteed

**Network ops**: Don't wait forever for unresponsive servers.

    procguard 10s curl https://api.example.com/health

**Script safety**: Ensure cleanup scripts actually finish.

    procguard -k 10s 60s ./cleanup.sh

**Coordinated startup**: Wait for dependencies before running.

    procguard --wait-for-file /tmp/db-ready 5m ./migrate

**Prompt detection**: Kill commands that unexpectedly prompt for input.

    procguard --stdin-timeout 5s ./test-suite  # fail if it prompts for input

**Stream watchdog**: Detect stalled data pipelines without consuming the stream.

    pg_dump mydb | procguard -S 2m --stdin-passthrough 4h gzip | \
        aws s3 cp - s3://backups/db-$(date +%Y%m%d).sql.gz

**CI keep-alive**: Prevent CI systems from killing long jobs.

    procguard --heartbeat 60s 2h ./integration-tests

**Resource sandboxing**: Enforce memory and CPU limits.

    procguard --mem-limit 4G 2h make -j8
    procguard --cpu-percent 50 1h ./batch-process

## Options

    procguard [OPTIONS] DURATION COMMAND [ARGS...]

**GNU-compatible flags:**

    -s, --signal SIG         signal to send (default: TERM)
    -k, --kill-after T       send SIGKILL if still running after T
    -v, --verbose            print signals to stderr
    -p, --preserve-status    exit with command's status, not 124
    -f, --foreground         don't create process group

**procguard extensions:**

    -q, --quiet              suppress error messages
    -c, --confine MODE       time mode: 'wall' (default) or 'active'
    -H, --heartbeat T        print status to stderr every T (for CI keep-alive)
    --json                   JSON output for scripting
    --on-timeout CMD         run CMD on timeout (before kill); %p = child PID
    --on-timeout-limit T     time limit for --on-timeout (default: 5s)
    --timeout-exit-code N    exit with N instead of 124 on timeout
    --wait-for-file PATH     wait for file to exist before starting command
    --wait-for-file-timeout T  timeout for --wait-for-file (default: wait forever)
    -r, --retry N            retry command up to N times on timeout
    --retry-delay T          delay between retries (default: 0)
    --retry-backoff Nx       multiply delay by N each retry (e.g., 2x)
    -S, --stdin-timeout T    kill command if stdin is idle for T
    --stdin-passthrough      non-consuming stdin idle detection (pair with -S)
    --mem-limit SIZE         kill if memory exceeds SIZE (e.g., 512M, 2G, 1T)
    --cpu-time T             hard CPU time limit via RLIMIT_CPU (e.g., 30s, 5m)
    --cpu-percent PCT        throttle CPU to PCT% via SIGSTOP/SIGCONT

**Duration format:** number with optional suffix `ms` (milliseconds), `us`/`µs` (microseconds), `s` (seconds), `m` (minutes), `h` (hours), `d` (days). Fractional values supported: `0.5s`, `1.5ms`, `100us`.

**Exit codes:**

    0       command completed successfully
    124     timed out (custom via --timeout-exit-code)
    125     procguard itself failed
    126     command found but not executable
    127     command not found
    128+N   command killed by signal N

## Time Modes

**wall** (default for `procguard`): Real elapsed time, including system sleep.

    procguard 1h ./build                   # fires after 1 hour wall-clock

**active** (default for `timeout` alias): Only counts time when awake. Matches GNU behavior.

    procguard -c active 1h ./benchmark     # pauses during sleep
    timeout 1h ./benchmark                 # same (timeout alias defaults to active)

Under the hood: `wall` uses `mach_continuous_time`, `active` uses `CLOCK_MONOTONIC_RAW`.

## Resource Limits

Enforce memory and CPU constraints without containers or root privileges.

**Memory limit** (`--mem-limit`): Kill process if physical memory exceeds threshold.

    procguard --mem-limit 2G 1h ./memory-hungry-app

**CPU time limit** (`--cpu-time`): Hard limit on total CPU seconds consumed.

    procguard --cpu-time 5m 1h ./compute-job

**CPU throttle** (`--cpu-percent`): Actively limit CPU usage percentage.

    procguard --cpu-percent 50 1h ./background-task

See [docs/resource-limits.md](docs/resource-limits.md) for details.

## JSON Output

Machine-readable output for CI/CD pipelines:

    $ procguard --json 1s sleep 0.5
    {"schema_version":8,"status":"completed","exit_code":0,"elapsed_ms":504,...}

See [docs/json-output.md](docs/json-output.md) for complete schema.

## Library Usage

The `procguard` crate exposes the core timeout functionality for embedding in your own tools:

```rust
use procguard::{RunConfig, RunResult, Signal, run_command, setup_signal_forwarding};
use std::time::Duration;

let _ = setup_signal_forwarding();

let config = RunConfig {
    timeout: Duration::from_secs(30),
    signal: Signal::SIGTERM,
    kill_after: Some(Duration::from_secs(5)),
    ..RunConfig::default()
};

let args = ["-c".to_string(), "sleep 10".to_string()];
match run_command("sh", &args, &config) {
    Ok(RunResult::Completed { status, rusage }) => {
        println!("Completed with exit code {:?}", status.code());
    }
    Ok(RunResult::TimedOut { signal, .. }) => {
        println!("Timed out, sent {:?}", signal);
    }
    Ok(_) => println!("Other outcome"),
    Err(e) => eprintln!("Error: {}", e),
}
```

**Platform:** macOS only (uses Darwin kernel APIs).

**API Docs:** [docs.rs/procguard](https://docs.rs/procguard)

> ⚠️ **Stability:** The library API is experimental. Use `..RunConfig::default()` when constructing configs.

## Development

    cargo test                  # run tests
    cargo test --test proptest  # property-based tests
    cargo clippy                # lint
    ./scripts/verify-all.sh     # full verification suite

**Contributing:** See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow.

**Verification:** See [docs/VERIFICATION.md](docs/VERIFICATION.md) for testing methodology.

## License

MIT
