#!/usr/bin/env bash
# phases_citations_test.sh — the red capability of tools/check-phases-citations.sh.
#
# None of these is observable from a passing run of the real tree: a gate whose
# find matched nothing, or whose exemption list had grown to cover everything,
# would print OK forever over the exact defect it exists to catch (#1210).
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-phases-citations.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); }

run_gate() {
  ( cd "$repo_root" && PHASES_CITATION_SCAN_ROOT="$1" bash "$gate" ) > "${tmp}/out" 2>&1
  echo $?
}

# ── 1. a citation from a shipped file is caught, and located ─────────────────
root="${tmp}/cited"; mkdir -p "${root}/docs"
printf 'nothing to see\n' > "${root}/clean.md"
printf 'See `.phases/dagger-adoption/parity-notes.md` for why.\n' > "${root}/docs/guide.md"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q "docs/guide.md" "${tmp}/out"; then
  pass "a .phases/ citation fails, naming the file and line"
else
  fail "a .phases/ citation was tolerated (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 2. a tree with no citation passes, and reports what it scanned ───────────
root="${tmp}/clean"; mkdir -p "$root"
printf 'See docs/contributing/dagger-parity-notes.md.\n' > "${root}/a.md"
rc=$(run_gate "$root")
if [ "$rc" -eq 0 ] && grep -q "file(s) scanned" "${tmp}/out"; then
  pass "a clean tree passes, and says how much it scanned"
else
  fail "a clean tree did not pass (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 3. an empty scan root fails LOUDLY, not vacuously ────────────────────────
root="${tmp}/empty"; mkdir -p "$root"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q "the search is wrong" "${tmp}/out"; then
  pass "an empty scan root fails loudly"
else
  fail "an empty scan root passed vacuously (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 4. an all-exempt tree fails rather than checking nothing ─────────────────
# The failure mode that turns this gate into decoration: the exemption list grows
# until nothing is left to check, and the gate still reports OK.
root="${tmp}/exempt"; mkdir -p "$root"
printf 'See `.phases/whatever/`\n' > "${root}/CHANGELOG.md"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q "checking nothing" "${tmp}/out"; then
  pass "a tree where every file is exempt fails rather than checking nothing"
else
  fail "an all-exempt tree passed (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

echo
if [ "$failures" -eq 0 ]; then
  echo "OK: check-phases-citations.sh can fail, in all four ways."
else
  echo "✗ ${failures} self-test(s) failed."
fi
exit "$failures"
