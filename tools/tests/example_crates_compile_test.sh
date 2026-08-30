#!/usr/bin/env bash
# example_crates_compile_test.sh — the red capability of
# tools/check-example-crates-compile.sh.
#
# None of these is observable from a passing run of the real tree: a gate whose
# find matched nothing, or which selected no standalone crate, or which read
# cargo's exit code through a pipe, would print OK forever over the exact defect
# it exists to catch (#1200).
#
# The fixtures are minimal cargo crates with no dependencies, so the cargo halves
# run offline in about a second.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-example-crates-compile.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); }

# <root>/<name>/{Cargo.toml,src/main.rs}. `standalone=yes` gives it its own
# [workspace], which is what puts a crate outside the main one.
make_crate() {
  local root="$1" name="$2" standalone="$3" body="$4"
  local dir="${root}/${name}"
  mkdir -p "${dir}/src"
  {
    echo '[package]'
    echo "name = \"${name}\""
    echo 'version = "0.0.0"'
    echo 'edition = "2021"'
    echo
    echo '[dependencies]'
    [ "$standalone" = "yes" ] && { echo; echo '[workspace]'; }
  } > "${dir}/Cargo.toml"
  printf '%s\n' "$body" > "${dir}/src/main.rs"
}

run_gate() {
  ( cd "$repo_root" && EXAMPLE_CRATES_SCAN_ROOT="$1" bash "$gate" ) > "${tmp}/out" 2>&1
  echo $?
}

# ── 1. a standalone crate that does not compile is caught ────────────────────
root="${tmp}/broken"; mkdir -p "$root"
make_crate "$root" "good_one" yes 'fn main() { println!("ok"); }'
make_crate "$root" "bad_one"  yes 'fn main() { let x: i32 = "not an integer"; }'
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q "bad_one" "${tmp}/out"; then
  pass "a standalone crate that does not compile fails, naming it"
else
  fail "a non-compiling crate was tolerated (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 2. all-good is green, and reports the count it checked ───────────────────
root="${tmp}/allgood"; mkdir -p "$root"
make_crate "$root" "one" yes 'fn main() {}'
make_crate "$root" "two" yes 'fn main() {}'
rc=$(run_gate "$root")
if [ "$rc" -eq 0 ] && grep -q "all 2 standalone" "${tmp}/out"; then
  pass "two compiling crates pass, and the count is reported"
else
  fail "a clean tree did not pass (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 3. an empty scan root fails LOUDLY, not vacuously ────────────────────────
root="${tmp}/empty"; mkdir -p "$root"
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q "the search is wrong" "${tmp}/out"; then
  pass "a scan root with no Cargo.toml fails loudly"
else
  fail "an empty scan root passed vacuously (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

# ── 4. only workspace members: the gate would be checking nothing ────────────
# The failure mode that turns this gate into decoration — every crate becomes a
# workspace member, the standalone list empties, and a naive gate reports OK.
root="${tmp}/members"; mkdir -p "$root"
make_crate "$root" "member" no 'fn main() {}'
rc=$(run_gate "$root")
if [ "$rc" -ne 0 ] && grep -q "would" "${tmp}/out"; then
  pass "a tree with no standalone crate fails rather than checking nothing"
else
  fail "a tree with nothing to check passed (rc=$rc)"; sed 's/^/       /' "${tmp}/out"
fi

echo
if [ "$failures" -eq 0 ]; then
  echo "OK: check-example-crates-compile.sh can fail, in all four ways."
else
  echo "✗ ${failures} self-test(s) failed."
fi
exit "$failures"
