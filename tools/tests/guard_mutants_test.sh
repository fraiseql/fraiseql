#!/usr/bin/env bash
# guard_mutants_test.sh — the verdicts of tools/run-guard-mutants.py.
#
# A runner that called every mutant KILLED would pass the real inventory too. Each
# verdict is proved on a synthetic tree whose shard commands stand in for cargo: they
# read the mutated file and print what a test run would. Every run must also leave the
# tree byte-identical.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
runner="${repo_root}/tools/run-guard-mutants.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
ws="${tmp}/ws"

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

file="crates/fraiseql-server/src/gate.rs"

# A tree with two guarded functions and a shard per behaviour a test run can have.
tree() {
  rm -rf "$ws"
  mkdir -p "${ws}/$(dirname "$file")" "${ws}/tools"
  cat > "${ws}/${file}" <<'RS'
fn one() { guard_one(); }
fn two() { guard_two(); }
RS
  cat > "${ws}/tools/guard-sites.toml" <<'TOML'
[[shard]]
name    = "kills"
crate   = "crates/fraiseql-server"
command = "if grep -q skipped crates/fraiseql-server/src/gate.rs; then echo 'test gate::refuses ... FAILED'; exit 101; fi"

[[shard]]
name    = "kills-both"
crate   = "crates/fraiseql-server"
command = "if [ $(grep -c skipped crates/fraiseql-server/src/gate.rs) = 2 ]; then echo 'test gate::refuses ... FAILED'; exit 101; fi"

[[shard]]
name    = "passes"
crate   = "crates/fraiseql-server"
command = "echo 'test result: ok'"

[[shard]]
name    = "other"
crate   = "crates/fraiseql-server"
command = "if grep -q skipped crates/fraiseql-server/src/gate.rs; then echo 'test gate::unrelated ... FAILED'; exit 101; fi"

[[shard]]
name    = "nobuild"
crate   = "crates/fraiseql-server"
command = "if grep -q skipped crates/fraiseql-server/src/gate.rs; then echo 'error[E0425]: cannot find value'; echo 'error: could not compile'; exit 101; fi"

[[site]]
id       = "one"
file     = "crates/fraiseql-server/src/gate.rs"
function = "one"
anchor   = "guard_one();"
mutant   = "skipped();"
killers  = ["refuses"]
shard    = "SHARD"
TOML
  cp "${ws}/${file}" "${tmp}/original"
}

# Run the runner; expect exit `$1` and output containing `$2`, and an untouched tree.
expect() {
  local want="$1" needle="$2" name="$3"
  GUARD_SITES_ROOT="$ws" python3 "$runner" > "${tmp}/out" 2>&1
  local rc=$?
  if [ "$rc" -ne "$want" ]; then
    fail "${name} — rc=${rc} (want ${want})"
  elif ! grep -qF -- "$needle" "${tmp}/out"; then
    fail "${name} — output did not contain '${needle}'"
  elif ! cmp -s "${ws}/${file}" "${tmp}/original"; then
    fail "${name} — the tree was not restored"
  else
    pass "$name"
  fi
}

shard() { sed -i "s/shard    = \"SHARD\"/shard    = \"$1\"/" "${ws}/tools/guard-sites.toml"; }

echo "guard mutants runner:"

tree; shard kills
expect 0 "one: KILLED" "1. a mutant its killer fails on is killed"

tree; shard passes
expect 1 "FAILED one: SURVIVED" "2. a mutant every test passes on survived"

tree; shard other
expect 1 "FAILED one: KILLED-ELSEWHERE gate::unrelated" "3. a kill by a test the site does not name fails"

tree; shard nobuild
expect 1 "FAILED one: NOBUILD" "4. a mutant that does not build fails"

tree; shard kills-both
cat >> "${ws}/tools/guard-sites.toml" <<'TOML'
joint    = "pair"

[[site]]
id       = "two"
file     = "crates/fraiseql-server/src/gate.rs"
function = "two"
anchor   = "guard_two();"
mutant   = "skipped();"
killers  = ["refuses"]
shard    = "kills-both"
joint    = "pair"
TOML
expect 0 "one + two: KILLED" "5. a joint's members are mutated together"

tree; shard always-red
cat >> "${ws}/tools/guard-sites.toml" <<'TOML'

[[shard]]
name    = "always-red"
crate   = "crates/fraiseql-server"
command = "echo 'test gate::refuses ... FAILED'; exit 101"
TOML
expect 1 "FAILED shard always-red: BASELINE-RED gate::refuses" "6. a shard red before any mutant fails, killing nothing"

tree; shard always-empty
cat >> "${ws}/tools/guard-sites.toml" <<'TOML'

[[shard]]
name    = "always-empty"
crate   = "crates/fraiseql-server"
command = "echo '     Running tests/gated_e2e.rs (target/debug/deps/gated_e2e-0)'; echo 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'"
TOML
expect 1 "FAILED shard always-empty: BASELINE-EMPTY tests/gated_e2e.rs" "7. a shard binary that runs no test fails, killing nothing"

if [ "$failures" -ne 0 ]; then
  echo "guard mutants runner: ${failures} case(s) FAILED"
  exit 1
fi
echo "guard mutants runner: all cases pass"
