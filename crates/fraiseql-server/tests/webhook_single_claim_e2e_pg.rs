//! An inbound webhook delivery is claimed once: by the spine's
//! `INSERT … ON CONFLICT (source, idempotency_key) DO NOTHING`, inside the delivery's
//! transaction.
//!
//! Each test drives the mounted route, the real `SpineEventHandler` and a real
//! `after:ingest` function plan against PostgreSQL, and counts durable rows: the
//! response is built before the spine is read back, so it cannot tell "deduplicated"
//! from "written twice". What the single claim must keep:
//!
//! * a redelivery is answered `duplicate`, records nothing, and plans no dispatch;
//! * a failure after the claim, inside the same transaction, rolls the claim back, so the sender's
//!   retry is processed;
//! * two concurrent deliveries of one event yield exactly one `processed`, one `duplicate`, one
//!   spine row and one dispatch;
//! * the spine DDL converges on a table an earlier version of it created, so a boot against an
//!   existing database can still emit.
//!
//! The triggers this suite installs act only on rows under its own route, so the
//! spine stays usable by every other suite in the leg.
//!
//! Self-skips when `DATABASE_URL` is unset (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** deletes its own route's spine rows on setup → run
//! `--test-threads=1`.
#![cfg(feature = "inbound")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use fraiseql_functions::{
    FunctionDefinition, FunctionModule, FunctionObserver, InboundMessage, IngestSource,
    RuntimeType, TriggerRegistry,
};
use fraiseql_guard::deployment::Deployment;
use fraiseql_server::{
    config::WebhookRouteConfig,
    inbound::{Emitted, WebhookInboundState, emit_in_tx, webhook_router, webhook_routes_check},
    subsystems::BeforeMutationHooks,
};
use fraiseql_test_support::try_database_url;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use sqlx::{Executor as _, PgPool, postgres::PgPoolOptions};
use tower::ServiceExt as _;

/// This suite's route. Its spine keys are `<len>:<segment>:<id>`, which is what the
/// triggers below and the cleanup in `setup` select on.
const SEGMENT: &str = "single-claim";
const SECRET_ENV: &str = "FRAISEQL_TEST_SINGLE_CLAIM_SECRET";
const SECRET: &str = "whsec_single_claim";

/// The schema holding this suite's fixtures: the poison switch, the trigger
/// functions, and the old-shape spine of the convergence test.
const FIXTURE_SCHEMA: &str = "fixture_single_claim";

fn key_prefix() -> String {
    format!("{}:{SEGMENT}:", SEGMENT.len())
}

fn spine_key(event_id: &str) -> String {
    format!("{}{event_id}", key_prefix())
}

/// `hex(HMAC-SHA256(secret, body))`, the `hmac-sha256` scheme's `X-Signature`.
fn sign(body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// The mounted router, with one `after:ingest` function so every recorded delivery
/// plans exactly one dispatch row.
fn router(pool: PgPool) -> Router {
    let mut routes = HashMap::new();
    routes.insert(
        SEGMENT.to_string(),
        WebhookRouteConfig {
            secret_env: Some(SECRET_ENV.to_string()),
            provider: "hmac-sha256".to_string(),
            ..Default::default()
        },
    );
    let built =
        webhook_routes_check(&routes, |_| Some(SECRET.to_string()), Deployment::Development)
            .expect("a valid route");

    let definitions = vec![FunctionDefinition::new(
        "on_ingest",
        "after:ingest",
        RuntimeType::Wasm,
    )];
    let module = FunctionModule {
        name:        "on_ingest".to_string(),
        source_hash: "test".to_string(),
        bytecode:    bytes::Bytes::new(),
        runtime:     RuntimeType::Wasm,
    };
    let hooks = BeforeMutationHooks::new(
        TriggerRegistry::load_from_definitions(&definitions).unwrap(),
        std::iter::once(("on_ingest".to_string(), module)).collect(),
        Arc::new(FunctionObserver::new()),
    );
    let state = WebhookInboundState::new(pool, &built, |name| {
        (name == SECRET_ENV).then(|| SECRET.to_string())
    })
    .with_hooks(Arc::new(hooks));
    webhook_router(state)
}

/// Connect, provision the spine, install this suite's triggers, and delete its rows.
async fn setup() -> Option<PgPool> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(8).connect(&url).await.unwrap();
    WebhookInboundState::init_spine(&pool).await.unwrap();
    let prefix = key_prefix();
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE SCHEMA IF NOT EXISTS {FIXTURE_SCHEMA};
         CREATE TABLE IF NOT EXISTS {FIXTURE_SCHEMA}.poison (on_key TEXT PRIMARY KEY);
         TRUNCATE {FIXTURE_SCHEMA}.poison;

         -- A failure after the claim, inside the delivery's transaction: the dispatch
         -- row is written after the spine insert, by the same handler.
         CREATE OR REPLACE FUNCTION {FIXTURE_SCHEMA}.fail_poisoned_dispatch() RETURNS trigger
         LANGUAGE plpgsql AS $f$
         BEGIN
             IF EXISTS (
                 SELECT 1 FROM _fraiseql_inbound_message m
                 JOIN {FIXTURE_SCHEMA}.poison p ON p.on_key = m.idempotency_key
                 WHERE m.pk_inbound_message = NEW.fk_inbound_message
             ) THEN
                 RAISE EXCEPTION 'poisoned dispatch';
             END IF;
             RETURN NEW;
         END $f$;
         DROP TRIGGER IF EXISTS single_claim_poison ON _fraiseql_inbound_dispatch;
         CREATE TRIGGER single_claim_poison BEFORE INSERT ON _fraiseql_inbound_dispatch
             FOR EACH ROW EXECUTE FUNCTION {FIXTURE_SCHEMA}.fail_poisoned_dispatch();

         -- Hold a freshly inserted `race-` key uncommitted for a while, so the other
         -- delivery of the pair reaches its claim while this one still owns the key.
         CREATE OR REPLACE FUNCTION {FIXTURE_SCHEMA}.hold_race_claim() RETURNS trigger
         LANGUAGE plpgsql AS $f$
         BEGIN
             IF starts_with(NEW.idempotency_key, '{prefix}race-') THEN
                 PERFORM pg_sleep(0.5);
             END IF;
             RETURN NULL;
         END $f$;
         DROP TRIGGER IF EXISTS single_claim_hold ON _fraiseql_inbound_message;
         CREATE TRIGGER single_claim_hold AFTER INSERT ON _fraiseql_inbound_message
             FOR EACH ROW EXECUTE FUNCTION {FIXTURE_SCHEMA}.hold_race_claim();"
    )))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM _fraiseql_inbound_message WHERE starts_with(idempotency_key, $1)")
        .bind(&prefix)
        .execute(&pool)
        .await
        .unwrap();
    Some(pool)
}

macro_rules! skip_if_no_db {
    () => {
        match setup().await {
            Some(pool) => pool,
            None => {
                eprintln!("skipping single-claim webhook test: DATABASE_URL not set");
                return;
            },
        }
    };
}

/// POST one signed delivery of `event_id`, returning the HTTP status and body.
async fn post(router: &Router, event_id: &str) -> (StatusCode, String) {
    let body = format!(r#"{{"id":"{event_id}","type":"order.created"}}"#);
    let request = Request::builder()
        .method("POST")
        .uri(format!("/webhooks/{SEGMENT}"))
        .header("X-Signature", sign(body.as_bytes()))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// The `status` field of a 200 response, or the HTTP status of anything else.
fn disposition((status, body): &(StatusCode, String)) -> String {
    if *status != StatusCode::OK {
        return format!("http {status}");
    }
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|json| json.get("status").and_then(|s| s.as_str()).map(str::to_string))
        .unwrap_or_else(|| format!("unreadable: {body}"))
}

/// The spine rows for one event, with their stored event types.
async fn claims(pool: &PgPool, event_id: &str) -> Vec<Option<String>> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT event_type FROM _fraiseql_inbound_message WHERE idempotency_key = $1",
    )
    .bind(spine_key(event_id))
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The dispatch rows planned for one event.
async fn dispatches(pool: &PgPool, event_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM _fraiseql_inbound_dispatch d \
         JOIN _fraiseql_inbound_message m ON m.pk_inbound_message = d.fk_inbound_message \
         WHERE m.idempotency_key = $1",
    )
    .bind(spine_key(event_id))
    .fetch_one(pool)
    .await
    .unwrap()
}

fn unique(label: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{label}-{nanos}")
}

#[tokio::test]
async fn a_redelivery_is_answered_duplicate_and_records_nothing() {
    let pool = skip_if_no_db!();
    let router = router(pool.clone());
    let id = unique("dup");

    let first = post(&router, &id).await;
    let second = post(&router, &id).await;

    assert_eq!(disposition(&first), "processed", "{first:?}");
    assert_eq!(
        disposition(&second),
        "duplicate",
        "a redelivery of a committed event is a duplicate: {second:?}"
    );
    assert_eq!(
        claims(&pool, &id).await,
        [Some("order.created".to_string())],
        "one spine row, carrying the verified event type"
    );
    assert_eq!(
        dispatches(&pool, &id).await,
        1,
        "after:ingest is planned once, not per delivery"
    );
}

#[tokio::test]
async fn a_failure_after_the_claim_rolls_it_back_and_the_retry_is_processed() {
    let pool = skip_if_no_db!();
    let router = router(pool.clone());
    let id = unique("rollback");

    // Reason: test SQL built from names and values this test defines.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {FIXTURE_SCHEMA}.poison (on_key) VALUES ($1)"
    )))
    .bind(spine_key(&id))
    .execute(&pool)
    .await
    .unwrap();

    let failed = post(&router, &id).await;
    assert!(
        failed.0.is_server_error(),
        "the dispatch write after the claim failed, so the delivery fails: {failed:?}"
    );
    assert_eq!(
        claims(&pool, &id).await,
        Vec::<Option<String>>::new(),
        "the claim rolled back with the failure: a claim that survived would answer the \
         sender's retry `duplicate`, and the event would never be dispatched"
    );

    // Reason: test SQL built from names and values this test defines.
    sqlx::query(sqlx::AssertSqlSafe(format!("TRUNCATE {FIXTURE_SCHEMA}.poison")))
        .execute(&pool)
        .await
        .unwrap();

    let retried = post(&router, &id).await;
    assert_eq!(
        disposition(&retried),
        "processed",
        "the sender's retry is processed: {retried:?}"
    );
    assert_eq!(claims(&pool, &id).await.len(), 1, "and claimed once");
    assert_eq!(dispatches(&pool, &id).await, 1, "with its dispatch");
}

#[tokio::test]
async fn a_concurrent_pair_of_one_delivery_yields_exactly_one_emit() {
    let pool = skip_if_no_db!();
    let router = router(pool.clone());
    let id = unique("race");

    // The `hold` trigger keeps the first insert uncommitted for half a second, so
    // the second delivery reaches its claim while the key is still in flight.
    let (a, b) = tokio::join!(post(&router, &id), post(&router, &id));

    let mut outcomes = [disposition(&a), disposition(&b)];
    outcomes.sort();
    assert_eq!(
        outcomes,
        ["duplicate".to_string(), "processed".to_string()],
        "exactly one of two concurrent deliveries is processed and the other is a \
         duplicate; got a={a:?} b={b:?}"
    );
    assert_eq!(claims(&pool, &id).await.len(), 1, "one spine row");
    assert_eq!(dispatches(&pool, &id).await, 1, "one dispatch");
}

/// The spine table as the DDL created it before `event_type` existed.
const SPINE_WITHOUT_EVENT_TYPE: &str = "\
CREATE TABLE _fraiseql_inbound_message (
    pk_inbound_message BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id                 UUID        NOT NULL DEFAULT gen_random_uuid(),
    source             TEXT        NOT NULL,
    idempotency_key    TEXT        NOT NULL,
    thread_key         TEXT,
    payload            JSONB       NOT NULL,
    received_at        TIMESTAMPTZ NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source, idempotency_key)
);";

/// Boot provisions the spine with `CREATE … IF NOT EXISTS`, which leaves a table an
/// earlier version created as it was. The DDL must still converge on it, or every
/// emit fails with "column `event_type` does not exist" until the database is reset.
#[tokio::test]
async fn the_spine_ddl_converges_on_a_table_created_without_event_type() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping single-claim webhook test: DATABASE_URL not set");
        return;
    };
    let schema = "fixture_single_claim_old_spine";
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                // Reason: test SQL built from names and values this test defines.
                conn.execute(sqlx::AssertSqlSafe(format!("SET search_path = {schema}")))
                    .await
                    .map(|_| ())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema};"
    )))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(SPINE_WITHOUT_EVENT_TYPE).execute(&pool).await.unwrap();

    for _ in 0..2 {
        sqlx::raw_sql(fraiseql_functions::migrations::inbound_migration_sql())
            .execute(&pool)
            .await
            .expect("the spine DDL runs on every boot, over whatever an earlier one created");
    }

    let message = InboundMessage::new(
        IngestSource::Webhook {
            provider: "hmac-sha256".to_string(),
        },
        "converged",
        fraiseql_time::Timestamp::parse_rfc3339("2026-11-25T12:00:00Z").unwrap(),
    );
    let mut tx = pool.begin().await.unwrap();
    let emitted = emit_in_tx(&mut tx, &message, Some("order.created")).await;
    tx.commit().await.unwrap();
    assert!(
        matches!(emitted, Ok(Emitted::New(_))),
        "an emit after the DDL converged must record the message: {emitted:?}"
    );

    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&pool)
        .await
        .unwrap();
}
