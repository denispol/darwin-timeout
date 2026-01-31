#!/bin/bash
#
# Build a universal (fat) binary for macOS.
#
# This creates a single binary that runs natively on both Apple Silicon
# (ARM64) and Intel (x86_64) Macs. The system automatically picks the
# right architecture at runtime.
#
# Prerequisites:
#   - Rust toolchain with both targets installed:
#     rustup target add aarch64-apple-darwin x86_64-apple-darwin
#   - For minimal builds: nightly toolchain + rust-src component
#
# Usage:
#   ./scripts/build-universal.sh          # stable build (~730KB universal)
#   ./scripts/build-universal.sh --minimal # nightly build-std (~~100KB universal)
#
# Output:
#   target/universal/procguard - The universal binary
#   target/universal/timeout   - Symlink to procguard
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

# Check for --minimal flag
MINIMAL=false
if [[ "${1:-}" == "--minimal" ]]; then
    MINIMAL=true
fi

echo "=== Building procguard universal binary ==="
if $MINIMAL; then
    echo "    Mode: minimal (nightly + build-std)"
else
    echo "    Mode: stable"
fi
echo ""

# Ensure both toolchains are installed
echo "Checking Rust targets..."
if ! rustup target list --installed | grep -q "aarch64-apple-darwin"; then
    echo "Installing aarch64-apple-darwin target..."
    rustup target add aarch64-apple-darwin
fi

if ! rustup target list --installed | grep -q "x86_64-apple-darwin"; then
    echo "Installing x86_64-apple-darwin target..."
    rustup target add x86_64-apple-darwin
fi

if $MINIMAL; then
    # Check nightly + rust-src for build-std
    if ! rustup run nightly rustc --version &>/dev/null; then
        echo "Installing nightly toolchain..."
        rustup install nightly
    fi
    if ! rustup component list --toolchain nightly | grep -q "rust-src (installed)"; then
        echo "Installing rust-src component..."
        rustup component add rust-src --toolchain nightly
    fi
fi

echo ""

# Build flags for minimal mode
# -Zlocation-detail=none: remove file/line info from panics
# -Zfmt-debug=none: remove Debug formatting code
# -Cpanic=immediate-abort: skip panic formatting entirely (requires -Zunstable-options)
# build-std: rebuild libstd optimized for size
# optimize_for_size: use smaller algorithms in libstd
if $MINIMAL; then
    RUSTFLAGS="-Zlocation-detail=none -Zfmt-debug=none -Zunstable-options -Cpanic=immediate-abort"
    BUILD_STD_FLAGS="-Z build-std=std,panic_abort -Z build-std-features=optimize_for_size"
    CARGO="cargo +nightly"
else
    RUSTFLAGS=""
    BUILD_STD_FLAGS=""
    CARGO="cargo"
fi

# Build for ARM64 (Apple Silicon)
echo "Building for aarch64-apple-darwin (Apple Silicon)..."
RUSTFLAGS="$RUSTFLAGS" $CARGO build --release $BUILD_STD_FLAGS --target aarch64-apple-darwin

# Build for x86_64 (Intel)
echo "Building for x86_64-apple-darwin (Intel)..."
RUSTFLAGS="$RUSTFLAGS" $CARGO build --release $BUILD_STD_FLAGS --target x86_64-apple-darwin

echo ""

# Create universal binary directory
mkdir -p target/universal

# Combine with lipo
echo "Creating universal binary with lipo..."
lipo -create \
    target/aarch64-apple-darwin/release/procguard \
    target/x86_64-apple-darwin/release/procguard \
    -output target/universal/procguard

# aggressive strip: -x removes local symbols, -S removes debug symbols
# (release profile already strips, this catches anything lipo preserved)
strip -x -S target/universal/procguard 2>/dev/null || true

# Optional: ad-hoc codesign for faster first launch
echo "Signing binary..."
codesign -s - target/universal/procguard 2>/dev/null || true

# Create timeout symlink for GNU compatibility
ln -sf procguard target/universal/timeout

echo ""
echo "=== Build complete ==="
echo ""

# Show results
echo "Binary info:"
file target/universal/procguard
echo ""

echo "Size:"
ls -lh target/universal/procguard
echo ""

echo "Architectures:"
lipo -info target/universal/procguard
echo ""

# Quick sanity test
echo "Sanity test:"
target/universal/procguard --version
target/universal/timeout --version
echo ""

echo "Universal binaries available at:"
echo "  target/universal/procguard (primary)"
echo "  target/universal/timeout (GNU-compatible symlink)"
