//! `Timestamp` as a PostgreSQL `timestamptz`, through sqlx and tokio-postgres, against a
//! real database, beside chrono's `DateTime<Utc>` over the same values.
//!
//! `#[ignore]`: needs `DATABASE_URL`. Run with:
//! `cargo test -p fraiseql-time --all-features --test timestamptz_pg -- --ignored
//! --test-threads=1`.

use chrono::{DateTime, Utc};
use fraiseql_test_support::database_url;
use fraiseql_time::Timestamp;
use sqlx::{PgPool, Row as _};
use tokio_postgres::NoTls;

async fn pool() -> PgPool {
    PgPool::connect(&database_url()).await.expect("sqlx connects")
}

async fn client() -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(&database_url(), NoTls)
        .await
        .expect("tokio-postgres connects");
    tokio::spawn(connection);
    client
}

fn at(second: i64, nanosecond: i32) -> Timestamp {
    Timestamp::new(second, nanosecond).expect("an instant in range")
}

const fn chrono_at(second: i64, nanosecond: i32) -> DateTime<Utc> {
    DateTime::from_timestamp(second, nanosecond.unsigned_abs()).expect("an instant in range")
}

/// Values in range read as the instant chrono read, through both drivers.
const IN_RANGE: &[&str] = &[
    "2025-11-20 12:00:00.123456+00",
    "2025-11-20 12:00:00.12+00",
    "2000-01-01 00:00:00+00",
    "1999-12-31 23:59:59.999999+00",
    "1969-12-31 23:59:59.5+00",
    "1900-01-01 00:00:00+00",
    "0001-01-01 00:00:00+00",
    "4713-11-24 00:00:00+00 BC",
    "9999-12-30 22:00:00.999999+00",
];

/// Values PostgreSQL holds and a `Timestamp` does not.
const OUT_OF_RANGE: &[(&str, &str)] = &[
    ("infinity", "timestamptz infinity is outside"),
    ("-infinity", "timestamptz -infinity is outside"),
    ("9999-12-31 23:59:59.999999+00", "timestamptz 252455615999999999 µs"),
    ("294276-12-31 23:59:59.999999+00", "timestamptz 9223371331199999999 µs"),
];

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn values_in_range_read_as_chrono_read_them() {
    let pool = pool().await;
    let client = client().await;
    for text in IN_RANGE {
        let sql = format!("select '{text}'::timestamptz");
        // Reason: the literal is one of the test's own constants.
        let ours: Timestamp = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
            .fetch_one(&pool)
            .await
            .expect("sqlx reads");
        let theirs: DateTime<Utc> =
            // Reason: the literal is one of the test's own constants.
            sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).fetch_one(&pool).await.expect("sqlx reads chrono");
        assert_eq!(ours.as_microsecond(), theirs.timestamp_micros(), "sqlx, {text}");

        let row = client.query_one(&sql, &[]).await.expect("tokio-postgres reads");
        let ours: Timestamp = row.get(0);
        let theirs: DateTime<Utc> = row.get(0);
        assert_eq!(ours.as_microsecond(), theirs.timestamp_micros(), "tokio-postgres, {text}");
    }
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn values_out_of_range_are_a_decode_error_naming_the_value() {
    let pool = pool().await;
    let client = client().await;
    for (text, named) in OUT_OF_RANGE {
        let sql = format!("select '{text}'::timestamptz");
        // Reason: the literal is one of the test's own constants.
        let error = sqlx::query_scalar::<_, Timestamp>(sqlx::AssertSqlSafe(sql.as_str()))
            .fetch_one(&pool)
            .await
            .expect_err("sqlx refuses");
        assert!(error.to_string().contains(named), "sqlx, {text}: {error}");

        let row = client.query_one(&sql, &[]).await.expect("the row arrives");
        let error = row.try_get::<_, Timestamp>(0).expect_err("tokio-postgres refuses");
        let source = std::error::Error::source(&error).map(ToString::to_string);
        assert!(
            source.as_deref().is_some_and(|s| s.contains(named)),
            "tokio-postgres, {text}: {error} / {source:?}"
        );
    }
}

/// Instants written, then read back by PostgreSQL as text in UTC.
const WRITTEN: &[(i64, i32)] = &[
    (1_763_640_000, 123_456_789),
    (1_763_640_000, 120_000_000),
    (946_684_800, 0),
    (946_684_799, 999_999_999),
    (946_684_799, 999_999),
    (-1, 999_999_999),
    (-62_135_596_800, 1_999),
    (-210_866_803_200, 0),
    (253_402_207_200, 999_999_999),
];

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn values_are_written_as_chrono_wrote_them() {
    const AS_TEXT: &str = "select ($1::timestamptz at time zone 'UTC')::text";
    let pool = pool().await;
    let client = client().await;
    for &(second, nanosecond) in WRITTEN {
        let what = format!("{second} s {nanosecond} ns");
        let ours: String = sqlx::query_scalar(AS_TEXT)
            .bind(at(second, nanosecond))
            .fetch_one(&pool)
            .await
            .expect("sqlx writes");
        let theirs: String = sqlx::query_scalar(AS_TEXT)
            .bind(chrono_at(second, nanosecond))
            .fetch_one(&pool)
            .await
            .expect("sqlx writes chrono");
        assert_eq!(ours, theirs, "sqlx, {what}");

        let ours: String = client
            .query_one(AS_TEXT, &[&at(second, nanosecond)])
            .await
            .expect("tokio-postgres writes")
            .get(0);
        let theirs: String = client
            .query_one(AS_TEXT, &[&chrono_at(second, nanosecond)])
            .await
            .expect("tokio-postgres writes chrono")
            .get(0);
        assert_eq!(ours, theirs, "tokio-postgres, {what}");
    }
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn a_value_written_reads_back_truncated_to_the_microsecond() {
    let pool = pool().await;
    let client = client().await;
    for &(second, nanosecond) in WRITTEN {
        let written = at(second, nanosecond);
        // Truncated toward 2000-01-01, not toward 1970: the two differ between the epochs.
        let since_2000 = written.duration_since(at(946_684_800, 0)).as_micros();
        let expected = at(946_684_800, 0)
            .checked_add(jiff::SignedDuration::from_micros(
                i64::try_from(since_2000).expect("within i64"),
            ))
            .expect("in range");
        let read: Timestamp = sqlx::query_scalar("select $1::timestamptz")
            .bind(written)
            .fetch_one(&pool)
            .await
            .expect("sqlx round trip");
        assert_eq!(read, expected, "sqlx, {second} s {nanosecond} ns");
        let read: Timestamp = client
            .query_one("select $1::timestamptz", &[&written])
            .await
            .expect("tokio-postgres round trip")
            .get(0);
        assert_eq!(read, expected, "tokio-postgres, {second} s {nanosecond} ns");
    }
}

/// sqlx's simple-query path returns text, in the session's time zone.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_text_form_reads_in_any_session_time_zone() {
    let pool = pool().await;
    let mut connection = pool.acquire().await.expect("a connection");
    for zone in ["UTC", "Asia/Kolkata", "America/St_Johns"] {
        // Reason: the literal is one of the test's own constants.
        let rows = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "set time zone '{zone}'; select '2025-11-20 12:00:00.12+00'::timestamptz"
        )))
        .fetch_all(&mut *connection)
        .await
        .expect("the text query runs");
        let read: Timestamp = rows[0].try_get(0).expect("the text form reads");
        assert_eq!(read, at(1_763_640_000, 120_000_000), "{zone}");
    }
    let rows = sqlx::raw_sql("select 'infinity'::timestamptz")
        .fetch_all(&mut *connection)
        .await
        .expect("the text query runs");
    let error = rows[0].try_get::<Timestamp, _>(0).expect_err("infinity is refused");
    assert!(error.to_string().contains("\"infinity\""), "{error}");
}
