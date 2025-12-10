darwin-timeout
==============

GNU `timeout` for macOS, done right. Works through sleep. 100KB. Zero dependencies.

    brew install denispol/tap/darwin-timeout

Works exactly like GNU timeout:

    timeout 30s ./slow-command           # kill after 30 seconds
    timeout -k 5 1m ./stubborn           # SIGTERM, then SIGKILL after 5s
    timeout --preserve-status 10s ./cmd  # exit with command's status

Plus features GNU doesn't have:

    timeout --json 5m ./test-suite       # JSON output for CI
    timeout -c active 1h ./benchmark     # pause timer during sleep (GNU behavior)
    timeout --on-timeout 'cleanup.sh' 30s ./task  # pre-timeout hook
    timeout --retry 3 30s ./flaky-test   # retry on timeout

**Coming from GNU coreutils?** darwin-timeout defaults to wall-clock time (survives sleep). Use `-c active` for GNU-like behavior where the timer pauses during sleep.

Why?
----

Apple doesn't ship `timeout`. The alternatives have problems:

**GNU coreutils** (`brew install coreutils`):

- 15.7MB and 475 files for one command
- **Stops counting when your Mac sleeps** (uses `nanosleep`)

**uutils** (Rust rewrite of coreutils):

- Also stops counting during sleep (uses `Instant`/`mach_absolute_time`)

darwin-timeout uses `mach_continuous_time`, the only macOS clock that keeps ticking through sleep. Set 1 hour, get 1 hour, even if you close your laptop.

**Scenario:** `timeout 1h ./build` with laptop sleeping 45min in the middle

```
                0        15min                 1h                    1h 45min
                ├──────────┬──────────────────────────────┬──────────────────────────────┤
   Real time    │▓▓▓▓▓▓▓▓▓▓│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│
                │  awake   │            sleep             │            awake             │
                └──────────┴──────────────────────────────┴──────────────────────────────┘

darwin-timeout  |██████████|██████████████████████████████^ fires at 1h ✓
                           (counts sleep time)

GNU timeout     |██████████|······························|██████████████████████████████^ fires at 1h 45min ✗
                           (pauses during sleep)

Legend: ▓ awake  ░ sleep  █ counting  · paused  ^ fire point
```

|                           | darwin-timeout | GNU coreutils |
|---------------------------|----------------|---------------|
| Works during system sleep | ✓              | ✗             |
| Selectable time mode      | ✓ (wall/active)| ✗ (active only)|
| JSON output               | ✓              | ✗             |
| Retry on timeout          | ✓              | ✗             |
| Stdin idle timeout        | ✓              | ✗             |
| Pre-timeout hooks         | ✓              | ✗             |
| CI heartbeat (keep-alive) | ✓              | ✗             |
| Wait-for-file             | ✓              | ✗             |
| Custom exit codes         | ✓              | ✗             |
| Env var configuration     | ✓              | ✗             |
| Binary size               | 100KB          | 15.7MB        |
| Startup time              | 3.6ms          | 4.2ms         |
| Zero CPU while waiting    | ✓ (kqueue)     | ✓ (nanosleep) |

*Performance data from [250 benchmark runs](#benchmarks) on Apple M4 Pro.*

100% GNU-compatible. All flags work identically (`-s`, `-k`, `-p`, `-f`, `-v`). Drop-in replacement for Apple Silicon and Intel Macs.

Install
-------

**Homebrew** (recommended):

    brew install denispol/tap/darwin-timeout

**Binary download:** Grab the universal binary (arm64 + x86_64) from [releases](https://github.com/denispol/darwin-timeout/releases).

**From source:**

    cargo build --release
    sudo cp target/release/timeout /usr/local/bin/

Shell completions are installed automatically with Homebrew. For manual install, see [completions/](completions/).

Quick Start
-----------

    timeout 30 ./slow-command           # kill after 30 seconds
    timeout -k 5 30 ./stubborn          # SIGTERM, then SIGKILL after 5s
    timeout --json 1m ./build           # JSON output for CI
    timeout -v 10 ./script              # verbose: shows signals sent

Use Cases
---------

**CI/CD**: Stop flaky tests before they hang your pipeline.

    timeout --json 5m ./run-tests

**Overnight builds**: Timeouts that work even when your Mac sleeps.

    timeout 2h make release             # 2 hours wall-clock, guaranteed

**Network ops**: Don't wait forever for unresponsive servers.

    timeout 10s curl https://api.example.com/health

**Script safety**: Ensure cleanup scripts actually finish.

    timeout -k 10s 60s ./cleanup.sh

**Coordinated startup**: Wait for dependencies before running.

    timeout --wait-for-file /tmp/db-ready 5m ./migrate

**Prompt detection**: Kill commands that unexpectedly prompt for input. Catch interactive tests hanging on stdin in unattended CI environments.

    timeout --stdin-timeout 5s ./test-suite  # fail if it prompts for input

> **Note:** `--stdin-timeout` alone **consumes stdin data** to detect activity—the child won't receive it. This is ideal for detecting unexpected prompts in non-interactive CI.

**Stream watchdog**: Detect stalled data pipelines without consuming the stream. If upstream stops sending data for too long, kill the pipeline. The child receives all data intact, which is perfect for production backup and log shipping pipelines.

    # Database backup: kill if pg_dump stalls for 2+ minutes
    # Protects against database locks, network issues, or hung queries
    pg_dump mydb | timeout -S 2m --stdin-passthrough 4h gzip | \
        aws s3 cp - s3://backups/db-$(date +%Y%m%d).sql.gz

    # Kubernetes log shipping: fail if pod stops emitting logs for 30s
    # Catches crashed pods, network issues, or stuck log tails
    kubectl logs -f deployment/app --since=10m | \
        timeout -S 30s --stdin-passthrough 24h ./ship-to-elasticsearch.sh

    # Real-time data sync: abort if upstream stops sending for 5 minutes
    nc data-source 9000 | timeout -S 5m --stdin-passthrough 48h ./process-stream.sh

> **How it works:** The timer resets on every stdin activity. When stdin reaches EOF (closed pipe), monitoring stops and wall clock timeout takes over. Data flows to the child untouched.

**CI keep-alive**: Many CI systems (GitHub Actions, GitLab CI, Travis) kill jobs that produce no output for 10-30 minutes. Long builds, test suites, or deployments can trigger this even when working correctly. The heartbeat flag prints periodic status messages to prevent these false timeouts:

    timeout --heartbeat 60s 2h ./integration-tests
    # every 60s: timeout: heartbeat: 5m 0s elapsed, command still running (pid 12345)

    # combine with --json for structured CI output
    timeout --heartbeat 30s --json 1h ./deploy.sh

Options
-------

    timeout [OPTIONS] DURATION COMMAND [ARGS...]

**GNU-compatible flags:**

    -s, --signal SIG         signal to send (default: TERM)
    -k, --kill-after T       send SIGKILL if still running after T
    -v, --verbose            print signals to stderr
    -p, --preserve-status    exit with command's status, not 124
    -f, --foreground         don't create process group

**darwin-timeout extensions:**

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
    -S, --stdin-timeout T    kill command if stdin is idle for T (consumes stdin; for prompt detection)
    --stdin-passthrough      non-consuming stdin idle detection (pair with -S)

**Duration format:** number with optional suffix `ms` (milliseconds), `us`/`µs` (microseconds), `s` (seconds), `m` (minutes), `h` (hours), `d` (days). Fractional values supported: `0.5s`, `1.5ms`, `100us`.

**Exit codes:**

    0       command completed successfully
    124     timed out, or --wait-for-file timed out (custom via --timeout-exit-code)
    125     timeout itself failed
    126     command found but not executable
    127     command not found
    128+N   command killed by signal N

Time Modes
----------

**wall** (default): Real elapsed time, including system sleep. A 1-hour timeout fires after 1 hour of wall-clock time, even if your Mac sleeps for 45 minutes.

    timeout 1h ./build
    timeout -c wall 1h ./build           # explicit

**active**: Only counts time when the system is awake. This matches GNU timeout behavior, useful for benchmarks or when you want the timer to pause during sleep.

    timeout -c active 1h ./benchmark     # pauses during sleep, like GNU timeout

Under the hood: `wall` uses `mach_continuous_time`, `active` uses `CLOCK_MONOTONIC_RAW`.

JSON Output
-----------

Machine-readable output for CI/CD pipelines and automation:

    $ timeout --json 1s sleep 0.5
    {"schema_version":5,"status":"completed","exit_code":0,"elapsed_ms":504,"user_time_ms":1,"system_time_ms":2,"max_rss_kb":1248}

    $ timeout --json 0.5s sleep 10
    {"schema_version":5,"status":"timeout","timeout_reason":"wall_clock","signal":"SIGTERM","signal_num":15,"killed":false,"command_exit_code":-1,"exit_code":124,"elapsed_ms":502,"user_time_ms":0,"system_time_ms":1,"max_rss_kb":1232}

**Status types:** `completed`, `timeout`, `signal_forwarded`, `error`

**Timeout reasons:** `wall_clock` (main timeout), `stdin_idle` (-S/--stdin-timeout)

Includes resource usage metrics: CPU time (`user_time_ms`, `system_time_ms`) and peak memory (`max_rss_kb`).

See [docs/json-output.md](docs/json-output.md) for complete schema documentation, field reference, and integration examples.

Environment Variables
---------------------

Configure defaults without CLI flags:

    TIMEOUT                       default duration if CLI arg isn't a valid duration
    TIMEOUT_SIGNAL                default signal (overridden by -s)
    TIMEOUT_KILL_AFTER            default kill-after (overridden by -k)
    TIMEOUT_RETRY                 default retry count (overridden by -r/--retry)
    TIMEOUT_HEARTBEAT             default heartbeat interval (overridden by -H/--heartbeat)
    TIMEOUT_STDIN_TIMEOUT         default stdin idle timeout (overridden by -S/--stdin-timeout)
    TIMEOUT_WAIT_FOR_FILE         default file to wait for
    TIMEOUT_WAIT_FOR_FILE_TIMEOUT timeout for wait-for-file

Pre-timeout Hooks
-----------------

Run a command when timeout fires, before sending the signal:

    timeout --on-timeout 'echo "killing $p" >> /tmp/log' 5s ./long-task
    timeout --on-timeout 'kill -QUIT %p' --on-timeout-limit 2s 30s ./server

`%p` is replaced with the child PID. Hooks have their own timeout (default 5s).

How It Works
------------

Built on Darwin kernel primitives:

- **kqueue + EVFILT_PROC + EVFILT_TIMER**: monitors process exit and timeout with zero CPU overhead
- **mach_continuous_time**: wall-clock that survives system sleep (the key differentiator)
- **CLOCK_MONOTONIC_RAW**: active-time clock, pauses during sleep
- **posix_spawn**: lightweight process creation (faster than fork+exec)
- **Signal forwarding**: SIGTERM/SIGINT/SIGHUP/SIGQUIT/SIGUSR1/SIGUSR2 forwarded to child process group
- **Process groups**: child runs in own group so signals reach all descendants

100KB `no_std` binary. Custom allocator, direct syscalls, no libstd runtime.

Benchmarks
----------

All benchmarks on Apple M4 Pro, macOS Tahoe 26.2, hyperfine 1.20.0.
See [docs/benchmarks/](docs/benchmarks/) for raw data and methodology.

    # Binary size
    darwin-timeout: 100KB
    GNU coreutils:  15.7MB (157x larger)

    # Startup overhead (250 runs across 5 sessions)
    darwin-timeout: 3.6ms ± 0.2ms
    GNU timeout:    4.2ms ± 0.2ms (18% slower)

    # Timeout precision (20 runs, 1s timeout)
    darwin-timeout: 1.014s ± 0.003s
    GNU timeout:    1.017s ± 0.001s (identical)

    # CPU while waiting
    darwin-timeout: 0.00 user, 0.00 sys (kqueue blocks)

    # Feature overhead (vs baseline)
    --json flag:      0% overhead
    --verbose flag:   0% overhead
    --retry flag:     0% overhead (when not triggered)
    --heartbeat flag: 0% overhead (prints only at intervals)

Development
-----------

    cargo test                  # run tests
    cargo clippy                # lint
    ./scripts/benchmark.sh      # run benchmarks

Library usage coming soon; core timeout logic will be available as a crate.

License
-------

MIT
