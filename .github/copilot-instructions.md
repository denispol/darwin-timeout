# Copilot Instructions for darwin-timeout

## Project Overview

darwin-timeout is a native macOS replacement for GNU `timeout`. single binary, ~500KB, zero dependencies at runtime.

## Code Style

- rust edition 2024, requires 1.90+
- lowercase comments, no fluff. explain *why*, not *what*
- block comments use `/* */` style for multi-line explanations
- short inline comments use `//`
- no doc comments on private functions unless complex
- prefer early returns over deep nesting

## Commit Style

conventional commits, lowercase, no period at end:

```
type(scope): short description

longer explanation if needed. lowercase prose, no bullets unless
listing multiple independent items. explain the *why*.
```

types: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `chore`

## Architecture

- `src/args.rs` - clap derive for CLI parsing
- `src/duration.rs` - GNU-compatible duration parsing (30s, 1.5m, 2h, 1d)
- `src/signal.rs` - signal name/number parsing
- `src/error.rs` - error types with GNU-compatible exit codes
- `src/runner.rs` - core timeout logic using kqueue + mach_continuous_time
- `src/main.rs` - entry point, arg resolution, JSON output

## Testing

- unit tests in each module
- integration tests in `tests/integration.rs` - must pass on both arm64 and x86_64
- benchmark tests in `tests/benchmarks.rs`
- run `cargo test --release` for full suite
- pre-commit hook runs fmt + clippy only (CI runs full tests)

## Dependencies

minimal deps policy. current:
- `clap` (CLI parsing, derive features)
- `clap_complete` (shell completions)
- `nix` (signal handling)
- `libc` (kqueue, mach APIs)

avoid adding new deps unless absolutely necessary. prefer manual impls over macro-heavy crates (we dropped thiserror for this reason).

## GNU Compatibility

exit codes must match GNU timeout exactly:
- 124: timed out
- 125: timeout itself failed
- 126: command not executable
- 127: command not found
- 137: killed by SIGKILL (128+9)

scripts written for GNU timeout should work unchanged.

## Platform

macOS only. uses darwin-specific APIs:
- `mach_continuous_time` for sleep-resilient timing
- `kqueue` for zero-CPU waiting
- process groups for signal delivery to children
