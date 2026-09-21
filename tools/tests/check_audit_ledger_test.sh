#!/usr/bin/env bash
# Unit tests for tools/check-audit-ledger.sh.
#
# Run directly:  bash tools/tests/check_audit_ledger_test.sh
# Exits non-zero if any assertion fails.
#
# Each case is a small audit and ledger written to a temp dir. The RED cases are the ones
# that matter: a missing row, an empty evidence cell, a placeholder, and an audit from which
# nothing is extracted (a vacuous pass). The GREEN case proves the extraction sees both
# heading IDs and bold MEDIUM titles, and only those (a bold bullet outside MEDIUM is not a
# finding).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-audit-ledger.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# assert_gate <name> <expected-rc> <expected-substring> <audit> <ledger>
assert_gate() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" want_rc="$2" want_sub="$3" audit="$4" ledger="$5"
    local out rc
    set +e
    out="$(AUDIT_LEDGER_AUDIT="$audit" AUDIT_LEDGER_LEDGER="$ledger" bash "$GATE" 2>&1)"
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

audit="$WORK/audit.md"
cat > "$audit" <<'EOF'
# Audit
## CRITICAL
### C1
text
## HIGH
### H1
text
### H2
text
## MEDIUM
- **Quotas accepted but never enforced** (`tenant.rs:1`). text
- **Broadcast mounted with no auth** (`admin.rs:9`). text
## CODE QUALITY (lower-severity)
- **Not a finding** — bold outside MEDIUM must not be required
EOF

complete="$WORK/complete.md"
cat > "$complete" <<'EOF'
| Finding | Severity | Status | Evidence |
|---|---|---|---|
| C1 | critical | fixed | CHANGELOG.md:10 v2.7.0 |
| H1 | high | fixed | CHANGELOG.md:11 v2.7.0 |
| H2 | high | open #1 | https://github.com/x/y/issues/1 |
| **Quotas accepted but never enforced** | medium | fixed | CHANGELOG.md:12 v2.8.0 |
| **Broadcast mounted with no auth** | medium | deleted with the feature | CHANGELOG.md:13 v2.15.0 |
EOF

missing_row="$WORK/missing.md"
grep -v '^| H2 ' "$complete" > "$missing_row"

missing_medium="$WORK/missing_medium.md"
grep -vF '**Broadcast mounted with no auth**' "$complete" > "$missing_medium"

empty_evidence="$WORK/empty.md"
sed 's#| H1 | high | fixed | CHANGELOG.md:11 v2.7.0 |#| H1 | high | fixed |  |#' "$complete" > "$empty_evidence"

placeholder="$WORK/placeholder.md"
sed 's#| C1 | critical | fixed | CHANGELOG.md:10 v2.7.0 |#| C1 | critical | fixed | TBD |#' "$complete" > "$placeholder"

empty_audit="$WORK/empty_audit.md"
printf '# Audit\n## HIGH\nno headings here\n' > "$empty_audit"

echo "check-audit-ledger.sh:"
assert_gate "complete ledger passes and counts both kinds" 0 "5 findings (3 C/H, 2 medium)" "$audit" "$complete"
assert_gate "missing H row is red"                          1 "MISSING: H2"                 "$audit" "$missing_row"
assert_gate "missing medium row is red"                     1 "MISSING: medium 'Broadcast mounted with no auth'" "$audit" "$missing_medium"
assert_gate "empty evidence cell is red"                    1 "NO EVIDENCE: H1"             "$audit" "$empty_evidence"
assert_gate "placeholder evidence is red"                   1 "NO EVIDENCE: C1"             "$audit" "$placeholder"
assert_gate "audit with nothing to extract is red"          1 "no findings extracted"       "$empty_audit" "$complete"
assert_gate "missing ledger file is red"                    1 "not found"                   "$audit" "$WORK/does-not-exist.md"

echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
