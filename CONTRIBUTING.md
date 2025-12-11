# Contributing to darwin-timeout

Thank you for your interest in contributing! This guide covers the development workflow and verification requirements.

## Quick Start

```bash
# clone and build
git clone https://github.com/denispol/darwin-timeout.git
cd darwin-timeout
cargo build --release

# run tests
cargo test

# check binary size (must be ≤150KB)
ls -la target/release/timeout
```

## Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Rust stable | 1.91+ | main development |
| Rust nightly | latest | cargo-fuzz |
| cargo-fuzz | 0.13.1 | fuzzing |
| kani-verifier | 0.66+ | formal verification |

### Installing Tools

```bash
# rust toolchains
rustup install stable nightly

# cargo-fuzz
cargo install cargo-fuzz

# kani (optional, for formal verification)
cargo install kani-verifier
kani setup
```

## Development Workflow

1. **Fork** the repository
2. **Create a branch** for your feature/fix
3. **Write code** following the style in existing files
4. **Add tests** (see verification requirements below)
5. **Run verification** to ensure nothing broke
6. **Submit PR** with clear description

### Code Style

- `/* */` for inline, `//` for end-of-line
- no floats (they bloat binary by ~8KB)
- no std except in tests

## Verification Requirements

This project uses a multi-layered verification approach. Different changes require different levels of testing.

### CI Auto-Verification Rules

CI automatically triggers extra verification based on which files you change. **If you add a new safety-critical module or parser, you must add it to the CI path filters.**

| File Changed | Auto-Triggered CI Jobs |
|--------------|----------------------|
| `src/sync.rs` | kani (19 proofs) |
| `src/process.rs` | kani |
| `src/throttle.rs` | kani |
| `src/proc_info.rs` | kani |
| `src/time_math.rs` | kani |
| `src/duration.rs` | fuzz (4×60s) |
| `src/signal.rs` | fuzz |
| `src/args.rs` | fuzz |
| `src/rlimit.rs` | fuzz |
| `fuzz/**/*.rs` | fuzz |
| Any `src/*.rs` | miri, fuzz-check |

**Always runs (every PR):**
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo audit` (security)
- `cargo test --lib` (152 unit tests)
- `cargo test --test integration` (167 tests)
- `cargo test --test proptest` (30 properties)
- Binary size check (≤150KB)
- Symbol count check (≤100)

**Path-triggered (automatic):**
- **Kani proofs**: When safety-critical files change (sync, process, throttle, proc_info, time_math)
- **Fuzz execution**: When parsing files change (duration, signal, args, rlimit)
- **Miri UB detection**: Always runs on any Rust file change

> ⚠️ **Adding new modules**: If you add a new module with `unsafe` code or state machines, add it to `.github/workflows/verify.yml` kani paths. If you add a new parser, add it to the fuzz paths.

### Verification Pyramid

```
                      ▲
                     ╱ ╲
                    ╱   ╲
                   ╱KANI ╲        19 proofs
                  ╱PROOFS ╲       mathematical certainty
                 ╱─────────╲
                ╱           ╲
               ╱ cargo-fuzz  ╲    4 targets, ~70M executions
              ╱───────────────╲
             ╱                 ╲
            ╱    PROPTEST       ╲  30 properties
           ╱─────────────────────╲
          ╱                       ╲
         ╱   INTEGRATION TESTS     ╲  real processes, signals
        ╱───────────────────────────╲
       ╱                             ╲
      ╱         UNIT TESTS            ╲  inline #[cfg(test)]
     ╱─────────────────────────────────╲
    ╱                                   ╲
   ╱          STATIC ANALYSIS            ╲  clippy, fmt, audit
  ╱───────────────────────────────────────╲
```

### What Tests to Add

| Change Type | Unit | Integration | Proptest | Fuzz | Kani |
|-------------|:----:|:-----------:|:--------:|:----:|:----:|
| New parsing function | ✓ | | ✓ | ✓ | |
| Process/signal handling | ✓ | ✓ | | | maybe |
| Unsafe blocks | ✓ | | | | ✓ |
| State machines | ✓ | ✓ | | | ✓ |
| Arithmetic operations | ✓ | | | | ✓ |
| CLI flags | ✓ | ✓ | | ✓ | |
| Bug fixes | ✓ regression | ✓ if process | | | |

### Running Verification

```bash
# minimum (always required)
cargo test
cargo clippy -- -D warnings
cargo fmt --check

# if changing parsers
cargo test --test proptest
cargo +nightly fuzz run <target> -- -max_total_time=60

# if changing unsafe/state machines
cargo kani

# full suite
./scripts/verify-all.sh
```

## PR Checklist

Before submitting:

```
[ ] cargo test passes
[ ] cargo clippy -- -D warnings passes
[ ] cargo fmt --check passes
[ ] Binary size ≤150KB (cargo build --release && ls -la target/release/timeout)
[ ] Added tests for new functionality
[ ] Updated docs if user-facing change
[ ] Commit messages are clear and descriptive
```

### Additional checks for specific changes:

**Parser changes:**
```
[ ] Added proptest properties
[ ] Ran fuzz target for 60+ seconds
```

**Unsafe/state machine changes:**
```
[ ] Added or updated kani proofs
[ ] cargo kani passes
```

**New features:**
```
[ ] Updated README.md if user-facing
[ ] Added integration test
```

## Module Overview

Understanding the codebase:

```
src/
├── main.rs       # entry point, arg handling, json output
├── runner.rs     # timeout logic, kqueue, signal forwarding
├── process.rs    # posix_spawn wrapper, RawChild
├── args.rs       # CLI parsing (no clap - too heavy)
├── duration.rs   # parse "30s", "1.5m" without floats
├── signal.rs     # POSIX signals parsing
├── error.rs      # TimeoutError enum, exit codes
├── rlimit.rs     # resource limit parsing
├── throttle.rs   # CPU throttling via SIGSTOP/SIGCONT
├── proc_info.rs  # darwin libproc API
├── time_math.rs  # checked integer time calculations
├── wait.rs       # --wait-for-file polling
├── sync.rs       # AtomicOnce for signal pipe
├── io.rs         # no_std print macros
├── panic.rs      # just abort, no formatting
└── allocator.rs  # thin libc malloc wrapper
```

## Binary Size Budget

Target: **≤150KB** release binary

Current: ~118KB

Every byte matters. Before adding dependencies or features, consider size impact:

```bash
# check size
cargo build --release && ls -la target/release/timeout

# find what's bloating
cargo llvm-lines --release --bin timeout | head -50

# common culprits
- floats (f64::from_str adds ~8KB)
- derive(Debug) on Duration types
- unnecessary error messages
```

## Exit Codes

Follow GNU timeout conventions:

| Code | Meaning |
|------|---------|
| 0-123 | Command exit code |
| 124 | Timeout occurred |
| 125 | Internal error |
| 126 | Command not executable |
| 127 | Command not found |
| 128+N | Command killed by signal N |

## Verification Details

See [docs/VERIFICATION.md](docs/VERIFICATION.md) for comprehensive documentation on:
- All 19 kani proofs
- All 4 fuzz targets
- All 30 proptest properties
- How to write new tests

## PR Format

Use this format for all pull requests:

```
## What
[one line: what this PR does]

## Why
[one line: why this change is needed]

## Changes
- [bullet list of changes made]

## Verification
- [ ] cargo test
- [ ] cargo clippy
- [ ] [other checks as needed]
```

Example:
```
## What
Fix -V flag accepted in option clusters

## Why
Fuzzing found -V--i2 called exit(0) instead of returning error

## Changes
- args.rs: add length check for -V/-h in bundled parser
- integration.rs: add regression tests

## Verification
- [x] cargo test
- [x] cargo clippy
- [x] cargo +nightly fuzz run parse_args (60s, 0 crashes)
```

## Getting Help

- Open an issue for bugs or feature requests
- Check existing issues before creating new ones
- For security issues, use GitHub Security Advisories

## License

By contributing, you agree that your contributions will be licensed under MIT.
