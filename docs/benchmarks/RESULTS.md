# Benchmark Results

Raw benchmark data for darwin-timeout performance claims.

## Test Environment

- **Date**: 2025-12-08
- **Machine**: MacBook Pro (16-inch, 2024)
- **CPU**: Apple M4 Pro (14 cores @ 4.5GHz)
- **Memory**: 24GB
- **OS**: macOS Tahoe 26.2
- **Kernel**: Darwin 25.2.0 (arm64)
- **Tool**: hyperfine 1.20.0

Full machine specs: [machine_specs.json](machine_specs.json)

## Startup Overhead

**5 benchmark sessions × 50 runs each = 250 total measurements**

| Binary | Mean | Std Dev | Runs |
|--------|------|---------|------|
| darwin-timeout | 3.6ms | ±0.2ms | 250 |
| GNU timeout | 4.2ms | ±0.2ms | 250 |

**darwin-timeout is 18% faster** (1.18x)

Raw data:
- [run1_startup.json](run1_startup.json)
- [run2_startup.json](run2_startup.json)
- [run3_startup.json](run3_startup.json)
- [run4_startup.json](run4_startup.json)
- [run5_startup.json](run5_startup.json)

## Timeout Precision (1 second)

**20 runs**

| Binary | Mean | Std Dev |
|--------|------|---------|
| darwin-timeout | 1.014s | ±0.003s |
| GNU timeout | 1.017s | ±0.001s |

Both implementations are equally precise.

Raw data: [precision_1s.json](precision_1s.json)

## Feature Overhead

### JSON Output (--json flag)

**50 runs**

| Mode | Mean | Overhead |
|------|------|----------|
| Plain | 3.6ms | - |
| JSON | 3.6ms | 0% |

Raw data: [json_overhead.json](json_overhead.json)

### Retry Flag (--retry 3)

**50 runs** (command succeeds, no retry triggered)

| Mode | Mean | Overhead |
|------|------|----------|
| No retry | 3.6ms | - |
| With --retry 3 | 3.6ms | 0% |

Raw data: [retry_overhead.json](retry_overhead.json)

## Binary Size

```
darwin-timeout: 100KB (101984 bytes)
GNU coreutils:  15.7MB
Ratio:          157x smaller
```

## Reproducing

```bash
# Install hyperfine
brew install hyperfine

# Build release binary
cargo build --release

# Run benchmarks
hyperfine --warmup 10 -N --runs 50 \
    -n "darwin-timeout" "./target/release/timeout 1 true" \
    -n "GNU timeout" "gtimeout 1 true"
```
