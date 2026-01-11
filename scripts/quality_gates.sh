#!/usr/bin/env bash
#
# quality_gates.sh - Run all quality checks before merge
#
# Runs quality checks for both Rust and Web (TypeScript/Svelte).
#
# Exit codes:
#   0 - All checks passed
#   1 - One or more checks failed
#
# Usage: ./scripts/quality_gates.sh
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WEB_DIR="$REPO_ROOT/web"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

failed=0

log_section() {
    echo -e "\n${BLUE}═══════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════${NC}"
}

log_step() {
    echo -e "\n${YELLOW}▶ $1${NC}"
}

log_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

log_failure() {
    echo -e "${RED}✗ $1${NC}"
}

# ============================================================
# RUST QUALITY GATES
# ============================================================
log_section "Rust Quality Gates"
cd "$REPO_ROOT"

# 1. Rust formatting check
log_step "Checking Rust formatting (cargo fmt)..."
if cargo fmt --check; then
    log_success "Rust formatting check passed"
else
    log_failure "Rust formatting check failed - run 'cargo fmt' to fix"
    failed=1
fi

# 2. Rust linting
log_step "Running Rust linter (cargo clippy)..."
if cargo clippy -- -D warnings; then
    log_success "Rust linting passed"
else
    log_failure "Rust linting failed"
    failed=1
fi

# 3. Rust tests
log_step "Running Rust tests (cargo test)..."
if cargo test; then
    log_success "Rust tests passed"
else
    log_failure "Rust tests failed"
    failed=1
fi

# 4. Rust coverage (optional - only if tarpaulin is installed)
if command -v cargo-tarpaulin &> /dev/null; then
    log_step "Running Rust coverage (cargo tarpaulin)..."
    if cargo tarpaulin --skip-clean --out Stdout; then
        log_success "Rust coverage report generated"
    else
        log_failure "Rust coverage failed"
        failed=1
    fi
else
    log_step "Skipping Rust coverage (cargo-tarpaulin not installed)"
fi

# ============================================================
# WEB QUALITY GATES
# ============================================================
log_section "Web Quality Gates (TypeScript/Svelte)"
cd "$WEB_DIR"

# Check if node_modules exists
if [ ! -d "node_modules" ]; then
    log_step "Installing npm dependencies..."
    npm ci
fi

# 1. Web formatting check
log_step "Checking Web formatting (Prettier)..."
if npm run format:check; then
    log_success "Web formatting check passed"
else
    log_failure "Web formatting check failed - run 'npm run format' to fix"
    failed=1
fi

# 2. Web linting
log_step "Running Web linter (ESLint)..."
if npm run lint; then
    log_success "Web linting passed"
else
    log_failure "Web linting failed - run 'npm run lint:fix' to auto-fix some issues"
    failed=1
fi

# 3. Web type checking
log_step "Running Web type checker (svelte-check)..."
if npm run check; then
    log_success "Web type checking passed"
else
    log_failure "Web type checking failed"
    failed=1
fi

# 4. Web tests with coverage
log_step "Running Web tests (Vitest)..."
if npm run test:coverage; then
    log_success "Web tests passed with coverage thresholds met"
else
    log_failure "Web tests failed or coverage thresholds not met"
    failed=1
fi

# ============================================================
# SUMMARY
# ============================================================
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [ $failed -eq 0 ]; then
    log_success "All quality gates passed!"
    exit 0
else
    log_failure "Quality gates failed - fix issues before merge"
    exit 1
fi
