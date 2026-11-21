//! A temporal `where` operand is held to its scalar's format before it meets its cast.
//!
//! A declared `DateTime`, `Date` or `Time` field compares as `::timestamptz`, `::date` or
//! `::time`, operand included, and PostgreSQL's input parser reads far more than the
//! scalars publish. It answers, without an error:
//!
//! * an offset-less `DateTime` as an instant in the session `TimeZone`;
//! * a `Time` carrying an offset as the bare time of day, the offset dropped;
//! * words such as `now` or `today`, whose rows change with the clock.
//!
//! Each was a wrong answer that read as a right one. Each is now refused, naming the path to
//! the operand and the field's type.
//! The session zone is pinned to `Asia/Tokyo` on every connection, so an offset-less operand
//! read in the session zone selects other rows than the same digits read in UTC.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test owns a uniquely named table.

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

fn schema(view: &str) -> CompiledSchema {
    let slot = TestTypeBuilder::new("Slot", view)
        .with_simple_field("id", FieldType::Int)
        .with_simple_field("at", FieldType::DateTime)
        .with_simple_field("day", FieldType::Date)
        .with_simple_field("opens", FieldType::Time)
        .build();
    let mut slots = TestQueryBuilder::new("slots", "Slot")
        .returns_list(true)
        .with_sql_source(view)
        .build();
    slots.auto_params.has_where = true;
    TestSchemaBuilder::new().with_type(slot).with_query(slots).build()
}

/// Row 1 is at 03:00Z, which is 12:00 in Tokyo; row 2 at 12:00Z. Row 1 opens at 09:00,
/// row 2 at 18:00.
async fn executor(test: &str) -> Option<Executor> {
    let view = format!("v_temporal_operand_{test}");
    let pg = fraiseql_test_support::postgres().await?;
    let url = pg.url();
    let separator = if url.contains('?') { '&' } else { '?' };
    let url = format!("{url}{separator}options=-c%20TimeZone%3D{}", ZONE.replace('/', "%2F"));
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the bound PostgreSQL");
    let shown = adapter.execute_raw_query("SELECT current_setting('TimeZone') AS zone").await;
    assert_eq!(shown.unwrap()[0]["zone"], json!(ZONE), "the session zone must be pinned");
    for stmt in [
        format!("DROP TABLE IF EXISTS {view}"),
        format!("CREATE TABLE {view} (data jsonb)"),
        format!(
            "INSERT INTO {view} (data) VALUES \
             ('{{\"id\":1,\"at\":\"2026-07-01T03:00:00Z\",\"day\":\"2026-07-01\",\
               \"opens\":\"09:00:00\"}}'), \
             ('{{\"id\":2,\"at\":\"2026-07-01T12:00:00Z\",\"day\":\"2026-07-02\",\
               \"opens\":\"18:00:00\"}}')"
        ),
    ] {
        adapter.execute_raw_query(&stmt).await.expect("provision the fixture");
    }
    Some(Executor::new(schema(&view), Arc::new(adapter)))
}

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

/// The ids a `where` selects, or the text of the refusal.
async fn ids_matching(exec: &Executor, where_arg: &Value) -> Result<Vec<i64>, String> {
    let query = format!("{{ slots(where: {}) {{ id }} }}", graphql_literal(where_arg));
    let response = exec.execute(&query, None).await.map_err(|e| e.to_string())?;
    if let Some(errors) = response.get("errors") {
        return Err(errors.to_string());
    }
    let mut ids: Vec<i64> = response["data"]["slots"]
        .as_array()
        .unwrap_or_else(|| panic!("a list at data.slots: {response}"))
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect();
    ids.sort_unstable();
    Ok(ids)
}

/// The control: operands in the published formats select by the instant, the date and the
/// time of day they denote.
#[tokio::test]
async fn an_operand_in_its_scalars_format_is_answered() {
    let Some(exec) = executor("answered").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    for (filter, expected) in [
        (json!({"at": {"lt": "2026-07-01T10:00:00Z"}}), vec![1]),
        (json!({"at": {"lt": "2026-07-01T19:00:00+09:00"}}), vec![1]),
        (json!({"at": {"in": ["2026-07-01T12:00:00.000Z"]}}), vec![2]),
        (json!({"day": {"eq": "2026-07-02"}}), vec![2]),
        (json!({"opens": {"lt": "12:00:00"}}), vec![1]),
        (json!({"opens": {"eq": "18:00:00.0"}}), vec![2]),
    ] {
        assert_eq!(ids_matching(&exec, &filter).await, Ok(expected), "{filter}");
    }
}

/// `2026-07-01T10:00:00` has no offset. Read in Tokyo it is 01:00Z and selects nothing; read
/// in UTC it would select row 1. Neither reading is the caller's: refused, naming `at`.
#[tokio::test]
async fn an_offset_less_date_time_operand_is_refused() {
    let Some(exec) = executor("offset_less").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    for filter in [
        json!({"at": {"lt": "2026-07-01T10:00:00"}}),
        json!({"at": {"in": ["2026-07-01T12:00:00Z", "2026-07-01T21:00:00"]}}),
        json!({"at": {"lt": "2026-07-01"}}),
        json!({"at": {"lt": "now"}}),
        json!({"at": {"lt": "2026-07-01T10:00:00-00:00"}}),
    ] {
        let refused = ids_matching(&exec, &filter).await.expect_err(&filter.to_string());
        assert!(
            refused.contains("`where.at.") && refused.contains("DateTime"),
            "{filter}: {refused}"
        );
    }
}

/// `::time` drops an offset without a word: `09:00:00+02:00` matched the row opening at
/// 09:00. A `Time` has no offset: refused, naming `opens`.
#[tokio::test]
async fn a_time_operand_with_an_offset_is_refused() {
    let Some(exec) = executor("time_offset").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    for filter in [
        json!({"opens": {"eq": "09:00:00+02:00"}}),
        json!({"opens": {"eq": "09:00:00Z"}}),
        json!({"opens": {"lt": "12:00"}}),
        json!({"opens": {"lt": "allballs"}}),
    ] {
        let refused = ids_matching(&exec, &filter).await.expect_err(&filter.to_string());
        assert!(
            refused.contains("`where.opens.") && refused.contains("Time"),
            "{filter}: {refused}"
        );
    }
}

/// A `Date` is `YYYY-MM-DD`: a date-time or a day-first spelling is refused, naming `day`.
#[tokio::test]
async fn a_date_operand_in_another_shape_is_refused() {
    let Some(exec) = executor("date_shape").await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    for filter in [
        json!({"day": {"eq": "2026-07-02T00:00:00Z"}}),
        json!({"day": {"eq": "02/07/2026"}}),
        json!({"day": {"lt": "today"}}),
    ] {
        let refused = ids_matching(&exec, &filter).await.expect_err(&filter.to_string());
        assert!(
            refused.contains("`where.day.") && refused.contains("Date"),
            "{filter}: {refused}"
        );
    }
}
