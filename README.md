darwin-timeout
==============

A native Darwin timeout command. One binary, ~500KB (release build), no dependencies.

Drop-in replacement for GNU `timeout` that actually works correctly on Apple platforms.

Currently supports macOS.

Why?
----

Apple platforms don't ship `timeout`. The usual macOS answer is `brew install coreutils`, but:

- It installs 15.7MB and 475 files just to get one command
- GNU timeout pauses when your Mac sleeps (uses `nanosleep`)

We use `mach_continuous_time`, so the timer keeps running through sleep. Set a 1 hour timeout, close your laptop for 7 hours, open it, and we fire immediately. GNU waits another hour.

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
| Single static binary      | ✓              | ✗             |
| Install size              | ~500KB         | 15.7MB        |
| Dependencies              | none           | glibc, etc.   |

Performance
-----------

Identical to GNU timeout. Both hit the same macOS kernel scheduling floor (~10ms).

    hyperfine './timeout 1s sleep 10' 'gtimeout 1s sleep 10'
    
    timeout  1.014s ± 0.002s
    gtimeout 1.013s ± 0.002s

Zero CPU while waiting (kqueue blocks until the event fires).

JSON Output
-----------

Exit codes are lossy: one 8-bit number can't tell you if SIGTERM was ignored, how long the command actually ran, or what exit code it had before you killed it. JSON preserves the full picture:

    $ timeout --json 30s ./my-build
    {"schema_version":2,"status":"timeout","signal":"SIGTERM","signal_num":15,"killed":false,"command_exit_code":-1,"exit_code":124,"elapsed_ms":30021}

    result=$(timeout --json 5m ./tests)
    jq -r '.status, .elapsed_ms, .killed' <<< "$result"

Install
-------

Requires macOS 10.12+ and Rust 1.90+ (build only).

**Apple Silicon (recent hardware):**

    cargo build --release
    cp target/release/timeout /usr/local/bin/

**Intel Mac (Tier 2 target as of Rust 1.90.0):**

    cargo build --release --target x86_64-apple-darwin
    cp target/x86_64-apple-darwin/release/timeout /usr/local/bin/

**Universal binary (ARM64 + x86_64):**

For distribution or shared `/usr/local/bin` across machines:

    ./scripts/build-universal.sh
    cp target/universal/timeout /usr/local/bin/

Requires both targets: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`

**Shell completions:**

    timeout --completions zsh  > ~/.zsh/completions/_timeout   # zsh
    timeout --completions bash >> ~/.bashrc                    # bash
    timeout --completions fish > ~/.config/fish/completions/timeout.fish

Quick Start
-----------

    timeout 30 ./slow-command           # kill after 30 seconds
    timeout -k 5 30 ./stubborn          # SIGTERM, then SIGKILL after 5s
    timeout --json 1m ./build           # JSON output for CI
    timeout -v 10 ./script              # verbose, shows signals sent

Use Cases
---------

**CI/CD pipelines**: Stop flaky tests before they hang your whole build.

    timeout --json 5m ./run-tests

**Build systems**: Catch infinite loops in code generation or compilation.

    timeout 30m make all

**Network operations**: Don't wait forever for unresponsive servers.

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

Fractional seconds:

    timeout 0.5s sleep 10

Minutes, hours, days:

    timeout 30m ./batch-job
    timeout 2h ./long-task

SIGKILL if SIGTERM is ignored:

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

Quiet mode (suppress error messages, JSON still outputs):

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

    -s, --signal SIG         signal to send (default: TERM)
    -k, --kill-after T       send SIGKILL if still running after T
    -f, --foreground         don't create process group
    -p, --preserve-status    exit with command's status, not 124
    -v, --verbose            print signals to stderr
    -q, --quiet              suppress error messages (mutually exclusive with -v)
    --timeout-exit-code N    exit with N instead of 124 on timeout
    --on-timeout CMD         run CMD on timeout (before kill); %p = child PID, %% = literal %
    --on-timeout-limit T     time limit for --on-timeout (default: 5s)
    --json                   JSON output for scripting

Environment Variables
---------------------

Environment variables provide default values when the corresponding CLI flag is not provided:

    TIMEOUT            default duration (used when first positional argument
                       is not a valid duration; if ambiguous, a warning is shown)
    TIMEOUT_SIGNAL     default signal (overridden by -s/--signal)
    TIMEOUT_KILL_AFTER default kill-after (overridden by -k/--kill-after)

Note: If `TIMEOUT` is set and the first positional argument could be interpreted as either
a duration or a command name, timeout will use the positional as duration and show a warning.
Use an explicit path (e.g., `./30s` instead of `30s`) to run a command named like a duration.

Duration
--------

Number with optional suffix: `s` (seconds, default), `m` (minutes), `h` (hours), `d` (days).
Suffixes are case-insensitive (`30S`, `1M`, `2H` work too).

`0` disables timeout.

Exit Codes
----------

    0       command completed
    124     timed out
    125     timeout failed
    126     command not executable
    127     command not found
    128+N   killed by signal N

Note: `--timeout-exit-code` values in the range 125-137 will conflict with standard exit codes
and may produce ambiguous results. A warning is shown if you use values in this range.

JSON Schema
-----------

With `--json`, output is a single JSON object. All output includes a `schema_version` field (currently `2`).
The `status` field determines which other fields are present:

| Field | Type | `completed` | `timeout` | `signal_forwarded` | `error` |
|-------|------|:-----------:|:---------:|:------------------:|:-------:|
| `schema_version` | number | ✓ | ✓ | ✓ | ✓ |
| `status` | string | ✓ | ✓ | ✓ | ✓ |
| `exit_code` | number | ✓ | ✓ | ✓ | ✓ |
| `elapsed_ms` | number | ✓ | ✓ | ✓ | ✓ |
| `signal` | string | | ✓ | ✓ | |
| `signal_num` | number | | ✓ | ✓ | |
| `killed` | boolean | | ✓ | | |
| `command_exit_code` | number | | ✓ | ✓ | |
| `hook_ran` | boolean | | ✓ | | |
| `hook_exit_code` | number/null | | ✓* | | |
| `hook_timed_out` | boolean | | ✓* | | |
| `hook_elapsed_ms` | number | | ✓* | | |
| `error` | string | | | | ✓ |

*Hook fields are only present when `--on-timeout` is configured.

**Status types:**

- `completed`: command finished before timeout
- `timeout`: command was killed due to timeout
- `signal_forwarded`: timeout received a signal (SIGTERM/SIGINT/SIGHUP) and forwarded it to the command
- `error`: timeout itself failed (command not found, permission denied, etc.)

**Examples:**

    # Command completed successfully
    {"schema_version":2,"status":"completed","exit_code":0,"elapsed_ms":504}

    # Command timed out, killed with SIGTERM
    {"schema_version":2,"status":"timeout","signal":"SIGTERM","signal_num":15,"killed":false,"command_exit_code":-1,"exit_code":124,"elapsed_ms":502}

    # Command timed out with hook
    {"schema_version":2,"status":"timeout","signal":"SIGTERM","signal_num":15,"killed":false,"command_exit_code":-1,"exit_code":124,"elapsed_ms":502,"hook_ran":true,"hook_exit_code":0,"hook_timed_out":false,"hook_elapsed_ms":15}

    # Command timed out, escalated to SIGKILL
    {"schema_version":2,"status":"timeout","signal":"SIGTERM","signal_num":15,"killed":true,"command_exit_code":137,"exit_code":124,"elapsed_ms":5023}

    # Timeout received SIGTERM and forwarded it
    {"schema_version":2,"status":"signal_forwarded","signal":"SIGTERM","signal_num":15,"command_exit_code":143,"exit_code":143,"elapsed_ms":1502}

    # Command not found
    {"schema_version":2,"status":"error","error":"command not found: nonexistent","exit_code":127,"elapsed_ms":1}

How It Works
------------

Built on Darwin kernel APIs available across all Apple platforms:

**kqueue** monitors process exit (EVFILT_PROC) and timeout (EVFILT_TIMER with NOTE_NSECONDS). No polling.

**mach_continuous_time** for wall-clock timing that survives system sleep. A 1 hour timeout takes 1 hour even if you close the lid.

**Signal forwarding**: SIGTERM/SIGINT/SIGHUP to timeout get forwarded to the child. No orphans on Ctrl-C.

**Process groups**: child runs in its own group so signals reach all descendants. `--foreground` disables this.

Testing
-------

    cargo test
    cargo clippy
    ./scripts/benchmark.sh

License
-------

MIT
