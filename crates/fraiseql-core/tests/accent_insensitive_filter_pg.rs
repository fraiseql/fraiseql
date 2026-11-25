//! The accent-insensitive string filters, run against PostgreSQL.
//!
//! A schema that declares `[accent_insensitive]` publishes `aicontains`, `aistartswith`,
//! `aiendswith`, `aieq`, `ailike`, `nailike`, `airegex` and `nairegex` on its string filters.
//! Each folds both sides through the declared function, an `IMMUTABLE` wrapper over
//! `unaccent`, and compares case-insensitively. A schema that declares nothing publishes none
//! of them and refuses them.
//!
//! **Execution engine:** `PostgreSQL` with the `unaccent` extension · **Infrastructure:**
//! `DATABASE_URL` · **Parallelism:** each test owns a uniquely named table; the extension,
//! the `fraiseql` schema and the wrapper are created under a transaction-scoped lock.

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter, where_clause::UnaccentFunction},
    runtime::Executor,
    schema::{AccentInsensitiveConfig, CompiledSchema, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestTypeBuilder};
use serde_json::{Value, json};

/// The rows every test filters: `(id, name)`.
const ROWS: &[(i64, &str)] = &[
    (1, "Crème brûlée"),
    (2, "CREME fraiche"),
    (3, "crème_50%"),
    (4, "Bézier curve"),
    (5, "Product 5"),
    (6, "100％ cotton"),
    (7, "creme50x"),
    (8, "Straße"),
];

/// The wrapper `fraiseql compile --emit-ddl` writes, at `function`.
fn wrapper_ddl(function: &str) -> String {
    format!(
        "CREATE OR REPLACE FUNCTION {function}(text) RETURNS text LANGUAGE sql IMMUTABLE \
         PARALLEL SAFE STRICT RETURN public.unaccent('public.unaccent'::regdictionary, $1)"
    )
}

/// `products` over `view`, a `Product { id name }` view, under `accent`.
fn schema(view: &str, accent: Option<AccentInsensitiveConfig>) -> CompiledSchema {
    let product = TestTypeBuilder::new("Product", view)
        .with_simple_field("id", FieldType::Int)
        .with_simple_field("name", FieldType::String)
        .build();
    let mut products = TestQueryBuilder::new("products", "Product")
        .returns_list(true)
        .with_sql_source(view)
        .build();
    products.auto_params.has_where = true;
    let mut schema = CompiledSchema {
        types: vec![product],
        queries: vec![products],
        accent_insensitive: accent,
        ..CompiledSchema::default()
    };
    schema.build_indexes();
    schema
}

/// An executor over a fresh table of [`ROWS`], declaring `accent`.
async fn executor(test: &str, accent: Option<AccentInsensitiveConfig>) -> Option<Executor> {
    let view = format!("v_accent_insensitive_{test}");
    let Some(pg) = fraiseql_test_support::postgres().await else {
        eprintln!("skipping: no PostgreSQL");
        return None;
    };
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    let values = ROWS
        .iter()
        .map(|(id, name)| {
            format!("('{}')", json!({"id": id, "name": name}).to_string().replace('\'', "''"))
        })
        .collect::<Vec<_>>()
        .join(", ");
    for stmt in [
        format!(
            "DO $do$ BEGIN PERFORM pg_advisory_xact_lock(hashtext('accent_insensitive_filter_pg')); \
             CREATE EXTENSION IF NOT EXISTS unaccent; CREATE SCHEMA IF NOT EXISTS fraiseql; \
             EXECUTE $ddl${}$ddl$; END $do$",
            wrapper_ddl("fraiseql.unaccent")
        ),
        format!("DROP TABLE IF EXISTS {view}"),
        format!("CREATE TABLE {view} (data jsonb)"),
        format!("INSERT INTO {view} (data) VALUES {values}"),
    ] {
        adapter.execute_raw_query(&stmt).await.expect("provision the fixture");
    }
    Some(Executor::new(schema(&view, accent), Arc::new(adapter)))
}

fn declared() -> Option<AccentInsensitiveConfig> {
    Some(AccentInsensitiveConfig::default())
}

/// The ids `{name: {<op>: <operand>}}` selects, or the text of the refusal.
async fn ids(exec: &Executor, op: &str, operand: &str) -> Result<Vec<i64>, String> {
    let query = format!(
        "{{ products(where: {{name: {{{op}: {}}}}}) {{ id }} }}",
        Value::String(operand.to_string())
    );
    let response = exec.execute(&query, None).await.map_err(|e| e.to_string())?;
    if let Some(errors) = response.get("errors") {
        return Err(errors.to_string());
    }
    let mut ids: Vec<i64> = response["data"]["products"]
        .as_array()
        .unwrap_or_else(|| panic!("a list at data.products: {response}"))
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect();
    ids.sort_unstable();
    Ok(ids)
}

async fn assert_ids(exec: &Executor, op: &str, operand: &str, expected: &[i64]) {
    assert_eq!(ids(exec, op, operand).await.as_deref(), Ok(expected), "{op}: {operand:?}");
}

async fn assert_refused(exec: &Executor, op: &str, operand: &str, needle: &str) {
    let err = ids(exec, op, operand).await.expect_err(op);
    assert!(err.contains(needle), "{op}: {operand:?}: {err}");
}

#[tokio::test]
async fn the_substring_operators_match_across_accents_and_case() {
    let Some(exec) = executor("substring", declared()).await else {
        return;
    };
    // The control: `icontains` folds case only.
    assert_ids(&exec, "icontains", "creme", &[2, 7]).await;
    assert_ids(&exec, "aicontains", "creme", &[1, 2, 3, 7]).await;
    assert_ids(&exec, "aicontains", "CRÊME", &[1, 2, 3, 7]).await;
    assert_ids(&exec, "aistartswith", "CRÈME", &[1, 2, 3, 7]).await;
    assert_ids(&exec, "aiendswith", "BRULEE", &[1]).await;
    assert_ids(&exec, "aieq", "creme brulee", &[1]).await;
    assert_ids(&exec, "aieq", "STRASSE", &[8]).await;
    assert_ids(&exec, "aieq", "creme", &[]).await;
}

/// A `contains`-family operand is matched literally: its `%`, `_` and `\` are escaped after
/// the fold, so a fullwidth `％` the dictionary folds to `%` is a percent sign, not a
/// wildcard.
#[tokio::test]
async fn a_substring_operand_is_matched_literally_after_the_fold() {
    let Some(exec) = executor("literal", declared()).await else {
        return;
    };
    assert_ids(&exec, "aicontains", "_50%", &[3]).await;
    assert_ids(&exec, "aicontains", "％", &[3, 6]).await;
    assert_ids(&exec, "aicontains", "＿", &[3]).await;
    assert_ids(&exec, "aistartswith", "100%", &[6]).await;
    assert_ids(&exec, "aiendswith", "\\", &[]).await;
}

/// A `like` pattern keeps its wildcards and its `\` escapes, and is folded itself.
#[tokio::test]
async fn a_like_pattern_is_folded_and_keeps_its_wildcards_and_escapes() {
    let Some(exec) = executor("like", declared()).await else {
        return;
    };
    assert_ids(&exec, "ailike", "cr_me%", &[1, 2, 3, 7]).await;
    assert_ids(&exec, "ailike", "%BRÛLÉE", &[1]).await;
    assert_ids(&exec, "ailike", "crème\\_50\\%", &[3]).await;
    assert_ids(&exec, "ailike", "creme%50%", &[3, 7]).await;
    assert_ids(&exec, "ailike", "cr_me_50_", &[3]).await;
    assert_ids(&exec, "nailike", "%creme%", &[4, 5, 6, 8]).await;
}

/// A regex pattern is folded before it is compiled: `[è]` is `[e]`, so it matches a plain
/// `e` too.
#[tokio::test]
async fn a_regex_pattern_is_folded_with_its_classes() {
    let Some(exec) = executor("regex", declared()).await else {
        return;
    };
    // The control: `iregex` folds case only.
    assert_ids(&exec, "iregex", "^cr[è]me", &[1, 3]).await;
    assert_ids(&exec, "airegex", "^cr[è]me", &[1, 2, 3, 7]).await;
    assert_ids(&exec, "airegex", "^cr[éè]me", &[1, 2, 3, 7]).await;
    assert_ids(&exec, "airegex", "brul[ée]+e$", &[1]).await;
    assert_ids(&exec, "nairegex", "^cr[éè]me", &[4, 5, 6, 8]).await;
}

/// The length and nesting limits on a regex pattern hold for the pattern PostgreSQL
/// compiles: a symbol the fold turns into regex syntax is refused.
#[tokio::test]
async fn a_regex_pattern_is_held_to_the_limits_after_the_fold() {
    let Some(exec) = executor("regex_limits", declared()).await else {
        return;
    };
    for op in ["airegex", "nairegex"] {
        assert_refused(&exec, op, "(a+)+", "nested quantifiers").await;
        assert_refused(&exec, op, &"a".repeat(1_001), "maximum length").await;
        assert_refused(&exec, op, "（a＋）＋", "folds before the pattern is compiled").await;
        assert_refused(&exec, op, "a\u{2c6}", "folds before the pattern is compiled").await;
    }
}

/// Undeclared, the operators are neither published nor accepted.
#[tokio::test]
async fn an_undeclared_schema_neither_publishes_nor_runs_them() {
    let Some(exec) = executor("undeclared", None).await else {
        return;
    };
    for op in [
        "aicontains",
        "aistartswith",
        "aiendswith",
        "aieq",
        "ailike",
        "nailike",
        "airegex",
        "nairegex",
    ] {
        assert_refused(&exec, op, "creme", "[accent_insensitive]").await;
    }
    let filter = schema("v_unused", None);
    let fields = &filter.find_input_type("StringFilter").unwrap().fields;
    assert!(fields.iter().all(|f| !f.name.starts_with("ai") && !f.name.starts_with("nai")));
}

#[test]
fn a_declared_schema_publishes_each_with_its_description() {
    let filter = schema("v_unused", declared());
    let fields = &filter.find_input_type("StringFilter").unwrap().fields;
    for op in [
        "aicontains",
        "aistartswith",
        "aiendswith",
        "aieq",
        "ailike",
        "nailike",
        "airegex",
        "nairegex",
    ] {
        let field = fields.iter().find(|f| f.name == op).unwrap_or_else(|| panic!("{op}"));
        assert_eq!(field.field_type, "String", "{op}");
        assert!(
            field
                .description
                .as_deref()
                .is_some_and(|d| d.contains("accent- and case-insensitive")),
            "{op}: {:?}",
            field.description
        );
    }
}

/// The SQL calls the function the schema names.
#[tokio::test]
async fn the_declared_function_is_the_one_called() {
    let missing = AccentInsensitiveConfig {
        function: UnaccentFunction::parse("fraiseql_absent.fold").unwrap(),
    };
    let Some(exec) = executor("named", Some(missing)).await else {
        return;
    };
    let err = ids(&exec, "aicontains", "creme").await.expect_err("an absent function");
    assert!(err.contains("fraiseql_absent"), "{err}");
}
