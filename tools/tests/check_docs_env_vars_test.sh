#!/usr/bin/env bash
# Unit tests for tools/check-docs-env-vars.sh.
#
# Run directly:  bash tools/tests/check_docs_env_vars_test.sh
# Exits non-zero if any assertion fails.
#
# Pins the audit-record exception added 2026-09-21: a variable named only by a dated record
# under docs/security/audits/ passes when CHANGELOG.md records its removal, and fails in every
# other combination — named by a runbook too, or never removed. The two original behaviours
# (a variable with a Rust reader passes, one without fails) are pinned alongside so the
# exception cannot widen them.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-docs-env-vars.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_root <dir> <docs-runbook-text> <audit-record-text> <changelog-text>
make_root() {
    local root="$1"
    mkdir -p "$root/crates/x/src" "$root/docs/security/audits" "$root/examples" "$root/tools"
    printf 'let _ = std::env::var("FRAISEQL_READ");\n' > "$root/crates/x/src/lib.rs"
    printf '# readme\n' > "$root/README.md"
    printf '%s\n' "$2" > "$root/docs/runbook.md"
    printf '%s\n' "$3" > "$root/docs/security/audits/2026-01-01.md"
    printf '%s\n' "$4" > "$root/CHANGELOG.md"
}

# assert_gate <name> <expected-rc> <expected-substring> <root>
assert_gate() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" want_rc="$2" want_sub="$3" root="$4"
    local out rc
    set +e
    out="$(DOCS_ENV_VARS_ROOT="$root" bash "$GATE" 2>&1)"
    rc=$?
    set -e
    if [[ "$rc" -eq "$want_rc" && "$out" == *"$want_sub"* ]]; then
        echo "  ok: $name"
    else
        echo "  FAIL: $name — rc=$rc (want $want_rc), output did not contain '$want_sub':" >&2
        printf '%s\n' "$out" | sed 's/^/      /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "check-docs-env-vars.sh:"

make_root "$WORK/reader" 'set FRAISEQL_READ' 'record' 'log'
assert_gate "variable with a Rust reader passes" 0 "OK" "$WORK/reader"

make_root "$WORK/orphan" 'set FRAISEQL_DEAD' 'record' 'log'
assert_gate "runbook variable with no reader is red" 1 "FRAISEQL_DEAD" "$WORK/orphan"

make_root "$WORK/record_removed" 'nothing' 'audit quotes FRAISEQL_GONE' '- FRAISEQL_GONE is **removed**: it did nothing'
assert_gate "audit record quoting a removed variable passes" 0 "OK" "$WORK/record_removed"

make_root "$WORK/record_not_removed" 'nothing' 'audit quotes FRAISEQL_GONE' '- FRAISEQL_GONE was renamed'
assert_gate "audit record quoting a variable never removed is red" 1 "FRAISEQL_GONE" "$WORK/record_not_removed"

make_root "$WORK/runbook_too" 'set FRAISEQL_GONE' 'audit quotes FRAISEQL_GONE' '- FRAISEQL_GONE is **removed**'
assert_gate "removed variable named by a runbook as well is red" 1 "docs/runbook.md" "$WORK/runbook_too"

make_root "$WORK/no_changelog" 'nothing' 'audit quotes FRAISEQL_GONE' ''
rm "$WORK/no_changelog/CHANGELOG.md"
assert_gate "audit record with no changelog at all is red" 1 "FRAISEQL_GONE" "$WORK/no_changelog"

echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
