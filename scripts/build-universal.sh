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
#   - Xcode command line tools (for lipo)
#
# Usage:
#   ./scripts/build-universal.sh
#
# Output:
#   target/universal/timeout - The universal binary
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "=== Building timeout universal binary ==="
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

echo ""

# Build for ARM64 (Apple Silicon)
echo "Building for aarch64-apple-darwin (Apple Silicon)..."
cargo build --release --target aarch64-apple-darwin

# Build for x86_64 (Intel)
echo "Building for x86_64-apple-darwin (Intel)..."
cargo build --release --target x86_64-apple-darwin

echo ""

# Create universal binary directory
mkdir -p target/universal

# Combine with lipo
echo "Creating universal binary with lipo..."
lipo -create \
    target/aarch64-apple-darwin/release/timeout \
    target/x86_64-apple-darwin/release/timeout \
    -output target/universal/timeout

# Strip is already done by the release profile, but ensure it
strip target/universal/timeout 2>/dev/null || true

# Optional: ad-hoc codesign for faster first launch
echo "Signing binary..."
codesign -s - target/universal/timeout 2>/dev/null || true

echo ""
echo "=== Build complete ==="
echo ""

# Show results
echo "Binary info:"
file target/universal/timeout
echo ""

echo "Size:"
ls -lh target/universal/timeout
echo ""

echo "Architectures:"
lipo -info target/universal/timeout
echo ""

# Quick sanity test
echo "Sanity test:"
target/universal/timeout --version
echo ""

echo "Universal binary available at: target/universal/timeout"
