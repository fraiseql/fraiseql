#!/usr/bin/env bash
# guard_sites_test.sh — the red capability of tools/check-guard-sites.py.
#
# The gate passes over the tree it was written against, so green is equally consistent
# with "the scan matches nothing". Every branch is proved on a synthetic tree: each
# broken inventory or unrowed guard call goes RED, naming its position, the legitimate
# shapes stay GREEN, and a scan that reads nothing fails rather than reporting clean.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-guard-sites.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

src="crates/fraiseql-server/src"
ws="${tmp}/ws"

# A fresh tree: one guarded function, its killer test, and an inventory rowing it.
tree() {
  rm -rf "$ws"
  mkdir -p "${ws}/${src}/routes" "${ws}/crates/fraiseql-server/tests" "${ws}/tools" "${ws}/${src}/held"
  cat > "${ws}/${src}/routes/auth.rs" <<'EOF'
pub fn admit(state: &State, user: &User) -> Result<(), Refusal> {
    check_revocation(state.revocation.as_ref(), user)?;
    Ok(())
}

pub fn check_revocation(store: Option<&Store>, user: &User) -> Result<(), Refusal> {
    store.map_or(Ok(()), |s| s.check(user))
}
EOF
  printf 'fn f(t: Tenant) { t.go(); }\n' > "${ws}/${src}/held/mod.rs"
  cat > "${ws}/crates/fraiseql-server/tests/auth_test.rs" <<'EOF'
#[test]
fn an_ended_session_is_refused() {}
EOF
  cat > "${ws}/tools/guard-sites.toml" <<'EOF'
[[family]]
name     = "session"
patterns = ['\bcheck_revocation\s*\(']

[[shard]]
name    = "unit"
crate   = "crates/fraiseql-server"
command = "FRAISEQL_TEST_REQUIRE_POSTGRES=1 cargo test"

[[held]]
path = "crates/fraiseql-server/src/held"

[[site]]
id       = "session.admit"
file     = "crates/fraiseql-server/src/routes/auth.rs"
function = "admit"
anchor   = "check_revocation(state.revocation.as_ref(), user)"
mutant   = "check_revocation(None, user)"
killers  = ["auth::an_ended_session_is_refused"]
shard    = "unit"
EOF
}

# Append `$2` to the file `$1` of the tree.
add() { printf '%s\n' "$2" >> "${ws}/$1"; }
# Replace the text `$2` with `$3` in the file `$1` of the tree.
edit() { python3 - "${ws}/$1" "$2" "$3" <<'EOF'
import sys
path, old, new = sys.argv[1:]
text = open(path).read()
assert old in text, f"fixture edit: {old!r} not in {path}"
open(path, "w").write(text.replace(old, new, 1))
EOF
}

# Run the gate on the tree; expect exit `$1`, and output containing `$2` when given.
expect() {
  local want="$1" needle="${2:-}" name="$3"
  GUARD_SITES_ROOT="$ws" python3 "$gate" > "${tmp}/out" 2>&1
  local rc=$?
  if [ "$rc" -ne "$want" ]; then
    fail "${name} — rc=${rc} (want ${want})"
  elif [ -n "$needle" ] && ! grep -qF -- "$needle" "${tmp}/out"; then
    fail "${name} — output did not contain '${needle}'"
  else
    pass "$name"
  fi
}

echo "guard sites gate:"

tree
expect 0 "1 site(s) resolved" "1. an inventoried site passes"

tree; add "${src}/routes/auth.rs" $'\npub fn admit_again(state: &State, user: &User) {\n    let _ = check_revocation(state.revocation.as_ref(), user);\n}'
expect 1 "${src}/routes/auth.rs:11:13: session guard" "2. a new guard call without a row fails at its position"

tree; edit "${src}/routes/auth.rs" "check_revocation(state.revocation.as_ref(), user)?;" "check_revocation(None, user)?;"
expect 1 "the anchor occurs 0 time(s)" "3. an anchor no longer in its function fails"

tree; edit "${src}/routes/auth.rs" "    Ok(())" $'    check_revocation(state.revocation.as_ref(), user)?;\n    Ok(())'
expect 1 "the anchor occurs 2 time(s)" "4. an anchor twice in its function fails"

tree; edit "${src}/routes/auth.rs" "pub fn admit(" "pub fn admit_user("
expect 1 "\`fn admit\` is defined 0 time(s)" "5. a renamed function fails"

tree; edit "tools/guard-sites.toml" "auth::an_ended_session_is_refused" "auth::a_test_that_does_not_exist"
expect 1 "killer \`auth::a_test_that_does_not_exist\` is not a test function" "6. a killer that names no test fails"

tree; edit "tools/guard-sites.toml" 'killers  = ["auth::an_ended_session_is_refused"]' 'killers  = []'
expect 1 "names no killer" "7. a site with no killer fails"

tree; edit "tools/guard-sites.toml" 'mutant   = "check_revocation(None, user)"' 'mutant   = "check_revocation(state.revocation.as_ref(), user)"'
expect 1 "the mutant is the anchor" "8. a mutant equal to its anchor fails"

tree; edit "tools/guard-sites.toml" 'shard    = "unit"' 'shard    = "nightly"'
expect 1 "shard \`nightly\` is not declared" "9. an undeclared shard fails"

tree; edit "tools/guard-sites.toml" 'shard    = "unit"' $'shard    = "unit"\nkiler    = "typo"'
expect 1 "unknown key(s) ['kiler']" "10. an unknown key fails"

tree; edit "tools/guard-sites.toml" 'shard    = "unit"' $'shard    = "unit"\njoint    = "alone"'
expect 1 "joint \`alone\` has one member" "11. a joint with one member fails"

tree; rm -rf "${ws}/${src}/held"
expect 1 "held path \`crates/fraiseql-server/src/held\` does not exist" "12. a stale held path fails"

tree; add "${src}/held/mod.rs" 'fn g(s: &State, u: &User) { let _ = check_revocation(s.revocation.as_ref(), u); }'
expect 0 "" "13. a guard call under a held path passes"

tree; add "${src}/routes/auth.rs" $'#[cfg(test)]\nmod tests {\n    fn t() { let _ = check_revocation(None, &u()); }\n}'
expect 0 "" "14. a guard call in a test module passes"

tree; add "${src}/routes/auth.rs" $'// check_revocation(None, user) is how it used to be\nfn doc() -> &\'static str { "check_revocation(" }'
expect 0 "" "15. a guard call in a comment or string passes"

tree; add "${src}/routes/auth.rs" $'fn tested() -> char { \'"\' }\nfn later(s: &State, u: &User) { let _ = check_revocation(s.revocation.as_ref(), u); }'
expect 1 "${src}/routes/auth.rs:10:41: session guard" "16. a quote character does not hide the call after it"

tree; add "${src}/routes/auth.rs" $'fn legacy(s: &State, u: &User) { let _ = check_revocation(s.revocation.as_ref(), u); }'
cat >> "${ws}/tools/guard-sites.toml" <<'EOF'

[[exempt]]
file     = "crates/fraiseql-server/src/routes/auth.rs"
function = "legacy"
reason   = "no production caller; deciding issue to be filed"
EOF
expect 0 "" "17. a guard call inside an exempt function passes"

tree; sed -i 's/check_revocation(state.revocation.as_ref(), user)?;/admit_inner(state, user)?;/' "${ws}/${src}/routes/auth.rs"
edit "tools/guard-sites.toml" 'anchor   = "check_revocation(state.revocation.as_ref(), user)"' 'anchor   = "admit_inner(state, user)"'
edit "tools/guard-sites.toml" 'mutant   = "check_revocation(None, user)"' 'mutant   = "Ok::<(), Refusal>(())"'
expect 1 "family \`session\` matched nothing" "18. a family that matches nothing fails"

tree; rm -rf "${ws}/crates/fraiseql-server"; mkdir -p "${ws}/crates/other/src"
expect 1 "the scan read the wrong tree" "19. a scan of the wrong tree fails"

tree; printf '[[site' >> "${ws}/tools/guard-sites.toml"
expect 1 "cannot be read" "20. an unreadable inventory fails"

sql_family() {
  cat >> "${ws}/tools/guard-sites.toml" <<'EOF'

[[family]]
name     = "sql"
scan     = "strings"
patterns = ['tenant_key IS NOT DISTINCT FROM']
EOF
}

tree; sql_family
add "${src}/routes/auth.rs" $'fn find(c: &Conn) { c.query("SELECT * FROM t WHERE tenant_key IS NOT DISTINCT FROM $1"); }'
expect 1 "${src}/routes/auth.rs:9:52: sql guard" "21. a strings family finds its pattern inside a string literal"

tree; sql_family
add "${src}/routes/auth.rs" $'// tenant_key IS NOT DISTINCT FROM $1 is how rows are scoped\n/* tenant_key IS NOT DISTINCT FROM */ fn find() {}'
expect 1 "family \`sql\` matched nothing" "22. a strings family still ignores comments"

tree; add "${src}/routes/auth.rs" $'fn find(c: &Conn) { c.query("check_revocation(x)"); }'
expect 0 "" "23. a code family does not see inside strings"

tree; sql_family
add "${src}/routes/auth.rs" $'fn find(tenant_key: &str) { let tenant_key = tenant_key; info!(tenant_key IS NOT DISTINCT FROM); }'
expect 1 "family \`sql\` matched nothing" "24. a strings family does not see code outside strings"

tree; sql_family
add "${src}/routes/auth.rs" $'fn split(e: &str) -> bool { e.contains("tenant_key") && e.contains("IS NOT DISTINCT FROM") }'
expect 1 "family \`sql\` matched nothing" "25. a strings family does not join two strings"

tree; sed -i 's/^name     = "session"/name     = "session"\nscan     = "sql"/' "${ws}/tools/guard-sites.toml"
expect 1 "family \`session\`: needs" "26. an unknown scan mode fails"

tree; sql_family
add "${src}/routes/auth.rs" $'const SCOPED: &str = "WHERE id = $1 AND tenant_key IS NOT DISTINCT FROM $2";'
cat >> "${ws}/tools/guard-sites.toml" <<'EOF'

[[site]]
id       = "sql.scoped"
file     = "crates/fraiseql-server/src/routes/auth.rs"
function = "SCOPED"
anchor   = "tenant_key IS NOT DISTINCT FROM $2"
mutant   = "(tenant_key IS NOT DISTINCT FROM $2 OR TRUE)"
killers  = ["auth::an_ended_session_is_refused"]
shard    = "unit"
EOF
expect 0 "2 site(s) resolved" "27. a site can sit in a const item"

tree; add "${src}/routes/auth.rs" $'pub struct Gate;
impl Gate {
    pub fn admit(s: &State, u: &User) { let _ = check_revocation(s.revocation.as_ref(), u); }
}'
expect 1 "\`fn admit\` is defined 2 time(s)" "28. a function name defined twice in a file is ambiguous"
edit "tools/guard-sites.toml" 'function = "admit"' 'function = "Gate::admit"'
edit "tools/guard-sites.toml" 'anchor   = "check_revocation(state.revocation.as_ref(), user)"' 'anchor   = "check_revocation(s.revocation.as_ref(), u)"'
edit "tools/guard-sites.toml" 'mutant   = "check_revocation(None, user)"' 'mutant   = "check_revocation(None, u)"'
expect 1 "${src}/routes/auth.rs:2:5: session guard" "29. Type::name resolves to the impl's method, so the free fn's call is unrowed"
edit "tools/guard-sites.toml" 'function = "Gate::admit"' 'function = "::admit"'
expect 1 "the anchor occurs 0 time(s) in \`fn ::admit\`" "29b. ::name is the free fn alone, and the method's anchor is not in it"
edit "tools/guard-sites.toml" 'anchor   = "check_revocation(s.revocation.as_ref(), u)"' 'anchor   = "check_revocation(state.revocation.as_ref(), user)"'
edit "tools/guard-sites.toml" 'mutant   = "check_revocation(None, u)"' 'mutant   = "check_revocation(None, user)"'
expect 1 "${src}/routes/auth.rs:11:49: session guard" "29c. ::name resolves to the free fn, so the impl method's call is unrowed"
if grep -qF "${src}/routes/auth.rs:2:5" "${tmp}/out"; then fail "29d. the free fn's rowed call was reported"; else pass "29d. the free fn's rowed call was not reported"; fi

rls_family() {
  cat >> "${ws}/tools/guard-sites.toml" <<'EOF'

[[family]]
name     = "rls"
scan     = "strings"
sql      = true
patterns = ['ALTER TABLE \w+ ENABLE ROW LEVEL SECURITY']
EOF
}
migration="crates/fraiseql-server/migrations/01_rls.sql"
rls_migration() {
  mkdir -p "${ws}/crates/fraiseql-server/migrations"
  printf '%s\n' "$1" > "${ws}/${migration}"
}
rls_site() {
  cat >> "${ws}/tools/guard-sites.toml" <<EOF

[[site]]
id       = "rls.t"
file     = "${migration}"
${1:-}anchor   = "ALTER TABLE t ENABLE ROW LEVEL SECURITY"
mutant   = "ALTER TABLE t DISABLE ROW LEVEL SECURITY"
killers  = ["auth::an_ended_session_is_refused"]
shard    = "unit"
EOF
}

tree; rls_family; rls_migration $'CREATE TABLE t (id int);\nALTER TABLE t ENABLE ROW LEVEL SECURITY;'
expect 1 "${migration}:2:1: rls guard" "30. a sql family finds its pattern in a .sql file, at its position"

tree; rls_family; rls_migration $'ALTER TABLE t ENABLE ROW LEVEL SECURITY;'; rls_site
expect 0 "2 site(s) resolved" "31. a .sql site names no function and resolves in the whole file"

tree; rls_family; rls_migration $'ALTER TABLE t ENABLE ROW LEVEL SECURITY;'; rls_site $'function = "admit"\n'
expect 1 "except in a \`.sql\` file" "32. a .sql site that names a function fails"

tree; rls_family; rls_migration $'-- ALTER TABLE t ENABLE ROW LEVEL SECURITY;\n/* ALTER TABLE t ENABLE ROW LEVEL SECURITY; */'
expect 1 "family \`rls\` matched nothing" "33. a SQL comment is not a guard"

tree; rls_family; rls_migration $'SELECT \'-- not a comment\'; ALTER TABLE t ENABLE ROW LEVEL SECURITY;'
expect 1 "${migration}:1:28: rls guard" "34. a -- inside a literal does not hide the DDL after it"

tree; rls_family; sed -i 's/^sql      = true/sql      = "yes"/' "${ws}/tools/guard-sites.toml"
rls_migration $'ALTER TABLE t ENABLE ROW LEVEL SECURITY;'
expect 1 "\`sql = true\` only with" "35. a malformed sql key fails"

tree; sed -i 's/^name     = "session"/name     = "session"\nsql      = true/' "${ws}/tools/guard-sites.toml"
expect 1 "\`sql = true\` only with" "36. sql = true on a code family fails"

tree; add "${src}/routes/auth.rs" ""
cat >> "${ws}/tools/guard-sites.toml" <<'EOF'

[[family]]
name     = "rls"
scan     = "strings"
patterns = ['ALTER TABLE \w+ ENABLE ROW LEVEL SECURITY']
EOF
rls_migration $'ALTER TABLE t ENABLE ROW LEVEL SECURITY;'
expect 1 "family \`rls\` matched nothing" "37. a family without sql = true does not read .sql files"

tree; rls_family; mkdir -p "${ws}/crates/fraiseql-server/tests/fixtures"
printf 'ALTER TABLE t ENABLE ROW LEVEL SECURITY;\n' > "${ws}/crates/fraiseql-server/tests/fixtures/rls.sql"
mkdir -p "${ws}/examples/demo/sql"; printf 'ALTER TABLE e ENABLE ROW LEVEL SECURITY;\n' > "${ws}/examples/demo/sql/01.sql"
expect 1 "examples/demo/sql/01.sql:1:1: rls guard" "38. an example's .sql is scanned; a test fixture's is not"
if grep -qF "tests/fixtures/rls.sql" "${tmp}/out"; then fail "38b. a test fixture's .sql was scanned"; else pass "38b. a test fixture's .sql was not scanned"; fi

# An open policy family, and exemptions narrowed to one statement by an anchor.
open_family() {
  cat >> "${ws}/tools/guard-sites.toml" <<'EOF'

[[family]]
name     = "open"
scan     = "strings"
sql      = true
patterns = ['CREATE POLICY \w+ ON \w+ [^;]*?\bWITH CHECK \(true\)']
EOF
}
exempt_sql() {
  cat >> "${ws}/tools/guard-sites.toml" <<EOF

[[exempt]]
file     = "${migration}"
${2:-}anchor   = "$1"
reason   = "writes are the owner's"
EOF
}
open_two=$'CREATE POLICY p_one ON t FOR INSERT WITH CHECK (true);\nCREATE POLICY p_two ON u FOR INSERT WITH CHECK (true);'

tree; open_family; rls_migration "$open_two"; exempt_sql "CREATE POLICY p_one ON t FOR INSERT WITH CHECK (true)"
expect 1 "${migration}:2:1: open guard \`CREATE POLICY p_two" "39. an exempt statement does not exempt the open policy beside it"
if grep -qF "${migration}:1:1" "${tmp}/out"; then fail "39b. the exempt statement was reported"; else pass "39b. the exempt statement was not reported"; fi

tree; open_family; rls_migration "$open_two"
exempt_sql "CREATE POLICY p_one ON t FOR INSERT WITH CHECK (true)"; exempt_sql "CREATE POLICY p_two ON u FOR INSERT WITH CHECK (true)"
expect 0 "" "40. every open policy exempt by its statement passes"

tree; open_family; rls_migration "$open_two"; exempt_sql "CREATE POLICY p_three ON v FOR INSERT WITH CHECK (true)"
expect 1 "the anchor does not occur exactly once" "41. an exempt anchor that is not in the file fails"

tree; open_family; rls_migration "$open_two"; exempt_sql "CREATE POLICY p_one ON t FOR INSERT WITH CHECK (true)" $'function = "admit"\n'
expect 1 "names its function, except in a \`.sql\` file" "42. a .sql exemption that names a function fails"

tree; add "${src}/routes/auth.rs" $'fn legacy(s: &State, u: &User) {\n    let _ = check_revocation(s.revocation.as_ref(), u);\n    let _ = check_revocation(None, u);\n}'
cat >> "${ws}/tools/guard-sites.toml" <<'EOF'

[[exempt]]
file     = "crates/fraiseql-server/src/routes/auth.rs"
function = "legacy"
anchor   = "check_revocation(s.revocation.as_ref(), u)"
reason   = "no production caller; deciding issue to be filed"
EOF
expect 1 "${src}/routes/auth.rs:11:13: session guard" "43. an anchor narrows a function's exemption to its statement"

tree; shard_block=$(python3 -c "import re,sys; print(re.search(r'\[\[shard\]\][^[]*', open(sys.argv[1]).read()).group(0).rstrip())" "${ws}/tools/guard-sites.toml")
printf '\n%s\n' "$shard_block" >> "${ws}/tools/guard-sites.toml"
expect 1 "duplicate name" "44. a shard name defined twice fails"

tree; sed -i 's/command = "FRAISEQL_TEST_REQUIRE_POSTGRES=1 cargo test"/command = "cargo test"/' "${ws}/tools/guard-sites.toml"
expect 1 "must set FRAISEQL_TEST_REQUIRE_POSTGRES=1" "45. a shard whose missing database would skip, not fail, fails"

tree; sed -i 's/command = "FRAISEQL_TEST_REQUIRE_POSTGRES=1 cargo test"/command = "cargo test -- FRAISEQL_TEST_REQUIRE_POSTGRES=1"/' "${ws}/tools/guard-sites.toml"
expect 1 "must set FRAISEQL_TEST_REQUIRE_POSTGRES=1" "46. the variable as a test argument, not the environment, fails"

if [ "$failures" -ne 0 ]; then
  echo "guard sites gate: ${failures} case(s) FAILED"
  exit 1
fi
echo "guard sites gate: all cases pass"
