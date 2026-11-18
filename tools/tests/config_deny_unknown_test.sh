#!/usr/bin/env bash
# config_deny_unknown_test.sh — the red capability of tools/check-config-deny-unknown.py
# for a hand-written `Deserialize`.
#
# Every case is a one-crate tree whose `ServerConfig` reaches `Section`. A derived
# `Section` is read from its attributes; a hand-written one only through the test its
# allow-list entry names, so each way that entry can pin nothing must go RED, by name.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-config-deny-unknown.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

mk() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }

ROOT_CONFIG='#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub section: Section,
}'

HAND_WRITTEN='pub struct Section {
    pub a: u64,
}

impl<'"'"'de> Deserialize<'"'"'de> for Section {
    fn deserialize<D: serde::Deserializer<'"'"'de>>(d: D) -> Result<Self, D::Error> {
        todo!()
    }
}'

PINNING_TEST='#[test]
fn an_unknown_key_is_refused() {}'

# new_case <n> <section source> <test source> <allow-list>
new_case() {
  root="${tmp}/case$1"
  mk "${root}/crates/server/src/config.rs" "${ROOT_CONFIG}

$2"
  mk "${root}/crates/server/src/config/tests.rs" "$3"
  mk "${root}/allow.txt" "$4"
}

run_gate() {
  (cd "$root" && CONFIG_DENY_UNKNOWN_EXPECTED=2 \
    CONFIG_DENY_UNKNOWN_HAND_WRITTEN=allow.txt python3 "$gate") > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

expect_green() {
  if [ "$(cat "${tmp}/rc")" = 0 ]; then pass "$1"; else fail "$1"; fi
}

expect_red() { # expect_red <text that must be printed> <label>
  if [ "$(cat "${tmp}/rc")" = 1 ] && grep -F -- "$1" "${tmp}/out" >/dev/null; then
    pass "$2"
  else
    fail "$2"
  fi
}

PIN='Section crates/server/src/config/tests.rs an_unknown_key_is_refused'

# ── 1. A derived section that denies unknown keys needs no entry ────────────────────
new_case 1 '#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub a: u64,
}' '' ''
run_gate
expect_green "a derived section with deny_unknown_fields"

# ── 2. A hand-written section with its pinning test listed ──────────────────────────
new_case 2 "$HAND_WRITTEN" "$PINNING_TEST" "$PIN"
run_gate
expect_green "a hand-written section pinned by a running test"

# ── 3. A hand-written section with no entry fails, by name ──────────────────────────
new_case 3 "$HAND_WRITTEN" "$PINNING_TEST" ''
run_gate
expect_red "Section" "an unlisted hand-written section is named"
expect_red "no test is listed for it" "an unlisted hand-written section says why"

# ── 4. Any path to the trait and any generics are a hand-written impl ───────────────
new_case 4 "${HAND_WRITTEN//impl<\'de> Deserialize/impl<\'de, T: Clone> serde::de::Deserialize}" "$PINNING_TEST" ''
run_gate
expect_red "no test is listed for it" "impl<'de, T> serde::de::Deserialize<'de> is hand-written"

# ── 5. The listed test does not exist ───────────────────────────────────────────────
new_case 5 "$HAND_WRITTEN" '#[test]
fn something_else() {}' "$PIN"
run_gate
expect_red "is not in crates/server/src/config/tests.rs" "a listed test that does not exist"

# ── 6. The listed test file does not exist ──────────────────────────────────────────
new_case 6 "$HAND_WRITTEN" "$PINNING_TEST" 'Section crates/server/src/missing.rs an_unknown_key_is_refused'
run_gate
expect_red "does not exist" "a listed test file that does not exist"

# ── 7. The listed fn is not a test ──────────────────────────────────────────────────
new_case 7 "$HAND_WRITTEN" 'fn an_unknown_key_is_refused() {}' "$PIN"
run_gate
expect_red "is not a \`#[test]\`" "a listed fn that is not a test"

# ── 8. The listed test is ignored ───────────────────────────────────────────────────
new_case 8 "$HAND_WRITTEN" '#[test]
#[ignore = "later"]
fn an_unknown_key_is_refused() {}' "$PIN"
run_gate
expect_red "is \`#[ignore]\`d" "a listed test that is ignored"

# ── 9. An entry whose struct is no longer written by hand is stale ──────────────────
new_case 9 '#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub a: u64,
}' "$PINNING_TEST" "$PIN"
run_gate
expect_red "remove the entry" "an entry that outlived its hand-written impl"

# ── 10. A malformed entry is refused, not read as no entry ──────────────────────────
new_case 10 "$HAND_WRITTEN" "$PINNING_TEST" 'Section an_unknown_key_is_refused'
run_gate
expect_red "malformed line" "a malformed entry"

# ── 11. A test with doc comments and `#[tokio::test]` is a test ─────────────────────
new_case 11 "$HAND_WRITTEN" '/// Refuses an unknown key.
#[tokio::test]
async fn an_unknown_key_is_refused() {}' "$PIN"
run_gate
expect_green "a documented #[tokio::test] pins"

echo
if [ "$failures" -ne 0 ]; then
  echo "config_deny_unknown_test: ${failures} case(s) failed"
  exit 1
fi
echo "config_deny_unknown_test: all cases passed"
