#!/usr/bin/env bash
# r_examples_parse_test.sh — the red capability of tools/check-r-examples-parse.sh.
#
# None of these is observable from a passing run of the real tree, which holds one
# R file that parses. A gate whose find matched nothing, or that reported a count
# it had not checked, or that read the parser's exit code through a pipe, would
# print OK forever over the exact defect it exists to catch (#1260) — and the
# defect it exists to catch is precisely "nobody ever ran this".
#
# The fixtures are a few bytes of R each; the parse is syntax-only, so no R
# package is needed and the whole run is one container start per case.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-r-examples-parse.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); }

run_gate() {
  ( cd "$repo_root" && R_EXAMPLES_SCAN_ROOT="$1" bash "$gate" ) > "${tmp}/out" 2>&1
  echo $?
}

# ── 1. a file that does not parse is caught, and named ───────────────────────
root="${tmp}/broken"; mkdir -p "$root"
printf 'f <- function(x) {\n  x + 1\n}\n' > "${root}/good.R"
printf 'g <- function(x) {\n  x + \n' > "${root}/bad.R"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q 'bad.R' "${tmp}/out"; then
  pass "a file that does not parse fails, naming it"
else
  fail "a syntax error was tolerated (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 2. all-good is green, and reports the count it checked ───────────────────
root="${tmp}/allgood"; mkdir -p "${root}/nested"
printf 'a <- 1\n' > "${root}/one.R"
printf 'b <- 2\n' > "${root}/nested/two.R"
printf 'd <- 3\n' > "${root}/three.r"
rc=$(run_gate "$root")
if [ "$rc" -eq 0 ] && grep -q 'parsed 3 of 3 file(s)' "${tmp}/out"; then
  pass "three parsable files pass, and the count matches what was found"
else
  fail "a clean tree did not pass with a 3-of-3 count (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 3. an empty scan root FAILS rather than passing over nothing ─────────────
# The gate's whole subject is a file nobody executes. A discovery that silently
# matched zero files would restore exactly that state while printing OK.
root="${tmp}/empty"; mkdir -p "$root"
printf 'not R\n' > "${root}/README.md"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q 'found no R file' "${tmp}/out"; then
  pass "an empty discovery fails instead of passing vacuously"
else
  fail "a scan root with no R files was reported as OK (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 4. a scan root that does not exist fails ─────────────────────────────────
rc=$(run_gate "${tmp}/no-such-directory")
if [ "$rc" -ne 0 ]; then
  pass "a missing scan root fails"
else
  fail "a missing scan root was reported as OK"; sed 's/^/       /' "${tmp}/out"
fi

# ── 5. the failure is the parser's, not the shell's ──────────────────────────
# A gate that pipes the parser into anything reads the pipe's status, not the
# parser's, and a syntax error becomes a pass. Assert the bad file is reported as
# FAIL by name, so the exit code and the report agree.
root="${tmp}/status"; mkdir -p "$root"
printf 'x <- 1\n' > "${root}/fine.R"
printf 'if (TRUE {\n' > "${root}/unbalanced.R"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q 'FAIL unbalanced.R' "${tmp}/out" && grep -q 'parsed 1 of 2' "${tmp}/out"; then
  pass "the parser's own failure sets the exit code and is reported per file"
else
  fail "the parser's failure did not reach the exit code and the report (rc=$rc)"
  sed 's/^/       /' "${tmp}/out"
fi

echo
if [ "$failures" -ne 0 ]; then
  echo "✗ ${failures} r-examples-parse gate self-test(s) failed."
  exit 1
fi
echo "OK: tools/check-r-examples-parse.sh can go red in every way it needs to."
