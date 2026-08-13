#!/usr/bin/env bash
# Unit tests for tools/check-deadlines.sh.
#
# Run directly:  bash tools/tests/check_deadlines_test.sh
# Exits non-zero if any assertion fails.
#
# The gate is the ONLY enforcement of the `# deadline:` risk-acceptance
# convention — neither cargo-deny nor cargo-audit has a native expiry field
# (cargo-deny 0.19 rejects anything but `id`/`reason` on an ignore entry), and
# a lapsed acceptance reddens a REQUIRED check on a date rather than on a push.
# So the gate's own boundary behaviour is worth pinning: an off-by-one here is
# a day of every-branch-blocked, and a file it forgets to scan is an acceptance
# that expires invisibly (#1103).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-deadlines.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# run_gate <today> <file...> → prints combined output, returns the gate's exit code.
run_gate() {
    local today="$1"
    shift
    DEADLINE_CHECK_TODAY="$today" DEADLINE_CHECK_FILES="$*" bash "$GATE" 2>&1
}

# assert_gate <name> <today> <expected-rc> <expected-substring> <file...>
assert_gate() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" today="$2" want_rc="$3" want_sub="$4"
    shift 4
    local out rc
    set +e
    out="$(run_gate "$today" "$@")"
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

# assert_gate_not <name> <today> <unwanted-substring> <file...>
assert_gate_not() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" today="$2" unwanted="$3"
    shift 3
    local out rc
    set +e
    out="$(run_gate "$today" "$@")"
    rc=$?
    set -e
    if [[ "$rc" -eq 0 && "$out" != *"$unwanted"* ]]; then
        echo "  ok: $name"
    else
        echo "  FAIL: $name — rc=$rc (want 0), output unexpectedly contained '$unwanted':" >&2
        printf '%s\n' "$out" | sed 's/^/      /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "check-deadlines.sh"

# ── Fixture: one accepted advisory carrying a deadline ────────────────────────
mk_fixture() {
    local path="$1" date_str="$2"
    cat > "$path" <<EOF
[advisories]
ignore = [
  # some justification prose
  # deadline: $date_str — re-evaluate the acceptance.
  {id = "RUSTSEC-2026-0098", reason = "test fixture"},
]
EOF
}

FIX="$WORK/deny.toml"

# ── Boundary: the deadline date is the FIRST day the acceptance is invalid ────
# The gate used a strict `<`, which passed ON the deadline and failed the day
# after — so a "2026-09-01" acceptance was still accepted on 2026-09-01. The
# convention is now inclusive, and this pair is what pins it.
mk_fixture "$FIX" "2026-12-01"
assert_gate "fails ON the deadline date" "2026-12-01" 1 "lapsed advisory deadline 2026-12-01" "$FIX"
assert_gate "fails after the deadline date" "2026-12-02" 1 "lapsed advisory deadline 2026-12-01" "$FIX"
assert_gate_not "passes the day before the deadline" "2026-11-30" "ERROR" "$FIX"

# ── The error must name the file, not just the line ───────────────────────────
# Two files are scanned now; a bare line number is ambiguous between them.
assert_gate "error names the offending file" "2026-12-01" 1 "$FIX:4" "$FIX"

# ── Warning window: the gate announces itself BEFORE it blocks ────────────────
# A deadline reddens a required check on a date, with no commit to bisect
# (#978's shape). A warning inside the window makes it visible on every
# preflight run for weeks beforehand, while still exiting 0.
assert_gate "warns inside the 30-day window" "2026-11-15" 0 "WARN: advisory deadline 2026-12-01" "$FIX"
assert_gate "warns on the last day before lapse" "2026-11-30" 0 "WARN: advisory deadline 2026-12-01" "$FIX"
assert_gate_not "silent outside the warning window" "2026-10-01" "WARN" "$FIX"

# ── Multi-file scope: .cargo/audit.toml is enforced too ───────────────────────
# deny.toml and .cargo/audit.toml both carry acceptances and are kept in
# lockstep by ids — but the deadline comments drifted apart unnoticed for
# months because only deny.toml was ever scanned.
AUDIT="$WORK/audit.toml"
mk_fixture "$FIX" "2027-06-01"
mk_fixture "$AUDIT" "2026-06-15"
assert_gate "a lapsed deadline in the second file fails" "2026-08-13" 1 "$AUDIT:4" "$FIX $AUDIT"
assert_gate_not "a clean second file passes" "2026-08-13" "ERROR" "$FIX"

# ── A file with no deadline comments is not an error ──────────────────────────
printf '[advisories]\nignore = []\n' > "$WORK/empty.toml"
assert_gate "no deadlines at all is OK" "2026-08-13" 0 "OK:" "$WORK/empty.toml"

# ── A missing file is a loud failure, not a silent pass ───────────────────────
# The gate greps a hardcoded path list; a rename that outruns the list would
# otherwise turn the whole gate into a no-op that still prints OK.
assert_gate "a missing scan target fails loudly" "2026-08-13" 1 "not found" "$WORK/does-not-exist.toml"

echo
if [[ "$TESTS_FAILED" -gt 0 ]]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions" >&2
    exit 1
fi
echo "PASSED: $TESTS_RUN assertions"
