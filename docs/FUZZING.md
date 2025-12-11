# Fuzzing darwin-timeout

comprehensive fuzzing setup using cargo-fuzz (libFuzzer) to catch bugs in input parsing and state machines.

## Quick Start

```bash
# install cargo-fuzz (requires nightly)
cargo install cargo-fuzz

# run a specific fuzz target for 60 seconds
cd fuzz
cargo +nightly fuzz run parse_duration -- -max_total_time=60

# run all targets
cargo +nightly fuzz run parse_duration -- -max_total_time=60
cargo +nightly fuzz run parse_signal -- -max_total_time=60
cargo +nightly fuzz run parse_args -- -max_total_time=60
cargo +nightly fuzz run parse_mem_limit -- -max_total_time=60
```

## Targets

### parse_duration
fuzzes duration string parsing ("30s", "1.5m", etc) - validates no crashes on malformed inputs, unicode, overflow attempts.

### parse_signal
fuzzes signal name/number parsing ("TERM", "9", "SIGKILL") - tests numeric edge cases, invalid names, case sensitivity.

### parse_args
fuzzes full CLI argument parsing - tests complex flag combinations, edge cases in option ordering, missing values.

### parse_mem_limit
fuzzes memory limit parsing ("100M", "1.5G", etc) - validates unit handling, overflow protection, malformed units.

## Interpreting Results

### No Crashes (Good!)
```
Done 30482032 runs in 61 second(s)
cov: 257 ft: 625 corp: 262/16Kb
```
- **30.5M executions** at ~500k/sec
- **257 unique code paths** discovered
- **262 test cases** in corpus (grew from seeds)
- **No artifacts/**: no crashes found

### Crash Found (Investigate!)
```
==12345== ERROR: libFuzzer: deadly signal
#123456 ...
artifact_prefix='./artifacts/'; Test unit written to ./artifacts/crash-abc123
```
- **crash file** saved in `fuzz/artifacts/target_name/crash-*`
- reproduce: `cargo +nightly fuzz run target_name artifacts/target_name/crash-abc123`
- debug: add `println!` or run under lldb

## Corpus Management

### Seed Corpus
handwritten test cases in `fuzz/corpus/target_name/`:
```
fuzz/corpus/parse_duration/
  valid_seconds    # "30s"
  valid_minutes    # "1.5m"
  empty            # ""
  overflow         # "99999999h"
```

**committed to git** - these are the baseline tests.

### Expanded Corpus
fuzzer-discovered test cases - grows during fuzzing:
```
fuzz/corpus/parse_duration/
  a1b2c3d4e5f6...  # 262 files after 60s fuzzing
```

**not committed** - ignored in .gitignore to avoid repo bloat.

to reset to seeds only:
```bash
cd fuzz/corpus/parse_duration
ls | grep -v "^valid_\|^invalid_\|^empty\|^overflow" | xargs rm
```

## Continuous Fuzzing

### Local (Overnight)
```bash
# 8 hours of fuzzing
cargo +nightly fuzz run parse_duration -- -max_total_time=28800
```

### CI Integration
`.github/workflows/verify.yml` checks that fuzz targets compile:
```yaml
- name: Check fuzz targets compile
  run: |
    cd fuzz
    cargo +nightly fuzz check
```

for actual fuzzing in CI, use OSS-Fuzz or dedicated fuzzing infrastructure.

## Advanced Usage

### Minimize Crash
shrink a crash file to smallest reproducer:
```bash
cargo +nightly fuzz cmin parse_duration
```

### Coverage Report
```bash
cargo +nightly fuzz coverage parse_duration
cargo cov -- show target/coverage/parse_duration \
    --format=html > coverage.html
```

### Dictionary Hints
fuzzer auto-discovers useful byte patterns:
```
"\000\000\000/" # null bytes + slash
"\011\000\000\000" # tab patterns
```

these are saved in fuzzer output and reused automatically.

## Size Impact

each fuzz target adds ~5-10KB to repository:
- `fuzz/Cargo.toml`: 1KB
- `fuzz/fuzz_targets/*.rs`: 1-2KB each
- seed corpus: ~500 bytes per target

expanded corpus (not committed): 270+ files per target after fuzzing.

## Troubleshooting

### "no such command: fuzz"
```bash
# install cargo-fuzz (not "fuzz")
cargo install cargo-fuzz
```

### "nightly required"
```bash
rustup install nightly
# or use current nightly
cargo +nightly fuzz run target_name
```

### Target exits immediately
check for panics in fuzz target:
```bash
RUST_BACKTRACE=1 cargo +nightly fuzz run target_name
```

## Bug Discoveries

### 2025-12-11: Bundled Flag Parser Accepts -V/-h (Fixed)

**Target**: parse_args  
**Input**: `-V--i2` (6 bytes: `2d 56 2d 2d 69 32`)  
**Executions**: Found at run #5364 (~1 second of fuzzing)

**Bug**: The bundled short option parser (lines 575-600 in `src/args.rs`) treated `-V` and `-h` as valid cluster members. Input `-V--i2` entered bundled parsing, matched `b'V'` at position 1, and called `exit(0)` inappropriately.

**Expected**: `-V` and `--version` should only work standalone, not within option clusters like `-Vfp` or malformed strings like `-V--i2`.

**Fix**: Added length check - if bytes.len() != 2, return error: "-V must be used alone, not in a cluster". Same for `-h`.

**Impact**: Real user-facing bug - malformed input caused exit instead of error. Fuzzing found this in under 2 seconds with 5364 executions.

**Regression Tests**: `test_version_short_flag_must_be_standalone`, `test_help_short_flag_must_be_standalone` in `tests/integration.rs`

**Artifacts**: Saved locally, not committed.
