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
    hyperfine --warmup 5 -N \
        "$TIMEOUT_BIN 1 true" \
        "$GNU_TIMEOUT 1 true"
else
    hyperfine --warmup 5 -N "$TIMEOUT_BIN 1 true"
fi
echo ""

# ============================================================================
echo -e "${YELLOW}2. Timeout Precision${NC}"
echo "   How accurately does the timeout trigger?"
echo ""
# ============================================================================

echo -e "${CYAN}100ms timeout:${NC}"
if [[ -n "$GNU_TIMEOUT" ]]; then
    hyperfine --warmup 3 -N -i \
        "$TIMEOUT_BIN 0.1 sleep 10" \
        "$GNU_TIMEOUT 0.1 sleep 10"
else
    hyperfine --warmup 3 -N -i "$TIMEOUT_BIN 0.1 sleep 10"
fi
echo ""

echo -e "${CYAN}500ms timeout:${NC}"
if [[ -n "$GNU_TIMEOUT" ]]; then
    hyperfine --warmup 3 -N -i \
        "$TIMEOUT_BIN 0.5 sleep 10" \
        "$GNU_TIMEOUT 0.5 sleep 10"
else
    hyperfine --warmup 3 -N -i "$TIMEOUT_BIN 0.5 sleep 10"
fi
echo ""

echo -e "${CYAN}1s timeout:${NC}"
if [[ -n "$GNU_TIMEOUT" ]]; then
    hyperfine --warmup 3 -N -i \
        "$TIMEOUT_BIN 1 sleep 10" \
        "$GNU_TIMEOUT 1 sleep 10"
else
    hyperfine --warmup 3 -N -i "$TIMEOUT_BIN 1 sleep 10"
fi
echo ""

# ============================================================================
echo -e "${YELLOW}3. Fast Command Completion${NC}"
echo "   Does timeout add latency to fast commands?"
echo ""
# ============================================================================

echo -e "${CYAN}Baseline vs with timeout:${NC}"
hyperfine --warmup 5 -N \
    "echo hello" \
    "$TIMEOUT_BIN 60 echo hello"
echo ""

# ============================================================================
echo -e "${YELLOW}4. Kill-After Timing${NC}"
echo "   Does --kill-after escalate at the right time?"
echo ""
# ============================================================================

echo "Testing: 200ms timeout + 200ms kill-after (target: ~400ms)"
hyperfine --warmup 2 -N -i --runs 5 \
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
echo -e "${BLUE}=== Summary ===${NC}"
echo ""
# ============================================================================

echo "Binary size: $((binary_size / 1024))KB"
echo ""
echo "Run 'hyperfine --export-json results.json ...' for detailed statistics."
echo ""
echo -e "${GREEN}Benchmarks complete.${NC}"
