//! Live-PostgreSQL integration tests for the #431 inbound webhook pipeline.
//!
//! These prove the pipeline's half of the contract with its handler, which only a
//! real database can: the handler is the delivery's one claim, its `Duplicate` is
//! reported as a duplicate, and its failure rolls its claim back with everything
//! else in the transaction. The handlers below claim the way the server's spine
//! does, with `INSERT … ON CONFLICT DO NOTHING` on the transaction they are given.
//! The signature-verification short-circuit is unit-tested in
//! `src/pipeline/tests.rs` (no DB) and is exercised here once end-to-end to prove a
//! forged delivery writes no row.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite,
//! which binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `webhooks.tb_claim_test` on setup → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable
#![allow(clippy::missing_const_for_fn)] // Reason: const fn not stable for all patterns used (matches the lib crate)

use fraiseql_test_support::try_database_url;
use fraiseql_webhooks::{
    Authenticated, Delivery, Disposition, EventHandler, Handled, InboundRequest, Result,
    SignatureError, SignatureVerifier, StaticSecretProvider, Verified, VerifiedEvent, WebhookError,
    WebhookPipeline,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};

// ── Test verifiers ────────────────────────────────────────────────────────────
// Signature crypto is covered by the per-provider unit/replay tests; these stand
// in for "valid" / "forged" so the pipeline tests stay focused on the claim + tx.

struct AcceptingVerifier;
impl SignatureVerifier for AcceptingVerifier {
    fn name(&self) -> &'static str {
        "accepting"
    }

    fn verify(
        &self,
        _request: &InboundRequest<'_>,
        _secret: &str,
    ) -> std::result::Result<Verified, SignatureError> {
        Ok(Verified::Body)
    }
}

struct RejectingVerifier;
impl SignatureVerifier for RejectingVerifier {
    fn name(&self) -> &'static str {
        "rejecting"
    }

    fn verify(
        &self,
        _request: &InboundRequest<'_>,
        _secret: &str,
    ) -> std::result::Result<Verified, SignatureError> {
        Err(SignatureError::Mismatch)
    }
}

// ── Test handlers ─────────────────────────────────────────────────────────────
// Each claims the delivery in `webhooks.tb_claim_test`, keyed by the verified
// event id, so a test can count how many deliveries recorded the event and
// whether the claim survived.

/// Claim the verified event on `tx`: `true` when this delivery inserted the key.
async fn claim(
    function_name: &str,
    event: &VerifiedEvent,
    tx: &mut Transaction<'_, Postgres>,
) -> Result<bool> {
    let claimed = sqlx::query_scalar::<_, i64>(
        "INSERT INTO webhooks.tb_claim_test (event_id, event_type, function_name, params) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (event_id) DO NOTHING RETURNING pk",
    )
    .bind(&event.id)
    .bind(&event.event_type)
    .bind(function_name)
    .bind(&event.params)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(claimed.is_some())
}

struct ClaimingHandler;
impl EventHandler for ClaimingHandler {
    async fn handle(
        &self,
        function_name: &str,
        event: &VerifiedEvent,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Handled> {
        if claim(function_name, event, tx).await? {
            Ok(Handled::Recorded(json!({ "handled": function_name })))
        } else {
            Ok(Handled::Duplicate)
        }
    }
}

/// Claims the delivery *then* fails — so a test can prove the claim is rolled back
/// with the failure (no "seen but unhandled" row left).
struct FailingHandler;
impl EventHandler for FailingHandler {
    async fn handle(
        &self,
        function_name: &str,
        event: &VerifiedEvent,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Handled> {
        assert!(claim(function_name, event, tx).await?, "precondition: a fresh claim");
        Err(WebhookError::Database("handler boom".to_string()))
    }
}

// ── Harness ───────────────────────────────────────────────────────────────────

/// Connect as the superuser `DATABASE_URL`, create the claim table, and truncate it
/// so each test starts clean. Returns `None` (skip) when unconfigured.
async fn setup() -> Option<PgPool> {
    let url = try_database_url()?;
    let admin = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::raw_sql(
        "CREATE SCHEMA IF NOT EXISTS webhooks;
         CREATE TABLE IF NOT EXISTS webhooks.tb_claim_test (
             pk            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             event_id      TEXT  NOT NULL UNIQUE,
             event_type    TEXT  NOT NULL,
             function_name TEXT  NOT NULL,
             params        JSONB NOT NULL
         );
         TRUNCATE webhooks.tb_claim_test RESTART IDENTITY;",
    )
    .execute(&admin)
    .await
    .unwrap();
    Some(admin)
}

macro_rules! skip_if_no_db {
    () => {
        match setup().await {
            Some(pool) => pool,
            None => {
                eprintln!("skipping #431 inbound pipeline test: DATABASE_URL not set");
                return;
            },
        }
    };
}

/// The headers a delivery arrives with. Borrowed by `InboundRequest`, so it has to
/// outlive it — these doubles read nothing out of it, but the type still does.
static NO_HEADERS: std::sync::LazyLock<std::collections::BTreeMap<String, String>> =
    std::sync::LazyLock::new(std::collections::BTreeMap::new);

fn delivery() -> Delivery<'static> {
    Delivery {
        function_name: "process_payment",
        request:       InboundRequest::new(&NO_HEADERS, br#"{"id":"evt_from_the_body"}"#, None),
    }
}

/// The `event_of` these tests pass: the id and params are the test's, and they
/// reach the handler only through this closure — which is the point of #1321, so
/// the tests exercise the real shape rather than a shortcut around it.
fn event_of(
    event_id: &'static str,
    params: Value,
) -> impl FnOnce(Authenticated<'_>) -> Result<VerifiedEvent> {
    move |_authenticated| {
        Ok(VerifiedEvent {
            id: event_id.to_string(),
            event_type: "payment_intent.succeeded".to_string(),
            params,
        })
    }
}

fn secrets() -> StaticSecretProvider {
    StaticSecretProvider::new().with_secret("stripe", "whsec_test")
}

async fn claimed_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM webhooks.tb_claim_test")
        .fetch_one(pool)
        .await
        .unwrap()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn fresh_delivery_is_processed_and_recorded() {
    let admin = skip_if_no_db!();
    let pipeline = WebhookPipeline::new(admin.clone(), secrets(), ClaimingHandler);

    let outcome = pipeline
        .process(
            &AcceptingVerifier,
            Some("stripe"),
            &delivery(),
            event_of("evt_1", json!({"id": "evt_1"})),
        )
        .await
        .unwrap();

    assert!(matches!(outcome, Disposition::Processed(_)), "a fresh delivery is processed");
    assert_eq!(claimed_count(&admin).await, 1, "the handler's claim committed");
}

/// The handler is given the event `event_of` built and nothing else of the
/// delivery (#1321): the id it claims on is the verified one, never the id the
/// unverified body carries.
#[tokio::test]
async fn the_handler_claims_on_the_verified_event_not_the_request() {
    let admin = skip_if_no_db!();
    let pipeline = WebhookPipeline::new(admin.clone(), secrets(), ClaimingHandler);

    pipeline
        .process(
            &AcceptingVerifier,
            Some("stripe"),
            &delivery(),
            event_of("evt_verified", json!({"n": 1})),
        )
        .await
        .unwrap();

    let (event_id, event_type, params): (String, String, Value) =
        sqlx::query_as("SELECT event_id, event_type, params FROM webhooks.tb_claim_test")
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(event_id, "evt_verified", "the claim key is the verified id, not the body's");
    assert_eq!(event_type, "payment_intent.succeeded");
    assert_eq!(params, json!({"n": 1}));
}

#[tokio::test]
async fn duplicate_delivery_is_reported_duplicate_and_recorded_once() {
    let admin = skip_if_no_db!();
    let pipeline = WebhookPipeline::new(admin.clone(), secrets(), ClaimingHandler);
    let d = delivery();
    let event = || event_of("evt_dup", json!({"id": "evt_dup"}));

    let first = pipeline.process(&AcceptingVerifier, Some("stripe"), &d, event()).await.unwrap();
    let second = pipeline.process(&AcceptingVerifier, Some("stripe"), &d, event()).await.unwrap();

    assert!(matches!(first, Disposition::Processed(_)), "first delivery is processed");
    assert!(
        matches!(second, Disposition::Duplicate),
        "the handler's Duplicate is the delivery's disposition, got: {second:?}",
    );
    assert_eq!(claimed_count(&admin).await, 1, "the duplicate records nothing");
}

#[tokio::test]
async fn concurrent_duplicate_deliveries_process_exactly_once() {
    let admin = skip_if_no_db!();
    let pipeline_a = WebhookPipeline::new(admin.clone(), secrets(), ClaimingHandler);
    let pipeline_b = WebhookPipeline::new(admin.clone(), secrets(), ClaimingHandler);
    let d = delivery();
    let event = || event_of("evt_race", json!({"id": "evt_race"}));

    // Two deliveries of the same event race. They serialise on the unique-key row
    // lock inside the handler's claim: exactly one inserts and commits, the other
    // waits, sees the conflict, and answers Duplicate.
    let (a, b) = tokio::join!(
        pipeline_a.process(&AcceptingVerifier, Some("stripe"), &d, event()),
        pipeline_b.process(&AcceptingVerifier, Some("stripe"), &d, event()),
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
    assert_eq!(duplicate, 1, "the other racer is a duplicate, got a={a:?} b={b:?}");
    assert_eq!(claimed_count(&admin).await, 1, "exactly one claim exists");
}

#[tokio::test]
async fn handler_failure_rolls_back_its_claim_so_retry_reprocesses() {
    let admin = skip_if_no_db!();
    let d = delivery();
    let event = || event_of("evt_retry", json!({"id": "evt_retry"}));

    // First attempt: the handler claims, then fails.
    let failing = WebhookPipeline::new(admin.clone(), secrets(), FailingHandler);
    let err = failing
        .process(&AcceptingVerifier, Some("stripe"), &d, event())
        .await
        .unwrap_err();
    assert!(matches!(err, WebhookError::Database(_)), "handler error surfaces, got: {err:?}");
    assert_eq!(
        claimed_count(&admin).await,
        0,
        "the claim is rolled back with the failure — nothing is recorded as processed",
    );

    // The sender retries, and this time the handler completes.
    let succeeding = WebhookPipeline::new(admin.clone(), secrets(), ClaimingHandler);
    let outcome = succeeding
        .process(&AcceptingVerifier, Some("stripe"), &d, event())
        .await
        .unwrap();
    assert!(
        matches!(outcome, Disposition::Processed(_)),
        "the retry reprocesses the event (it was not lost as 'seen but unhandled')",
    );
    assert_eq!(claimed_count(&admin).await, 1, "the retry commits the claim");
}

#[tokio::test]
async fn forged_signature_writes_no_row() {
    let admin = skip_if_no_db!();
    let pipeline = WebhookPipeline::new(admin.clone(), secrets(), ClaimingHandler);

    let err = pipeline
        .process(
            &RejectingVerifier,
            Some("stripe"),
            &delivery(),
            event_of("evt_forged", json!({})),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, WebhookError::SignatureInvalid(_)),
        "a forged signature is rejected, got: {err:?}",
    );
    assert_eq!(claimed_count(&admin).await, 0, "a rejected delivery never reaches the handler");
}
