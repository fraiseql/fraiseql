#!/usr/bin/env bash
# no_first_party_hs256_test.sh — the red capability of tools/check-no-first-party-hs256.sh.
#
# A passing run of the real tree shows none of these: an unlisted file fails, a comment is
# not a hit, a listed file that stopped matching fails, the one directory entry matches
# its own directory only, and a tree where nothing matches fails rather than passing.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-no-first-party-hs256.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

# $1 root, $2 exempt list, $3 pending list
run_gate() {
  HS256_GATE_ROOT="$1" HS256_GATE_EXEMPT="$2" HS256_GATE_PENDING="$3" bash "$gate" \
    > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

# A tree whose only hit is the exempt directory, so "matched nothing" never fires by accident.
new_tree() {
  local root="${tmp}/$1"
  mkdir -p "${root}/crates/fraiseql-arrow/src" "${root}/crates/fraiseql-server/tests" "${root}/examples"
  printf 'let h = Header::new(Algorithm::HS256);\n' > "${root}/crates/fraiseql-arrow/src/auth.rs"
  echo "$root"
}
exempt=$'\ncrates/fraiseql-arrow/\n'

# ── 1. An unlisted file naming the removed path fails, and is named ─────────────────
root=$(new_tree unlisted)
printf 'let c = ServerConfig { auth_hs256: None, ..Default::default() };\n' \
  > "${root}/crates/fraiseql-server/tests/a_test.rs"
run_gate "$root" "$exempt" ""
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "crates/fraiseql-server/tests/a_test.rs" "${tmp}/out" >/dev/null; then
  pass "an unlisted file naming auth_hs256 fails the gate"
else
  fail "an unlisted file naming auth_hs256 should fail the gate"
fi

# ── 2. A comment naming the path is prose, not code ─────────────────────────────────
root=$(new_tree comment)
printf '// Once booted with [auth_hs256]; now with [auth_issuer].\n  /// `JwtValidator` is gone.\n' \
  > "${root}/crates/fraiseql-server/tests/a_test.rs"
run_gate "$root" "$exempt" ""
if [ "$(cat "${tmp}/rc")" = "0" ]; then
  pass "a comment naming the removed path is not a hit"
else
  fail "a comment naming the removed path should not fail the gate"
fi

# ── 3. A pending entry that no longer matches fails, so the list only shrinks ───────
root=$(new_tree stale)
printf 'fn clean() {}\n' > "${root}/crates/fraiseql-server/tests/a_test.rs"
run_gate "$root" "$exempt" $'\ncrates/fraiseql-server/tests/a_test.rs\n'
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "no longer matching" "${tmp}/out" >/dev/null; then
  pass "a pending entry that stopped matching fails the gate"
else
  fail "a pending entry that stopped matching should fail the gate"
fi

# ── 4. A pending entry that still matches passes ────────────────────────────────────
root=$(new_tree pending)
printf 'let k = EncodingKey::from_secret(b"s");\n' > "${root}/crates/fraiseql-server/tests/a_test.rs"
run_gate "$root" "$exempt" $'\ncrates/fraiseql-server/tests/a_test.rs\n'
if [ "$(cat "${tmp}/rc")" = "0" ]; then
  pass "a pending file that still matches passes"
else
  fail "a pending file that still matches should pass"
fi

# ── 5. The directory entry covers its own directory, not a sibling sharing its prefix ─
root=$(new_tree prefix)
mkdir -p "${root}/crates/fraiseql-arrowx/src"
printf 'let v = JwtValidator::new();\n' > "${root}/crates/fraiseql-arrowx/src/lib.rs"
run_gate "$root" "$exempt" ""
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "crates/fraiseql-arrowx/src/lib.rs" "${tmp}/out" >/dev/null; then
  pass "an exempt directory does not cover a sibling sharing its name as a prefix"
else
  fail "an exempt directory must not cover crates/fraiseql-arrowx/"
fi

# ── 6. Matching nothing is a failure, not a pass ────────────────────────────────────
root="${tmp}/empty"; mkdir -p "${root}/crates/x/src" "${root}/examples"
printf 'fn main() {}\n' > "${root}/crates/x/src/main.rs"
run_gate "$root" "" ""
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "matched no file at all" "${tmp}/out" >/dev/null; then
  pass "a tree where nothing matches fails the gate"
else
  fail "a tree where nothing matches should fail the gate"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: ${failures} check(s) of tools/check-no-first-party-hs256.sh failed"
  exit 1
fi
echo "OK: tools/check-no-first-party-hs256.sh refuses what it must"
