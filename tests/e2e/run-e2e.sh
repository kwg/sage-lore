#!/usr/bin/env bash
# E2E test runner for sage-lore
#
# Creates a clean project directory NEXT to sage-lore and runs the
# sage-dice E2E test from there. sage-lore ships the test data,
# but execution happens in an isolated project.
#
# Usage:
#   ./tests/e2e/run-e2e.sh                    # creates ../sage-dice-e2e/
#   ./tests/e2e/run-e2e.sh /tmp/my-test       # custom location
#   SAGE_LLM_BACKEND=ollama ./tests/e2e/run-e2e.sh  # use ollama

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SAGE_LORE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SAGE_LORE_BIN="${SAGE_LORE_BIN:-$SAGE_LORE_ROOT/target/release/sage-lore}"
E2E_DIR="${1:-$(dirname "$SAGE_LORE_ROOT")/sage-dice-e2e}"

# Check binary exists
if [ ! -f "$SAGE_LORE_BIN" ]; then
    echo "sage-lore binary not found at $SAGE_LORE_BIN"
    echo "Build first: cargo build --release"
    exit 1
fi

echo "=== sage-lore E2E test ==="
echo "  Source:  $SCRIPT_DIR"
echo "  Project: $E2E_DIR"
echo "  Binary:  $SAGE_LORE_BIN"
echo "  Backend: ${SAGE_LLM_BACKEND:-claude}"
echo ""

# Create clean project directory
rm -rf "$E2E_DIR"
mkdir -p "$E2E_DIR"

# Copy test infrastructure
cp -r "$SCRIPT_DIR/fixtures"   "$E2E_DIR/"
cp -r "$SCRIPT_DIR/adapters"   "$E2E_DIR/"
cp -r "$SCRIPT_DIR/.sage-lore" "$E2E_DIR/"
cp    "$SCRIPT_DIR"/*.scroll   "$E2E_DIR/"
cp    "$SCRIPT_DIR/Cargo.toml" "$E2E_DIR/"

# Init git (executor requires it)
cd "$E2E_DIR"
git init -q
git add -A
git commit -q -m "e2e test project init"

echo "Project created at $E2E_DIR"
echo "Running E2E..."
echo ""

# Run the test
exec "$SAGE_LORE_BIN" run e2e-sage-dice.scroll --project . --var project_root=. -v
