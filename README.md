# procguard

Kill runaway processes before they freeze your Mac.

```bash
procguard --mem-limit 4G 2h make -j8
```

macOS has no native memory limits or timeout command. procguard adds both.

    brew install denispol/tap/procguard

## What else?

```bash
procguard --cpu-percent 50 1h ./batch          # throttle CPU
procguard --cpu-time 5m 1h ./compute           # cap CPU seconds
procguard --retry 3 --json 5m ./test           # CI-friendly
procguard --heartbeat 60s 2h ./long-job        # keep CI alive
procguard --wait-for-file /tmp/ready 5m ./app  # wait for deps
procguard --on-timeout 'cleanup.sh' 30m ./job  # hook before kill
```

## GNU-compatible `timeout`

Apple doesn't ship one. procguard does:

```bash
timeout 30s ./command           # GNU-compatible
timeout -k 5 1m ./stubborn      # all the flags work
```

Same behavior, same exit codes. Your Linux scripts just work.

## Install

```bash
brew install denispol/tap/procguard
# or
cargo install procguard
```

Both install `procguard` and `timeout` binaries.

## The Rust stuff

`no_std`. ~100KB. 3.6ms startup. Zero dependencies beyond libc.

Built on Darwin internals most people never touch:

- **kqueue** for zero-CPU event waiting
- **posix_spawn** instead of fork (matters on Apple Silicon)
- **proc_pid_rusage** for memory stats without entitlements
- **mach_continuous_time** - the only clock that survives sleep

19 [Kani](https://github.com/model-checking/kani) proofs verify critical invariants: mathematical proofs, not just tests.

### As a library

```rust
use procguard::{RunConfig, RunResult, run_command};
use std::time::Duration;

let config = RunConfig {
    timeout: Duration::from_secs(30),
    kill_after: Some(Duration::from_secs(5)),
    ..Default::default()
};

let args = ["-c".to_string(), "sleep 10".to_string()];
match run_command("sh", &args, &config) {
    Ok(RunResult::TimedOut { .. }) => println!("timed out"),
    Ok(RunResult::Completed { status, .. }) => println!("exit {}", status.code().unwrap_or(-1)),
    _ => {}
}
```

    cargo add procguard

## Reference

```
procguard [OPTIONS] DURATION COMMAND [ARGS...]

Timeout:
  -s, --signal SIG        signal to send (default: TERM)
  -k, --kill-after T      SIGKILL if still running after T
  -p, --preserve-status   exit with command's status
  -f, --foreground        don't create process group

Resources:
  --mem-limit SIZE        kill if memory exceeds (512M, 2G)
  --cpu-time T            CPU time limit (30s, 5m)
  --cpu-percent PCT       throttle to PCT%

Lifecycle:
  -r, --retry N              retry N times on timeout
  --retry-delay T            delay between retries
  --retry-backoff Nx         exponential backoff (2x, 3x)
  --wait-for-file PATH       wait for file before starting
  --wait-for-file-timeout T  timeout for file wait
  --on-timeout CMD           run before killing (%p = PID)
  --on-timeout-limit T       timeout for hook (default: 5s)

Input/Output:
  -v, --verbose              show signals sent
  -q, --quiet                suppress errors
  --json                     machine-readable output
  -H, --heartbeat T          periodic status messages
  -S, --stdin-timeout T      kill if stdin idle for T
  --stdin-passthrough        non-consuming stdin detection
  --timeout-exit-code N      custom exit code on timeout

Time:
  -c, --confine MODE         'wall' (default) or 'active'
```

`wall` = real time including sleep. `active` = pauses during sleep (GNU-compatible).

**Exit codes:** 0 ok, 124 timeout, 125 error, 126 not executable, 127 not found, 128+N signal

## Development

```bash
cargo test               # 349 tests
./scripts/verify-all.sh  # + fuzz + kani proofs
```

[CONTRIBUTING.md](CONTRIBUTING.md) · [docs/VERIFICATION.md](docs/VERIFICATION.md)

## License

MIT
