#!/usr/bin/env bash
# Unit tests for tools/lib/dry_run_tolerance.sh.
#
# Run directly:  bash tools/tests/dry_run_tolerance_test.sh
# Exits non-zero if any assertion fails.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# shellcheck source=tools/lib/dry_run_tolerance.sh
source "$REPO_ROOT/tools/lib/dry_run_tolerance.sh"

# Stand-in for the real publish order; only membership matters here.
CRATES="fraiseql-error fraiseql-db fraiseql-codegen fraiseql-core fraiseql-server fraiseql-cli fraiseql"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# assert_tolerable <name> <logfile> <expected-missing>   (rc 0, prints names)
assert_tolerable() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local got rc
    set +e
    got="$(dry_run_failure_is_tolerable "$2" "$CRATES")"
    rc=$?
    set -e
    if [[ "$rc" -eq 0 && "$(printf '%s' "$got" | tr '\n' ' ' | xargs)" == "$3" ]]; then
        echo "  ok: $1"
    else
        echo "  FAIL: $1 — rc=$rc missing=[$got] (expected rc=0 missing=[$3])" >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# assert_not_tolerable <name> <logfile>   (rc 1)
assert_not_tolerable() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local rc
    set +e
    dry_run_failure_is_tolerable "$2" "$CRATES" >/dev/null
    rc=$?
    set -e
    if [[ "$rc" -eq 1 ]]; then
        echo "  ok: $1"
    else
        echo "  FAIL: $1 — rc=$rc (expected 1)" >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# A — a brand-new sibling that was never published (the fraiseql-codegen case).
cat > "$WORK/A.log" <<'EOF'
   Packaging fraiseql-cli v9.9.9 (/src/crates/fraiseql-cli)
   Verifying fraiseql-cli v9.9.9
error: failed to prepare local package for uploading

Caused by:
  no matching package named `fraiseql-codegen` found
  location searched: registry `crates-io`
EOF
assert_tolerable "codegen never published → tolerated" "$WORK/A.log" "fraiseql-codegen"

# B — a sibling published, but not at the synchronized new version (the floor-bump case).
cat > "$WORK/B.log" <<'EOF'
   Packaging fraiseql-core v9.9.9 (/src/crates/fraiseql-core)
error: failed to prepare local package for uploading

Caused by:
  failed to select a version for the requirement `fraiseql-db = "^9.9.9"`
  candidate versions found which didn't match: 2.4.0, 2.3.2
  location searched: crates.io index
EOF
assert_tolerable "sibling not at new version → tolerated" "$WORK/B.log" "fraiseql-db"

# C1 — a real compile error with no prepare marker → never tolerable.
cat > "$WORK/C1.log" <<'EOF'
   Compiling fraiseql-core v9.9.9
error[E0599]: no method named `frobnicate` found
error: could not compile `fraiseql-core` (lib) due to 1 previous error
EOF
assert_not_tolerable "compile error (no prepare marker) → NOT tolerated" "$WORK/C1.log"

# C2 — even with the prepare marker, a compile error must hard-fail.
cat > "$WORK/C2.log" <<'EOF'
error: failed to prepare local package for uploading
error[E0599]: no method named `frobnicate` found
error: could not compile `fraiseql-core` due to 1 previous error
EOF
assert_not_tolerable "prepare marker + compile error → NOT tolerated" "$WORK/C2.log"

# D — an external (non-sibling) dependency is the unresolved one → hard-fail.
cat > "$WORK/D.log" <<'EOF'
error: failed to prepare local package for uploading

Caused by:
  no matching package named `some-external-crate` found
  location searched: registry `crates-io`
EOF
assert_not_tolerable "external dep unresolved → NOT tolerated" "$WORK/D.log"

# E — prepare failure with no extractable unresolved package → hard-fail.
cat > "$WORK/E.log" <<'EOF'
error: failed to prepare local package for uploading

Caused by:
  some other packaging problem unrelated to dependency resolution
EOF
assert_not_tolerable "prepare failure, no missing sibling → NOT tolerated" "$WORK/E.log"

echo ""
if [[ "$TESTS_FAILED" -eq 0 ]]; then
    echo "dry_run_tolerance_test: all ${TESTS_RUN} checks passed."
else
    echo "dry_run_tolerance_test: ${TESTS_FAILED}/${TESTS_RUN} checks FAILED." >&2
    exit 1
fi
