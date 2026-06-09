#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code, panics are acceptable

//! Behavioural proof of the canonical `duration_ms` computation
//! ([`fraiseql_db::changelog::duration_ms_sql`]) against real PostgreSQL.
//!
//! The headline test sets `fraiseql.started_at` to 90.25 s in the past and
//! asserts the expression yields ~90250 ms — it FAILS against any
//! `EXTRACT(MILLISECONDS FROM interval)` implementation (which would return
//! 30250: the seconds-within-the-minute × 1000, truncating intervals ≥ 1 min).

use fraiseql_db::{
    DatabaseAdapter, PostgresAdapter,
    changelog::{CLOCK_TIMESTAMP_DIRECTIVE, STARTED_AT_VAR, duration_ms_sql},
};

/// Connect to the harness-provided Postgres (Dagger-bound in CI, or a local
/// spawn with the `local-testcontainers` feature).
async fn connect_pg() -> (tokio_postgres::Client, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let (client, connection) = tokio_postgres::connect(svc.url(), tokio_postgres::NoTls)
        .await
        .expect("failed to connect");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {e}");
        }
    });
    (client, svc)
}

#[tokio::test]
async fn duration_ms_full_wallclock_over_one_minute() {
    let (mut client, _svc) = connect_pg().await;
    let txn = client.transaction().await.unwrap();

    // Stamp started_at 90.25 s in the past, on the DB clock (txn-local).
    txn.execute(
        "SELECT set_config($1, (clock_timestamp() - interval '90.25 seconds')::text, true)",
        &[&STARTED_AT_VAR],
    )
    .await
    .unwrap();

    let expr = duration_ms_sql(STARTED_AT_VAR);
    let d: i32 = txn.query_one(&format!("SELECT {expr} AS d"), &[]).await.unwrap().get("d");

    // Full wall-clock: ~90250 ms (allow scheduling slack). NOT the 30250 ms the
    // EXTRACT(MILLISECONDS) trap would yield for an interval >= 1 minute.
    assert!(
        (90_000..=91_500).contains(&d),
        "expected ~90250 ms full wall-clock, got {d} (the MILLISECONDS bug yields ~30250)"
    );
    assert!(
        d > 60_000,
        "interval >= 1 minute must not truncate to seconds-within-the-minute"
    );
}

#[tokio::test]
async fn started_at_directive_stamps_the_db_clock_through_the_adapter() {
    // Prove the full clock-unification path: resolve_session_variables emits the
    // directive, the adapter's apply_session_vars resolves it to clock_timestamp()
    // on the function's own transaction, and the function reads back a real DB
    // timestamp (NOT the sentinel literal, NOT an app-clock RFC-3339 value).
    let (client, svc) = connect_pg().await;
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_echo_started_at()
             RETURNS TABLE(started_at_value text)
             LANGUAGE sql AS $$ SELECT current_setting('fraiseql.started_at', true) $$;",
        )
        .await
        .unwrap();

    let adapter = PostgresAdapter::new(svc.url()).await.expect("build adapter");

    let rows = adapter
        .execute_function_call_with_session(
            "public.fn_echo_started_at",
            &[],
            &[(STARTED_AT_VAR, CLOCK_TIMESTAMP_DIRECTIVE)],
        )
        .await
        .expect("call echo function with the started_at directive");

    let value = rows[0]
        .get("started_at_value")
        .and_then(serde_json::Value::as_str)
        .expect("started_at was stamped")
        .to_owned();
    assert_ne!(
        value, CLOCK_TIMESTAMP_DIRECTIVE,
        "directive must be resolved, not stored verbatim"
    );
    assert!(!value.is_empty(), "started_at must be set on the function's connection");
    // It parses as a timestamp on the DB clock.
    value
        .parse::<chrono::DateTime<chrono::Utc>>()
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&value, "%Y-%m-%d %H:%M:%S%.f%#z")
                .map(|n| n.and_utc())
        })
        .unwrap_or_else(|_| panic!("started_at `{value}` should parse as a DB timestamp"));
}

#[tokio::test]
async fn duration_ms_subsecond_precision() {
    let (mut client, _svc) = connect_pg().await;
    let txn = client.transaction().await.unwrap();

    txn.execute(
        "SELECT set_config($1, (clock_timestamp() - interval '250 milliseconds')::text, true)",
        &[&STARTED_AT_VAR],
    )
    .await
    .unwrap();

    let expr = duration_ms_sql(STARTED_AT_VAR);
    let d: i32 = txn.query_one(&format!("SELECT {expr} AS d"), &[]).await.unwrap().get("d");

    assert!((200..=600).contains(&d), "expected ~250 ms, got {d}");
}
