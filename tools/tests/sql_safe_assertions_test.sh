#!/usr/bin/env bash
# Unit tests for tools/check-sql-safe-assertions.py.
#
# Run directly:  bash tools/tests/sql_safe_assertions_test.sh
# Exits non-zero if any assertion fails.
#
# Most cases are shapes the gate must REJECT: a reason that belongs to a neighbouring
# statement, or one cut off by a blank line, must not cover a call.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-sql-safe-assertions.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# assert_gate <name> <expected-exit> <contents of a .rs file>
assert_gate() {
    local name="$1" want_exit="$2" contents="$3"
    TESTS_RUN=$((TESTS_RUN + 1))
    local dir="$WORK/$name"
    mkdir -p "$dir"
    printf '%s\n' "$contents" >"$dir/lib.rs"
    local out rc
    set +e
    out="$(SQL_SAFE_ROOT="$dir" python3 "$GATE" 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want_exit" ]; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        echo "FAIL: $name: exit $rc, want $want_exit"
        echo "$out" | sed 's/^/    /'
    fi
}

assert_gate reason_above 0 '
fn f(t: &str) {
    // Reason: `t` is a constant.
    sqlx::query(sqlx::AssertSqlSafe(t));
}'

assert_gate reason_on_the_line 0 '
fn f(t: &str) {
    sqlx::query(sqlx::AssertSqlSafe(t)); // Reason: `t` is a constant.
}'

assert_gate reason_above_a_statement_that_continues_onto_the_call 0 '
fn f(t: &str) {
    // Reason: `t` is a constant.
    let rows =
        sqlx::query(
            sqlx::AssertSqlSafe(t),
        );
}'

assert_gate reason_in_a_comment_block_above 0 '
fn f(t: &str) {
    // Reason: `t` is a constant.
    // More about it.
    sqlx::query(sqlx::AssertSqlSafe(t));
}'

assert_gate no_reason 1 '
fn f(t: &str) {
    sqlx::query(sqlx::AssertSqlSafe(t));
}'

assert_gate a_comment_that_is_no_reason 1 '
fn f(t: &str) {
    // `t` is a constant.
    sqlx::query(sqlx::AssertSqlSafe(t));
}'

assert_gate an_empty_reason 1 '
fn f(t: &str) {
    // Reason:
    sqlx::query(sqlx::AssertSqlSafe(t));
}'

assert_gate a_blank_line_cuts_the_reason_off 1 '
fn f(t: &str) {
    // Reason: `t` is a constant.

    sqlx::query(sqlx::AssertSqlSafe(t));
}'

assert_gate the_neighbours_reason_does_not_cover_a_call 1 '
fn f(a: &str, b: &str) {
    // Reason: `a` is a constant.
    sqlx::query(sqlx::AssertSqlSafe(a));
    sqlx::query(sqlx::AssertSqlSafe(b));
}'

assert_gate a_reason_for_the_statement_above_does_not_cover_the_next_line 1 '
fn f(a: &str, b: &str) {
    // Reason: `a` is a constant.
    let x = 1;
    sqlx::query(sqlx::AssertSqlSafe(b));
}'

assert_gate an_import_is_refused 1 '
use sqlx::AssertSqlSafe;'

assert_gate an_alias_is_refused 1 '
use sqlx::AssertSqlSafe as Trusted;'

assert_gate prose_and_strings_are_not_calls 0 '
//! `AssertSqlSafe(x)` is described here.
fn f() {
    let s = "AssertSqlSafe(t)";
    let r = r#"AssertSqlSafe(t)"#;
    /* AssertSqlSafe(t) */
}'

assert_gate a_reason_inside_a_string_is_no_reason 1 '
fn f(t: &str) {
    let note = "// Reason: no";
    sqlx::query(sqlx::AssertSqlSafe(t));
}'

# Rule 3: text handed to a sqlx::QueryBuilder is a literal or has a reason.
assert_gate builder_literal_text 0 '
fn f(id: i64) {
    let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new("SELECT 1");
    qb.push(" WHERE id = ").push_bind(id);
    qb.push(
        r" ORDER BY id",
    );
}'

assert_gate builder_runtime_text_without_a_reason 1 '
fn f(column: &str) {
    let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new("SELECT 1");
    qb.push(column);
}'

assert_gate builder_runtime_text_with_a_reason 0 '
fn f(column: &str) {
    let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new("SELECT 1");
    // Reason: `column` is a constant.
    qb.push(column);
}'

assert_gate builder_runtime_text_later_in_a_chain 1 '
fn f(query: &mut sqlx::QueryBuilder<sqlx::Postgres>, column: &str, id: i64) {
    query
        .push(" AND ")
        .push_bind(id)
        .push(format!(" OR {column} IS NULL"));
}'

assert_gate builder_reason_on_the_chain_line 0 '
fn f(query: &mut sqlx::QueryBuilder<sqlx::Postgres>, column: &str) {
    query
        .push(" AND ")
        // Reason: `column` is a constant.
        .push(column);
}'

assert_gate builder_reason_for_another_link_does_not_cover 1 '
fn f(query: &mut sqlx::QueryBuilder<sqlx::Postgres>, a: &str, b: &str) {
    query
        // Reason: `a` is a constant.
        .push(a)
        .push(b);
}'

assert_gate builder_constructor_runtime_text 1 '
fn f(sql: String) {
    let qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(sql);
}'

assert_gate builder_untyped_binding_is_followed 1 '
fn f(column: &str) {
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 1");
    qb.push_unseparated(column);
}'

assert_gate builder_separated_is_followed 1 '
fn f(qb: &mut sqlx::QueryBuilder<sqlx::Postgres>, column: &str) {
    let mut list = qb.separated(", ");
    list.push(column);
}'

assert_gate a_vec_push_is_not_builder_text 0 '
fn f(qb: &mut sqlx::QueryBuilder<sqlx::Postgres>, name: String) {
    let mut names = Vec::new();
    names.push(name);
    qb.push(" AND true");
}'

assert_gate a_builder_import_is_refused 1 '
use sqlx::QueryBuilder;'

assert_gate a_builder_import_in_braces_is_refused 1 '
use sqlx::{Postgres, QueryBuilder};'

assert_gate a_builder_module_import_is_refused 1 '
use sqlx::query_builder;'

assert_gate a_sqlx_glob_import_is_refused 1 '
use sqlx::*;'

assert_gate a_renamed_sqlx_is_refused 1 '
use sqlx as db;'

assert_gate a_sqlx_renamed_through_self_is_refused 1 '
use sqlx::{self as db, PgPool};'

assert_gate a_sqlx_renamed_as_a_crate_is_refused 1 '
extern crate sqlx as db;'

assert_gate a_trait_import_for_its_methods_is_allowed 0 '
use sqlx::{PgPool, Row as _};'

assert_gate a_builder_alias_is_refused 1 '
type Qb = sqlx::QueryBuilder<sqlx::Postgres>;'

assert_gate a_builder_returned_is_refused 1 '
fn make() -> sqlx::QueryBuilder<sqlx::Postgres> {
    sqlx::QueryBuilder::new("SELECT 1")
}'

assert_gate builder_prose_and_strings_are_not_calls 0 '
//! `sqlx::QueryBuilder::new(sql)` and `qb.push(column)` are described here.
fn f() {
    let s = "sqlx::QueryBuilder::new(sql)";
}'

# Rule 5: the real tree has calls; the gate run on it passes and counts them.
TESTS_RUN=$((TESTS_RUN + 1))
if ! out="$(python3 "$GATE" 2>&1)" || ! grep -qE 'ok \([1-9][0-9]* calls.* [1-9][0-9]* builder texts' <<<"$out"; then
    TESTS_FAILED=$((TESTS_FAILED + 1))
    echo "FAIL: the tree's own calls: $out"
fi

# Rule 5: a tree in which the gate finds no call at all is refused, not passed as clean.
TESTS_RUN=$((TESTS_RUN + 1))
empty="$WORK/empty-tree"
mkdir -p "$empty/tools" "$empty/crates/a/src"
cp "$GATE" "$empty/tools/"
printf 'fn f() {}\n' >"$empty/crates/a/src/lib.rs"
git -C "$empty" init -q
if python3 "$empty/tools/check-sql-safe-assertions.py" >/dev/null 2>&1; then
    TESTS_FAILED=$((TESTS_FAILED + 1))
    echo "FAIL: a tree with no call passed"
fi

# Rule 5: a tree with assertions but no text handed to a builder is refused too.
TESTS_RUN=$((TESTS_RUN + 1))
nobuilder="$WORK/no-builder-tree"
mkdir -p "$nobuilder/tools" "$nobuilder/crates/a/src"
cp "$GATE" "$nobuilder/tools/"
printf 'fn f(t: &str) {\n    // Reason: `t` is a constant.\n    sqlx::query(sqlx::AssertSqlSafe(t));\n}\n' >"$nobuilder/crates/a/src/lib.rs"
git -C "$nobuilder" init -q
if python3 "$nobuilder/tools/check-sql-safe-assertions.py" >/dev/null 2>&1; then
    TESTS_FAILED=$((TESTS_FAILED + 1))
    echo "FAIL: a tree with no builder text passed"
fi

echo "sql_safe_assertions_test: $TESTS_RUN run, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
