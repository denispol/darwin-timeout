darwin-timeout
==============

Native macOS replacement for GNU `timeout`. Single `no_std` binary, **83KB**, zero dependencies.

**185x smaller** than `brew install coreutils`. **20% faster startup**. **Identical timeout precision**.

Drop-in replacement that works correctly on Apple Silicon and Intel Macs.

Why?
----

Apple doesn't ship `timeout`. The usual answer is `brew install coreutils`, but:

- Installs 15.7MB and 475 files just to get one command
- GNU timeout pauses when your Mac sleeps (uses `nanosleep`)

We use `mach_continuous_time`, so the timer keeps running through sleep. Set a 1-hour timeout, close your laptop for 7 hours, open it, and we fire immediately. GNU waits another hour.

|                           | darwin-timeout | GNU coreutils |
|---------------------------|----------------|---------------|
| Works during system sleep | ✓              | ✗             |
| Zero CPU while waiting    | ✓ (kqueue)     | ✓ (nanosleep) |
| Signal forwarding         | ✓              | ✓             |
| Process group handling    | ✓              | ✓             |
| JSON output for CI        | ✓              | ✗             |
| Pre-timeout hook          | ✓              | ✗             |
| Custom exit code          | ✓              | ✗             |
| Quiet mode                | ✓              | ✗             |
| Env var configuration     | ✓              | ✗             |
| Shell completions         | ✓              | ✓             |
| Single static binary      | ✓              | ✗             |
| Install size              | 83KB           | 15.7MB (185x) |
| Startup time              | 4ms            | 5ms           |
| Dependencies              | none           | glibc, etc.   |

Performance
-----------

Identical timeout precision. 20% faster startup. Zero CPU while waiting. [Detailed benchmarks](#benchmarks).

    # Timeout precision (50 runs, 10 warmup)
    hyperfine './timeout 1s sleep 10' 'gtimeout 1s sleep 10'
    
    timeout  1.017s ± 0.003s
    gtimeout 1.017s ± 0.003s

    # Startup overhead (100 runs, 10 warmup)
    hyperfine './timeout 1 true' 'gtimeout 1 true'
    
    timeout  4ms ± 0ms  (1.20x faster)
    gtimeout 5ms ± 1ms

    # CPU during 2s wait
    /usr/bin/time -l ./timeout 2 sleep 60
    
    0.00 user, 0.00 sys  (kqueue blocks, no polling)

Install
-------

Requires macOS 10.12+ and Rust 1.90+ (build only).

**From source:**

    cargo build --release
    sudo cp target/release/timeout /usr/local/bin/

**Universal binary (ARM64 + x86_64):**

    ./scripts/build-universal.sh
    sudo cp target/universal/timeout /usr/local/bin/

Requires both targets: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`

**Shell completions:**

    # zsh
    mkdir -p ~/.zsh/completions
    cp completions/timeout.zsh ~/.zsh/completions/_timeout
    # add to .zshrc: fpath=(~/.zsh/completions $fpath)

    # bash
    sudo cp completions/timeout.bash /etc/bash_completion.d/timeout

    # fish
    cp completions/timeout.fish ~/.config/fish/completions/

Quick Start
-----------

    timeout 30 ./slow-command           # Kill after 30 seconds
    timeout -k 5 30 ./stubborn          # SIGTERM, then SIGKILL after 5s
    timeout --json 1m ./build           # JSON output for CI
    timeout -v 10 ./script              # Verbose, shows signals sent

Use Cases
---------

**CI/CD**: Stop flaky tests before they hang your entire build.

    timeout --json 5m ./run-tests

**Build systems**: Catch infinite loops in code generation.

    timeout 30m make all

**Network ops**: Don't wait forever for unresponsive servers.

    timeout 10s curl https://api.example.com/health

**Script safety**: Ensure cleanup scripts actually finish.

    timeout -k 10s 60s ./cleanup.sh

Examples
--------

Command completes:

    $ timeout 5s sleep 1
    $ echo $?
    0

Command times out (exit 124):

    $ timeout 1s sleep 10
    $ echo $?
    124

Fractional seconds, minutes, hours, days:

    timeout 0.5s sleep 10
    timeout 30m ./batch-job
    timeout 2h ./long-task

SIGKILL if SIGTERM ignored:

    timeout -k 5s 30s ./ignores-sigterm

Verbose:

    $ timeout -v 1s sleep 10
    timeout: sending signal SIGTERM to command

Custom signal:

    timeout -s HUP 5s ./daemon

Preserve command's exit status:

    $ timeout --preserve-status 1s sleep 10
    $ echo $?
    143

JSON for CI:

    $ timeout --json 1s sleep 0.5
    {"schema_version":2,"status":"completed","exit_code":0,"elapsed_ms":504}

    $ timeout --json 0.5s sleep 10
    {"schema_version":2,"status":"timeout","signal":"SIGTERM","signal_num":15,"killed":false,"command_exit_code":-1,"exit_code":124,"elapsed_ms":502}

Quiet mode:

    $ timeout -q 1s nonexistent-command
    $ echo $?
    127

Custom exit code on timeout:

    $ timeout --timeout-exit-code 42 1s sleep 10
    $ echo $?
    42

Run cleanup on timeout:

    timeout --on-timeout 'echo timed out >> /tmp/log' 5s ./long-task

Environment variable for duration:

    TIMEOUT=30s timeout ./my-command

Options
-------

    -s, --signal SIG         Signal to send (default: TERM)
    -k, --kill-after T       Send SIGKILL if still running after T
    -f, --foreground         Don't create process group
    -p, --preserve-status    Exit with command's status, not 124
    -v, --verbose            Print signals to stderr
    -q, --quiet              Suppress error messages (mutually exclusive with -v)
    --timeout-exit-code N    Exit with N instead of 124 on timeout
    --on-timeout CMD         Run CMD on timeout (before kill); %p = child PID
    --on-timeout-limit T     Time limit for --on-timeout (default: 5s)
    --json                   JSON output for scripting

Environment Variables
---------------------

    TIMEOUT            Default duration (used if CLI arg isn't a valid duration)
    TIMEOUT_SIGNAL     Default signal (overridden by -s/--signal)
    TIMEOUT_KILL_AFTER Default kill-after (overridden by -k/--kill-after)

Duration Format
---------------

Number with optional suffix: `s` (seconds, default), `m` (minutes), `h` (hours), `d` (days).
Case-insensitive. `0` disables timeout.

Exit Codes
----------

    0       Command completed
    124     Timed out
    125     Timeout failed
    126     Command not executable
    127     Command not found
    128+N   Killed by signal N

JSON Schema
-----------

With `--json`, output is a single JSON object with `schema_version` (currently `2`).

**Status types:**

- `completed`: Command finished before timeout
- `timeout`: Command was killed due to timeout
- `signal_forwarded`: Timeout received a signal and forwarded it
- `error`: Timeout itself failed

**Fields by status:**

| Field | Type | completed | timeout | signal_forwarded | error |
|-------|------|:---------:|:-------:|:----------------:|:-----:|
| schema_version | number | ✓ | ✓ | ✓ | ✓ |
| status | string | ✓ | ✓ | ✓ | ✓ |
| exit_code | number | ✓ | ✓ | ✓ | ✓ |
| elapsed_ms | number | ✓ | ✓ | ✓ | ✓ |
| signal | string | | ✓ | ✓ | |
| signal_num | number | | ✓ | ✓ | |
| killed | boolean | | ✓ | | |
| command_exit_code | number | | ✓ | ✓ | |
| hook_* | various | | ✓* | | |
| error | string | | | | ✓ |

*hook fields present only when `--on-timeout` is configured.

How It Works
------------

Built on Darwin kernel APIs:

**kqueue**: Monitors process exit (EVFILT_PROC) and timeout (EVFILT_TIMER with NOTE_NSECONDS). No polling.

**mach_continuous_time**: Wall-clock timing that survives system sleep. 1-hour timeout = 1 hour even if you close the lid.

**Signal forwarding**: SIGTERM/SIGINT/SIGHUP to timeout get forwarded to child. No orphans on Ctrl-C.

**Process groups**: Child runs in its own group so signals reach all descendants. `--foreground` disables this.

**no_std binary**: Custom allocator, direct syscalls, no libstd runtime. That's why it's 83KB instead of 500KB.

Testing
-------

    cargo test
    cargo clippy
    ./scripts/benchmark.sh

Benchmarks
----------

All benchmarks run on Apple M3 Pro, macOS 15.1, Rust 1.90.0, hyperfine 1.18.0.
Compared against GNU coreutils 9.5 (`gtimeout` via Homebrew).

**Binary size**

    $ ls -l target/release/timeout
    -rwxr-xr-x  1 user  staff  85184  timeout
    
    83KB (no_std, stripped, lto=fat, opt-level=z)

**Test 1: Startup overhead**

100 runs, 10 warmup. Measures time to spawn, run `true`, and exit.

    $ hyperfine --warmup 10 -N --runs 100 './timeout 1 true' 'gtimeout 1 true'
    
    Benchmark 1: ./timeout 1 true
      Time (mean ± σ):       4.1 ms ±   0.4 ms
      Range (min … max):     3.4 ms …   5.1 ms
    
    Benchmark 2: gtimeout 1 true
      Time (mean ± σ):       4.9 ms ±   0.7 ms  
      Range (min … max):     4.0 ms …   7.2 ms
    
    Summary: timeout ran 1.20 ± 0.21 times faster than gtimeout

**Test 2: 1s timeout precision**

50 runs, 10 warmup. Target: 1000ms.

    $ hyperfine --warmup 10 -N --runs 50 -i './timeout 1s sleep 10' 'gtimeout 1s sleep 10'
    
    Benchmark 1: ./timeout 1s sleep 10
      Time (mean ± σ):      1.017 s ±  0.003 s
      Range (min … max):    1.010 s …  1.022 s
    
    Benchmark 2: gtimeout 1s sleep 10
      Time (mean ± σ):      1.017 s ±  0.003 s
      Range (min … max):    1.010 s …  1.021 s
    
    Summary: identical (1.00 ± 0.00)

**Test 3: 100ms timeout precision**

30 runs, 5 warmup. Target: 100ms.

    $ hyperfine --warmup 5 -N --runs 30 -i './timeout 0.1 sleep 10' 'gtimeout 0.1 sleep 10'
    
    Benchmark 1: ./timeout 0.1 sleep 10
      Time (mean ± σ):     114.6 ms ±   2.8 ms
      Range (min … max):   109.5 ms … 119.5 ms
    
    Benchmark 2: gtimeout 0.1 sleep 10
      Time (mean ± σ):     115.7 ms ±   2.3 ms
      Range (min … max):   110.3 ms … 120.0 ms
    
    Summary: timeout ran 1.01 ± 0.03 times faster
    Note: ~15ms overhead is macOS kernel scheduling floor.

**Test 4: Kill-after escalation**

10 runs, 3 warmup. 200ms timeout + 200ms kill-after = target 400ms.

    $ hyperfine --warmup 3 -N --runs 10 -i "./timeout -k 0.2 0.2 bash -c 'trap \"\" TERM; sleep 60'"
    
    Benchmark 1: ./timeout -k 0.2 0.2 bash -c 'trap "" TERM; sleep 60'
      Time (mean ± σ):     421.0 ms ±   2.8 ms
      Range (min … max):   416.1 ms … 424.4 ms
    
    ~21ms overhead (5%) for signal delivery and process reaping.

**Test 5: CPU usage during wait**

2-second wait, measuring user/sys time.

    $ /usr/bin/time -l ./timeout 2 sleep 60
    
            2.00 real         0.00 user         0.00 sys
    
    Zero CPU — kqueue blocks until event fires.

**Test 6: Fast command latency**

100 runs, 10 warmup. Overhead of timeout wrapper on fast commands.

    $ hyperfine --warmup 10 -N --runs 100 'echo hello' './timeout 60 echo hello'
    
    Benchmark 1: echo hello
      Time (mean ± σ):       1.4 ms ±   0.2 ms
    
    Benchmark 2: ./timeout 60 echo hello  
      Time (mean ± σ):       3.7 ms ±   0.3 ms
    
    ~2.3ms overhead for process spawning and kqueue setup.

License
-------

MIT
