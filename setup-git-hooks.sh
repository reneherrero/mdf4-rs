#!/bin/bash
# Setup script to install git hooks
#
# This script installs the pre-commit hook that runs clippy checks
# before allowing commits.

set -e

REPO_ROOT="$(git rev-parse --show-toplevel)"
GIT_HOOKS_DIR="$REPO_ROOT/.git/hooks"
GITHOOKS_DIR="$REPO_ROOT/.githooks"

if [ ! -d "$GITHOOKS_DIR" ]; then
    echo "Error: .githooks directory not found"
    exit 1
fi

# Create .git/hooks directory if it doesn't exist
mkdir -p "$GIT_HOOKS_DIR"

# Install pre-commit hook
if [ -f "$GITHOOKS_DIR/pre-commit" ]; then
    ln -sf "../../.githooks/pre-commit" "$GIT_HOOKS_DIR/pre-commit"
    chmod +x "$GIT_HOOKS_DIR/pre-commit"
    echo "✓ Pre-commit hook installed successfully"
    echo ""
    echo "The hook will now run clippy checks before each commit."
    echo "To skip the hook, use: git commit --no-verify"
else
    echo "Error: pre-commit hook not found in .githooks/"
    exit 1
fi
