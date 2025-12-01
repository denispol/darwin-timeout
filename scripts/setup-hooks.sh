#!/bin/bash
#
# Install git hooks for darwin-timeout development.
#
# Usage: ./scripts/setup-hooks.sh
#
# This installs:
#   - pre-commit: fast fmt + clippy check
#   - pre-push: fmt + clippy + unit tests
#
# Hooks are idempotent - safe to run multiple times.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOKS_DIR="$(dirname "$SCRIPT_DIR")/.git/hooks"

echo "Installing git hooks..."

# pre-commit hook
cp "$SCRIPT_DIR/pre-commit" "$HOOKS_DIR/pre-commit"
chmod +x "$HOOKS_DIR/pre-commit"
echo "  ✓ pre-commit (fmt + clippy)"

# pre-push hook
cp "$SCRIPT_DIR/pre-push" "$HOOKS_DIR/pre-push"
chmod +x "$HOOKS_DIR/pre-push"
echo "  ✓ pre-push (fmt + clippy + unit tests)"

echo ""
echo "Done! Hooks installed to .git/hooks/"
echo "To skip hooks temporarily: git commit --no-verify / git push --no-verify"
