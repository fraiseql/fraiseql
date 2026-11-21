//! A temporal field is served only in its scalar's format.
//!
//! A view writes each column type in one shape: a `timestamp` never with an offset, a
//! `timestamptz` always with one. A `DateTime` field whose view writes a `timestamp` was
//! served `2026-07-01T12:00:00`, which each client then read in whatever zone it assumes.
//! Such a value is now a field error naming the field and why, its position `null`, at the
//! root and in an embedded object alike. The value itself is never quoted back.
//!
//! The session zone is pinned to `Asia/Tokyo`, so the `timestamptz` control is written with
//! an offset other than UTC's.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test owns a uniquely named table and view.

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

const ZONE: &str = "Asia/Tokyo";

/// `Slot.at` is a nullable `DateTime` its view writes from a `timestamp`; `Slot.stamp` one
/// written from a `timestamptz`. `Slot.inner.at` embeds the `timestamp` again.
fn schema(view: &str) -> CompiledSchema {
    let slot = TestTypeBuilder::new("Slot", view)
        .with_simple_field("id", FieldType::Int)
        .with_nullable_field("at", FieldType::DateTime)
        .with_simple_field("stamp", FieldType::DateTime)
        .with_simple_field("wall", FieldType::LocalDateTime)
        .with_simple_field("day", FieldType::Date)
        .with_simple_field("opens", FieldType::Time)
        .with_simple_field("inner", FieldType::Object("Inner".to_string()))
        .build();
    let inner = TestTypeBuilder::new("Inner", view)
        .with_nullable_field("at", FieldType::DateTime)
        .with_simple_field("wall", FieldType::LocalDateTime)
        .build();
    let slots = TestQueryBuilder::new("slots", "Slot")
        .returns_list(true)
        .with_sql_source(view)
        .build();
    TestSchemaBuilder::new()
        .with_type(slot)
        .with_type(inner)
        .with_query(slots)
        .build()
}

async fn executor(test: &str) -> Option<Executor> {
    let table = format!("tb_served_temporal_{test}");
    let view = format!("v_served_temporal_{test}");
    let pg = fraiseql_test_support::postgres().await?;
    let url = pg.url();
    let separator = if url.contains('?') { '&' } else { '?' };
    let url = format!("{url}{separator}options=-c%20TimeZone%3D{}", ZONE.replace('/', "%2F"));
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the bound PostgreSQL");
    let shown = adapter.execute_raw_query("SELECT current_setting('TimeZone') AS zone").await;
    assert_eq!(shown.unwrap()[0]["zone"], json!(ZONE), "the session zone must be pinned");
    for stmt in [
        format!("DROP VIEW IF EXISTS {view}"),
        format!("DROP TABLE IF EXISTS {table}"),
        format!(
            "CREATE TABLE {table} (id int, at_ts timestamp, at_tz timestamptz, d date, \
             t time)"
        ),
        format!(
            "INSERT INTO {table} VALUES \
             (1, '2026-07-01 12:00:00', '2026-07-01 12:00:00+00', '2026-07-01', '09:30:00')"
        ),
        format!(
            "CREATE VIEW {view} AS SELECT id, jsonb_build_object('id', id, 'at', at_ts, \
             'stamp', at_tz, 'wall', at_ts, 'day', d, 'opens', t, \
             'inner', jsonb_build_object('at', at_ts, 'wall', at_ts)) AS data FROM {table}"
        ),
    ] {
        adapter.execute_raw_query(&stmt).await.expect("provision the fixture");
    }
    Some(Executor::new(schema(&view), Arc::new(adapter)))
}

/// The response to `{ slots { <selection> } }`.
async fn slots(exec: &Executor, selection: &str) -> Value {
    exec.execute(&format!("{{ slots {{ {selection} }} }}"), None).await.unwrap()
}

/// The control: each scalar written from its own column type is served as the view wrote
/// it, the `timestamptz` with the session zone's offset.
#[tokio::test]
async fn a_temporal_key_written_from_its_own_column_type_is_served() {
    let Some(exec) = executor("served").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let response = slots(&exec, "stamp wall day opens inner { wall }").await;
    assert_eq!(response.get("errors"), None, "{response}");
    assert_eq!(
        response["data"]["slots"][0],
        json!({
            "stamp": "2026-07-01T21:00:00+09:00",
            "wall": "2026-07-01T12:00:00",
            "day": "2026-07-01",
            "opens": "09:30:00",
            "inner": {"wall": "2026-07-01T12:00:00"},
        })
    );
}

/// A `DateTime` written from a `timestamp` has no offset: a field error naming the field and
/// why, its position `null`, at the root and embedded. The rest of the row is served.
#[tokio::test]
async fn a_date_time_written_from_a_timestamp_is_a_field_error() {
    let Some(exec) = executor("refused").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let response = slots(&exec, "id at inner { at }").await;
    assert_eq!(
        response["data"]["slots"][0],
        json!({"id": 1, "at": null, "inner": {"at": null}}),
        "{response}"
    );
    let errors = response["errors"].as_array().unwrap_or_else(|| panic!("{response}"));
    let found: Vec<(&str, &Value)> =
        errors.iter().map(|e| (e["message"].as_str().unwrap(), &e["path"])).collect();
    let why = "and the stored value is a String that has no time-zone offset, which a DateTime \
               always carries";
    assert_eq!(
        found,
        [
            (format!("`Slot.at` is a `DateTime`, {why}").as_str(), &json!(["slots", 0, "at"])),
            (
                format!("`Inner.at` is a `DateTime`, {why}").as_str(),
                &json!(["slots", 0, "inner", "at"])
            ),
        ]
    );
    assert!(!response["errors"].to_string().contains("12:00"), "{response}");
}
