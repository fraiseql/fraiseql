//! #1798: a temporal bucket of a `date` column is a date, whatever the session time zone.
//!
//! `DATE_TRUNC(<grain>, <date>)` resolves to `date_trunc(text, timestamptz)`: the date is
//! read as midnight in the session's `TimeZone` and the bucket comes back as an instant. East
//! of UTC that instant is the previous day in UTC, so every bucket was keyed one day (or one
//! month, one year) early. The sums were right; only the key was wrong.
//!
//! Each test pins the session zone on its own pool (`options=-c TimeZone=…`) and asserts it
//! took before reading a bucket: a zone that silently stayed UTC would hide the defect.
//!
//! #1929: a `timestamp` column (no time zone) is a local date-time, published as
//! `LocalDateTime`: its keys read `2026-07-01T00:00:00`, and a `where` on it compares local
//! date-times, never instants in the session zone.
//! `Asia/Tokyo` (UTC+9) is the zone that shows it; `America/Los_Angeles` is the control, where the
//! old key already had the right date part. `timestamp` and `timestamptz` columns are pinned
//! as they are.
//!
//! #2176: every key is served without a field error, so each is in the format of the scalar
//! its column is published as: a `timestamp` key a `LocalDateTime`, a `timestamptz` key a
//! `DateTime`, a `date` key a `Date`.
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
        inject_params:            indexmap::IndexMap::default(),
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
            filter("at_ts", SqlType::LocalTimestamp),
            filter("at_tz", SqlType::TimestampTz),
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

/// Add rows to the fixture, `values` written as an `INSERT`'s `VALUES` list.
async fn insert(values: &str) {
    let url = fraiseql_test_support::try_database_url().unwrap();
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let _: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(&format!("INSERT INTO {TABLE} VALUES {values}"))
        .await
        .unwrap();
}

/// The distinct values of `column`, in order: `groupBy: {column: true}`.
async fn column_keys(executor: &Executor, column: &str) -> Vec<Value> {
    let request = json!({
        "groupBy": { column: true },
        "aggregates": [{ "amount_sum": true }],
        "orderBy": { column: "ASC" },
    });
    let response = executor
        .execute(&format!("{{ {ROOT} {{ {column} }} }}"), Some(&request))
        .await
        .unwrap_or_else(|e| panic!("by {column}: {e}"));
    // Each key is held to the scalar its column is published as: a refused one is a field
    // error, its key `null`.
    assert_eq!(response.get("errors"), None, "by {column}: {response}");
    response["data"][ROOT]
        .as_array()
        .unwrap_or_else(|| panic!("by {column}: {response}"))
        .iter()
        .map(|row| row[column].clone())
        .collect()
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
    assert_eq!(response.get("errors"), None, "{column} by {grain}: {response}");
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

/// A `timestamp` column buckets in no zone, and its key is a local date-time: a `T`
/// between date and time, and no offset.
#[tokio::test]
async fn a_timestamp_bucket_is_a_local_date_time() {
    for zone in [EAST, WEST] {
        let Some(executor) = executor_in(zone).await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        assert_eq!(
            keys(&executor, "at_ts", "month", "amount_sum").await,
            vec![json!("2026-07-01T00:00:00"), json!("2026-12-01T00:00:00")],
            "{zone}"
        );
    }
}

/// The values of a `timestamp` column, grouped by the column itself: local date-times,
/// a fraction written in 3, 6 or 9 digits as a `timestamptz` one is.
#[tokio::test]
async fn a_timestamp_group_key_is_a_local_date_time() {
    let Some(executor) = executor_in(EAST).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    insert(
        "(2, '2026-07-03', '2026-07-03 08:30:00.12', '2026-07-03 08:30:00.12+00', 8, 80, '{}'), \
         (2, '2026-07-04', '2026-07-04 08:30:00.123456', '2026-07-04 08:30:00.123456+00', 16, \
         160, '{}')",
    )
    .await;
    assert_eq!(
        column_keys(&executor, "at_ts").await,
        [
            "2026-07-01T12:00:00",
            "2026-07-02T12:00:00",
            "2026-07-03T08:30:00.120",
            "2026-07-04T08:30:00.123456",
            "2026-12-31T12:00:00",
        ]
        .map(|k| json!(k))
        .to_vec()
    );
    // The `timestamptz` column alongside, pinned: an instant in UTC with its offset.
    assert_eq!(
        column_keys(&executor, "at_tz").await,
        [
            "2026-07-01T12:00:00+00:00",
            "2026-07-02T12:00:00+00:00",
            "2026-07-03T08:30:00.120+00:00",
            "2026-07-04T08:30:00.123456+00:00",
            "2026-12-31T12:00:00+00:00",
        ]
        .map(|k| json!(k))
        .to_vec()
    );
}

/// A `where` on a `timestamp` column compares local date-times, whatever the session zone.
/// Read as instants in `America/Los_Angeles`, 02:30 on 2026-03-08 (inside the hour the
/// clocks skip) became 03:30, and the row fell outside `< 03:15`.
#[tokio::test]
async fn a_timestamp_filter_compares_local_date_times_in_any_session_zone() {
    let Some(executor) = executor_in(WEST).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    insert("(3, '2026-03-08', '2026-03-08 02:30', '2026-03-08 10:30+00', 32, 320, '{}')").await;
    let request = json!({
        "where": { "at_ts_gte": "2026-03-08T00:00:00", "at_ts_lt": "2026-03-08T03:15:00" },
        "aggregates": [{ "amount_sum": true }],
    });
    let response = executor
        .execute(&format!("{{ {ROOT} {{ amount_sum }} }}"), Some(&request))
        .await
        .unwrap();
    assert_eq!(response["data"][ROOT][0]["amount_sum"], json!(32), "{response}");
}

/// An operand on a `timestamp` column is a local date-time. `::timestamp` drops an offset or a
/// `Z` without a word, so `03:15+05:00` would compare as `03:15`: refused, naming the key, as
/// is a string that is no local date-time at all.
#[tokio::test]
async fn a_local_date_time_operand_with_an_offset_is_refused() {
    let Some(executor) = executor_in(WEST).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    for (operand, says) in [
        ("2026-03-08T03:15:00+05:00", "offset"),
        ("2026-03-08T03:15:00Z", "offset"),
        ("2026-03-08 03:15:00", "LocalDateTime"),
    ] {
        let request = json!({
            "where": { "at_ts_lt": operand },
            "aggregates": [{ "amount_sum": true }],
        });
        let outcome = executor
            .execute(&format!("{{ {ROOT} {{ amount_sum }} }}"), Some(&request))
            .await;
        let text = match &outcome {
            Ok(response) => response.to_string(),
            Err(e) => e.to_string(),
        };
        assert!(
            outcome.is_err() || outcome.as_ref().is_ok_and(|r| r.get("errors").is_some()),
            "{operand}: {text}"
        );
        assert!(text.contains("at_ts_lt") && text.contains(says), "{operand}: {text}");
    }
}

/// An operand on a `timestamptz` or a `date` filter column holds its scalar's format, as a
/// `timestamp` one does: an offset-less instant would be read in the session zone, and a
/// date-time compared with a date would be cut to its day. Refused, naming the key.
#[tokio::test]
async fn an_instant_or_date_operand_outside_its_format_is_refused() {
    let Some(executor) = executor_in(EAST).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    for (key, operand, says) in [
        ("at_tz_lt", "2026-07-01T12:00:00", "DateTime"),
        ("at_tz_gte", "today", "DateTime"),
        ("day_eq", "2026-07-01T00:00:00Z", "Date"),
        ("day_lt", "07/01/2026", "Date"),
    ] {
        let request = json!({
            "where": { key: operand },
            "aggregates": [{ "amount_sum": true }],
        });
        let outcome = executor
            .execute(&format!("{{ {ROOT} {{ amount_sum }} }}"), Some(&request))
            .await;
        let text = match &outcome {
            Ok(response) => response.to_string(),
            Err(e) => e.to_string(),
        };
        assert!(
            outcome.is_err() || outcome.as_ref().is_ok_and(|r| r.get("errors").is_some()),
            "{key} {operand}: {text}"
        );
        assert!(text.contains(key) && text.contains(says), "{key} {operand}: {text}");
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
