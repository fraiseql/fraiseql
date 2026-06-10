//! Live-PostgreSQL integration test for the `perf` change-log reader (#392).
//!
//! It provisions the real change-log contract objects (`core.tb_entity_change_log`
//! and `core.v_entity_change_log`) from the shipped migration, seeds a known set
//! of rows, and asserts [`PerfReader::load_samples`] decodes every column. That
//! covers `NULL` durations, the `duration_calc_version` marker pulled out of the
//! `extra_metadata` JSONB, and the trailing-window filter.
//!
//! Self-skips when no `DATABASE_URL` is set (no Dagger leg runs the CLI against a
//! live DB; this is local-only verification). Run against the warm dev database:
//!
//! ```bash
//! DATABASE_URL=postgres://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-cli --features test-postgres --test perf_against_db
//! ```

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_cli::commands::perf::{
    analysis::{RegressionParams, regression_scan},
    reader::{ChangeLogSample, PerfReader},
};
use fraiseql_observers::migrations::entity_change_log_contract_sql;

/// Rebuild the contract objects from the shipped migration and seed a fixed set
/// of rows. Returns the raw client so the caller can read back if needed.
async fn provision(url: &str) -> Option<tokio_postgres::Client> {
    let (client, conn) = match tokio_postgres::connect(url, tokio_postgres::NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping #392 perf reader test: {e}");
            return None;
        },
    };
    tokio::spawn(async move {
        let _ = conn.await;
    });

    // Start from a known slate, then install the real contract (table + view).
    client
        .batch_execute(
            "CREATE SCHEMA IF NOT EXISTS core; \
             DROP VIEW IF EXISTS core.v_entity_change_log; \
             DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE;",
        )
        .await
        .unwrap();
    client.batch_execute(entity_change_log_contract_sql()).await.unwrap();

    // Seed: a v2 row with full metadata, a NULL-duration row, a legacy v1 row,
    // and a row aged out of the 30-day window.
    client
        .batch_execute(
            "INSERT INTO core.tb_entity_change_log \
               (object_type, modification_type, duration_ms, object_id, trace_id, created_at, extra_metadata) \
             VALUES \
               ('PerfProbeUser','UPDATE',120,gen_random_uuid(),'trace-1',now(),'{\"duration_calc_version\":2}'::jsonb), \
               ('PerfProbeUser','INSERT',NULL,gen_random_uuid(),NULL,now(),'{}'::jsonb), \
               ('PerfProbeOrder','DELETE',5,NULL,NULL,now() - interval '3 days','{\"duration_calc_version\":1}'::jsonb), \
               ('PerfProbeOld','UPDATE',999,NULL,NULL,now() - interval '60 days','{\"duration_calc_version\":2}'::jsonb);",
        )
        .await
        .unwrap();

    Some(client)
}

fn find<'a>(
    samples: &'a [ChangeLogSample],
    object_type: &str,
    modification_type: &str,
) -> &'a ChangeLogSample {
    samples
        .iter()
        .find(|s| s.object_type == object_type && s.modification_type == modification_type)
        .expect("expected (object_type, modification_type) sample present")
}

#[tokio::test]
async fn reader_decodes_contract_rows_and_applies_window() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };
    if provision(&url).await.is_none() {
        return;
    }

    let reader = PerfReader::connect(&url).expect("connect perf reader");
    let samples = reader.load_samples(30, None).await.expect("load samples");

    // The 60-day-old row is outside the 30-day window.
    assert!(
        samples.iter().all(|s| s.object_type != "PerfProbeOld"),
        "aged-out row must be excluded by the trailing window: {samples:?}"
    );

    // v2 row: full decode, marker extracted from the JSONB.
    let updated = find(&samples, "PerfProbeUser", "UPDATE");
    assert_eq!(updated.duration_ms, Some(120));
    assert_eq!(updated.duration_calc_version, Some(2));
    assert_eq!(updated.trace_id.as_deref(), Some("trace-1"));
    assert!(updated.object_id.is_some(), "object_id::text decodes to Some");
    assert!(updated.created_at_epoch > 0.0, "created_at decodes to a positive epoch");

    // NULL duration + absent marker decode to None (not an error).
    let inserted = find(&samples, "PerfProbeUser", "INSERT");
    assert_eq!(inserted.duration_ms, None);
    assert_eq!(inserted.duration_calc_version, None);
    assert_eq!(inserted.trace_id, None);

    // Legacy v1 row + NULL object_id.
    let deleted = find(&samples, "PerfProbeOrder", "DELETE");
    assert_eq!(deleted.duration_ms, Some(5));
    assert_eq!(deleted.duration_calc_version, Some(1));
    assert_eq!(deleted.object_id, None);
}

#[tokio::test]
async fn object_type_filter_restricts_rows() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };
    if provision(&url).await.is_none() {
        return;
    }

    let reader = PerfReader::connect(&url).expect("connect perf reader");
    let samples = reader.load_samples(30, Some("PerfProbeOrder")).await.expect("load samples");

    assert!(!samples.is_empty(), "the filtered object_type has a row in-window");
    assert!(
        samples.iter().all(|s| s.object_type == "PerfProbeOrder"),
        "filter restricts to the requested object_type: {samples:?}"
    );
}

#[tokio::test]
async fn regression_scan_flags_a_real_regression_end_to_end() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };
    let Some(client) = provision(&url).await else {
        return;
    };
    // Replace the reader-fixture rows with a clean regression scenario:
    // SmokeE2E/UPDATE doubles from 100ms (baseline window) to 200ms (recent).
    client
        .batch_execute(
            "TRUNCATE core.tb_entity_change_log; \
             INSERT INTO core.tb_entity_change_log \
               (object_type, modification_type, duration_ms, created_at, extra_metadata) \
             SELECT 'SmokeE2E','UPDATE',100, now() - interval '10 days', \
               '{\"duration_calc_version\":2}'::jsonb FROM generate_series(1,10); \
             INSERT INTO core.tb_entity_change_log \
               (object_type, modification_type, duration_ms, created_at, extra_metadata) \
             SELECT 'SmokeE2E','UPDATE',200, now() - interval '1 days', \
               '{\"duration_calc_version\":2}'::jsonb FROM generate_series(1,10);",
        )
        .await
        .unwrap();

    let reader = PerfReader::connect(&url).expect("connect perf reader");
    let now_epoch = reader.db_now_epoch().await.expect("db clock");
    let samples = reader.load_samples(30, None).await.expect("load samples");

    let params = RegressionParams {
        recent_days:   7,
        baseline_days: 7,
        min_samples:   5,
        threshold_pct: 25.0,
        min_delta_ms:  5.0,
    };
    let report = regression_scan(&samples, &params, now_epoch);

    assert_eq!(report.findings.len(), 1, "the doubled latency regresses: {report:?}");
    let f = &report.findings[0];
    assert_eq!(f.object_type, "SmokeE2E");
    assert_eq!(f.modification_type, "UPDATE");
    assert!((f.pct_change - 100.0).abs() < 1.0, "≈ +100%: {}", f.pct_change);
}

#[tokio::test]
async fn non_postgres_url_is_rejected() {
    assert!(
        PerfReader::connect("mysql://localhost/db").is_err(),
        "non-postgres URL rejected"
    );
}
