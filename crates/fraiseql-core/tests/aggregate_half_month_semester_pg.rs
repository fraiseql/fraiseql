//! The `half_month` and `semester` buckets, executed against PostgreSQL.
//!
//! A half-month starts on the 1st or the 16th, and a semester on 1 January or 1 July: the
//! period starts a v1 application's reports use. Each case below is a boundary: the 15th
//! and the 16th, 30 June and 1 July, the end of February in a leap year and in a common
//! one, and a `timestamptz` instant on either side of both daylight-saving changes.
//!
//! Every row's `amount` is a distinct power of two, so a bucket's sum names exactly the
//! rows it holds.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test` leg
//! and runs in the Dagger `integration: postgres` leg (`--test '*'`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_hms_bucket` fixture → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, FactRowScope, FactTableMetadata, FilterColumn, MeasureColumn,
        SemiAdditiveReduction, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const TABLE: &str = "tf_hms_bucket";
const ROOT: &str = "hms_bucket_aggregate";
/// A zone with daylight saving: a `timestamptz` bucket starts at local midnight on either
/// side of a change.
const ZONE: &str = "Europe/Paris";

/// `(amount, day, at_ts, at_tz)`: each boundary the buckets are pinned on.
const ROWS: [(i64, &str, &str, &str); 8] = [
    (1, "2026-01-01", "2026-01-01 00:00:00", "2026-03-15 23:59:59+01"),
    (2, "2026-01-15", "2026-01-15 23:59:59", "2026-03-16 00:00:00+01"),
    (4, "2026-01-16", "2026-01-16 00:00:00", "2026-03-31 12:00:00+02"),
    (8, "2026-02-28", "2026-02-28 12:00:00", "2026-06-30 23:30:00+02"),
    (16, "2024-02-29", "2024-02-29 12:00:00", "2026-07-01 00:30:00+02"),
    (32, "2026-06-30", "2026-06-30 23:59:59", "2026-10-25 12:00:00+01"),
    (64, "2026-07-01", "2026-07-01 00:00:00", "2026-01-01 00:00:00+01"),
    (128, "2026-12-31", "2026-12-31 23:59:59", "2026-12-31 23:59:59+01"),
];

fn filter(name: &str, sql_type: SqlType) -> FilterColumn {
    FilterColumn {
        name: name.to_string(),
        sql_type,
        indexed: true,
        hierarchy: None,
    }
}

fn fact_table() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                None,
        row_scope:                FactRowScope::Server,
        inject_params:            indexmap::IndexMap::default(),
        measures:                 vec![
            MeasureColumn {
                name:       "amount".to_string(),
                sql_type:   SqlType::BigInt,
                nullable:   false,
                additivity: Additivity::Additive,
                column:     None,
            },
            MeasureColumn {
                name:       "balance".to_string(),
                sql_type:   SqlType::BigInt,
                nullable:   false,
                additivity: Additivity::SemiAdditive {
                    over:   "day".to_string(),
                    using:  SemiAdditiveReduction::Last,
                    entity: vec!["account_id".to_string()],
                },
                column:     None,
            },
        ],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![
            filter("account_id", SqlType::BigInt),
            filter("day", SqlType::Date),
            filter("at_ts", SqlType::LocalTimestamp),
            filter("at_tz", SqlType::TimestampTz),
        ],
        calendar_dimensions:      vec![],
        native_dimension_mapping: HashMap::new(),
    }
}

/// `url` with the session `TimeZone` pinned on every connection of the pool.
fn in_zone(url: &str, zone: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}options=-c%20TimeZone%3D{}", zone.replace('/', "%2F"))
}

/// The fixture, and an executor whose sessions run in [`ZONE`]. `None` without a database.
async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&in_zone(&url, ZONE)).await.unwrap();
    let shown: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query("SELECT current_setting('TimeZone') AS zone")
        .await
        .unwrap();
    assert_eq!(shown[0]["zone"], json!(ZONE), "the session zone must be pinned");
    let values = ROWS
        .map(|(amount, day, at_ts, at_tz)| {
            format!("(1, '{day}', '{at_ts}', '{at_tz}', {amount}, {}, '{{}}')", amount * 10)
        })
        .join(", ");
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!(
            "CREATE TABLE {TABLE} (account_id bigint, day date, at_ts timestamp, \
             at_tz timestamptz, amount bigint, balance bigint, data jsonb)"
        ),
        format!("INSERT INTO {TABLE} VALUES {values}"),
    ] {
        let _: Vec<HashMap<String, Value>> = adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    schema.add_fact_table(TABLE.to_string(), fact_table());
    Some(Executor::new(schema, Arc::new(adapter)))
}

/// Each `{column}_{grain}` bucket in order, with `measure` over it, for `request`'s `where`.
async fn buckets(
    executor: &Executor,
    column: &str,
    grain: &str,
    measure: &str,
    filter: Option<Value>,
) -> Vec<(String, String)> {
    let alias = format!("{column}_{grain}");
    let mut request = json!({
        "groupBy": { alias.clone(): true },
        "aggregates": [{ measure: true }],
        "orderBy": { alias.clone(): "ASC" },
    });
    if let Some(filter) = filter {
        request["where"] = filter;
    }
    let response = executor
        .execute(&format!("{{ {ROOT} {{ {alias} {measure} }} }}"), Some(&request))
        .await
        .unwrap_or_else(|e| panic!("{column} by {grain}: {e}"));
    assert_eq!(response.get("errors"), None, "{column} by {grain}: {response}");
    response["data"][ROOT]
        .as_array()
        .unwrap_or_else(|| panic!("{column} by {grain}: {response}"))
        .iter()
        .map(|row| {
            let text = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
            (text(&row[&alias]), text(&row[measure]))
        })
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
}

/// A `date` column's half-month starts on the 1st or the 16th: the 15th and the 16th part,
/// and the end of February falls in the month's second half, leap year or not.
#[tokio::test]
async fn a_date_half_month_starts_on_the_1st_or_the_16th() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert_eq!(
        buckets(&executor, "day", "half_month", "amount_sum", None).await,
        pairs(&[
            ("2024-02-16", "16"),
            ("2026-01-01", "3"),
            ("2026-01-16", "4"),
            ("2026-02-16", "8"),
            ("2026-06-16", "32"),
            ("2026-07-01", "64"),
            ("2026-12-16", "128"),
        ])
    );
}

/// A `date` column's semester starts on 1 January or 1 July: 30 June and 1 July part.
#[tokio::test]
async fn a_date_semester_starts_on_1_january_or_1_july() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert_eq!(
        buckets(&executor, "day", "semester", "amount_sum", None).await,
        pairs(&[
            ("2024-01-01", "16"),
            ("2026-01-01", "47"),
            ("2026-07-01", "192")
        ])
    );
}

/// A `timestamp` column buckets in no zone: the last second of the 15th and of 30 June stay
/// in their period, and each key is a local date-time.
#[tokio::test]
async fn a_timestamp_half_month_and_semester_are_local_date_times() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert_eq!(
        buckets(&executor, "at_ts", "half_month", "amount_sum", None).await,
        pairs(&[
            ("2024-02-16T00:00:00", "16"),
            ("2026-01-01T00:00:00", "3"),
            ("2026-01-16T00:00:00", "4"),
            ("2026-02-16T00:00:00", "8"),
            ("2026-06-16T00:00:00", "32"),
            ("2026-07-01T00:00:00", "64"),
            ("2026-12-16T00:00:00", "128"),
        ])
    );
    assert_eq!(
        buckets(&executor, "at_ts", "semester", "amount_sum", None).await,
        pairs(&[
            ("2024-01-01T00:00:00", "16"),
            ("2026-01-01T00:00:00", "47"),
            ("2026-07-01T00:00:00", "192"),
        ])
    );
}

/// A `timestamptz` bucket starts at local midnight in the session zone, on either side of
/// a daylight-saving change: 16 March and 1 January at `+01`, 16 June, 1 July and 16 October
/// at `+02`. The keys are those instants, rendered in UTC.
#[tokio::test]
async fn a_timestamptz_half_month_and_semester_start_at_local_midnight_across_dst() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert_eq!(
        buckets(&executor, "at_tz", "half_month", "amount_sum", None).await,
        pairs(&[
            ("2025-12-31T23:00:00+00:00", "64"),
            ("2026-02-28T23:00:00+00:00", "1"),
            ("2026-03-15T23:00:00+00:00", "6"),
            ("2026-06-15T22:00:00+00:00", "8"),
            ("2026-06-30T22:00:00+00:00", "16"),
            ("2026-10-15T22:00:00+00:00", "32"),
            ("2026-12-15T23:00:00+00:00", "128"),
        ])
    );
    assert_eq!(
        buckets(&executor, "at_tz", "semester", "amount_sum", None).await,
        pairs(&[
            ("2025-12-31T23:00:00+00:00", "79"),
            ("2026-06-30T22:00:00+00:00", "176")
        ])
    );
}

/// A semi-additive balance carried into half-months with no row: both halves of every
/// month in range are buckets, the 1st and the 16th, whether or not a row falls in them.
#[tokio::test]
async fn a_balance_is_carried_into_every_half_month_in_range() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let range = json!({ "day_gte": "2026-01-01", "day_lte": "2026-03-31" });
    assert_eq!(
        buckets(&executor, "day", "half_month", "balance_sum", Some(range)).await,
        pairs(&[
            ("2026-01-01", "20"),
            ("2026-01-16", "40"),
            ("2026-02-01", "40"),
            ("2026-02-16", "80"),
            ("2026-03-01", "80"),
            ("2026-03-16", "80"),
        ])
    );
}

/// A semi-additive balance carried into a semester with no row: the series steps by six
/// months from 1 January.
#[tokio::test]
async fn a_balance_is_carried_into_every_semester_in_range() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let range = json!({ "day_gte": "2026-01-01", "day_lte": "2027-06-30" });
    assert_eq!(
        buckets(&executor, "day", "semester", "balance_sum", Some(range)).await,
        pairs(&[
            ("2026-01-01", "320"),
            ("2026-07-01", "1280"),
            ("2027-01-01", "1280")
        ])
    );
}
