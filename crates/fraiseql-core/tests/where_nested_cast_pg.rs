//! #1157: a nested `where` comparison gets its declared cast, like a top-level one.
//!
//! `where_field_types` mapped **only** the entry type's own fields into `casts`,
//! and said so: "Only the top level is mapped. A nested relation path
//! (`machine.id`) has no entry, and the generator falls back to the JSON value's
//! shape." `FieldTypeMap::get` has always resolved a dotted path
//! (`path.join(".")`) — the lookup side was ready and nothing populated it.
//!
//! So one level down from where #798 and #800 were fixed, both defects were
//! still live:
//!
//! * **#798 class** — a date, timestamp or UUID range comparison casts to `::numeric` and aborts
//!   the statement. Loud, but only at runtime.
//! * **#800 class** — `in: [19.9]` misses a stored `19.90` that `eq: 19.9` matches. **Silent**: the
//!   query succeeds and returns the wrong rows.
//!
//! The #800 shape is why this suite executes SQL rather than comparing generated
//! strings. A string assertion cannot tell a filter that returns the right rows
//! from one that returns too few.
//!
//! # Running
//!
//! ```bash
//! DATABASE_URL=postgres://…  cargo test -p fraiseql-core --test where_nested_cast_pg
//! ```
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** owns a uniquely named view, so it does not collide with the
//! fixtures other suites share in the same database (#1169).

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    runtime::Executor,
    schema::{CompiledSchema, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

/// Each test owns its own table.
///
/// The fixture is DDL, and DDL is the shared resource: four tests all running
/// `DROP`/`CREATE` on one name collide on `pg_class_relname_nsp_index` the
/// moment `cargo test` runs them in parallel, which it does by default.
/// Requiring `--test-threads=1` instead would be the #1114 defect — a suite that
/// assumes serialization with nothing enforcing it reads as broken the first
/// time someone runs it normally. Per-test names remove the sharing rather than
/// coordinating around it, and keep the suite clear of the cross-binary
/// collision in #1169.
fn table_for(test: &str) -> String {
    format!("v_nested_cast_1157_{test}")
}

/// `Order` with a `customer` relation to `Customer`, whose fields are exactly
/// the three types #798 and #800 were about: a numeric, a timestamp and a date.
fn schema(view: &str) -> CompiledSchema {
    let order = TestTypeBuilder::new("Order", view)
        .with_simple_field("id", FieldType::Int)
        .with_simple_field("total", FieldType::Decimal)
        .with_simple_field("customer", FieldType::Object("Customer".to_string()))
        .build();

    let customer = TestTypeBuilder::new("Customer", "v_customer_1157")
        .with_simple_field("id", FieldType::Int)
        .with_simple_field("creditLimit", FieldType::Decimal)
        .with_simple_field("signedUpAt", FieldType::DateTime)
        .with_simple_field("renewsOn", FieldType::Date)
        .build();

    let mut orders = TestQueryBuilder::new("orders", "Order")
        .returns_list(true)
        .with_sql_source(view)
        .build();
    orders.auto_params.has_where = true;

    TestSchemaBuilder::new()
        .with_type(order)
        .with_type(customer)
        .with_query(orders)
        .build()
}

/// Three orders whose embedded customer carries values chosen so that **text
/// ordering and typed ordering disagree**. That choice is the whole fixture:
///
/// * `credit_limit` — `9.00`, `19.90`, `100.00`. As numbers `9 < 19.9 < 100`; as text `"100.00" <
///   "19.90" < "9.00"`. So `gte: 10` selects two rows typed and all three as text.
/// * `signed_up_at` — row 1 is `10:00+02:00`, i.e. **08:00Z**, which is *earlier* than row 2's
///   `09:00Z` while sorting *later* as text.
///
/// A fixture of uniform `Z`-suffixed timestamps and round numbers passes with or
/// without the cast, because text comparison happens to agree — which is how a
/// missing cast reads as working code.
fn fixture_sql(view: &str) -> String {
    format!(
        "DROP TABLE IF EXISTS {view};
CREATE TABLE {view} (data jsonb);
INSERT INTO {view} (data) VALUES
  ('{{\"id\":1,\"total\":10.00,\"customer\":{{\"id\":10,\"credit_limit\":9.00,
     \"signed_up_at\":\"2024-01-01T10:00:00+02:00\",\"renews_on\":\"2024-01-01\"}}}}'::jsonb),
  ('{{\"id\":2,\"total\":20.00,\"customer\":{{\"id\":20,\"credit_limit\":19.90,
     \"signed_up_at\":\"2024-01-01T09:00:00Z\",\"renews_on\":\"2024-06-15\"}}}}'::jsonb),
  ('{{\"id\":3,\"total\":30.00,\"customer\":{{\"id\":30,\"credit_limit\":100.00,
     \"signed_up_at\":\"2024-12-31T23:59:59Z\",\"renews_on\":\"2024-12-31\"}}}}'::jsonb);"
    )
}

async fn executor(test: &str) -> Option<(Executor<PostgresAdapter>, String)> {
    let view = table_for(test);
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    for stmt in fixture_sql(&view).split(";\n") {
        if stmt.trim().is_empty() {
            continue;
        }
        adapter
            .execute_raw_query(stmt)
            .await
            .expect("provision the nested-cast fixture");
    }
    Some((Executor::new(schema(&view), Arc::new(adapter)), view))
}

/// Run one `where:` argument through the whole live path — GraphQL parse,
/// schema-typed where parse, SQL generation, execution — and return the matching
/// `id`s in order.
async fn ids_matching(
    exec: &Executor<PostgresAdapter>,
    where_arg: &Value,
) -> Result<Vec<i64>, String> {
    let query = format!("{{ orders(where: {}) {{ id }} }}", graphql_literal(where_arg));
    let response = exec.execute(&query, None).await.map_err(|e| format!("execute failed: {e}"))?;
    if let Some(errors) = response.get("errors") {
        return Err(format!("query returned errors: {errors}"));
    }
    let rows = response["data"]["orders"]
        .as_array()
        .ok_or_else(|| format!("expected a list at data.orders, got: {response}"))?;
    let mut ids: Vec<i64> =
        rows.iter().map(|r| r["id"].as_i64().expect("fixture has id")).collect();
    ids.sort_unstable();
    Ok(ids)
}

/// Render a JSON value as a GraphQL object literal — object keys are bare names
/// in GraphQL, not quoted strings.
fn graphql_literal(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let inner: Vec<String> =
                map.iter().map(|(k, v)| format!("{k}: {}", graphql_literal(v))).collect();
            format!("{{{}}}", inner.join(", "))
        },
        Value::Array(items) => {
            format!("[{}]", items.iter().map(graphql_literal).collect::<Vec<_>>().join(", "))
        },
        other => other.to_string(),
    }
}

/// The control. A *top-level* range filter has had its declared cast since #798,
/// so if this fails the suite is measuring something other than nesting.
#[tokio::test]
async fn a_top_level_range_filter_still_works() {
    let Some((exec, _view)) = executor("a_top_level_range_filter_still_works").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    assert_eq!(
        ids_matching(&exec, &json!({"total": {"gte": 20.0}})).await.unwrap(),
        vec![2, 3],
        "the top-level cast is the control for this suite"
    );
}

/// A nested timestamp range must compare as an instant, not as text.
///
/// Row 1 is `10:00+02:00` — 08:00Z, *before* the bound — but sorts *after* it as
/// text. An uncast comparison therefore returns it.
#[tokio::test]
async fn a_nested_timestamp_range_compares_as_an_instant() {
    let Some((exec, _view)) = executor("a_nested_timestamp_range_compares_as_an_instant").await
    else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let got =
        ids_matching(&exec, &json!({"customer": {"signedUpAt": {"gte": "2024-01-01T09:00:00Z"}}}))
            .await;

    assert_eq!(
        got.as_deref(),
        Ok(&[2, 3][..]),
        "#1157: a nested timestamp range must get its declared DateTime cast. Order 1 signed up \
         at 08:00Z, before the bound — but its `+02:00` rendering sorts after it as text, so an \
         uncast comparison includes it"
    );
}

/// Second control: a nested **numeric** range already worked, and must keep
/// working.
///
/// Value-shape inference gets this one right — a JSON number infers to numeric —
/// which is precisely why the defect is narrower than "nested filters are
/// broken". It bites where the JSON *type* and the SQL type disagree: a string
/// that denotes an instant, a date or a UUID. Pinning the case that already
/// passed is what stops the fix from regressing it.
#[tokio::test]
async fn a_nested_numeric_range_compares_as_a_number() {
    let Some((exec, _view)) = executor("a_nested_numeric_range_compares_as_a_number").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    assert_eq!(
        ids_matching(&exec, &json!({"customer": {"creditLimit": {"gte": 10}}}))
            .await
            .as_deref(),
        Ok(&[2, 3][..]),
        "#1157: a nested numeric range must get its declared Decimal cast. 9.00 is below the \
         bound as a number and above it as text"
    );
}

/// #800 class, one level down, and the reason this suite executes SQL.
///
/// `19.90` is stored; `19.9` is filtered. With a `Numeric` cast the two are the
/// same number. Without one, `in` compares text and `"19.9" != "19.90"`, so the
/// row silently drops out — with no error anywhere.
#[tokio::test]
async fn nested_in_agrees_with_nested_eq() {
    let Some((exec, _view)) = executor("nested_in_agrees_with_nested_eq").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let eq = ids_matching(&exec, &json!({"customer": {"creditLimit": {"eq": 19.9}}}))
        .await
        .expect("eq executes");
    let in_ = ids_matching(&exec, &json!({"customer": {"creditLimit": {"in": [19.9]}}}))
        .await
        .expect("in executes");

    assert_eq!(eq, vec![2], "#1157: nested `eq: 19.9` must match the stored 19.90 — got {eq:?}");
    assert_eq!(
        in_, eq,
        "#1157: nested `in: [19.9]` must return exactly what `eq: 19.9` returns. This is #800 \
         one level down, and it is silent: the query succeeds and returns the wrong rows"
    );
}
