#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1561: a `where` comparison on a field the query maps to a native column reads the
//! column, and selects exactly the rows the document key selects.
//!
//! Two queries read one table whose columns hold the same values as its documents: one
//! maps every field to its column, the other maps none. Every operator the column route
//! takes — on every field type it takes — must return the same rows from both, including
//! rows whose value is NULL or absent. A second table holds columns that disagree with
//! their documents, which is how a test tells, by rows alone, which of the two a filter
//! read. The text column is collated by bytes, unlike the database, so a comparison
//! that took the column's collation instead of the document's would also disagree.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_wnc_item` and `tv_wnc_drift` tables.

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{CompiledSchema, FieldDefinition, FieldType, NativeColumn},
};
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

const TABLE: &str = "tv_wnc_item";
const DRIFT: &str = "tv_wnc_drift";

const U1: &str = "0b7e3c1a-6d1f-4c2e-9a51-1f0a2b3c4d01";
const U2: &str = "0b7e3c1a-6d1f-4c2e-9a51-1f0a2b3c4d02";

/// Every field, its GraphQL type, and the column type `compile --database` records.
const FIELDS: [(&str, FieldType, &str); 8] = [
    ("id", FieldType::Uuid, "uuid"),
    ("name", FieldType::String, "text"),
    ("qty", FieldType::Int, "integer"),
    ("amount", FieldType::Decimal, "numeric"),
    ("active", FieldType::Boolean, "boolean"),
    ("createdAt", FieldType::DateTime, "timestamp with time zone"),
    ("day", FieldType::Date, "date"),
    ("atTime", FieldType::Time, "time without time zone"),
];

fn query(name: &str, source: &str, native: bool) -> fraiseql_core::schema::QueryDefinition {
    let mut query = TestQueryBuilder::new(name, "Item")
        .returns_list(true)
        .with_sql_source(source)
        .build();
    query.auto_params.has_where = true;
    if native {
        for (field, _, pg_type) in FIELDS {
            query.native_columns.insert(field.to_string(), NativeColumn::nullable(pg_type));
        }
        query.native_columns.insert("code".to_string(), NativeColumn::nullable("text"));
    }
    query
}

fn schema() -> CompiledSchema {
    let mut item = TestTypeBuilder::new("Item", TABLE).with_simple_field("pk", FieldType::Int);
    for (field, field_type, _) in FIELDS {
        item = item.with_field(FieldDefinition::nullable(field, field_type));
    }
    // A field only a `read:code` caller may read, filtered on or not.
    item = item.with_field(
        TestFieldBuilder::nullable("code", FieldType::String)
            .requires_scope("read:code")
            .build(),
    );
    let mut schema = TestSchemaBuilder::new()
        .with_type(item.build())
        .with_query(query("items", TABLE, true))
        .with_query(query("documentItems", TABLE, false))
        .with_query(query("driftItems", DRIFT, true))
        .with_query(query("documentDriftItems", DRIFT, false))
        .build();
    schema.build_indexes();
    schema
}

/// `(pk, id, name, qty, amount, active, created_at, day, at_time)`, as SQL literals.
const ROWS: [&str; 7] = [
    "(1, '0b7e3c1a-6d1f-4c2e-9a51-1f0a2b3c4d01', 'alpha', 1, 1.50, true, \
     '2024-01-01T08:00:00Z', '2024-01-01', '08:00')",
    "(2, '0b7e3c1a-6d1f-4c2e-9a51-1f0a2b3c4d02', 'beta', 3, 2.25, false, \
     '2024-01-02T10:00:00Z', '2024-01-02', '10:30')",
    "(3, '0b7e3c1a-6d1f-4c2e-9a51-1f0a2b3c4d03', 'Alpha', 3, 10, true, \
     '2024-01-03T00:00:00Z', '2024-01-03', '23:59')",
    "(4, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
    "(5, '0b7e3c1a-6d1f-4c2e-9a51-1f0a2b3c4d05', 'gamma', 2147483647, 0.1, NULL, \
     '2023-12-31T23:00:00Z', '2023-12-31', '00:00')",
    "(6, '0b7e3c1a-6d1f-4c2e-9a51-1f0a2b3c4d06', 'b', -5, -1, false, \
     '2024-01-01T08:00:00.5Z', '2024-01-01', '08:00:01')",
    "(7, '0b7e3c1a-6d1f-4c2e-9a51-1f0a2b3c4d07', '', 0, 0, true, \
     '2024-06-01T12:00:00Z', '2024-06-01', '12:00')",
];

/// The document each row's columns describe, under the stored (`snake_case`) keys; a NULL
/// column is an absent key.
const DOCUMENT: &str = "jsonb_strip_nulls(jsonb_build_object('pk', pk, 'id', id, 'name', name, \
                        'qty', qty, 'amount', amount, 'active', active, 'created_at', \
                        created_at, 'day', day, 'at_time', at_time))";

async fn setup() -> Option<(PostgresAdapter, Executor)> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    // `name` sorts by bytes (`"C"`), unlike the database's default collation: the
    // document key's text compares under the default, so the column must too.
    let columns = "pk bigint PRIMARY KEY, id uuid, name text COLLATE \"C\", qty integer, \
                   amount numeric, active boolean, created_at timestamptz, day date, at_time \
                   time, code text, data jsonb";
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("DROP TABLE IF EXISTS {DRIFT}"),
        format!("CREATE TABLE {TABLE} ({columns})"),
        format!(
            "INSERT INTO {TABLE} (pk, id, name, qty, amount, active, created_at, day, at_time) \
             VALUES {}",
            ROWS.join(", ")
        ),
        format!("UPDATE {TABLE} SET data = {DOCUMENT}"),
        // Row 1: every column says one thing and the document another. Row 2: every
        // column is NULL and the document holds the same values as row 1's.
        format!("CREATE TABLE {DRIFT} ({columns})"),
        format!(
            "INSERT INTO {DRIFT} (pk, id, name, qty, amount, active, created_at, day, at_time, \
             data, code) VALUES (1, '{U1}', 'column', 7, 7.5, true, '2024-01-01T00:00:00Z', \
             '2024-01-01', '07:00', jsonb_build_object('pk', 1, 'id', \
             '{U2}', 'name', 'document', 'qty', 9, 'amount', 9.5, 'active', false, \
             'created_at', '2025-01-01T00:00:00Z', 'day', '2025-01-01', 'at_time', '09:00', \
             'code', 'secret'), 'secret')"
        ),
        format!(
            "INSERT INTO {DRIFT} (pk, data) VALUES (2, jsonb_build_object('pk', 2, 'id', '{U2}', \
             'name', 'document', 'qty', 9, 'amount', 9.5, 'active', false, 'created_at', \
             '2025-01-01T00:00:00Z', 'day', '2025-01-01', 'at_time', '09:00'))"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let executor = Executor::new(schema(), Arc::new(PostgresAdapter::new(&url).await.unwrap()));
    Some((adapter, executor))
}

/// The pks `query` returns under `filter`, sorted, or the error it raised.
async fn pks(executor: &Executor, query: &str, filter: &Value) -> Result<Vec<i64>, String> {
    let document = format!("query($w: ItemWhereInput) {{ {query}(where: $w) {{ pk }} }}");
    let response = executor
        .execute(&document, Some(&json!({ "w": filter })))
        .await
        .map_err(|e| e.to_string())?;
    let rows = response["data"][query]
        .as_array()
        .unwrap_or_else(|| panic!("{query} under {filter}: {response}"));
    let mut pks: Vec<i64> = rows.iter().map(|r| r["pk"].as_i64().unwrap()).collect();
    pks.sort_unstable();
    Ok(pks)
}

/// Every operator the column route takes, on every field type it takes.
fn filters() -> Vec<Value> {
    let upper = U1.to_uppercase();
    vec![
        json!({ "name": { "eq": "alpha" } }),
        json!({ "name": { "eq": "Alpha" } }),
        json!({ "name": { "eq": "" } }),
        json!({ "name": { "neq": "alpha" } }),
        json!({ "name": { "gt": "b" } }),
        json!({ "name": { "gte": "beta" } }),
        json!({ "name": { "lt": "beta" } }),
        json!({ "name": { "lte": "alpha" } }),
        json!({ "name": { "in": ["alpha", "gamma", "nope"] } }),
        json!({ "name": { "nin": ["alpha"] } }),
        json!({ "name": { "in": [] } }),
        json!({ "name": { "nin": [] } }),
        json!({ "name": { "isnull": true } }),
        json!({ "name": { "isnull": false } }),
        json!({ "qty": { "eq": 3 } }),
        json!({ "qty": { "neq": 3 } }),
        json!({ "qty": { "gt": 1 } }),
        json!({ "qty": { "gte": 3 } }),
        json!({ "qty": { "lt": 3 } }),
        json!({ "qty": { "lte": -5 } }),
        json!({ "qty": { "in": [1, 3] } }),
        json!({ "qty": { "nin": [3] } }),
        json!({ "qty": { "eq": 3_000_000_000_i64 } }),
        json!({ "qty": { "gt": 2_147_483_646 } }),
        json!({ "qty": { "isnull": true } }),
        json!({ "amount": { "eq": 1.5 } }),
        json!({ "amount": { "eq": "1.50" } }),
        json!({ "amount": { "gt": 1 } }),
        json!({ "amount": { "lt": 0.2 } }),
        json!({ "amount": { "in": [10, 2.25] } }),
        json!({ "active": { "eq": true } }),
        json!({ "active": { "neq": true } }),
        json!({ "active": { "eq": false } }),
        json!({ "active": { "isnull": true } }),
        json!({ "createdAt": { "eq": "2024-01-01T10:00:00+02:00" } }),
        json!({ "createdAt": { "gt": "2024-01-01T08:00:00Z" } }),
        json!({ "createdAt": { "lte": "2024-01-02T10:00:00Z" } }),
        json!({ "createdAt": { "in": ["2024-01-03T00:00:00Z", "2023-12-31T23:00:00Z"] } }),
        json!({ "day": { "eq": "2024-01-02" } }),
        json!({ "day": { "gt": "2024-01-01" } }),
        json!({ "day": { "lte": "2024-01-01" } }),
        json!({ "atTime": { "eq": "08:00:00" } }),
        json!({ "atTime": { "gt": "09:00" } }),
        json!({ "id": { "eq": U1 } }),
        json!({ "id": { "eq": upper } }),
        json!({ "id": { "neq": U1 } }),
        json!({ "id": { "in": [upper, U2] } }),
        json!({ "id": { "nin": [U1] } }),
        json!({ "id": { "eq": "not-a-uuid" } }),
        json!({ "id": { "in": [U1, "not-a-uuid"] } }),
        json!({ "id": { "isnull": true } }),
        json!({ "qty": { "is_not_null": true } }),
        json!({ "_or": [{ "name": { "eq": "alpha" } }, { "qty": { "gt": 2 } }] }),
        json!({ "_not": { "qty": { "eq": 3 } } }),
        json!({ "name": { "neq": "alpha" }, "active": { "eq": true } }),
    ]
}

#[tokio::test]
async fn a_column_selects_the_rows_the_document_selects() {
    let Some((_adapter, executor)) = setup().await else {
        eprintln!("skipped: DATABASE_URL not set");
        return;
    };
    for filter in filters() {
        let column = pks(&executor, "items", &filter).await;
        let document = pks(&executor, "documentItems", &filter).await;
        assert!(column.is_ok(), "{filter} is a filter both can answer: {column:?}");
        assert_eq!(column, document, "the column and the document disagree under {filter}");
    }
}

/// On a row whose columns disagree with its document, a filter mapped to a column
/// matches the column's value and a filter mapped to nothing matches the document's —
/// so the routing is shown by rows, for each field type.
#[tokio::test]
async fn a_mapped_field_is_compared_on_its_column() {
    let Some((_adapter, executor)) = setup().await else {
        eprintln!("skipped: DATABASE_URL not set");
        return;
    };
    // (filter, rows read from the columns, rows read from the documents)
    let cases = [
        (json!({ "id": { "eq": U1 } }), vec![1], vec![]),
        (json!({ "name": { "eq": "column" } }), vec![1], vec![]),
        (json!({ "qty": { "eq": 7 } }), vec![1], vec![]),
        (json!({ "amount": { "eq": 7.5 } }), vec![1], vec![]),
        (json!({ "active": { "eq": true } }), vec![1], vec![]),
        (json!({ "createdAt": { "eq": "2024-01-01T00:00:00Z" } }), vec![1], vec![]),
        (json!({ "day": { "eq": "2024-01-01" } }), vec![1], vec![]),
        (json!({ "atTime": { "eq": "07:00" } }), vec![1], vec![]),
        (json!({ "qty": { "neq": 9 } }), vec![1], vec![]),
        (json!({ "qty": { "gt": 6 } }), vec![1], vec![1, 2]),
        (json!({ "qty": { "gte": 7 } }), vec![1], vec![1, 2]),
        (json!({ "qty": { "lt": 8 } }), vec![1], vec![]),
        (json!({ "qty": { "lte": 7 } }), vec![1], vec![]),
        (json!({ "qty": { "in": [7] } }), vec![1], vec![]),
        (json!({ "qty": { "nin": [9] } }), vec![1], vec![]),
        (json!({ "qty": { "isnull": true } }), vec![2], vec![]),
        (json!({ "qty": { "is_not_null": true } }), vec![1], vec![1, 2]),
        (json!({ "id": { "in": [U1, U2] } }), vec![1], vec![1, 2]),
    ];
    for (filter, column, document) in cases {
        assert_eq!(pks(&executor, "driftItems", &filter).await, Ok(column), "{filter}");
        assert_eq!(pks(&executor, "documentDriftItems", &filter).await, Ok(document), "{filter}");
    }
    // An operator the column route does not take reads the document, column or not.
    let by_document = json!({ "name": { "startswith": "doc" } });
    assert_eq!(pks(&executor, "driftItems", &by_document).await, Ok(vec![1, 2]));
    // So does a malformed UUID, alone or in a list: compared as text it matches nothing,
    // where casting it to the column's type would have raised.
    let malformed = json!({ "id": { "eq": "not-a-uuid" } });
    assert_eq!(pks(&executor, "driftItems", &malformed).await, Ok(vec![]));
    let in_a_list = json!({ "id": { "in": [U2, "not-a-uuid"] } });
    assert_eq!(pks(&executor, "driftItems", &in_a_list).await, Ok(vec![1, 2]));
}

/// A field the caller may not read may not decide which rows come back (ruling AA 3), and
/// mapping it to a column changes nothing about that: the filter is refused before it is
/// rendered, with the same error either way.
#[tokio::test]
async fn a_field_the_caller_may_not_read_is_refused_on_a_column_too() {
    let Some((_adapter, executor)) = setup().await else {
        eprintln!("skipped: DATABASE_URL not set");
        return;
    };
    let filter = json!({ "code": { "eq": "secret" } });
    let column = pks(&executor, "driftItems", &filter).await;
    let document = pks(&executor, "documentDriftItems", &filter).await;
    assert!(column.is_err(), "the filter on an unreadable field is refused: {column:?}");
    assert_eq!(column, document, "refused the same way as on the document");
}
