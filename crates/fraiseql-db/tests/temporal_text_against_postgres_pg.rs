#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! The response text of `timestamp`, `timestamptz` and `date` cells, checked against
//! PostgreSQL's own rendering of the same values.
//!
//! The adapter writes these cells with hand-written calendar arithmetic, so the oracle is
//! PostgreSQL: each value is generated in SQL from a named date plus a microsecond offset,
//! and decoded beside `to_char`'s text for it. The expected string is `to_char`'s,
//! rewritten to the response's rules: an astronomical year (1 BC is year 0), signed with at
//! least four digits outside 0 to 9999, and the fraction in 3 or 6 digits, none at a whole
//! second.

use fraiseql_db::{postgres::PostgresAdapter, traits::DatabaseAdapter};

/// Named instants across the calendar: PostgreSQL's first, the era boundary at 1 BC/1 AD,
/// the leap days of 1900, 2000 and 2400, both epochs, the last four-digit year and the
/// first five-digit one, and the last year the response renders.
const ANCHORS: &[&str] = &[
    "4713-01-01 00:00:00 BC",
    "0044-03-15 12:00:00 BC",
    "0001-01-01 00:00:00 BC",
    "0001-12-31 23:59:59.999999 BC",
    "0001-01-01 00:00:00",
    "1900-02-28 12:00:00",
    "1900-03-01 00:00:00",
    "1970-01-01 00:00:00",
    "2000-01-01 00:00:00",
    "2000-02-29 00:00:00",
    "2024-01-15 10:30:00",
    "2100-02-28 23:59:59",
    "2400-02-29 00:00:00",
    "9999-12-31 23:59:59.999999",
    "10000-01-01 00:00:00",
    // A day before the last the response renders, so a day later is still rendered.
    "262142-12-30 00:00:00",
];

/// Microsecond offsets from each anchor: either side of it, a fraction of each width, a
/// second and a day either way.
const OFFSETS: &[i64] = &[
    0,
    1,
    -1,
    500_000,
    123_456,
    123_400,
    1_000,
    -1_000_000,
    86_400_000_000,
    -86_400_000_000,
];

/// `to_char`'s year field, which counts BC years from 1, as an astronomical year.
fn year(digits: &str, era: &str) -> i64 {
    let year: i64 = digits.parse().unwrap();
    if era == "BC" { 1 - year } else { year }
}

fn year_text(year: i64) -> String {
    if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{year:+05}")
    }
}

/// `YYYY-MM-DD"T"HH24:MI:SS.US BC` → the response's `timestamp` text.
fn expected_date_time(to_char: &str) -> String {
    let (stamp, era) = to_char.rsplit_once(' ').unwrap();
    let (date, time) = stamp.split_once('T').unwrap();
    let (digits, month_day) = date.split_once('-').unwrap();
    let (whole, micros) = time.split_once('.').unwrap();
    let fraction = match micros {
        "000000" => String::new(),
        _ if micros.ends_with("000") => format!(".{}", &micros[..3]),
        _ => format!(".{micros}"),
    };
    format!("{}-{month_day}T{whole}{fraction}", year_text(year(digits, era)))
}

/// `YYYY-MM-DD BC` → the response's `date` text.
fn expected_date(to_char: &str) -> String {
    let (date, era) = to_char.rsplit_once(' ').unwrap();
    let (digits, month_day) = date.split_once('-').unwrap();
    format!("{}-{month_day}", year_text(year(digits, era)))
}

async fn adapter() -> (PostgresAdapter, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let adapter = PostgresAdapter::new(svc.url()).await.expect("adapter");
    (adapter, svc)
}

#[tokio::test]
async fn every_value_in_the_sweep_is_postgres_own_calendar_in_the_response_form() {
    let (adapter, _svc) = adapter().await;
    let mut values = Vec::new();
    for anchor in ANCHORS {
        for offset in OFFSETS {
            // PostgreSQL has no instant before its first.
            if anchor.starts_with("4713-") && *offset < 0 {
                continue;
            }
            values.push(format!(
                "({}, timestamp '{anchor}' + interval '{offset} microseconds')",
                values.len()
            ));
        }
    }
    let rows = adapter
        .execute_raw_query(&format!(
            "SELECT n, v AS ts, v AT TIME ZONE 'UTC' AS tz, v::date AS d, \
                    to_char(v, 'YYYY-MM-DD\"T\"HH24:MI:SS.US BC') AS pg_ts, \
                    to_char(v::date, 'YYYY-MM-DD BC') AS pg_d \
             FROM (VALUES {}) AS s(n, v) ORDER BY n",
            values.join(", ")
        ))
        .await
        .expect("the sweep decodes");

    assert_eq!(rows.len(), values.len(), "one row per value");
    for row in &rows {
        let pg_ts = row["pg_ts"].as_str().unwrap();
        let want = expected_date_time(pg_ts);
        assert_eq!(row["ts"].as_str(), Some(want.as_str()), "timestamp of {pg_ts}");
        assert_eq!(
            row["tz"].as_str(),
            Some(format!("{want}+00:00").as_str()),
            "timestamptz of {pg_ts}"
        );
        let pg_d = row["pg_d"].as_str().unwrap();
        assert_eq!(row["d"].as_str(), Some(expected_date(pg_d).as_str()), "date of {pg_d}");
    }
}

/// A value stored before 2000-01-01 is a negative count of microseconds. The second it
/// falls in is the one below it, not the one toward zero.
#[tokio::test]
async fn a_microsecond_before_postgres_epoch_is_the_last_one_of_1999() {
    let (adapter, _svc) = adapter().await;
    let rows = adapter
        .execute_raw_query(
            "SELECT timestamp '2000-01-01' - interval '1 microsecond' AS ts, \
                    timestamptz '2000-01-01 00:00:00+00' - interval '1.5 seconds' AS tz",
        )
        .await
        .expect("decodes");

    assert_eq!(rows[0]["ts"], "1999-12-31T23:59:59.999999");
    assert_eq!(rows[0]["tz"], "1999-12-31T23:59:58.500+00:00");
}

#[tokio::test]
async fn postgres_first_and_last_date_and_timestamp() {
    let (adapter, _svc) = adapter().await;
    let rows = adapter
        .execute_raw_query(
            "SELECT date '4713-01-01 BC' AS d, timestamp '4713-01-01 00:00:00 BC' AS ts, \
                    timestamptz '4713-01-01 00:00:00+00 BC' AS tz",
        )
        .await
        .expect("PostgreSQL's first date and timestamp decode");
    assert_eq!(rows[0]["d"], "-4712-01-01");
    assert_eq!(rows[0]["ts"], "-4712-01-01T00:00:00");
    assert_eq!(rows[0]["tz"], "-4712-01-01T00:00:00+00:00");

    // Past year 262142 the response has never rendered a value: PostgreSQL's last
    // timestamp (294276) and date (5874897), and `infinity`, are refused by type.
    for (sql, ty) in [
        ("timestamp '294276-12-31 23:59:59.999999'", "timestamp"),
        ("timestamptz '294276-12-31 23:59:59.999999+00'", "timestamptz"),
        ("date '5874897-12-31'", "date"),
        ("date '262143-01-01'", "date"),
        ("timestamp 'infinity'", "timestamp"),
        ("timestamptz '-infinity'", "timestamptz"),
        ("date 'infinity'", "date"),
    ] {
        let refused = adapter.execute_raw_query(&format!("SELECT {sql} AS v")).await;
        assert!(
            matches!(&refused, Err(fraiseql_error::FraiseQLError::Database { message, .. })
                if message.starts_with(&format!("column `v` of PostgreSQL type `{ty}` is not decoded"))),
            "{sql} is refused by type: {refused:?}"
        );
    }
}
