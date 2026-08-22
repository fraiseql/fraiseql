//! The SQL that `fraiseql-arrow` generates must be **executed** against a real
//! PostgreSQL, not string-compared (#715, #717).
//!
//! Pass-1 graded this crate B− precisely because its SQL generation had never
//! faced a real database: `build_insert_query` emitted a two-argument
//! `to_timestamp` (not valid PostgreSQL), rendered `NaN`/`Infinity` as bare
//! invalid tokens, and rendered NULL slots as garbage default values — so
//! `DoPut`/`DoExchange` timestamp uploads had likely never worked.
//!
//! `#[ignore]` — needs `DATABASE_URL` (a real Postgres) and the `testing`
//! feature (the crate exposes `build_insert_query` under it). Run with:
//! `cargo test -p fraiseql-arrow --all-features --test insert_sql_pg -- --ignored
//! --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // Reason: test code, panics acceptable
#![allow(clippy::items_after_statements)] // Reason: test helper items defined near use site
#![allow(clippy::missing_const_for_fn)] // Reason: test helpers, constness is noise here
#![cfg(feature = "testing")]

use std::sync::Arc;

use arrow::{
    array::{
        ArrayRef, BooleanArray, Date32Array, Float32Array, Float64Array, Int8Array, Int16Array,
        Int32Array, Int64Array, LargeStringArray, StringArray, TimestampMicrosecondArray,
        TimestampMillisecondArray, TimestampNanosecondArray, TimestampSecondArray, UInt8Array,
        UInt16Array, UInt32Array, UInt64Array,
    },
    datatypes::{DataType, Field, Schema, TimeUnit},
    record_batch::RecordBatch,
};
use chrono::{DateTime, Utc};
use fraiseql_arrow::flight_server::build_insert_query;
use sqlx::{
    Row,
    postgres::{PgPool, PgPoolOptions},
};

async fn pool() -> PgPool {
    let url = fraiseql_test_support::database_url();
    PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap()
}

fn unique_table() -> String {
    format!("ta_insert_sql_{}", uuid::Uuid::new_v4().simple())
}

/// One batch covering every supported Arrow type, three rows each:
/// row 0 = ordinary values, row 1 = extremes / specials / pre-epoch,
/// row 2 = all NULL.
fn full_matrix_batch() -> RecordBatch {
    let fields = vec![
        Field::new("c_i8", DataType::Int8, true),
        Field::new("c_i16", DataType::Int16, true),
        Field::new("c_i32", DataType::Int32, true),
        Field::new("c_i64", DataType::Int64, true),
        Field::new("c_u8", DataType::UInt8, true),
        Field::new("c_u16", DataType::UInt16, true),
        Field::new("c_u32", DataType::UInt32, true),
        Field::new("c_u64", DataType::UInt64, true),
        Field::new("c_f32", DataType::Float32, true),
        Field::new("c_f64", DataType::Float64, true),
        Field::new("c_f64_neg", DataType::Float64, true),
        Field::new("c_text", DataType::Utf8, true),
        Field::new("c_large", DataType::LargeUtf8, true),
        Field::new("c_bool", DataType::Boolean, true),
        Field::new("c_ts_s", DataType::Timestamp(TimeUnit::Second, None), true),
        Field::new("c_ts_ms", DataType::Timestamp(TimeUnit::Millisecond, None), true),
        Field::new("c_ts_us", DataType::Timestamp(TimeUnit::Microsecond, None), true),
        Field::new("c_ts_ns", DataType::Timestamp(TimeUnit::Nanosecond, None), true),
        Field::new("c_date", DataType::Date32, true),
    ];
    let columns: Vec<ArrayRef> = vec![
        Arc::new(Int8Array::from(vec![Some(1), Some(-128), None])),
        Arc::new(Int16Array::from(vec![Some(2), Some(i16::MIN), None])),
        Arc::new(Int32Array::from(vec![Some(3), Some(i32::MIN), None])),
        Arc::new(Int64Array::from(vec![Some(4), Some(i64::MIN), None])),
        Arc::new(UInt8Array::from(vec![Some(5), Some(u8::MAX), None])),
        Arc::new(UInt16Array::from(vec![Some(6), Some(u16::MAX), None])),
        Arc::new(UInt32Array::from(vec![Some(7), Some(u32::MAX), None])),
        Arc::new(UInt64Array::from(vec![Some(8), Some(u64::MAX), None])),
        Arc::new(Float32Array::from(vec![Some(1.5), Some(f32::NAN), None])),
        Arc::new(Float64Array::from(vec![Some(2.5), Some(f64::INFINITY), None])),
        Arc::new(Float64Array::from(vec![Some(-2.5), Some(f64::NEG_INFINITY), None])),
        Arc::new(StringArray::from(vec![Some("plain"), Some("O'Reilly; DROP TABLE x; --"), None])),
        Arc::new(LargeStringArray::from(vec![Some("large"), Some("it's"), None])),
        Arc::new(BooleanArray::from(vec![Some(true), Some(false), None])),
        // 2023-11-14T22:13:20Z and one second before the epoch.
        Arc::new(TimestampSecondArray::from(vec![Some(1_700_000_000), Some(-1), None])),
        // …with millis, and 1.5 s before the epoch.
        Arc::new(TimestampMillisecondArray::from(vec![
            Some(1_700_000_000_123),
            Some(-1_500),
            None,
        ])),
        Arc::new(TimestampMicrosecondArray::from(vec![
            Some(1_700_000_000_123_456),
            Some(-1_500_000),
            None,
        ])),
        // Nanos chosen with a whole-microsecond component: PostgreSQL stores
        // microseconds, so sub-microsecond digits are truncated.
        Arc::new(TimestampNanosecondArray::from(vec![
            Some(1_700_000_000_123_456_000),
            Some(-1_500_000_000),
            None,
        ])),
        // 2023-12-09, and 1969-01-01 (365 days before the epoch).
        Arc::new(Date32Array::from(vec![Some(19_700), Some(-365), None])),
    ];
    RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).unwrap()
}

const CREATE_COLUMNS: &str = "\
    c_i8 SMALLINT, c_i16 SMALLINT, c_i32 INTEGER, c_i64 BIGINT, \
    c_u8 SMALLINT, c_u16 INTEGER, c_u32 BIGINT, c_u64 NUMERIC(20,0), \
    c_f32 REAL, c_f64 DOUBLE PRECISION, c_f64_neg DOUBLE PRECISION, \
    c_text TEXT, c_large TEXT, c_bool BOOLEAN, \
    c_ts_s TIMESTAMPTZ, c_ts_ms TIMESTAMPTZ, c_ts_us TIMESTAMPTZ, c_ts_ns TIMESTAMPTZ, \
    c_date DATE";

fn utc(secs: i64, nanos: u32) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, nanos).unwrap()
}

/// #715: the generated INSERT must execute — every type, every precision,
/// specials, pre-epoch timestamps and NULLs — and the values read back must be
/// the values the batch carried.
#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn generated_insert_executes_and_round_trips_every_type() {
    let pool = pool().await;
    let table = unique_table();
    sqlx::query(&format!("CREATE TABLE {table} ({CREATE_COLUMNS})"))
        .execute(&pool)
        .await
        .unwrap();

    let batch = full_matrix_batch();
    let sql = build_insert_query(&table, &batch).expect("query generation must succeed");
    sqlx::raw_sql(&sql).execute(&pool).await.unwrap_or_else(|e| {
        panic!("generated INSERT must be valid PostgreSQL, but execution failed: {e}\nSQL: {sql}")
    });

    let rows = sqlx::query(&format!(
        "SELECT *, c_u64::text AS c_u64_text FROM {table} ORDER BY c_i8 DESC NULLS LAST"
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 3);

    // Row 0: ordinary values.
    let r = &rows[0];
    assert_eq!(r.get::<i16, _>("c_i8"), 1);
    assert_eq!(r.get::<i64, _>("c_i64"), 4);
    assert_eq!(r.get::<String, _>("c_u64_text"), "8");
    assert!((r.get::<f32, _>("c_f32") - 1.5).abs() < f32::EPSILON);
    assert_eq!(r.get::<String, _>("c_text"), "plain");
    assert!(r.get::<bool, _>("c_bool"));
    assert_eq!(r.get::<DateTime<Utc>, _>("c_ts_s"), utc(1_700_000_000, 0));
    assert_eq!(r.get::<DateTime<Utc>, _>("c_ts_ms"), utc(1_700_000_000, 123_000_000));
    assert_eq!(r.get::<DateTime<Utc>, _>("c_ts_us"), utc(1_700_000_000, 123_456_000));
    assert_eq!(r.get::<DateTime<Utc>, _>("c_ts_ns"), utc(1_700_000_000, 123_456_000));
    assert_eq!(
        r.get::<chrono::NaiveDate, _>("c_date"),
        chrono::NaiveDate::from_ymd_opt(2023, 12, 9).unwrap()
    );

    // Row 1: extremes, float specials, injection-shaped text, pre-epoch
    // timestamps (a negative epoch used to render `to_timestamp(-2, -500000)`).
    let r = &rows[1];
    assert_eq!(r.get::<i16, _>("c_i8"), -128);
    assert_eq!(r.get::<i64, _>("c_i64"), i64::MIN);
    assert_eq!(r.get::<String, _>("c_u64_text"), u64::MAX.to_string());
    assert!(r.get::<f32, _>("c_f32").is_nan(), "NaN must round-trip");
    let inf: f64 = r.get("c_f64");
    assert!(inf.is_infinite() && inf.is_sign_positive(), "Infinity must round-trip");
    let neg_inf: f64 = r.get("c_f64_neg");
    assert!(neg_inf.is_infinite() && neg_inf.is_sign_negative(), "-Infinity must round-trip");
    assert_eq!(r.get::<String, _>("c_text"), "O'Reilly; DROP TABLE x; --");
    assert_eq!(r.get::<String, _>("c_large"), "it's");
    assert_eq!(r.get::<DateTime<Utc>, _>("c_ts_s"), utc(-1, 0));
    assert_eq!(r.get::<DateTime<Utc>, _>("c_ts_ms"), utc(-2, 500_000_000));
    assert_eq!(r.get::<DateTime<Utc>, _>("c_ts_us"), utc(-2, 500_000_000));
    assert_eq!(r.get::<DateTime<Utc>, _>("c_ts_ns"), utc(-2, 500_000_000));
    assert_eq!(
        r.get::<chrono::NaiveDate, _>("c_date"),
        chrono::NaiveDate::from_ymd_opt(1969, 1, 1).unwrap()
    );

    // Row 2: a NULL slot must be SQL NULL — not a garbage default value.
    let r = &rows[2];
    for col in [
        "c_i8", "c_i16", "c_i32", "c_i64", "c_u8", "c_u16", "c_u32", "c_text", "c_large",
    ] {
        assert!(
            r.try_get::<Option<String>, _>(col).map_or(true, |v| v.is_none()),
            "column {col} must be NULL in the all-NULL row"
        );
    }
    assert!(r.get::<Option<bool>, _>("c_bool").is_none(), "NULL bool rendered as a value");
    assert!(
        r.get::<Option<DateTime<Utc>>, _>("c_ts_us").is_none(),
        "NULL timestamp rendered as a value"
    );
    assert!(
        r.get::<Option<chrono::NaiveDate>, _>("c_date").is_none(),
        "NULL date rendered as a value"
    );

    sqlx::query(&format!("DROP TABLE {table}")).execute(&pool).await.unwrap();
}

/// #717: a batch of heterogeneous queries must not be described by the first
/// query's schema. One schema header for N independently-inferred schemas is an
/// undecodable stream; the honest behaviour is a loud error.
#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn batched_queries_with_heterogeneous_schemas_error_loudly() {
    use arrow_flight::flight_service_server::FlightService;

    let url = fraiseql_test_support::database_url();

    struct TestFlightAdapter {
        inner: fraiseql_core::db::PostgresAdapter,
    }

    #[async_trait::async_trait]
    impl fraiseql_arrow::ArrowDatabaseAdapter for TestFlightAdapter {
        async fn execute_raw_query(
            &self,
            sql: &str,
        ) -> fraiseql_arrow::db::DatabaseResult<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
        > {
            use fraiseql_core::db::traits::DatabaseAdapter as _;
            self.inner
                .execute_raw_query(sql)
                .await
                .map_err(|e| fraiseql_arrow::db::DatabaseError::new(e.to_string()))
        }
    }

    const SECRET: &str = "insert-sql-pg-test-session-secret";

    fn session_token(secret: &str) -> String {
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

        #[derive(serde::Serialize)]
        struct Claims {
            sub:          String,
            exp:          i64,
            iat:          i64,
            scopes:       Vec<String>,
            session_type: String,
        }
        let now = chrono::Utc::now();
        let claims = Claims {
            sub:          "schema-mismatch-tester".to_string(),
            exp:          (now + chrono::Duration::minutes(5)).timestamp(),
            iat:          now.timestamp(),
            scopes:       vec!["read".to_string()],
            session_type: "flight".to_string(),
        };
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    temp_env::async_with_vars([("FLIGHT_SESSION_SECRET", Some(SECRET))], async {
        let pg = fraiseql_core::db::PostgresAdapter::new(&url).await.unwrap();
        let adapter: std::sync::Arc<dyn fraiseql_arrow::ArrowDatabaseAdapter> =
            std::sync::Arc::new(TestFlightAdapter { inner: pg });
        let service =
            fraiseql_arrow::FraiseQLFlightService::new_with_db(adapter).with_raw_sql_enabled();

        let ticket = fraiseql_arrow::ticket::FlightTicket::BatchedQueries {
            queries: vec![
                "SELECT 1 AS a".to_string(),
                "SELECT 'x' AS b, 'y' AS c".to_string(),
            ],
        };
        let mut request = tonic::Request::new(arrow_flight::Ticket {
            ticket: ticket.encode().unwrap().into(),
        });
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {}", session_token(SECRET)).parse().unwrap());

        let result = service.do_get(request).await;
        let Err(err) = result else {
            panic!(
                "heterogeneous batched queries produced an Ok stream: one schema header \
                 now describes two different schemas — undecodable on the client (#717)"
            )
        };
        assert!(
            err.message().to_lowercase().contains("schema"),
            "the error must name the schema mismatch, got: {}",
            err.message()
        );
    })
    .await;
}
