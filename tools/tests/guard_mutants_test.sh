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

# The rig check reads DATABASE_URL: the cases run without one unless they set it.
unset DATABASE_URL
# A stand-in psql answering how many `vector` extensions the server has: `$1`.
stub_psql() {
  mkdir -p "${tmp}/bin"
  printf '#!/bin/sh\necho %s\n' "$1" > "${tmp}/bin/psql"
  chmod +x "${tmp}/bin/psql"
}

# A tree with two guarded functions and a shard per behaviour a test run can have.
tree() {
  rm -rf "$ws"
  mkdir -p "${ws}/$(dirname "$file")" "${ws}/tools"
  git -C "$ws" init -q
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

# Run the runner with any arguments after `$3`; expect exit `$1` and output containing
# `$2`, and an untouched tree.
expect() {
  local want="$1" needle="$2" name="$3"
  shift 3
  GUARD_SITES_ROOT="$ws" python3 "$runner" "$@" > "${tmp}/out" 2>&1
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
expect 0 "one + two: KILLED" "5b. selecting one member of a joint mutates the whole joint" --site one

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

tree
mkdir -p "${ws}/crates/fraiseql-server/migrations"
printf -- '-- RLS\nALTER TABLE t ENABLE ROW LEVEL SECURITY;\n' > "${ws}/crates/fraiseql-server/migrations/01_rls.sql"
cp "${ws}/crates/fraiseql-server/migrations/01_rls.sql" "${tmp}/sql-original"
cat > "${ws}/tools/guard-sites.toml" <<'TOML'
[[shard]]
name    = "sql"
crate   = "crates/fraiseql-server"
command = "if grep -q DISABLE crates/fraiseql-server/migrations/01_rls.sql; then echo 'test rls::reads_its_own ... FAILED'; exit 101; fi"

[[site]]
id       = "rls"
file     = "crates/fraiseql-server/migrations/01_rls.sql"
anchor   = "ALTER TABLE t ENABLE ROW LEVEL SECURITY"
mutant   = "ALTER TABLE t DISABLE ROW LEVEL SECURITY"
killers  = ["reads_its_own"]
shard    = "sql"
TOML
expect 0 "rls: KILLED" "8. a .sql site is mutated in its file, which names no function"
if cmp -s "${ws}/crates/fraiseql-server/migrations/01_rls.sql" "${tmp}/sql-original"; then
  pass "8b. the .sql file is restored"
else
  fail "8b. the .sql file was not restored"
fi

tree; shard kills
sed -i 's|^command = "if grep -q skipped crates/fraiseql-server/src/gate.rs; then echo .test gate::refuses ... FAILED.; exit 101; fi"|command = "if grep -q skipped crates/fraiseql-server/src/gate.rs; then mkdir -p proptest-regressions; echo seed > proptest-regressions/gate.txt; echo '"'"'test gate::refuses ... FAILED'"'"'; exit 101; fi"|' "${ws}/tools/guard-sites.toml"
expect 1 "FAILED proptest-regressions/gate.txt: LEFT-BEHIND" "9. a file a shard run writes under a mutant fails the run"

tree; shard kills; stub_psql 0
DATABASE_URL=postgres://rig/db PATH="${tmp}/bin:${PATH}" expect 2 "RIG: DATABASE_URL has no \`vector\` extension" "10. a database without pgvector stops the run as a rig fault"
if grep -qF "shard kills:" "${tmp}/out"; then fail "10b. a shard ran on the rig at fault"; else pass "10b. no shard ran on the rig at fault"; fi

tree; shard kills; stub_psql 1
DATABASE_URL=postgres://rig/db PATH="${tmp}/bin:${PATH}" expect 0 "one: KILLED" "11. a database with pgvector runs the shards"

# An interrupted run. The `blocks` shard stops with the mutant in place: it writes its pid
# to the sentinel and becomes a sleep the case then ends.
blocking_tree() {
  tree; shard blocks
  cat >> "${ws}/tools/guard-sites.toml" <<'TOML'

[[shard]]
name    = "blocks"
crate   = "crates/fraiseql-server"
command = "if grep -q skipped crates/fraiseql-server/src/gate.rs; then echo $$ > ${GUARD_TEST_SENTINEL}; exec sleep 20; fi"
TOML
  git -C "$ws" add -A
  git -C "$ws" -c user.name=guard -c user.email=guard@example.invalid commit -qm tree
}

# Start a run and wait until its mutant is in place; `run_pid` is the runner's.
start_blocked() {
  rm -f "${tmp}/sentinel"
  GUARD_TEST_SENTINEL="${tmp}/sentinel" GUARD_SITES_ROOT="$ws" python3 "$runner" > "${tmp}/out" 2>&1 &
  run_pid=$!
  for _ in $(seq 300); do
    [ -s "${tmp}/sentinel" ] && return 0
    sleep 0.1
  done
  return 1
}

# End the run with signal `$1`, then the shard's sleep should the runner not have.
interrupt() {
  kill "-$1" "$run_pid"
  wait "$run_pid" 2> /dev/null
  kill -9 "$(cat "${tmp}/sentinel")" 2> /dev/null
}

blocking_tree
if ! start_blocked; then
  fail "12. a run ended by SIGTERM restores its mutant — the mutant never ran"
else
  interrupt TERM
  if ! cmp -s "${ws}/${file}" "${tmp}/original"; then
    fail "12. a run ended by SIGTERM restores its mutant — the mutant was left in place"
  elif [ -e "${ws}/.git/guard-mutants" ]; then
    fail "12. a run ended by SIGTERM restores its mutant — its record was left behind"
  else
    pass "12. a run ended by SIGTERM restores its mutant"
  fi
fi

blocking_tree
if ! start_blocked; then
  fail "13. a mutant left by a killed run is restored by the next start — the mutant never ran"
else
  interrupt KILL
  if cmp -s "${ws}/${file}" "${tmp}/original"; then
    fail "13. precondition: SIGKILL leaves the mutant in place"
  else
    expect 0 "RESTORED ${file}: an interrupted run left a mutant in it" \
      "13. a mutant left by a killed run is restored by the next start" --list
  fi
fi

blocking_tree
if ! start_blocked; then
  fail "14. a file changed since the killed run stops the next — the mutant never ran"
else
  interrupt KILL
  echo "// written after the run was killed" >> "${ws}/${file}"
  GUARD_SITES_ROOT="$ws" python3 "$runner" --list > "${tmp}/out" 2>&1
  rc=$?
  if [ "$rc" -ne 2 ] || ! grep -qF "${file} differs from HEAD" "${tmp}/out"; then
    fail "14. a file changed since the killed run stops the next — rc=${rc}"
  elif ! grep -qF "// written after" "${ws}/${file}"; then
    fail "14. a file changed since the killed run stops the next — the change was overwritten"
  else
    pass "14. a file changed since the killed run stops the next"
  fi
  cp "${tmp}/original" "${ws}/${file}"
  expect 0 "1 mutant(s) selected" "14b. the run starts once the file matches HEAD" --list
  if [ -e "${ws}/.git/guard-mutants" ]; then
    fail "14c. the record is cleared once the file matches HEAD"
  else
    pass "14c. the record is cleared once the file matches HEAD"
  fi
fi

# Lanes. Listing only: no shard runs, so the commands need only name their builds. `a1`
# and `a2` compile one build (package `a`, the same features in another order).
tree
cat > "${ws}/tools/guard-sites.toml" <<'TOML'
[[shard]]
name    = "a1"
crate   = "crates/a"
command = "cargo test -p a --features 'x,y' --lib -- first"

[[shard]]
name    = "a2"
crate   = "crates/a"
command = "cargo test -p a --features 'y,x' --test second"

[[shard]]
name    = "b"
crate   = "crates/b"
command = "cargo test -p b --lib"

[[site]]
id = "s1"
file = "crates/fraiseql-server/src/gate.rs"
function = "one"
anchor = "guard_one();"
mutant = "skipped();"
killers = ["refuses"]
shard = "a1"

[[site]]
id = "s2"
file = "crates/fraiseql-server/src/gate.rs"
function = "two"
anchor = "guard_two();"
mutant = "skipped();"
killers = ["refuses"]
shard = "a2"

[[site]]
id = "s3"
file = "crates/fraiseql-server/src/gate.rs"
function = "one"
anchor = "guard_one();"
mutant = "skipped();"
killers = ["refuses"]
shard = "b"
TOML
lane_ids() { GUARD_SITES_ROOT="$ws" python3 "$runner" --list --lane "$1" | grep -oE '^s[0-9]' | tr '\n' ' '; }
first="$(lane_ids 1/2)"; second="$(lane_ids 2/2)"
if [ "$first" != "s1 s2 " ] || [ "$second" != "s3 " ]; then
  echo "lane 1/2: '${first}', lane 2/2: '${second}'" > "${tmp}/out"
  fail "15. each build runs in one lane, and each lane runs a share"
else
  pass "15. each build runs in one lane, and each lane runs a share"
fi
if [ "$(lane_ids 1/1)" != "s1 s2 s3 " ]; then
  lane_ids 1/1 > "${tmp}/out"
  fail "15b. one lane runs the whole selection"
else
  pass "15b. one lane runs the whole selection"
fi
expect 2 "--lane takes I/N" "15c. a lane outside its count is refused" --list --lane 3/2

if [ "$failures" -ne 0 ]; then
  echo "guard mutants runner: ${failures} case(s) FAILED"
  exit 1
fi
echo "guard mutants runner: all cases pass"
