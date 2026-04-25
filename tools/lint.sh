#!/usr/bin/env bash
# tools/lint.sh — single lint harness for FraiseQL
#
# Consolidates all custom lint checks into one script so CI needs a single step
# and developers can run `make lint` locally to reproduce the full check suite.
#
# Exit 0 on pass. Exit 1 with a summary on any failure.
#
# Usage:
#   bash tools/lint.sh                 # Run all checks
#   bash tools/lint.sh test-imports    # Run a single check by name
#
# Environment variables (override defaults):
#   UNWRAP_ALLOW_LIMIT             (default: 3, matches CI invocation)
#   ASYNC_TRAIT_LIMIT              (default: 160, current baseline 155 + headroom)
#   FRAISEQL_DB_LIB_ALLOWS_MAX     (default: 40)
#   FRAISEQL_CORE_CAST_ALLOWS_MAX  (default: 20)

set -euo pipefail

ERRORS=0
FAILED=()
ONLY="${1:-}"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

pass() { echo "  ✅ $1"; }
fail() {
    echo "  ❌ $1"
    ERRORS=$((ERRORS + 1))
    FAILED+=("$1")
}

run_check() {
    local name="$1"; shift
    # If a specific check was requested, skip all others.
    if [[ -n "$ONLY" && "$ONLY" != "$name" ]]; then return; fi
    echo "→ $name"
    if "$@" 2>&1; then
        pass "$name"
    else
        fail "$name"
    fi
}

# ---------------------------------------------------------------------------
# Check 1: no bare DATABASE_URL in test files (use test utilities instead)
# ---------------------------------------------------------------------------
check_test_imports() {
    bash tools/check-test-imports.sh
}
run_check "test-imports" check_test_imports

# ---------------------------------------------------------------------------
# Check 2: crate LoC budgets
# ---------------------------------------------------------------------------
check_crate_sizes() {
    bash tools/check-crate-sizes.sh
}
run_check "crate-sizes" check_crate_sizes

# ---------------------------------------------------------------------------
# Check 3: no production unwrap allows beyond limit
# ---------------------------------------------------------------------------
check_unwrap() {
    local limit="${UNWRAP_ALLOW_LIMIT:-3}"
    local count
    count=$(grep -rn 'allow.*unwrap_used' crates/*/src/ --include="*.rs" \
        | grep -v "test" | grep -v '#!\[allow' | wc -l)
    echo "  unwrap allows in production code: $count / $limit"
    if [ "$count" -gt "$limit" ]; then
        echo "  ERROR: $count production unwrap allows exceeds limit of $limit"
        grep -rn 'allow.*unwrap_used' crates/*/src/ --include="*.rs" \
            | grep -v "test" | grep -v '#!\[allow' || true
        return 1
    fi
}
run_check "no-unwrap-in-lib" check_unwrap

# ---------------------------------------------------------------------------
# Check 4: no empty/placeholder .expect() messages in production code
# ---------------------------------------------------------------------------
check_expect() {
    local count
    count=$(grep -rn '\.expect("")\|\.expect("TODO")\|\.expect("todo")\|\.expect("FIXME")\|\.expect("fixme")' \
        crates/*/src/ --include="*.rs" | grep -v test | wc -l)
    if [ "$count" -gt 0 ]; then
        echo "  ERROR: $count .expect() calls with empty/placeholder messages:"
        grep -rn '\.expect("")\|\.expect("TODO")\|\.expect("todo")\|\.expect("FIXME")\|\.expect("fixme")' \
            crates/*/src/ --include="*.rs" | grep -v test || true
        return 1
    fi
    echo "  no empty .expect() calls found"
}
run_check "expect-documented" check_expect

# ---------------------------------------------------------------------------
# Check 5: #[async_trait] count gate (tracks RFC 3425 cleanup progress)
# ---------------------------------------------------------------------------
check_async_trait() {
    local limit="${ASYNC_TRAIT_LIMIT:-160}"
    local count
    count=$(grep -rn "#\[async_trait\]" crates/*/src/ --include="*.rs" | wc -l)
    echo "  async_trait usages: $count (limit: $limit)"
    if [ "$count" -gt "$limit" ]; then
        echo "  ERROR: $count async_trait usages exceeds baseline $limit"
        echo "  New dyn-dispatch traits must add:"
        echo "    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)"
        return 1
    fi
}
run_check "async-trait-budget" check_async_trait

# ---------------------------------------------------------------------------
# Check 6: fraiseql-db must not have HIGH-risk cast lints at crate level
# ---------------------------------------------------------------------------
check_gate_db() {
    local max="${FRAISEQL_DB_LIB_ALLOWS_MAX:-40}"
    local count
    count=$(grep -c '#!\[allow(clippy' crates/fraiseql-db/src/lib.rs 2>/dev/null || echo 0)
    echo "  fraiseql-db lib.rs crate-level allows: $count (max: $max)"
    for lint in cast_possible_truncation cast_precision_loss cast_sign_loss; do
        if grep -q "allow.*$lint" crates/fraiseql-db/src/lib.rs 2>/dev/null; then
            echo "  ERROR: HIGH-risk cast lint $lint must not be allowed at crate level in fraiseql-db"
            return 1
        fi
    done
    if [ "$count" -gt "$max" ]; then
        echo "  ERROR: $count crate-level allows in fraiseql-db exceeds $max"
        return 1
    fi
    echo "  OK: no HIGH-risk cast lints at crate level"
}
run_check "lint-gate-db" check_gate_db

# ---------------------------------------------------------------------------
# Check 7: fraiseql-core narrow cast allows must not proliferate
# ---------------------------------------------------------------------------
check_gate_core() {
    local max="${FRAISEQL_CORE_CAST_ALLOWS_MAX:-20}"
    local count
    count=$(grep -r '#\[allow(clippy::cast' crates/fraiseql-core/src/ | wc -l)
    echo "  fraiseql-core narrow cast allows: $count (max: $max)"
    for lint in cast_possible_truncation cast_precision_loss cast_sign_loss; do
        if grep -r "^#!\[allow.*$lint" crates/fraiseql-core/src/lib.rs 2>/dev/null | grep -q .; then
            echo "  ERROR: HIGH-risk cast lint $lint must not be allowed at crate level in fraiseql-core"
            return 1
        fi
    done
    if [ "$count" -gt "$max" ]; then
        echo "  ERROR: $count narrow cast allows exceeds $max"
        return 1
    fi
}
run_check "lint-gate-core" check_gate_core

# ---------------------------------------------------------------------------
# Check 8: fraiseql-federation must not import fraiseql-server (dep gate)
# Allowed dependency direction: error ← db ← core ← federation ← server
# Forbidden: federation → server (would create a cycle since server depends on federation)
# ---------------------------------------------------------------------------
check_dep_gate() {
    if grep -r "fraiseql.server\|fraiseql_server" crates/fraiseql-federation/src/ --include="*.rs" -l 2>/dev/null | grep -q .; then
        echo "  ERROR: fraiseql-federation imports fraiseql-server — would create a dependency cycle"
        grep -r "fraiseql.server\|fraiseql_server" crates/fraiseql-federation/src/ --include="*.rs" -l
        return 1
    fi
    echo "  no fraiseql-server imports in fraiseql-federation (dep direction correct)"
}
run_check "dep-gate-federation-server" check_dep_gate

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
if [ "$ERRORS" -gt 0 ]; then
    echo "❌ $ERRORS lint check(s) failed: ${FAILED[*]}"
    exit 1
else
    echo "✅ All lint checks passed."
fi
