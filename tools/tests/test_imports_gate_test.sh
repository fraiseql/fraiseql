#!/usr/bin/env bash
# Unit tests for tools/check-test-imports.sh.
#
# Run directly:  bash tools/tests/test_imports_gate_test.sh
# Exits non-zero if any assertion fails.
#
# This gate ran in `make preflight` and in the required Dagger ShellGates leg for its
# whole life and never rejected anything: its pattern was passed to `grep` in BRE mode,
# where `\(` and `\)` open and close a group rather than matching parentheses, so it
# searched for `std::env::var"DATABASE_URL"` — a string that cannot occur in Rust
# source. Fifteen files accumulated behind it (#1075).
#
# So the assertions below are all red-capability assertions. Each is a shape the gate
# must reject, and the first one is the literal text the original could not see.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-test-imports.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# fixture <name> <relative-file> <contents> [allow-contents]
# Builds a tree shaped like the repo (crates/<crate>/{src,tests}/…) and runs the gate.
fixture() {
    local name="$1" relpath="$2" contents="$3" allow="${4:-}"
    local dir="$WORK/$name"
    mkdir -p "$dir/$(dirname "$relpath")"
    printf '%s\n' "$contents" >"$dir/$relpath"
    printf '%s\n' "# fixture allowlist${allow:+
$allow}" >"$dir/allow.txt"
    echo "$dir"
}

# assert_gate <name> <expected-exit> <expected-substring> <dir>
assert_gate() {
    local name="$1" want_exit="$2" want_text="$3" dir="$4"
    TESTS_RUN=$((TESTS_RUN + 1))

    local out rc
    set +e
    out="$(TEST_IMPORTS_ROOT="$dir" TEST_IMPORTS_ALLOW="$dir/allow.txt" bash "$GATE" 2>&1)"
    rc=$?
    set -e

    if [ "$rc" -ne "$want_exit" ]; then
        echo "  FAIL: $name — expected exit $want_exit, got $rc"
        printf '%s\n' "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    if ! printf '%s' "$out" | grep -qF "$want_text"; then
        echo "  FAIL: $name — output did not contain: $want_text"
        printf '%s\n' "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    echo "  ok: $name"
}

echo "=== tools/check-test-imports.sh ==="

# THE regression: the exact text the BRE-escaped pattern could not match.
d="$(fixture bre crates/fraiseql-core/tests/wire_test.rs \
'#[tokio::test]
async fn t() {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///bench".to_string());
}')"
assert_gate "the literal std::env::var(\"DATABASE_URL\") is rejected" \
    1 "crates/fraiseql-core/tests/wire_test.rs" "$d"

# The old scope was crates/*/tests/ only, so unit tests in src/ were invisible even to
# a working pattern. Four real violations lived there.
d="$(fixture srctests crates/fraiseql-observers/src/listener/tests.rs \
'#[tokio::test]
async fn t() {
    let Ok(url) = std::env::var("DATABASE_URL") else { return };
}')"
assert_gate "a unit test under src/ is in scope" \
    1 "crates/fraiseql-observers/src/listener/tests.rs" "$d"

# An inline `#[cfg(test)] mod` in a production file is a test too, and is not named
# tests.rs — a scope keyed on the filename would miss it.
d="$(fixture inlinemod crates/fraiseql-observers/src/executor/dispatch.rs \
'#[cfg(test)]
mod dispatch_632_tests {
    #[tokio::test]
    async fn t() {
        let url = std::env::var("DATABASE_URL").expect("must be set");
    }
}')"
assert_gate "an inline #[cfg(test)] mod in a production file is in scope" \
    1 "crates/fraiseql-observers/src/executor/dispatch.rs" "$d"

# `use std::env;` then `env::var(…)` is the same violation spelled differently, and is
# what the fully-qualified-prefix pattern would still have missed after an escaping fix.
d="$(fixture shortform crates/fraiseql-wire/tests/tls_integration.rs \
'use std::env;
fn cfg() -> Option<String> {
    env::var("DATABASE_URL").ok()
}')"
assert_gate "the unqualified env::var spelling is rejected" \
    1 "crates/fraiseql-wire/tests/tls_integration.rs" "$d"

# The sibling variables have their own resolution policy and end in the same token, so
# a suffix-blind match would produce false positives on all three.
d="$(fixture siblings crates/fraiseql-db/tests/tls_test.rs \
'fn t() {
    let a = std::env::var("TLS_DATABASE_URL").ok();
    let b = std::env::var("TEST_DATABASE_URL").ok();
    let c = std::env::var("STANDBY_DATABASE_URL").ok();
}')"
assert_gate "TLS_/TEST_/STANDBY_DATABASE_URL are not DATABASE_URL" \
    0 "OK:" "$d"

# The canonical helper is the whole point: a file using it must pass.
d="$(fixture helper crates/fraiseql-core/tests/wire_test.rs \
'#[tokio::test]
async fn t() {
    let url = fraiseql_test_support::database_url();
    let maybe = fraiseql_test_support::try_database_url();
}')"
assert_gate "the canonical helper passes" 0 "OK:" "$d"

# An allowed production reader passes...
d="$(fixture allowed crates/fraiseql-cli/src/commands/migrate.rs \
'pub fn run() {
    if let Ok(url) = std::env::var("DATABASE_URL") { let _ = url; }
}' \
'crates/fraiseql-cli/src/commands/migrate.rs')"
assert_gate "an allowlisted production reader passes" 0 "OK:" "$d"

# ...but a row that no longer reads the variable is a stale exemption, and a gate that
# only fails one way rots into an allowlist nobody prunes.
d="$(fixture stale crates/fraiseql-cli/src/commands/migrate.rs \
'pub fn run() {
    let url = fraiseql_test_support::database_url();
}' \
'crates/fraiseql-cli/src/commands/migrate.rs')"
assert_gate "a stale allowlist row is rejected" 1 "stale rows" "$d"

# A row naming a file that no longer exists is the same failure by another route.
d="$(fixture ghost crates/fraiseql-cli/src/commands/migrate.rs \
'pub fn run() {}' \
'crates/fraiseql-cli/src/commands/deleted.rs')"
assert_gate "an allowlist row for a deleted file is rejected" 1 "stale rows" "$d"

# A clean tree passes, so the gate is not merely always-red.
d="$(fixture clean crates/fraiseql-core/tests/pure_test.rs \
'#[test]
fn t() { assert_eq!(1 + 1, 2); }')"
assert_gate "a clean tree passes" 0 "OK:" "$d"

echo ""
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions"
    exit 1
fi
echo "PASSED: $TESTS_RUN assertions"
