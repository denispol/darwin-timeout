# procguard

**The missing process supervisor for macOS.**

Linux has cgroups. macOS has nothing—until now.

```bash
procguard --mem-limit 4G 2h make -j8       # kill if memory exceeds 4GB
procguard --cpu-percent 50 1h ./batch      # throttle to 50% CPU
procguard --cpu-time 5m 1h ./compute       # hard 5-minute CPU limit
procguard --wait-for-file /tmp/ready 5m ./app  # coordinated startup
procguard --json --retry 3 5m ./test-suite     # CI integration
```

    brew install denispol/tap/procguard

## Why This Exists

macOS has no native way to:

| Need | Without procguard |
|------|-------------------|
| Kill a build at 8GB RAM | Watch Activity Monitor manually |
| Throttle CPU to save battery | Run inside Docker |
| Get JSON from process execution | Write wrapper scripts |
| Timeout that survives sleep | Accept wrong behavior |

procguard handles all of this. No containers, no root, no daemons.

## Built with Rust

procguard is a `no_std` Rust binary. No runtime dependencies. No allocator overhead. Just raw Darwin syscalls.

| Metric | Value |
|--------|-------|
| Binary size | **~100KB** (universal arm64+x86_64) |
| Startup time | **3.6ms** |
| Memory overhead | **<1MB** |
| Dependencies | **0** (libc only) |
| Unsafe blocks | **19** (each formally verified) |

This isn't "Rust for safety"—it's Rust for **precision**. Every byte matters when you're building system tooling.

### Formal Verification

Every unsafe block has a [Kani](https://github.com/model-checking/kani) proof. Not tests—**mathematical proofs** that the code cannot:
- Overflow on any arithmetic
- Dereference invalid memory
- Race on signal handling state

```
src/sync.rs      → AtomicOnce initialization proof
src/throttle.rs  → CPU throttle state machine proof
src/proc_info.rs → Buffer alignment proof
src/time_math.rs → Overflow-free time calculations
```

This is what Rust makes possible. See [docs/VERIFICATION.md](docs/VERIFICATION.md).

## Includes GNU-compatible `timeout`

Apple doesn't ship `timeout`. procguard includes a fully compatible implementation:

```bash
timeout 30s ./command              # exact GNU behavior
timeout -k 5 1m ./stubborn         # all flags work: -s, -k, -v, -p, -f
timeout --preserve-status 10s ./cmd
```

Same exit codes. Same signal handling. Scripts written for Linux just work.

The `procguard` binary adds features on top: resource limits, JSON output, retry logic, coordinated startup. Use whichever fits your workflow—both are installed together.

**Bonus:** procguard uses `mach_continuous_time`, the only macOS clock that survives system sleep. A 1-hour timeout takes 1 hour of wall time, even if your laptop sleeps.

## Testing

| Method | Coverage | What It Catches |
|--------|----------|-----------------|
| Unit tests | 154 tests | Logic errors, edge cases |
| Integration tests | 185 tests | Real process behavior, signals, I/O |
| Library API tests | 10 tests | Public API usability |
| Property-based (proptest) | 30 properties | Input invariants |
| Fuzzing (cargo-fuzz) | 4 targets, ~70M executions | Crashes from malformed input |
| Formal verification (kani) | 19 proofs | Memory safety, no overflows |

Fuzzing found and fixed bugs before release. Formal proofs guarantee the unsafe code is correct.

## Install

**Homebrew** (recommended):

    brew install denispol/tap/procguard

**Cargo:**

    cargo install procguard    # installs both procguard and timeout binaries

**Binary download:** Universal binary (arm64 + x86_64) from [releases](https://github.com/denispol/procguard/releases).

**From source:**

    cargo build --release
    sudo cp target/release/procguard /usr/local/bin/
    sudo cp target/release/timeout /usr/local/bin/

**As a Rust library:**

    cargo add procguard

Shell completions installed automatically with Homebrew. For manual install, see [completions/](completions/).

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

procguard is a learning resource for `no_std` Rust and Darwin systems programming.

```bash
cargo test                  # 349 tests
cargo test --test proptest  # property-based tests
cargo clippy                # lint
./scripts/verify-all.sh     # full verification (tests + fuzz + kani)
```

**Architecture highlights:**
- `src/runner.rs` — kqueue event loop, zero-CPU waiting
- `src/process.rs` — posix_spawn wrapper, lighter than fork+exec
- `src/throttle.rs` — CPU throttling via SIGSTOP/SIGCONT integral control
- `src/proc_info.rs` — Darwin libproc API for memory stats
- `src/time_math.rs` — checked arithmetic, no overflow possible

**Contributing:** See [CONTRIBUTING.md](CONTRIBUTING.md).

**Verification methodology:** See [docs/VERIFICATION.md](docs/VERIFICATION.md).

## License

MIT
