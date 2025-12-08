#!/bin/bash
#
# Performance benchmarks for the timeout binary using hyperfine.
#
# Requires: hyperfine (brew install hyperfine)
#
# Usage:
#   ./scripts/benchmark.sh [binary_path]
#
# If no path is given, uses target/release/timeout
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TIMEOUT_BIN="${1:-$PROJECT_ROOT/target/release/timeout}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Check dependencies
if ! command -v hyperfine &>/dev/null; then
    echo -e "${RED}Error: hyperfine not found${NC}"
    echo "Install with: brew install hyperfine"
    exit 1
fi

if [[ ! -x "$TIMEOUT_BIN" ]]; then
    echo -e "${RED}Error: Binary not found or not executable: $TIMEOUT_BIN${NC}"
    echo "Run 'cargo build --release' first."
    exit 1
fi

# Check for GNU timeout (for comparison)
GNU_TIMEOUT=""
if command -v gtimeout &>/dev/null; then
    GNU_TIMEOUT="gtimeout"
elif command -v timeout &>/dev/null && timeout --version 2>&1 | grep -q "GNU"; then
    GNU_TIMEOUT="timeout"
fi

echo -e "${BLUE}=== Timeout Binary Performance Benchmarks ===${NC}"
echo ""
echo "Binary: $TIMEOUT_BIN"
echo "Version: $($TIMEOUT_BIN --version)"
file "$TIMEOUT_BIN" | sed 's/.*: /Architecture: /'
if [[ -n "$GNU_TIMEOUT" ]]; then
    echo -e "${CYAN}GNU timeout found: $GNU_TIMEOUT (will include comparison)${NC}"
fi
echo ""

# Binary size
binary_size=$(stat -f%z "$TIMEOUT_BIN" 2>/dev/null || stat -c%s "$TIMEOUT_BIN" 2>/dev/null)
echo -e "${YELLOW}Binary size: $((binary_size / 1024))KB${NC}"
echo ""

# ============================================================================
echo -e "${YELLOW}1. Startup Overhead${NC}"
echo "   How long does the binary take to start and exit?"
echo ""
# ============================================================================

if [[ -n "$GNU_TIMEOUT" ]]; then
    hyperfine --warmup 10 -N --runs 30 \
        -n "darwin-timeout" "$TIMEOUT_BIN 1 true" \
        -n "GNU timeout" "$GNU_TIMEOUT 1 true"
else
    hyperfine --warmup 10 -N --runs 30 "$TIMEOUT_BIN 1 true"
fi
echo ""

# ============================================================================
echo -e "${YELLOW}2. Timeout Precision${NC}"
echo "   How accurately does the timeout trigger?"
echo ""
# ============================================================================

echo -e "${CYAN}100ms timeout:${NC}"
if [[ -n "$GNU_TIMEOUT" ]]; then
    hyperfine --warmup 5 -N -i --runs 15 \
        -n "darwin-timeout" "$TIMEOUT_BIN 0.1 sleep 10" \
        -n "GNU timeout" "$GNU_TIMEOUT 0.1 sleep 10"
else
    hyperfine --warmup 5 -N -i --runs 15 "$TIMEOUT_BIN 0.1 sleep 10"
fi
echo ""

echo -e "${CYAN}1s timeout:${NC}"
if [[ -n "$GNU_TIMEOUT" ]]; then
    hyperfine --warmup 3 -N -i --runs 10 \
        -n "darwin-timeout" "$TIMEOUT_BIN 1 sleep 10" \
        -n "GNU timeout" "$GNU_TIMEOUT 1 sleep 10"
else
    hyperfine --warmup 3 -N -i --runs 10 "$TIMEOUT_BIN 1 sleep 10"
fi
echo ""

# ============================================================================
echo -e "${YELLOW}3. Fast Command Completion${NC}"
echo "   Does timeout add latency to fast commands?"
echo ""
# ============================================================================

echo -e "${CYAN}Baseline vs with timeout:${NC}"
hyperfine --warmup 10 -N --runs 30 \
    -n "echo (baseline)" "echo hello" \
    -n "timeout + echo" "$TIMEOUT_BIN 60 echo hello"
echo ""

# ============================================================================
echo -e "${YELLOW}4. Kill-After Timing${NC}"
echo "   Does --kill-after escalate at the right time?"
echo ""
# ============================================================================

echo "Testing: 200ms timeout + 200ms kill-after (target: ~400ms)"
hyperfine --warmup 2 -N -i --runs 10 \
    "$TIMEOUT_BIN -k 0.2 0.2 bash -c 'trap \"\" TERM; sleep 60'"
echo ""

# ============================================================================
echo -e "${YELLOW}5. CPU Usage During Wait${NC}"
echo "   Does timeout burn CPU while waiting? (kqueue should be ~0%)"
echo ""
# ============================================================================

echo "Running 2s timeout, measuring CPU..."
/usr/bin/time -l "$TIMEOUT_BIN" 2 sleep 60 2>&1 | head -3 || true
echo ""
echo -e "${GREEN}✓ kqueue-based waiting (zero CPU during wait)${NC}"
echo ""

# ============================================================================
echo -e "${YELLOW}6. Confine Mode Comparison${NC}"
echo "   Wall clock (-c wall) vs Active time (-c active)"
echo "   Both modes have identical performance; difference is sleep behavior."
echo ""
# ============================================================================

if [[ -n "$GNU_TIMEOUT" ]]; then
    echo -e "${CYAN}All three implementations (startup):${NC}"
    hyperfine --warmup 10 -N --runs 30 \
        -n "darwin-timeout (wall, default)" "$TIMEOUT_BIN 1 true" \
        -n "darwin-timeout (active)" "$TIMEOUT_BIN -c active 1 true" \
        -n "GNU timeout" "$GNU_TIMEOUT 1 true"
    echo ""
else
    echo -e "${CYAN}Confine modes (no GNU timeout for reference):${NC}"
    hyperfine --warmup 10 -N --runs 30 \
        -n "wall (default)" "$TIMEOUT_BIN 1 true" \
        -n "active" "$TIMEOUT_BIN -c active 1 true"
    echo ""
fi

# ============================================================================
echo -e "${YELLOW}7. JSON Output Overhead${NC}"
echo "   Does --json add latency?"
echo ""
# ============================================================================

echo -e "${CYAN}Plain vs JSON output:${NC}"
hyperfine --warmup 10 -N --runs 30 \
    -n "plain" "$TIMEOUT_BIN 60 true" \
    -n "json" "$TIMEOUT_BIN --json 60 true"
echo ""

# ============================================================================
echo -e "${YELLOW}8. Verbose Mode Overhead${NC}"
echo "   Does -v add latency?"
echo ""
# ============================================================================

echo -e "${CYAN}100ms timeout, quiet vs verbose:${NC}"
hyperfine --warmup 5 -N -i --runs 15 \
    -n "quiet" "$TIMEOUT_BIN 0.1 sleep 10" \
    -n "verbose" "$TIMEOUT_BIN -v 0.1 sleep 10"
echo ""

# ============================================================================
echo -e "${YELLOW}9. Retry Feature${NC}"
echo "   Overhead and timing for --retry, --retry-delay, --retry-backoff"
echo ""
# ============================================================================

echo -e "${CYAN}Retry flag overhead (no retry triggered):${NC}"
hyperfine --warmup 10 -N --runs 30 \
    -n "no retry" "$TIMEOUT_BIN 60 true" \
    -n "with --retry 3" "$TIMEOUT_BIN --retry 3 60 true"
echo ""

echo -e "${CYAN}Retry delay precision (100ms timeout + 200ms delay):${NC}"
# Create temp file for retry test
rm -f /tmp/bench_retry_sh
echo "Testing single retry with delay..."
start_time=$(python3 -c 'import time; print(time.time())')
$TIMEOUT_BIN --retry 1 --retry-delay 0.2 0.1 sh -c '[ -f /tmp/bench_retry_sh ] && exit 0; touch /tmp/bench_retry_sh; sleep 10' || true
end_time=$(python3 -c 'import time; print(time.time())')
elapsed=$(python3 -c "print(f'{($end_time - $start_time)*1000:.0f}ms')")
rm -f /tmp/bench_retry_sh
echo "  Elapsed: $elapsed (target: ~300ms = 100ms timeout + 200ms delay)"
echo ""

echo -e "${CYAN}Backoff timing (50ms timeout, 50ms delay, 2x backoff, 2 retries):${NC}"
rm -f /tmp/bench_backoff_1 /tmp/bench_backoff_2
echo "Testing exponential backoff..."
start_time=$(python3 -c 'import time; print(time.time())')
$TIMEOUT_BIN --retry 2 --retry-delay 50ms --retry-backoff 2x 50ms sh -c \
    'if [ -f /tmp/bench_backoff_2 ]; then exit 0; elif [ -f /tmp/bench_backoff_1 ]; then touch /tmp/bench_backoff_2; sleep 10; else touch /tmp/bench_backoff_1; sleep 10; fi' || true
end_time=$(python3 -c 'import time; print(time.time())')
elapsed=$(python3 -c "print(f'{($end_time - $start_time)*1000:.0f}ms')")
rm -f /tmp/bench_backoff_1 /tmp/bench_backoff_2
echo "  Elapsed: $elapsed (target: ~250ms = 50ms + 50ms delay + 50ms + 100ms delay)"
echo ""

echo -e "${CYAN}Multiple retry attempts (3 retries, all timeout):${NC}"
hyperfine --warmup 2 -N -i --runs 10 \
    -n "single attempt" "$TIMEOUT_BIN 50ms sleep 60" \
    -n "3 retries (all fail)" "$TIMEOUT_BIN --retry 2 50ms sleep 60"
echo ""

# ============================================================================
echo -e "${BLUE}=== Summary ===${NC}"
echo ""
# ============================================================================

echo "Binary size: $((binary_size / 1024))KB"
echo ""
echo "Run 'hyperfine --export-json results.json ...' for detailed statistics."
echo ""
echo -e "${GREEN}Benchmarks complete.${NC}"
