//! Live-PostgreSQL integration tests for the #431 inbound webhook pipeline.
//!
//! These prove the parts that only a real database can: the atomic idempotency
//! claim, the single-transaction handoff (claim + handler commit or roll back
//! together), and the deny-by-default RLS posture of the delivery ledger. The
//! signature-verification short-circuit is unit-tested in `src/pipeline/tests.rs`
//! (no DB) and is exercised here once end-to-end to prove a forged delivery writes
//! no row.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite,
//! which binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `webhooks` tables on setup → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable
#![allow(clippy::missing_const_for_fn)] // Reason: const fn not stable for all patterns used (matches the lib crate)

use std::str::FromStr;

use fraiseql_test_support::try_database_url;
use fraiseql_webhooks::{
    Delivery, Disposition, EventHandler, PostgresIdempotencyStore, Result, SignatureError,
    SignatureVerifier, StaticSecretProvider, WebhookError, WebhookPipeline,
};
use serde_json::{Value, json};
use sqlx::{
    PgPool, Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};

const READER_ROLE: &str = "fraiseql_webhooks_rls_reader";
const ROLE_PASSWORD: &str = "webhooks_rls_test_password";

// ── Test verifiers ────────────────────────────────────────────────────────────
// Signature crypto is covered by the per-provider unit/replay tests; these stand
// in for "valid" / "forged" so the pipeline tests stay focused on dedup + tx.

struct AcceptingVerifier;
impl SignatureVerifier for AcceptingVerifier {
    fn name(&self) -> &'static str {
        "accepting"
    }

    fn signature_header(&self) -> &'static str {
        "X-Test-Signature"
    }

    fn verify(
        &self,
        _payload: &[u8],
        _signature: &str,
        _secret: &str,
        _timestamp: Option<&str>,
        _url: Option<&str>,
    ) -> std::result::Result<bool, SignatureError> {
        Ok(true)
    }
}

struct RejectingVerifier;
impl SignatureVerifier for RejectingVerifier {
    fn name(&self) -> &'static str {
        "rejecting"
    }

    fn signature_header(&self) -> &'static str {
        "X-Test-Signature"
    }

    fn verify(
        &self,
        _payload: &[u8],
        _signature: &str,
        _secret: &str,
        _timestamp: Option<&str>,
        _url: Option<&str>,
    ) -> std::result::Result<bool, SignatureError> {
        Ok(false)
    }
}

// ── Test handlers ─────────────────────────────────────────────────────────────
// Each handler records its invocation in `webhooks.tb_handled_test` so a test can
// assert exactly how many times the handler ran (and whether its effects survived).

struct RecordingHandler;
impl EventHandler for RecordingHandler {
    async fn handle(
        &self,
        function_name: &str,
        params: Value,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Value> {
        sqlx::query("INSERT INTO webhooks.tb_handled_test (function_name, params) VALUES ($1, $2)")
            .bind(function_name)
            .bind(&params)
            .execute(&mut **tx)
            .await?;
        Ok(json!({ "handled": function_name }))
    }
}

/// Writes its side effect *then* fails — so a test can prove the effect is rolled
/// back together with the idempotency claim (no "seen but unhandled" row left).
struct FailingHandler;
impl EventHandler for FailingHandler {
    async fn handle(
        &self,
        function_name: &str,
        params: Value,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Value> {
        sqlx::query("INSERT INTO webhooks.tb_handled_test (function_name, params) VALUES ($1, $2)")
            .bind(function_name)
            .bind(&params)
            .execute(&mut **tx)
            .await?;
        Err(WebhookError::Database("handler boom".to_string()))
    }
}

// ── Harness ───────────────────────────────────────────────────────────────────

/// Connect as the superuser `DATABASE_URL`, create the ledger + a side table, and
/// truncate both so each test starts clean. Returns `None` (skip) when unconfigured.
async fn setup() -> Option<(PostgresIdempotencyStore, PgPool)> {
    let url = try_database_url()?;
    let admin = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let store = PostgresIdempotencyStore::new(admin.clone());
    store.init().await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS webhooks.tb_handled_test (
             pk            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             function_name TEXT  NOT NULL,
             params        JSONB NOT NULL
         );",
    )
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query("TRUNCATE webhooks.tb_inbound_delivery, webhooks.tb_handled_test RESTART IDENTITY")
        .execute(&admin)
        .await
        .unwrap();
    Some((store, admin))
}

macro_rules! skip_if_no_db {
    () => {
        match setup().await {
            Some(pair) => pair,
            None => {
                eprintln!("skipping #431 inbound pipeline test: DATABASE_URL not set");
                return;
            },
        }
    };
}

fn delivery(event_id: &str, params: Value) -> Delivery<'_> {
    Delivery {
        provider: "stripe",
        event_id,
        event_type: "payment_intent.succeeded",
        function_name: "process_payment",
        body: b"{}",
        signature: "sig",
        timestamp: None,
        url: None,
        params,
    }
}

fn secrets() -> StaticSecretProvider {
    StaticSecretProvider::new().with_secret("stripe", "whsec_test")
}

async fn delivery_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM webhooks.tb_inbound_delivery")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn handled_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM webhooks.tb_handled_test")
        .fetch_one(pool)
        .await
        .unwrap()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn fresh_delivery_is_processed_and_recorded() {
    let (store, admin) = skip_if_no_db!();
    let pipeline = WebhookPipeline::new(admin.clone(), secrets(), store, RecordingHandler);

    let outcome = pipeline
        .process(&AcceptingVerifier, "stripe", &delivery("evt_1", json!({"id": "evt_1"})))
        .await
        .unwrap();

    assert!(matches!(outcome, Disposition::Processed(_)), "a fresh delivery is processed");
    assert_eq!(delivery_count(&admin).await, 1, "the claim row is committed");
    assert_eq!(handled_count(&admin).await, 1, "the handler ran once and its effect committed");
}

#[tokio::test]
async fn duplicate_delivery_is_discarded_and_handler_runs_once() {
    let (store, admin) = skip_if_no_db!();
    let pipeline = WebhookPipeline::new(admin.clone(), secrets(), store, RecordingHandler);
    let d = delivery("evt_dup", json!({"id": "evt_dup"}));

    let first = pipeline.process(&AcceptingVerifier, "stripe", &d).await.unwrap();
    let second = pipeline.process(&AcceptingVerifier, "stripe", &d).await.unwrap();

    assert!(matches!(first, Disposition::Processed(_)), "first delivery is processed");
    assert!(
        matches!(second, Disposition::Duplicate),
        "a duplicate (provider, event_id) is silently discarded, got: {second:?}",
    );
    assert_eq!(delivery_count(&admin).await, 1, "the duplicate adds no second claim row");
    assert_eq!(
        handled_count(&admin).await,
        1,
        "the handler ran exactly once (not on the duplicate)"
    );
}

#[tokio::test]
async fn concurrent_duplicate_deliveries_process_exactly_once() {
    let (store_a, admin) = skip_if_no_db!();
    let store_b = PostgresIdempotencyStore::new(admin.clone());
    let pipeline_a = WebhookPipeline::new(admin.clone(), secrets(), store_a, RecordingHandler);
    let pipeline_b = WebhookPipeline::new(admin.clone(), secrets(), store_b, RecordingHandler);
    let d = delivery("evt_race", json!({"id": "evt_race"}));

    // Two deliveries of the same (provider, event_id) race. They serialise on the
    // unique-key row lock inside the atomic claim: exactly one inserts and commits,
    // the other waits, sees the conflict, and is discarded.
    let (a, b) = tokio::join!(
        pipeline_a.process(&AcceptingVerifier, "stripe", &d),
        pipeline_b.process(&AcceptingVerifier, "stripe", &d),
    );

    let processed = [&a, &b]
        .iter()
        .filter(|r| matches!(r.as_ref().unwrap(), Disposition::Processed(_)))
        .count();
    let duplicate = [&a, &b]
        .iter()
        .filter(|r| matches!(r.as_ref().unwrap(), Disposition::Duplicate))
        .count();

    assert_eq!(processed, 1, "exactly one racer processes the event, got a={a:?} b={b:?}");
    assert_eq!(duplicate, 1, "the other racer is discarded as a duplicate, got a={a:?} b={b:?}");
    assert_eq!(delivery_count(&admin).await, 1, "exactly one claim row exists");
    assert_eq!(handled_count(&admin).await, 1, "the handler ran exactly once under the race");
}

#[tokio::test]
async fn handler_failure_rolls_back_claim_and_effects_so_retry_reprocesses() {
    let (store, admin) = skip_if_no_db!();
    let d = delivery("evt_retry", json!({"id": "evt_retry"}));

    // First attempt: the handler writes its side effect, then fails.
    let failing = WebhookPipeline::new(admin.clone(), secrets(), store, FailingHandler);
    let err = failing.process(&AcceptingVerifier, "stripe", &d).await.unwrap_err();
    assert!(matches!(err, WebhookError::Database(_)), "handler error surfaces, got: {err:?}");
    assert_eq!(
        delivery_count(&admin).await,
        0,
        "the claim is rolled back with the handler — nothing is recorded as processed",
    );
    assert_eq!(handled_count(&admin).await, 0, "the handler's side effect is rolled back too");

    // The sender retries: a fresh store over the same DB, now with a working handler.
    let store2 = PostgresIdempotencyStore::new(admin.clone());
    let succeeding = WebhookPipeline::new(admin.clone(), secrets(), store2, RecordingHandler);
    let outcome = succeeding.process(&AcceptingVerifier, "stripe", &d).await.unwrap();
    assert!(
        matches!(outcome, Disposition::Processed(_)),
        "the retry reprocesses the event (it was not lost as 'seen but unhandled')",
    );
    assert_eq!(delivery_count(&admin).await, 1, "the retry now commits the claim");
    assert_eq!(handled_count(&admin).await, 1, "the retry runs the handler to completion");
}

#[tokio::test]
async fn forged_signature_writes_no_delivery_row() {
    let (store, admin) = skip_if_no_db!();
    let pipeline = WebhookPipeline::new(admin.clone(), secrets(), store, RecordingHandler);

    let err = pipeline
        .process(&RejectingVerifier, "stripe", &delivery("evt_forged", json!({})))
        .await
        .unwrap_err();

    assert!(
        matches!(err, WebhookError::SignatureInvalid(_)),
        "a forged signature is rejected, got: {err:?}",
    );
    assert_eq!(delivery_count(&admin).await, 0, "a rejected delivery claims no idempotency row");
    assert_eq!(handled_count(&admin).await, 0, "a rejected delivery never reaches the handler");
}

/// The delivery ledger is RLS deny-by-default: a `NOBYPASSRLS` role with `SELECT`
/// granted still reads nothing (no permissive policy), while the owner that runs
/// the pipeline sees every row.
#[tokio::test]
async fn rls_denies_inbound_delivery_ledger_by_default() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #431 RLS test: DATABASE_URL not set");
        return;
    };
    let (store, admin) = setup().await.unwrap();
    let pipeline = WebhookPipeline::new(admin.clone(), secrets(), store, RecordingHandler);
    pipeline
        .process(&AcceptingVerifier, "stripe", &delivery("evt_rls", json!({})))
        .await
        .unwrap();

    // A NOBYPASSRLS reader with SELECT on the ledger (idempotent across runs).
    sqlx::query(&format!(
        "DO $$ BEGIN
             IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{READER_ROLE}') THEN
                 CREATE ROLE {READER_ROLE} LOGIN PASSWORD '{ROLE_PASSWORD}' NOSUPERUSER NOBYPASSRLS;
             END IF;
         END $$"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!(
        "ALTER ROLE {READER_ROLE} NOSUPERUSER NOBYPASSRLS LOGIN PASSWORD '{ROLE_PASSWORD}'"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA webhooks TO {READER_ROLE}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("GRANT SELECT ON webhooks.tb_inbound_delivery TO {READER_ROLE}"))
        .execute(&admin)
        .await
        .unwrap();

    let reader_opts = PgConnectOptions::from_str(&url)
        .unwrap()
        .username(READER_ROLE)
        .password(ROLE_PASSWORD);
    let reader = PgPoolOptions::new().max_connections(2).connect_with(reader_opts).await.unwrap();

    assert_eq!(
        delivery_count(&reader).await,
        0,
        "deny-by-default: a NOBYPASSRLS reader sees zero rows (no permissive policy)",
    );
    assert_eq!(delivery_count(&admin).await, 1, "the owner bypasses RLS and sees the row");
}
