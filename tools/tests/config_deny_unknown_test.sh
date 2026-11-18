#!/usr/bin/env bash
# config_deny_unknown_test.sh — the red capability of tools/check-config-deny-unknown.py.
#
# Cases 1-11 are a one-crate tree whose `ServerConfig` reaches `Section`. A derived
# `Section` is read from its attributes; a hand-written one only through the test its
# allow-list entry names, so each way that entry can pin nothing must go RED, by name.
#
# Cases T1-T13 are a `fraiseql-cli` tree whose `TomlSchema` reaches tables there and in
# `fraiseql-core`: a table that drops a key it does not take, a flatten, an open table and
# its list, a type read through another, and a name two crates define, each RED by name.
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
    CONFIG_DENY_UNKNOWN_HAND_WRITTEN=allow.txt python3 "$gate" --only=server) > "${tmp}/out" 2>&1
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

# ── the walk from `TomlSchema` ───────────────────────────────────────────────────────

# new_toml_case <n> <cli source> <core source> <open list> [expected count]
new_toml_case() {
  root="${tmp}/toml$1"
  mk "${root}/crates/fraiseql-cli/src/config.rs" "#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TomlSchema {
    pub section: Section,
}

$2"
  mk "${root}/crates/fraiseql-core/src/schema.rs" "$3"
  mk "${root}/open.txt" "$4"
  toml_expected="${5:-2}"
}

run_toml_gate() {
  (cd "$root" && CONFIG_DENY_UNKNOWN_TOML_EXPECTED="$toml_expected" \
    CONFIG_DENY_UNKNOWN_OPEN_TABLES=open.txt \
    CONFIG_DENY_UNKNOWN_HAND_WRITTEN=none.txt python3 "$gate" --only=toml) > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

CLOSED='#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub a: u64,
}'
OPEN='#[derive(Deserialize)]
pub struct Section {
    pub a: u64,
}'
FLATTENED='#[derive(Deserialize)]
pub struct Section {
    pub name: String,
    #[serde(flatten)]
    pub rest: HashMap<String, String>,
}'
OPEN_ENTRY='Section crates/fraiseql-cli/src/config.rs its keys are names the author chooses'

# ── T1. A closed table needs no entry ───────────────────────────────────────────────
new_toml_case 1 "$CLOSED" '' ''
run_toml_gate
expect_green "a table with deny_unknown_fields"

# ── T2. A table without deny_unknown_fields is named ────────────────────────────────
new_toml_case 2 "$OPEN" '' ''
run_toml_gate
expect_red "Section" "an open table is named"
expect_red "no \`deny_unknown_fields\`" "an open table says why"

# ── T3. A flatten without deny_unknown_fields is named as a flatten ─────────────────
new_toml_case 3 "$FLATTENED" '' ''
run_toml_gate
expect_red "cannot deny unknown keys through a flatten" "a flatten is named as one"

# ── T4. A table listed open, with its reason, passes ────────────────────────────────
new_toml_case 4 "$FLATTENED" '' "$OPEN_ENTRY"
run_toml_gate
expect_green "a table listed open with its reason"

# ── T5. An open entry for a table that now denies unknown keys is stale ─────────────
new_toml_case 5 "$CLOSED" '' "$OPEN_ENTRY"
run_toml_gate
expect_red "but it denies unknown keys: remove the entry" "an entry for a table now closed"

# ── T6. An open entry the walk does not reach is stale ──────────────────────────────
new_toml_case 6 "$CLOSED" '' 'Gone crates/fraiseql-cli/src/config.rs no longer a table'
run_toml_gate
expect_red "reaches no table of that name in that file" "an entry for a table not reached"

# ── T7. A malformed open entry is refused, not read as no entry ─────────────────────
new_toml_case 7 "$FLATTENED" '' 'Section crates/fraiseql-cli/src/config.rs'
run_toml_gate
expect_red "malformed line" "a malformed open entry"

# ── T8. A type read through a closed one has its keys ───────────────────────────────
new_toml_case 8 '#[derive(Deserialize)]
#[serde(from = "FlatSection")]
pub struct Section {
    pub name: String,
    #[serde(flatten)]
    pub source: Source,
}

#[derive(Deserialize)]
#[serde(tag = "source", deny_unknown_fields)]
enum FlatSection {
    Jwt { name: String, claim: String },
}' '' '' 3
run_toml_gate
expect_green "a flatten read through a closed type"

# ── T9. ... and the type it is read through is checked ──────────────────────────────
new_toml_case 9 '#[derive(Deserialize)]
#[serde(from = "FlatSection")]
pub struct Section {
    pub name: String,
}

#[derive(Deserialize)]
#[serde(tag = "source")]
enum FlatSection {
    Jwt { name: String, claim: String },
}' '' '' 3
run_toml_gate
expect_red "FlatSection" "the type read through is checked"

# ── T10. A unit enum has no key to be unknown ───────────────────────────────────────
new_toml_case 10 '#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub mode: Mode,
}

#[derive(Deserialize)]
pub enum Mode {
    /// Every row checked, as `{table}_strict` names it.
    Strict,
    Lax,
}' '' '' 3
run_toml_gate
expect_green "a unit enum, braces in its doc"

# ── T11. A name both crates define is read by its written path ──────────────────────
DUAL_CORE='#[derive(Deserialize)]
pub struct Vector {
    pub dimensions: u32,
}'
DUAL_CLI='#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vector {
    pub dimensions: u32,
}'
new_toml_case 11 "#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub vector: Option<fraiseql_core::schema::Vector>,
}

${DUAL_CLI}" "$DUAL_CORE" '' 3
run_toml_gate
expect_red "crates/fraiseql-core/src/schema.rs" "a crate path picks that crate's definition"

# ── T12. ... by the \`use\` that brings it in ───────────────────────────────────────
new_toml_case 12 "use fraiseql_core::schema::{Other, Vector};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub vector: Option<Vector>,
}

${DUAL_CLI}" "$DUAL_CORE" '' 3
run_toml_gate
expect_red "crates/fraiseql-core/src/schema.rs" "a use picks that crate's definition"

# ── T14. A table named in a variant spread over lines is reached ───────────────────
new_toml_case 14 '#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub source: Source,
}

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Source {
    Jwt { claim: String, key: Key },
    Header {
        name: String,
        limit: Limit,
    },
    Literal(Literal),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Key {
    pub id: String,
}

pub struct Literal(pub String);

#[derive(Deserialize)]
pub struct Limit {
    pub max: u32,
}' '' '' 6
run_toml_gate
expect_red "Limit " "a type named in a multi-line struct variant is reached"

# ── T15. ... and a one-line variant reaches every field ─────────────────────────────
new_toml_case 15 '#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub source: Source,
}

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Source {
    Jwt { claim: String, key: Key },
}

#[derive(Deserialize)]
pub struct Key {
    pub id: String,
}' '' '' 4
run_toml_gate
expect_red "Key " "the second field of a one-line variant is reached"

# ── T16. A field type wrapped onto its own line is reached ─────────────────────────
new_toml_case 16 '#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub accounts: Option<
        std::collections::HashMap<String, Account>,
    >,
}

#[derive(Deserialize)]
pub struct Account {
    pub id: String,
}' '' '' 3
run_toml_gate
expect_red "Account " "a field type on a continuation line is reached"

# ── T17. A newtype is its inner value, not the next braced item ────────────────────
new_toml_case 17 '#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub id: Id,
}

#[derive(Deserialize)]
pub struct Id(pub String);

#[derive(Deserialize)]
pub struct Unreached {
    pub a: u64,
}' '' '' 3
run_toml_gate
expect_green "a newtype has no keys of its own"

# ── T18. A table named in a tuple variant is reached ───────────────────────────────
new_toml_case 18 '#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub source: Source,
}

#[derive(Deserialize)]
pub enum Source {
    Inline(Inner),
}

#[derive(Deserialize)]
pub struct Inner {
    pub a: u64,
}' '' '' 4
run_toml_gate
expect_red "Inner " "a table named in a tuple variant is reached"

# ── T13. The walk reaching fewer types than counted fails ───────────────────────────
new_toml_case 13 "$CLOSED" '' '' 3
run_toml_gate
expect_red "reached 2 types, expected 3" "a walk that measures less"

echo
if [ "$failures" -ne 0 ]; then
  echo "config_deny_unknown_test: ${failures} case(s) failed"
  exit 1
fi
echo "config_deny_unknown_test: all cases passed"
