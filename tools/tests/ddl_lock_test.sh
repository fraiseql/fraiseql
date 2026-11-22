#!/usr/bin/env bash
# Unit tests for tools/check-ddl-lock.py.
#
# Run directly:  bash tools/tests/ddl_lock_test.sh
# Exits non-zero if any assertion fails.
#
# Most cases are shapes the gate must REJECT: DDL text reaching an executor without the
# lock, however it was built or passed along.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-ddl-lock.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# check <name> <expected-exit> <expected text in the output, or ""> ; files come from
# the FILES array: pairs of (relative path, contents).
check() {
    local name="$1" want_exit="$2" want_text="$3"
    TESTS_RUN=$((TESTS_RUN + 1))
    local dir="$WORK/$name"
    mkdir -p "$dir"
    local i
    for ((i = 0; i < ${#FILES[@]}; i += 2)); do
        mkdir -p "$(dirname "$dir/${FILES[i]}")"
        printf '%s\n' "${FILES[i + 1]}" >"$dir/${FILES[i]}"
    done
    local out rc
    set +e
    out="$(DDL_LOCK_ROOT="$dir" python3 "$GATE" 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want_exit" ] || { [ -n "$want_text" ] && ! grep -qF -- "$want_text" <<<"$out"; }; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        echo "FAIL: $name: exit $rc, want $want_exit${want_text:+, with \"$want_text\"}"
        sed 's/^/    /' <<<"$out"
    fi
}

# One file.
gate() {
    FILES=(lib.rs "$3")
    check "$1" "$2" "${4:-}"
}

# --- DDL reaching an executor unlocked: refused --------------------------------------

gate literal 1 '
async fn f(pool: &PgPool) {
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS t (a INT)").execute(pool).await;
}' "lib.rs:3"

gate lowercase_literal 1 '
async fn f(client: &Client) {
    client.batch_execute("create schema if not exists core").await;
}'

gate constant 1 '
pub const SCHEMA_SQL: &str = "CREATE SCHEMA IF NOT EXISTS core;";
async fn f(pool: &PgPool) {
    sqlx::raw_sql(SCHEMA_SQL).execute(pool).await;
}' "lib.rs:4"

gate raw_string_constant 1 '
pub const SCHEMA_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS core.t (
        a INT
    );
"#;
async fn f(pool: &PgPool) {
    sqlx::raw_sql(SCHEMA_SQL).execute(pool).await;
}'

gate function_returning_ddl 1 '
pub const fn schema_sql() -> &'"'"'static str {
    "CREATE TABLE IF NOT EXISTS t (a INT);"
}
async fn f(pool: &PgPool) {
    sqlx::raw_sql(schema_sql()).execute(pool).await;
}'

gate format_bound_locally 1 '
async fn f(adapter: &A, schema: &str) {
    let ddl = format!("CREATE SCHEMA IF NOT EXISTS {schema}");
    adapter.execute_raw_query(&ddl).await;
}' "lib.rs:4"

gate format_built_by_a_function 1 '
pub fn create_schema_ddl(key: &str) -> Result<String> {
    Ok(format!("CREATE SCHEMA IF NOT EXISTS tenant_{key}"))
}
async fn provision(key: &str, adapter: &A) -> Result<()> {
    let ddl = create_schema_ddl(key)?;
    adapter.execute_raw_query(&ddl).await?;
    Ok(())
}' "lib.rs:7"

gate built_by_a_writer 1 '
fn write_view(sql: &mut String, name: &str) {
    sql.push_str(&format!("CREATE OR REPLACE VIEW {name} AS SELECT 1;"));
}
fn view_sql(name: &str) -> String {
    let mut sql = String::new();
    sql.push_str("-- generated");
    write_view(&mut sql, name);
    sql
}
async fn validate(sql: &str) {
    let txn = client.transaction().await;
    txn.batch_execute(sql).await;
}
async fn run(name: &str) {
    let sql = view_sql(name);
    validate(&sql).await;
}' "lib.rs:17"

gate concat 1 '
const DDL: &str = concat!("CREATE TABLE IF NOT EXISTS t ", "(a INT)");
async fn f(pool: &PgPool) {
    sqlx::raw_sql(DDL).execute(pool).await;
}'

FILES=(lib.rs '
const DDL: &str = include_str!("schema.sql");
async fn f(pool: &PgPool) {
    sqlx::raw_sql(DDL).execute(pool).await;
}' schema.sql 'CREATE TABLE IF NOT EXISTS t (a INT);')
check include_str 1 "lib.rs:4"

FILES=(a/schema.rs '
pub const STORE_SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";' b/store.rs '
async fn init(pool: &PgPool) {
    sqlx::raw_sql(crate::a::schema::STORE_SCHEMA_SQL).execute(pool).await;
}')
check constant_from_another_file 1 "store.rs:3"

FILES=(a/schema.rs '
pub const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";' b/store.rs '
use crate::a::schema::{Other, SCHEMA_SQL as IDENTITY_SQL};
async fn init(pool: &PgPool) {
    sqlx::raw_sql(IDENTITY_SQL).execute(pool).await;
}')
check constant_imported_under_another_name 1 "store.rs:4"

gate split_line_call 1 '
const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";
async fn f(pool: &PgPool) {
    sqlx::raw_sql(
        SCHEMA_SQL,
    )
    .execute(pool)
    .await;
}'

gate tokio_postgres_execute_with_params 1 '
async fn f(conn: &Client) {
    conn.execute(
        "CREATE SEQUENCE IF NOT EXISTS seq_t START 1",
        &[],
    )
    .await;
}'

gate sqlx_query 1 '
async fn f(pool: &PgPool) {
    sqlx::query("ALTER TABLE t ADD COLUMN IF NOT EXISTS b INT").execute(pool).await;
}'

gate assert_sql_safe_format 1 '
async fn f(pool: &PgPool, s: &str) {
    // Reason: `s` is a validated identifier.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA IF NOT EXISTS {s}"))).execute(pool).await;
}'

gate a_wrapper_that_passes_its_text_on 1 '
const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";
async fn apply_script(pool: &PgPool, sql: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(pool).await;
}
async fn init(pool: &PgPool) {
    apply_script(pool, SCHEMA_SQL).await;
}' "lib.rs:7"

gate a_method_that_passes_its_text_on 1 '
const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";
impl Store {
    async fn apply_script(&self, sql: &str) {
        self.client.batch_execute(sql).await;
    }
    async fn init(&self) {
        self.apply_script(SCHEMA_SQL).await;
    }
}' "lib.rs:8"

gate the_lock_beside_the_ddl_not_around_it 1 '
const A_SQL: &str = "CREATE TABLE IF NOT EXISTS a (x INT);";
const B_SQL: &str = "CREATE TABLE IF NOT EXISTS b (x INT);";
async fn f(client: &Client) {
    client.batch_execute(&format!("{}\n{B_SQL}", lock_prefixed(A_SQL))).await;
}'

gate a_reason_does_not_waive_it 1 '
const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";
async fn f(pool: &PgPool) {
    // Reason: the crate'"'"'s own DDL constant.
    sqlx::raw_sql(sqlx::AssertSqlSafe(SCHEMA_SQL)).execute(pool).await;
}'

gate a_waiver_on_the_statement_before_does_not_cover 1 '
const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";
async fn f(pool: &PgPool) {
    // No DDL lock: a third-party extension.
    sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS pgcrypto").execute(pool).await;
    sqlx::raw_sql(SCHEMA_SQL).execute(pool).await;
}' "lib.rs:6"

# --- the lock, or a stated reason: accepted -----------------------------------------

gate lock_prefixed 0 '
const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";
async fn f(pool: &PgPool) {
    // Reason: the crate'"'"'s own DDL constant, behind the fixed lock statement.
    sqlx::raw_sql(sqlx::AssertSqlSafe(fraiseql_guard::ddl::lock_prefixed(SCHEMA_SQL)))
        .execute(pool)
        .await;
}'

gate lock_prefixed_on_batch_execute 0 '
const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";
async fn f(client: &Client) {
    client.batch_execute(&lock_prefixed(SCHEMA_SQL)).await;
}'

gate locked_statement_bound_locally 0 '
pub fn create_schema_ddl(key: &str) -> Result<String> {
    Ok(format!("CREATE SCHEMA IF NOT EXISTS tenant_{key}"))
}
async fn provision(key: &str, adapter: &A) -> Result<()> {
    let ddl = fraiseql_guard::ddl::locked_statement(&create_schema_ddl(key)?)
        .map_err(|e| E::validation(e.to_string()))?;
    adapter.execute_raw_query(&ddl).await?;
    Ok(())
}'

gate a_locked_wrapper 0 '
const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS t (a INT);";
async fn run_script(pool: &PgPool, ddl: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(lock_prefixed(ddl))).execute(pool).await;
}
async fn init(pool: &PgPool) {
    run_script(pool, SCHEMA_SQL).await;
}'

gate waived 0 '
async fn f(client: &Client) {
    // No DDL lock: a third-party extension a compile step reads from.
    client
        .batch_execute("CREATE EXTENSION IF NOT EXISTS plpgsql_check")
        .await;
}'

gate test_code 0 '
#[cfg(test)]
mod tests {
    async fn setup(pool: &PgPool) {
        sqlx::raw_sql("CREATE TABLE IF NOT EXISTS t (a INT)").execute(pool).await;
    }
}'

gate ddl_in_comments_and_messages 0 '
/// Runs `CREATE TABLE IF NOT EXISTS` through the lock.
async fn f(pool: &PgPool) {
    // CREATE SCHEMA IF NOT EXISTS core
    let rows = sqlx::query("SELECT 1").fetch_all(pool).await.context("could not CREATE TABLE");
}'

gate not_ddl 0 '
async fn f(pool: &PgPool) {
    sqlx::query("INSERT INTO t (created_at) VALUES (now())").execute(pool).await;
    sqlx::query("SELECT create_table_name FROM t").execute(pool).await;
}'

# --- rule 2: the lock key ------------------------------------------------------------

gate another_key 1 '
const MY_KEY: i64 = 42;
async fn f(tx: &mut Tx) {
    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(MY_KEY).execute(&mut *tx).await;
}' "other than"

gate another_key_inline 1 '
async fn f(client: &Client) {
    client.batch_execute("SELECT pg_advisory_lock(42); CREATE TABLE t (a INT);").await;
}' "other than"

gate the_ddl_key 0 '
async fn f(tx: &mut Tx) {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(fraiseql_guard::ddl::MIGRATION_LOCK_KEY)
        .execute(&mut *tx)
        .await;
}'

gate another_key_waived 0 '
const RING_LOCK: i64 = 42;
async fn f(tx: &mut Tx) {
    // Not the DDL lock: it orders writes to the ring'"'"'s rows.
    sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(RING_LOCK).execute(&mut *tx).await;
}'

gate a_try_lock_is_a_lease 0 '
async fn f(pool: &PgPool) {
    sqlx::query("SELECT pg_try_advisory_lock($1)").bind(7_i64).execute(pool).await;
}'

echo "ddl lock gate: $TESTS_RUN cases, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
