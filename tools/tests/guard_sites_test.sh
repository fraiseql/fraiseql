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
command = "cargo test"

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

tree; sed -i 's/^name     = "session"/name     = "session"\nscan     = "sql"/' "${ws}/tools/guard-sites.toml"
expect 1 "family \`session\`: needs" "24. an unknown scan mode fails"

if [ "$failures" -ne 0 ]; then
  echo "guard sites gate: ${failures} case(s) FAILED"
  exit 1
fi
echo "guard sites gate: all cases pass"
