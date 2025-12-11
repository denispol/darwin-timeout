# Verification Guide

Comprehensive verification using five complementary methods: unit tests, integration tests, proptest, cargo-fuzz, and kani.

## Quick Reference

```bash
# unit + integration tests
cargo test

# property-based tests
cargo test --test proptest

# fuzzing (requires nightly)
cargo +nightly fuzz run parse_duration -- -max_total_time=60

# formal verification
cargo kani

# full verification suite
./scripts/verify-all.sh
```

## Verification Stack

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         VERIFICATION PYRAMID                                │
│                            darwin-timeout                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│                              ▲                                              │
│                             ╱ ╲                                             │
│                            ╱   ╲                                            │
│                           ╱     ╲                                           │
│                          ╱ KANI  ╲         19 proofs                        │
│                         ╱ FORMAL  ╲        mathematical certainty           │
│                        ╱ PROOFS    ╲       ALL inputs (bounded)             │
│                       ╱─────────────╲                                       │
│                      ╱               ╲                                      │
│                     ╱   cargo-fuzz    ╲    4 targets, ~70M executions       │
│                    ╱   CRASH DISCOVERY ╲   random bytes → crashes           │
│                   ╱     (libFuzzer)     ╲  1 bug found, 0 crashes           │
│                  ╱───────────────────────╲                                  │
│                 ╱                         ╲                                 │
│                ╱       PROPTEST            ╲   30 properties                │
│               ╱    PROPERTY-BASED           ╲  ~7500 cases/run              │
│              ╱   invariants + generators     ╲ roundtrip, ordering          │
│             ╱─────────────────────────────────╲                             │
│            ╱                                   ╲                            │
│           ╱        INTEGRATION TESTS            ╲  tests/*.rs               │
│          ╱       real processes, signals         ╲ spawn, kill, wait        │
│         ╱        kqueue, resource limits          ╲ arm64 + x86_64          │
│        ╱───────────────────────────────────────────╲                        │
│       ╱                                             ╲                       │
│      ╱              UNIT TESTS                       ╲  #[cfg(test)]        │
│     ╱          inline module tests                    ╲ per-function        │
│    ╱           edge cases, error paths                 ╲ fast, isolated     │
│   ╱─────────────────────────────────────────────────────╲                   │
│  ╱                                                       ╲                  │
│ ╱                    STATIC ANALYSIS                      ╲ cargo clippy    │
│╱  cargo fmt, cargo audit, cargo deny, size checks (<150KB) ╲ CI gate        │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Methods Overview

| Method | Finds | Speed | Coverage |
|--------|-------|-------|----------|
| Static analysis | style, known CVEs, size | seconds | 100% code |
| Unit tests | logic errors, edge cases | seconds | targeted |
| Integration tests | process lifecycle, IPC | seconds | real syscalls |
| Proptest | invariant violations | seconds | ~7500 cases |
| cargo-fuzz | crashes, panics, hangs | minutes-hours | ~500k exec/sec |
| Kani | ALL bugs in bounded scope | minutes | mathematical proof |

## Verification Matrix

Which methods cover which modules:

| Component | Unit | Integration | Proptest | Fuzz | Kani |
|-----------|:----:|:-----------:|:--------:|:----:|:----:|
| duration.rs | ✓ | | ✓ | ✓ | |
| signal.rs | ✓ | | ✓ | ✓ | |
| args.rs | ✓ | ✓ | | ✓ | |
| rlimit.rs | ✓ | | ✓ | ✓ | |
| process.rs | ✓ | ✓ | | | ✓ |
| runner.rs | ✓ | ✓ | | | |
| sync.rs | ✓ | | | | ✓ |
| throttle.rs | ✓ | ✓ | | | ✓ |
| proc_info.rs | ✓ | | | | ✓ |
| time_math.rs | ✓ | | | | ✓ |
| wait.rs | ✓ | ✓ | | | |

---

## 1. Unit Tests

Inline `#[cfg(test)]` modules testing pure functions.

```bash
cargo test --lib
```

**Coverage targets:**
- every public function
- edge cases: zero, max, overflow, empty, negative
- error paths: invalid input, malformed data

**Example:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn parse_zero_seconds() {
        assert_eq!(parse_duration("0s").unwrap(), Duration::ZERO);
    }
    
    #[test]
    fn parse_overflow_rejected() {
        assert!(parse_duration("99999999999999999999s").is_err());
    }
}
```

---

## 2. Integration Tests

`tests/*.rs` files spawning real processes.

```bash
cargo test --test integration
```

**Coverage targets:**
- timeout behavior (wall clock, active time)
- signal forwarding (SIGTERM, SIGKILL, SIGINT)
- process groups and cleanup
- resource limits (memory, CPU)
- exit codes (124, 125, 126, 127, 128+N)

**Example:**
```rust
#[test]
fn timeout_kills_after_duration() {
    let output = Command::new("./target/release/timeout")
        .args(["0.1s", "sleep", "10"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(124));
}
```

---

## 3. Proptest (Property-Based Testing)

Generate random valid inputs, verify properties hold.

```bash
cargo test --test proptest
```

### Current Properties (30)

**Duration Parsing:**
- valid units parse correctly (s, m, h, d, ms, us, µs)
- ordering preserved: if a > b then parse(a) >= parse(b)
- fractional equivalence: 1.5s = 1500ms
- whitespace handling: " 30s " = "30s"
- case insensitivity: "30S" = "30s"
- negative values always error
- overflow detected and rejected

**Signal Parsing:**
- all Signal enum variants parse and roundtrip
- case insensitive: TERM = term = Term
- SIG prefix optional: SIGTERM = TERM
- invalid numbers (0, 32+, 65+) error
- numeric strings parse: "9" = SIGKILL

**Memory Limits:**
- valid units parse: K, M, G, T (and KB, MB, GB, TB)
- case insensitive: 1g = 1G
- overflow detected for large values

### Writing New Properties

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn my_property(input in my_strategy()) {
        let result = my_function(&input);
        prop_assert!(result.is_ok() || result.is_err());  // no panic
    }
}
```

---

## 4. cargo-fuzz (Coverage-Guided Fuzzing)

Random bytes → find crashes, panics, hangs.

### Installation

```bash
cargo install cargo-fuzz
rustup install nightly
```

### Running Fuzz Targets

```bash
# single target, 60 seconds
cargo +nightly fuzz run parse_duration -- -max_total_time=60

# all targets
for target in parse_duration parse_signal parse_args parse_mem_limit; do
    cargo +nightly fuzz run $target -- -max_total_time=60
done

# overnight fuzzing (8 hours)
cargo +nightly fuzz run parse_duration -- -max_total_time=28800
```

### Current Targets (4)

| Target | File | Purpose |
|--------|------|---------|
| parse_duration | fuzz/fuzz_targets/parse_duration.rs | duration strings |
| parse_signal | fuzz/fuzz_targets/parse_signal.rs | signal names/numbers |
| parse_args | fuzz/fuzz_targets/parse_args.rs | CLI argument parsing |
| parse_mem_limit | fuzz/fuzz_targets/parse_mem_limit.rs | memory limit strings |

### Interpreting Results

**No crashes (good):**
```
Done 30482032 runs in 61 second(s)
cov: 257 ft: 625 corp: 262/16Kb
```
- 30.5M executions at ~500k/sec
- 257 unique code paths discovered
- 262 test cases in corpus
- no artifacts/ = no crashes

**Crash found (investigate):**
```
==12345== ERROR: libFuzzer: deadly signal
artifact_prefix='./artifacts/'; Test unit written to ./artifacts/crash-abc123
```

Reproduce and debug:
```bash
cargo +nightly fuzz run parse_duration fuzz/artifacts/parse_duration/crash-abc123
cargo +nightly fuzz tmin parse_duration fuzz/artifacts/parse_duration/crash-abc123
RUST_BACKTRACE=1 cargo +nightly fuzz run parse_duration ...
```

### Corpus Management

**Seed corpus** (committed to git):
```
fuzz/corpus/parse_duration/
├── valid_seconds      # "30s"
├── valid_minutes      # "1.5m"
├── empty              # ""
└── overflow           # "99999999999999h"
```

**Expanded corpus** (gitignored):
- grows during fuzzing (200+ files after 60s)
- automatically reused on next run
- reset: `git clean -fdx fuzz/corpus/`

### Writing New Fuzz Targets

```rust
/* fuzz/fuzz_targets/my_target.rs */
#![no_main]
use libfuzzer_sys::fuzz_target;
use darwin_timeout::my_function;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = my_function(s);  // must not panic
    }
});
```

---

## 5. Kani (Formal Verification)

Mathematical proofs that properties hold for ALL inputs (within bounds).

### Installation

```bash
cargo install kani-verifier
kani setup
```

### Running Proofs

```bash
# all proofs (~2-3 minutes)
cargo kani

# single proof
cargo kani --harness verify_no_sigcont_after_mark_exited
```

### Current Proofs (19)

| Module | Proof | Property |
|--------|-------|----------|
| **sync.rs** | | |
| | verify_set_get_consistency | set value equals get value |
| | verify_set_only_once_basic | second set returns false |
| | verify_state_machine_monotonic | state only increases |
| **proc_info.rs** | | |
| | verify_aligned_buffer_8byte | buffer 8-byte aligned |
| | verify_field_offsets_in_bounds | all field reads in bounds |
| | verify_read_u64_no_panic | read_u64 never panics |
| | verify_buffer_size_sufficient | buffer fits rusage_info_v4 |
| **process.rs** | | |
| | verify_wait_only_once | wait returns Some only once |
| | verify_kill_idempotent_after_exit | kill after exit is no-op |
| | verify_exit_status_extraction | exit code extraction correct |
| | verify_signal_extraction | signal extraction correct |
| **throttle.rs** | | |
| | verify_no_sigcont_after_mark_exited | no SIGCONT to dead process |
| | verify_suspend_resume_idempotent | suspend/resume state correct |
| | verify_budget_calculation_no_overflow | CPU budget math safe |
| **time_math.rs** | | |
| | verify_elapsed_ns_none_on_backwards | backwards clock → None |
| | verify_remaining_ns_saturates_to_zero | past deadline → 0 |
| | verify_advance_ns_saturates_on_overflow | overflow → saturate |
| | verify_adjust_ns_none_on_underflow | underflow → None |
| | verify_deadline_reached_consistency | deadline logic consistent |

### Writing New Proofs

```rust
#[cfg(kani)]
mod verification {
    use super::*;
    
    #[kani::proof]
    fn verify_my_property() {
        let x: u64 = kani::any();
        let y: u64 = kani::any();
        
        kani::assume(x > 0);  // constrain input space
        
        let result = my_function(x, y);
        
        assert!(result.is_some());  // property must hold
    }
    
    #[kani::proof]
    #[kani::unwind(10)]  // for loops
    fn verify_loop_terminates() {
        let n: usize = kani::any();
        kani::assume(n < 10);
        let result = loop_function(n);
        assert!(result.len() == n);
    }
}
```

### When to Add Kani Proofs

Required for:
- new `unsafe` blocks
- state machine changes
- arithmetic that could overflow
- security-critical invariants
- PID/signal handling logic

---

## Verification Results

### Current Status (2025-12-11)

| Method | Count | Result | Executions |
|--------|-------|--------|------------|
| Unit tests | ~150 | ✓ passing | - |
| Integration | ~30 | ✓ passing | - |
| Proptest | 30 | ✓ passing | ~7500/run |
| cargo-fuzz | 4 targets | ✓ 0 crashes | ~70M total |
| Kani | 19 proofs | ✓ 19/19 | - |

### Bugs Found

| Date | Method | Target | Bug | Fix |
|------|--------|--------|-----|-----|
| 2025-12-11 | cargo-fuzz | parse_args | -V/-h accepted in clusters | args.rs:575-600 |

### Coverage Gaps

Areas not yet covered by formal verification:
- runner.rs main loop (too complex for kani)
- kqueue interactions (FFI, not verifiable)
- signal handler (async, hard to model)

Mitigated by integration tests and extensive fuzzing.

---

## CI Integration

### GitHub Actions

```yaml
name: Verification
on: [push, pull_request]

jobs:
  test:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - name: Unit and integration tests
        run: cargo test
      - name: Property tests
        run: cargo test --test proptest

  fuzz-check:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - run: rustup install nightly
      - run: cargo install cargo-fuzz
      - name: Check fuzz targets compile
        run: cd fuzz && cargo +nightly fuzz check

  kani:
    runs-on: ubuntu-latest
    if: contains(github.event.pull_request.labels.*.name, 'needs-kani')
    steps:
      - uses: actions/checkout@v4
      - run: cargo install kani-verifier && kani setup
      - run: cargo kani
```

---

## Troubleshooting

### cargo-fuzz

**"no such command: fuzz"**
```bash
cargo install cargo-fuzz
```

**"nightly required"**
```bash
rustup install nightly
cargo +nightly fuzz run target_name
```

### Kani

**"kani not found"**
```bash
cargo install kani-verifier
kani setup
```

**Proof times out**
- reduce `#[kani::unwind(N)]` bound
- add more `kani::assume()` constraints
- split into smaller proofs

### Proptest

**"too many shrink iterations"**
- simplify the property
- use smaller input ranges

---

## Size Impact

| Component | Size | Committed |
|-----------|------|-----------|
| fuzz/Cargo.toml | 1KB | ✓ |
| fuzz/fuzz_targets/*.rs | 4KB | ✓ |
| fuzz/corpus/*/ (seeds) | 2KB | ✓ |
| fuzz/corpus/*/ (expanded) | 50KB+ | ✗ |
| tests/proptest.rs | 8KB | ✓ |
| kani proofs (inline) | 3KB | ✓ |

Total committed: ~18KB
