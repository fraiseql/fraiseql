#!/usr/bin/env bash
# auth_schema_qualification_test.sh — the red capability of
# tools/check-auth-schema-qualification.py.
#
# A passing run of the real tree says nothing about the gate: it would pass just as
# quietly if every rule matched nothing. What has to hold is that each rule reddens on the
# shape it names, that each rule STAYS GREEN on the adjacent shape that is correct (a
# comment or doc comment naming the old location, an engine-owned table in `core`, the
# engine's `_system.session_state`, an application `tb_user` outside the auth crate, a
# longer name sharing a prefix), and that the gate refuses a tree whose auth DDL it can no
# longer see.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-auth-schema-qualification.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

run_gate() {
  python3 "$gate" --root "$1" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

rc() { cat "${tmp}/rc"; }
said() { grep -F -- "$1" "${tmp}/out" >/dev/null; }

tables=(
  tb_user tb_auth_identity tb_password_credential tb_password_reset_token
  tb_email_verification_token tb_otp_code tb_otp_send_budget tb_mfa_enrollment
  tb_mfa_challenge tb_saml_replay tb_saml_idp tb_scim_group tb_scim_group_member
  tb_scim_token sessions
)

# A tree shaped like this repository's, correct as it stands: the auth DDL in `auth`, a
# store querying it, and every twin the rules must not fire on.
fixture() {
  local root="$1"
  mkdir -p "${root}/crates/fraiseql-auth/src" "${root}/crates/fraiseql-auth/tests"
  mkdir -p "${root}/crates/fraiseql-server/src" "${root}/crates/fraiseql-server/tests"
  mkdir -p "${root}/tools"

  {
    echo '//! Auth DDL. Formerly `core.tb_user` and `_system.sessions`; see the changelog.'
    echo 'pub const SCHEMA_SQL: &str = r#"'
    echo 'CREATE SCHEMA IF NOT EXISTS auth;'
    for t in "${tables[@]}"; do
      echo "CREATE TABLE IF NOT EXISTS auth.${t} (id UUID PRIMARY KEY);"
    done
    echo 'CREATE INDEX IF NOT EXISTS idx_user_email ON auth.tb_user (id);'
    echo '"#;'
  } > "${root}/crates/fraiseql-auth/src/ddl.rs"

  cat > "${root}/crates/fraiseql-auth/src/store.rs" <<'RS'
/// Reads `auth.tb_user`. It used to read core.tb_user, which is why
/// "core.tb_user" appears in this doc comment.
pub async fn find(pool: &PgPool) -> Result<()> {
    // A comment quoting the old query: SELECT id FROM core.tb_user
    /* and a block comment: DELETE FROM _system.sessions */
    let quote = '"';
    let _ = sqlx::query("SELECT id FROM auth.tb_user WHERE email = $1");
    let _ = sqlx::query("DELETE FROM auth.sessions WHERE expires_at < now()");
    let _ = format!("failed to update sessions: {quote}");
    Ok(())
}
RS

  cat > "${root}/crates/fraiseql-server/src/engine.rs" <<'RS'
pub const API_KEY_SQL: &str = "CREATE TABLE IF NOT EXISTS core.tb_api_key (id UUID)";
pub const ADMIN_SQL: &str = "SELECT 1 FROM core.tb_admin_token";
pub const STATE_SQL: &str = "SELECT 1 FROM _system.session_state";
pub const OPS_SQL: &str = "SELECT 1 FROM _system.async_operations";
pub const PREFIX_SQL: &str = "SELECT 1 FROM core.tb_user_secret";
pub const LOG_SQL: &str = "SELECT 1 FROM pg_tables WHERE schemaname = 'core' AND tablename = 'tb_entity_change_log'";
pub const STATE_CATALOG_SQL: &str = "SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = '_system' AND c.relname = 'session_state'";
RS

  # An application table conventionally named tb_user, outside the auth crate.
  cat > "${root}/crates/fraiseql-server/tests/app_tables.rs" <<'RS'
const SEED: &str = "CREATE TABLE tb_user (id INT); INSERT INTO tb_user VALUES (1);";
RS

  cat > "${root}/tools/conformance.py" <<'PY'
# The old location was core.tb_scim_token; this comment must not count.
QUERY = "SELECT count(*) FROM auth.tb_scim_token"
PY
}

expect_green() {
  local name="$1" root="$2"
  run_gate "$root"
  if [[ "$(rc)" == 0 ]]; then pass "$name"; else fail "$name (expected green)"; fi
}

expect_red() {
  local name="$1" root="$2" needle="$3"
  run_gate "$root"
  if [[ "$(rc)" != 0 ]] && said "$needle"; then
    pass "$name"
  else
    fail "$name (expected red naming: $needle)"
  fi
}

case_dir() { local d="${tmp}/$1"; rm -rf "$d"; mkdir -p "$d"; fixture "$d"; echo "$d"; }

echo "auth schema qualification gate:"

d="$(case_dir green)"
expect_green "the correct tree, with every twin, is green" "$d"

# Rule 1: a former schema in a code literal, in each place one can hide.
d="$(case_dir r1_store)"
echo 'const Q: &str = "SELECT id FROM core.tb_user";' >> "${d}/crates/fraiseql-auth/src/store.rs"
expect_red "rule 1: core.tb_user in an auth store" "$d" "store.rs:12: \`core.tb_user\`"

d="$(case_dir r1_sessions)"
echo 'const Q: &str = "DELETE FROM _system.sessions";' >> "${d}/crates/fraiseql-auth/src/store.rs"
expect_red "rule 1: _system.sessions" "$d" "\`_system.sessions\`"

d="$(case_dir r1_raw)"
printf 'const Q: &str = r#"\nUPDATE core.tb_mfa_challenge SET x = 1\n"#;\n' \
  >> "${d}/crates/fraiseql-auth/src/store.rs"
expect_red "rule 1: a raw string, line number of the match" "$d" "store.rs:13: \`core.tb_mfa_challenge\`"

d="$(case_dir r1_server)"
echo 'const Q: &str = "SELECT count(*) FROM core.tb_mfa_enrollment";' \
  >> "${d}/crates/fraiseql-server/src/engine.rs"
expect_red "rule 1: outside the auth crate" "$d" "engine.rs:8: \`core.tb_mfa_enrollment\`"

d="$(case_dir r1_test)"
echo 'const Q: &str = "SELECT to_regclass('"'"'core.tb_user'"'"')";' \
  >> "${d}/crates/fraiseql-server/tests/app_tables.rs"
expect_red "rule 1: in a test file" "$d" "app_tables.rs:2: \`core.tb_user\`"

d="$(case_dir r1_quoted)"
echo 'const Q: &str = "SELECT 1 FROM \"core\".\"tb_scim_group\"";' \
  >> "${d}/crates/fraiseql-auth/src/store.rs"
expect_red "rule 1: quoted identifiers" "$d" "store.rs:12: \`\"core\".\"tb_scim_group\"\`"

d="$(case_dir r1_python)"
echo 'OLD = f"SELECT 1 FROM core.tb_scim_token WHERE id = {1}"' >> "${d}/tools/conformance.py"
expect_red "rule 1: a Python f-string" "$d" "conformance.py:3: \`core.tb_scim_token\`"

# Rule 1, the catalog form: the old schema as a predicate beside an auth table's name.
d="$(case_dir r1_catalog_pg_tables)"
printf '%s\n' 'const Q: &str = "SELECT indexname FROM pg_indexes WHERE schemaname = '"'"'core'"'"' AND tablename IN ('"'"'tb_user'"'"', '"'"'tb_auth_identity'"'"')";' \
  >> "${d}/crates/fraiseql-auth/tests/account.rs"
expect_red "rule 1: pg_indexes filtered on schemaname = 'core'" "$d" "account.rs:1: \`schemaname = 'core'\` looks up auth table \`tb_user\`"

d="$(case_dir r1_catalog_nspname)"
printf '%s\n' 'const Q: &str = "SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = '"'"'_system'"'"' AND c.relname = '"'"'sessions'"'"'";' \
  >> "${d}/crates/fraiseql-server/tests/app_tables.rs"
expect_red "rule 1: pg_namespace filtered on nspname = '_system'" "$d" "\`nspname = '_system'\` looks up auth table \`sessions\`"

# Rule 2: unqualified in a SQL table position, inside the auth crate only.
d="$(case_dir r2_from)"
echo 'const Q: &str = "SELECT id FROM tb_auth_identity";' >> "${d}/crates/fraiseql-auth/src/store.rs"
expect_red "rule 2: FROM tb_auth_identity" "$d" "\`FROM tb_auth_identity\` names an auth table unqualified"

d="$(case_dir r2_insert)"
echo 'const Q: &str = "INSERT INTO sessions (id) VALUES ($1)";' \
  >> "${d}/crates/fraiseql-auth/tests/sessions.rs"
expect_red "rule 2: INSERT INTO sessions in an auth test" "$d" "\`INTO sessions\`"

d="$(case_dir r2_references)"
echo 'const Q: &str = "user_id UUID REFERENCES tb_user (id)";' >> "${d}/crates/fraiseql-auth/src/store.rs"
expect_red "rule 2: REFERENCES tb_user" "$d" "\`REFERENCES tb_user\`"

# Rule 3: the gate refuses a tree whose auth DDL it can no longer see.
d="$(case_dir r3_missing)"
sed -i '/auth\.tb_otp_send_budget (/d' "${d}/crates/fraiseql-auth/src/ddl.rs"
expect_red "rule 3: one table no longer created in auth" "$d" "no literal creates \`auth.tb_otp_send_budget\`"

d="$(case_dir r3_comment_only)"
sed -i 's/^CREATE TABLE IF NOT EXISTS auth.tb_saml_idp .*$//' "${d}/crates/fraiseql-auth/src/ddl.rs"
echo '// CREATE TABLE IF NOT EXISTS auth.tb_saml_idp (id UUID);' >> "${d}/crates/fraiseql-auth/src/ddl.rs"
expect_red "rule 3: a DDL line that survives only in a comment" "$d" "no literal creates \`auth.tb_saml_idp\`"

d="$(case_dir r3_test_only)"
sed -i 's/^CREATE TABLE IF NOT EXISTS auth.tb_saml_replay .*$//' "${d}/crates/fraiseql-auth/src/ddl.rs"
echo 'const D: &str = "CREATE TABLE IF NOT EXISTS auth.tb_saml_replay (id UUID)";' \
  >> "${d}/crates/fraiseql-auth/tests/ddl.rs"
expect_red "rule 3: DDL in a test does not count as the crate's" "$d" "no literal creates \`auth.tb_saml_replay\`"

d="$(case_dir empty)"
rm -rf "${d}/crates/fraiseql-auth"
expect_red "rule 3: no auth crate at all" "$d" "no literal creates \`auth.tb_user\`"

if [[ "$failures" -ne 0 ]]; then
  echo "✗ ${failures} case(s) failed"
  exit 1
fi
echo "✓ all cases"
