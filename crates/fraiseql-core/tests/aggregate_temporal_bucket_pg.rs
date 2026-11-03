//! #1798: a temporal bucket of a `date` column is a date, whatever the session time zone.
//!
//! `DATE_TRUNC(<grain>, <date>)` resolves to `date_trunc(text, timestamptz)`: the date is
//! read as midnight in the session's `TimeZone` and the bucket comes back as an instant. East
//! of UTC that instant is the previous day in UTC, so every bucket was keyed one day (or one
//! month, one year) early. The sums were right; only the key was wrong.
//!
//! Each test pins the session zone on its own pool (`options=-c TimeZone=…`) and asserts it
//! took before reading a bucket: a zone that silently stayed UTC would hide the defect.
//! `Asia/Tokyo` (UTC+9) is the zone that shows it; `America/Los_Angeles` is the control, where the
//! old key already had the right date part. `timestamp` and `timestamptz` columns are pinned
//! as they are.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test` leg
//! and runs in the Dagger `integration: postgres` leg (`--test '*'`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_tzb_bucket` fixture → run
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

const TABLE: &str = "tf_tzb_bucket";
const ROOT: &str = "tzb_bucket_aggregate";
const EAST: &str = "Asia/Tokyo";
const WEST: &str = "America/Los_Angeles";

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
        measures:                 vec![
            MeasureColumn {
                name:       "amount".to_string(),
                sql_type:   SqlType::BigInt,
                nullable:   false,
                additivity: Additivity::Additive,
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
            },
        ],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![
            filter("account_id", SqlType::BigInt),
            filter("day", SqlType::Date),
            filter("at_ts", SqlType::Timestamp),
            filter("at_tz", SqlType::Timestamp),
        ],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

/// `url` with the session `TimeZone` pinned on every connection of the pool.
fn in_zone(url: &str, zone: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}options=-c%20TimeZone%3D{}", zone.replace('/', "%2F"))
}

/// The fixture, and an executor whose sessions run in `zone`. `None` without a database.
async fn executor_in(zone: &str) -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&in_zone(&url, zone)).await.unwrap();
    let shown: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query("SELECT current_setting('TimeZone') AS zone")
        .await
        .unwrap();
    assert_eq!(shown[0]["zone"], json!(zone), "the session zone must be pinned");
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!(
            "CREATE TABLE {TABLE} (account_id bigint, day date, at_ts timestamp, \
             at_tz timestamptz, amount bigint, balance bigint, data jsonb)"
        ),
        format!(
            "INSERT INTO {TABLE} VALUES \
             (1, '2026-07-01', '2026-07-01 12:00', '2026-07-01 12:00+00', 1, 10, '{{}}'), \
             (1, '2026-07-02', '2026-07-02 12:00', '2026-07-02 12:00+00', 2, 20, '{{}}'), \
             (1, '2026-12-31', '2026-12-31 12:00', '2026-12-31 12:00+00', 4, 40, '{{}}')"
        ),
    ] {
        let _: Vec<HashMap<String, Value>> = adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    schema.add_fact_table(TABLE.to_string(), fact_table());
    Some(Executor::new(schema, Arc::new(adapter)))
}

/// The bucket keys of `{column}` by `grain`, in order: `groupBy: {column_grain: true}`, keyed
/// by `column_grain`.
async fn keys(executor: &Executor, column: &str, grain: &str, measure: &str) -> Vec<Value> {
    let alias = format!("{column}_{grain}");
    let request = json!({
        "groupBy": { alias.clone(): true },
        "aggregates": [{ measure: true }],
        "orderBy": { alias.clone(): "ASC" },
    });
    let response = executor
        .execute(&format!("{{ {ROOT} {{ {alias} }} }}"), Some(&request))
        .await
        .unwrap_or_else(|e| panic!("{column} by {grain}: {e}"));
    response["data"][ROOT]
        .as_array()
        .unwrap_or_else(|| panic!("{column} by {grain}: {response}"))
        .iter()
        .map(|row| row[&alias].clone())
        .collect()
}

/// Each grain of the `date` column, and the dates its buckets start on.
const DATE_BUCKETS: [(&str, &[&str]); 5] = [
    ("day", &["2026-07-01", "2026-07-02", "2026-12-31"]),
    ("week", &["2026-06-29", "2026-12-28"]),
    ("month", &["2026-07-01", "2026-12-01"]),
    ("quarter", &["2026-07-01", "2026-10-01"]),
    ("year", &["2026-01-01"]),
];

async fn assert_date_buckets(zone: &str) {
    let Some(executor) = executor_in(zone).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    for (grain, dates) in DATE_BUCKETS {
        let expected: Vec<Value> = dates.iter().map(|d| json!(d)).collect();
        assert_eq!(
            keys(&executor, "day", grain, "amount_sum").await,
            expected,
            "{zone}: a {grain} bucket of a date column is the date it starts on"
        );
    }
}

/// East of UTC: the zone the old key shifted to the day before.
#[tokio::test]
async fn a_date_bucket_is_its_date_east_of_utc() {
    assert_date_buckets(EAST).await;
}

/// West of UTC the old key had the right date part: what this asserts beyond it is the key
/// being a date at all.
#[tokio::test]
async fn a_date_bucket_is_its_date_west_of_utc() {
    assert_date_buckets(WEST).await;
}

/// The control for the east test: west of UTC the key's date part was already right, so a
/// red east test is the zone shift, not the format.
#[tokio::test]
async fn west_of_utc_the_date_part_was_already_right() {
    let Some(executor) = executor_in(WEST).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    for (grain, dates) in DATE_BUCKETS {
        let parts: Vec<String> = keys(&executor, "day", grain, "amount_sum")
            .await
            .iter()
            .map(|k| k.as_str().unwrap_or_default().chars().take(10).collect())
            .collect();
        assert_eq!(parts, dates, "{WEST}: the date part of a {grain} bucket");
    }
}

/// A balance carried forward into months with no row is keyed by the month it is carried
/// into, as a date: the carried-forward series is bucketed as the grouping is.
#[tokio::test]
async fn a_carried_forward_date_bucket_is_its_date_east_of_utc() {
    let Some(executor) = executor_in(EAST).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let months: Vec<Value> = (7..=12).map(|m| json!(format!("2026-{m:02}-01"))).collect();
    assert_eq!(keys(&executor, "day", "month", "balance_sum").await, months);
}

/// A `timestamp` column buckets as it did: a timestamp, in no zone.
#[tokio::test]
async fn a_timestamp_bucket_is_unchanged() {
    for zone in [EAST, WEST] {
        let Some(executor) = executor_in(zone).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        assert_eq!(
            keys(&executor, "at_ts", "month", "amount_sum").await,
            vec![json!("2026-07-01 00:00:00"), json!("2026-12-01 00:00:00")],
            "{zone}"
        );
    }
}

/// A `timestamptz` column buckets as it did: at midnight in the session zone, as an instant.
#[tokio::test]
async fn a_timestamptz_bucket_is_unchanged() {
    for (zone, expected) in [
        (
            EAST,
            [
                "2026-06-30T15:00:00+00:00",
                "2026-07-01T15:00:00+00:00",
                "2026-12-30T15:00:00+00:00",
            ],
        ),
        (
            WEST,
            [
                "2026-07-01T07:00:00+00:00",
                "2026-07-02T07:00:00+00:00",
                "2026-12-31T08:00:00+00:00",
            ],
        ),
    ] {
        let Some(executor) = executor_in(zone).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        assert_eq!(
            keys(&executor, "at_tz", "day", "amount_sum").await,
            expected.map(|k| json!(k)).to_vec(),
            "{zone}"
        );
    }
}
