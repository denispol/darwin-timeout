#!/bin/bash
# verify-all.sh - run full verification suite
#
# usage: ./scripts/verify-all.sh [--quick]
#   --quick: skip fuzzing and kani (CI mode)

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

QUICK=false
if [[ "$1" == "--quick" ]]; then
    QUICK=true
fi

echo "=== darwin-timeout verification suite ==="
echo ""

# static analysis
echo -e "${YELLOW}[1/7] cargo fmt --check${NC}"
cargo fmt --check
echo -e "${GREEN}✓ formatting ok${NC}"
echo ""

echo -e "${YELLOW}[2/7] cargo clippy${NC}"
cargo clippy -- -D warnings
echo -e "${GREEN}✓ clippy ok${NC}"
echo ""

# unit tests
echo -e "${YELLOW}[3/7] cargo test --lib${NC}"
cargo test --lib
echo -e "${GREEN}✓ unit tests ok${NC}"
echo ""

# integration tests
echo -e "${YELLOW}[4/7] cargo test --test integration${NC}"
cargo test --test integration
echo -e "${GREEN}✓ integration tests ok${NC}"
echo ""

# proptest
echo -e "${YELLOW}[5/7] cargo test --test proptest${NC}"
cargo test --test proptest
echo -e "${GREEN}✓ proptest ok (30 properties)${NC}"
echo ""

# binary size check
echo -e "${YELLOW}[6/7] binary size check${NC}"
cargo build --release
SIZE=$(stat -f%z target/release/timeout 2>/dev/null || stat -c%s target/release/timeout)
MAX_SIZE=153600  # 150KB
if [ "$SIZE" -le "$MAX_SIZE" ]; then
    echo -e "${GREEN}✓ binary size ok: ${SIZE} bytes (limit: ${MAX_SIZE})${NC}"
else
    echo -e "${RED}✗ binary too large: ${SIZE} bytes (limit: ${MAX_SIZE})${NC}"
    exit 1
fi
echo ""

if [ "$QUICK" = true ]; then
    echo -e "${YELLOW}[7/7] skipping fuzz/kani (--quick mode)${NC}"
    echo ""
    echo -e "${GREEN}=== quick verification passed ===${NC}"
    exit 0
fi

# fuzz target check (compile only)
echo -e "${YELLOW}[7a/7] fuzz targets compile check${NC}"
if command -v cargo-fuzz &> /dev/null; then
    cd fuzz && cargo +nightly fuzz check && cd ..
    echo -e "${GREEN}✓ fuzz targets compile${NC}"
else
    echo -e "${YELLOW}⚠ cargo-fuzz not installed, skipping${NC}"
fi
echo ""

# kani proofs
echo -e "${YELLOW}[7b/7] kani formal verification${NC}"
if command -v kani &> /dev/null; then
    cargo kani
    echo -e "${GREEN}✓ all 19 kani proofs pass${NC}"
else
    echo -e "${YELLOW}⚠ kani not installed, skipping${NC}"
    echo "  install: cargo install kani-verifier && kani setup"
fi
echo ""

echo -e "${GREEN}=== full verification passed ===${NC}"
