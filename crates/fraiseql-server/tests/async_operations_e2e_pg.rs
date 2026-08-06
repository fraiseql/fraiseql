//! #391 end to end against PostgreSQL: durable submit / status / cancel, with
//! each of P19's six saga-recovery failure modes pinned by its own test.
//!
//! The HTTP tests drive the production mount (`Server::serve_on_listener` with
//! a real pool, HS256 auth, and live workers); the claim/recovery semantics
//! drive `AsyncOperationStore` directly — they are about what the store's
//! conditional UPDATEs allow, and simulating a dead worker needs a hand on
//! `heartbeat_at`. Multi-tenant execution routing rides the same
//! `tenant_dispatch` seam as `/graphql` and MCP (#858's two-tenant e2e pins
//! that seam); here the recorded `tenant_key` is asserted through submission
//! and the single-tenant dispatch path.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: server`
//! suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** owns `_system.async_operations` (`TRUNCATE`d per test) and a
//! `p29_async` schema → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable
#![allow(clippy::future_not_send, clippy::cast_precision_loss)] // Reason: test code — single-runtime futures; small backdate values

use std::{sync::Arc, time::Duration};

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
        MutationOperation, QueryDefinition, TypeDefinition,
    },
};
use fraiseql_server::{
    Server,
    async_operations::AsyncOperationStore,
    server_config::{AsyncOperationsConfig, Hs256Config, ServerConfig},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

const SCHEMA: &str = "p29_async";
const SECRET_ENV: &str = "FRAISEQL_TEST_P29_ASYNC_HS256_SECRET";
const SECRET: &str = "p29-async-secret-0123456789-0123456789";
const ISSUER: &str = "https://async.test.fraiseql";
const AUDIENCE: &str = "async-test";

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<std::collections::HashMap<String, Value>> = adapter
        .execute_raw_query(sql)
        .await
        .unwrap_or_else(|e| panic!("fixture `{sql}`: {e}"));
}

async fn provision(adapter: &PostgresAdapter) {
    let stmts = vec![
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        // This suite is about the operations table, not the change-log; give the
        // outbox a fresh compatible table so mutations write without ambient
        // interference (the #936 discipline: own what you assert on).
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_item (id uuid PRIMARY KEY, label text NOT NULL)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_item AS SELECT id, jsonb_build_object('id', id, 'label', \
             label) AS data FROM {SCHEMA}.tb_item"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_item(p_label text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid; BEGIN \
             n := gen_random_uuid(); \
             INSERT INTO {SCHEMA}.tb_item (id, label) VALUES (n, p_label); \
             v.succeeded := true; v.state_changed := true; v.message := 'created'; \
             v.entity_type := 'AsyncItem'; v.entity_id := n; \
             v.entity := jsonb_build_object('id', n, 'label', p_label); \
             RETURN v; END; $$"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_fail_item(p_label text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             BEGIN RAISE EXCEPTION 'deliberate failure for %', p_label; END; $$"
        ),
    ];
    // #942/#982: the change-log table comes from the ONE shared provisioner
    // (migration-08 contract), one statement per exec call.
    let mut stmts = stmts;
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());

    for stmt in stmts {
        exec(adapter, stmt.as_str()).await;
    }
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut item = TypeDefinition::new("AsyncItem", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(item);

    schema.queries.push(
        QueryDefinition::new("items", "AsyncItem")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_item")),
    );

    for (name, function) in [
        ("createItem", "fn_create_item"),
        ("failItem", "fn_fail_item"),
    ] {
        let mut m = MutationDefinition::new(name, "AsyncItem");
        m.sql_source = Some(format!("{SCHEMA}.{function}"));
        m.operation = MutationOperation::Insert {
            table: "tb_item".to_string(),
        };
        m.arguments = vec![ArgumentDefinition::new("label", FieldType::String)];
        schema.mutations.push(m);
    }
    schema.build_indexes();
    schema
}

fn config() -> ServerConfig {
    ServerConfig {
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        async_operations: Some(AsyncOperationsConfig {
            operations: vec!["createItem".to_string(), "failItem".to_string()],
            workers: 1,
            poll_interval_ms: 100,
            ..AsyncOperationsConfig::default()
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

fn mint_token(sub: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs(),
    )
    .expect("epoch seconds fit i64");
    let claims = json!({
        "sub": sub, "iss": ISSUER, "aud": AUDIENCE, "iat": now, "exp": now + 3600,
    });
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint token")
}

struct Rig {
    url:       String,
    pool:      PgPool,
    store:     AsyncOperationStore,
    client:    reqwest::Client,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

/// Boot the REAL server (auth layer, mounted routes, live workers) on an
/// ephemeral port, with a database pool so `[async_operations]` boots.
async fn boot() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("adapter");
    provision(&adapter).await;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.expect("pool");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = temp_env::async_with_vars(
        [(SECRET_ENV, Some(SECRET))],
        Box::pin(Server::new(
            config(),
            schema(),
            Arc::new(PostgresAdapter::new(&url).await.expect("server adapter")),
            Some(pool.clone()),
        )),
    )
    .await
    .expect("Server::new");

    // Fresh slate for the assertions below (init ran during Server::new).
    sqlx::query("TRUNCATE _system.async_operations")
        .execute(&pool)
        .await
        .expect("truncate");

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
            .expect("server task");
    });
    tokio::time::sleep(Duration::from_millis(80)).await;

    Some(Rig {
        url:       format!("http://127.0.0.1:{port}"),
        pool:      pool.clone(),
        store:     AsyncOperationStore::new(pool),
        client:    reqwest::Client::new(),
        _shutdown: tx,
    })
}

impl Rig {
    async fn submit(
        &self,
        operation: &str,
        query: &str,
        variables: Value,
        token: &str,
        idempotency_key: Option<&str>,
    ) -> (u16, Value) {
        let mut req = self
            .client
            .post(format!("{}/operations/v1/{operation}", self.url))
            .bearer_auth(token)
            .json(&json!({ "query": query, "variables": variables }));
        if let Some(k) = idempotency_key {
            req = req.header("idempotency-key", k);
        }
        let resp = req.send().await.expect("submit");
        let status = resp.status().as_u16();
        (status, resp.json().await.unwrap_or(Value::Null))
    }

    async fn status(&self, op_id: &str, token: &str) -> (u16, Value) {
        let resp = self
            .client
            .get(format!("{}/operations/v1/{op_id}", self.url))
            .bearer_auth(token)
            .send()
            .await
            .expect("status");
        let status = resp.status().as_u16();
        (status, resp.json().await.unwrap_or(Value::Null))
    }

    /// Poll until the operation reaches a terminal state (or time out).
    async fn wait_terminal(&self, op_id: &str, token: &str) -> Value {
        for _ in 0..100 {
            let (code, body) = self.status(op_id, token).await;
            assert_eq!(code, 200, "status must stay readable: {body}");
            let s = body["status"].as_str().unwrap_or_default();
            if s == "succeeded" || s == "failed" || s == "cancelled" {
                return body;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("operation {op_id} never reached a terminal state");
    }

    /// Insert a row directly in a given state (the recovery tests' fixture).
    async fn plant(&self, state: &str, heartbeat_age_secs: i64) -> Uuid {
        let row = sqlx::query(
            "INSERT INTO _system.async_operations
               (submitter, operation, document, security_context, state, max_attempts,
                claim_token, heartbeat_at, attempts)
             VALUES ('planter', 'createItem', 'mutation { createItem { id } }', '{}', $1, 3,
                     gen_random_uuid(), now() - make_interval(secs => $2), 1)
             RETURNING op_id",
        )
        .bind(state)
        .bind(heartbeat_age_secs as f64)
        .fetch_one(&self.pool)
        .await
        .expect("plant row");
        row.get("op_id")
    }

    async fn row_state(&self, op_id: Uuid) -> (String, i32) {
        sqlx::query("SELECT state, attempts FROM _system.async_operations WHERE op_id = $1")
            .bind(op_id)
            .fetch_one(&self.pool)
            .await
            .map(|r| (r.get::<String, _>("state"), r.get::<i32, _>("attempts")))
            .expect("row present")
    }
}

// ── The happy path, through the production mount ─────────────────────────────

#[tokio::test]
async fn submit_executes_through_the_real_pipeline_and_succeeds() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP submit_executes: no DATABASE_URL");
        return;
    };
    let token = mint_token("async-user-1");

    let (code, body) = rig
        .submit(
            "createItem",
            "mutation Create($label: String) { createItem(label: $label) { id label } }",
            json!({ "label": "made-async" }),
            &token,
            None,
        )
        .await;
    assert_eq!(code, 202, "submission returns immediately: {body}");
    assert_eq!(body["status"], "queued");
    let op_id = body["op_id"].as_str().expect("op_id returned").to_string();

    let terminal = rig.wait_terminal(&op_id, &token).await;
    assert_eq!(terminal["status"], "succeeded", "envelope: {terminal}");
    assert_eq!(
        terminal["result"]["data"]["createItem"]["label"], "made-async",
        "the stored result is the real GraphQL envelope"
    );

    // The write REALLY happened — same pipeline as /graphql, against the DB.
    let n: i64 = sqlx::query(&format!(
        "SELECT count(*) AS n FROM {SCHEMA}.tb_item WHERE label = 'made-async'"
    ))
    .fetch_one(&rig.pool)
    .await
    .unwrap()
    .get("n");
    assert_eq!(n, 1, "the mutation executed exactly once");
}

/// P19 mode 4: the same `Idempotency-Key` + body replays the same `op_id`; the
/// operation is stored and executed once.
#[tokio::test]
async fn idempotent_submission_returns_the_same_op_id_once() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP idempotent_submission: no DATABASE_URL");
        return;
    };
    let token = mint_token("async-user-2");
    let q = "mutation Create($label: String) { createItem(label: $label) { id } }";

    let (c1, b1) = rig
        .submit("createItem", q, json!({"label": "idem"}), &token, Some("key-1"))
        .await;
    let (c2, b2) = rig
        .submit("createItem", q, json!({"label": "idem"}), &token, Some("key-1"))
        .await;
    assert_eq!((c1, c2), (202, 202));
    assert_eq!(b1["op_id"], b2["op_id"], "replayed submission returns the SAME op_id");

    let rows: i64 = sqlx::query("SELECT count(*) AS n FROM _system.async_operations")
        .fetch_one(&rig.pool)
        .await
        .unwrap()
        .get("n");
    assert_eq!(rows, 1, "one stored operation, not a duplicate");

    // Same key, DIFFERENT body → conflict, not a second operation.
    let (c3, _) = rig
        .submit("createItem", q, json!({"label": "other"}), &token, Some("key-1"))
        .await;
    assert_eq!(c3, 422, "a reused key with a different body is refused");
}

/// P19 mode 6: status is read from the stored row — flip the row by hand and
/// the API reports exactly that, nothing inferred.
#[tokio::test]
async fn status_reads_the_stored_row() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP status_reads_stored: no DATABASE_URL");
        return;
    };
    let token = mint_token("async-user-3");
    let (_, body) = rig
        .submit(
            "createItem",
            "mutation { createItem(label: \"probe\") { id } }",
            json!({}),
            &token,
            None,
        )
        .await;
    let op_id = body["op_id"].as_str().unwrap().to_string();
    rig.wait_terminal(&op_id, &token).await;

    sqlx::query(
        "UPDATE _system.async_operations SET state = 'failed', error = 'planted-by-test' \
         WHERE op_id = $1::uuid",
    )
    .bind(&op_id)
    .execute(&rig.pool)
    .await
    .unwrap();

    let (_, status) = rig.status(&op_id, &token).await;
    assert_eq!(status["status"], "failed", "the API reports the ROW: {status}");
    assert_eq!(status["error"], "planted-by-test");
}

/// A failing execution records its error terminally (P19 mode 3: results are
/// never discarded) and respects `max_attempts = 1` (no silent retry).
#[tokio::test]
async fn failed_execution_records_the_error() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP failed_execution: no DATABASE_URL");
        return;
    };
    let token = mint_token("async-user-4");
    let (_, body) = rig
        .submit(
            "failItem",
            "mutation { failItem(label: \"boom\") { id } }",
            json!({}),
            &token,
            None,
        )
        .await;
    let op_id = body["op_id"].as_str().unwrap().to_string();

    let terminal = rig.wait_terminal(&op_id, &token).await;
    assert_eq!(terminal["status"], "failed");
    assert!(
        terminal["error"].as_str().unwrap_or_default().contains("deliberate failure"),
        "the execution error is recorded, not discarded: {terminal}"
    );
    assert_eq!(terminal["attempts"], 1, "max_attempts = 1 means exactly one attempt");
}

// ── Recovery semantics, at the store's conditional UPDATEs ───────────────────

/// P19 mode 1: terminal states are never claimable — recovery cannot re-run
/// completed (or failed, or cancelled) work, however stale the row looks.
#[tokio::test]
async fn terminal_states_are_never_reclaimed() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP terminal_never_reclaimed: no DATABASE_URL");
        return;
    };
    for terminal in ["succeeded", "failed", "cancelled"] {
        let op_id = rig.plant(terminal, 10_000).await;
        let claimed = rig.store.claim(10, 1).await.expect("claim");
        assert!(
            claimed.iter().all(|c| c.op.op_id != op_id),
            "a `{terminal}` operation must never be claimed"
        );
        let (state, attempts) = rig.row_state(op_id).await;
        assert_eq!((state.as_str(), attempts), (terminal, 1), "row untouched");
    }
}

/// P19 mode 2: a `running` row is claimable ONLY when its heartbeat is stale —
/// a live long execution is never stolen.
#[tokio::test]
async fn running_is_reclaimable_only_when_stale() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP running_stale_claim: no DATABASE_URL");
        return;
    };
    let fresh = rig.plant("running", 0).await;
    let stale = rig.plant("running", 10_000).await;

    let claimed = rig.store.claim(10, 300).await.expect("claim");
    let ids: Vec<Uuid> = claimed.iter().map(|c| c.op.op_id).collect();
    assert!(!ids.contains(&fresh), "a fresh heartbeat protects the running claim");
    assert!(ids.contains(&stale), "a stale heartbeat makes the row recoverable");
    let (_, attempts) = rig.row_state(stale).await;
    assert_eq!(attempts, 2, "reclaiming counts a real attempt");
}

/// P19 mode 3 (supersession half): after a stale row is reclaimed, the original
/// worker's late completion is a claim-guarded NO-OP — the retry's outcome
/// cannot be clobbered.
#[tokio::test]
async fn late_completion_after_supersession_is_a_noop() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP late_completion: no DATABASE_URL");
        return;
    };
    let op_id = rig.plant("queued", 0).await;

    // First claim (worker A), then backdate its heartbeat and reclaim (worker B).
    let a = rig.store.claim(1, 300).await.expect("claim A");
    let a_token = a.first().expect("claimed").claim_token;
    sqlx::query(
        "UPDATE _system.async_operations SET heartbeat_at = now() - interval '1 hour' \
         WHERE op_id = $1",
    )
    .bind(op_id)
    .execute(&rig.pool)
    .await
    .unwrap();
    let b = rig.store.claim(1, 300).await.expect("claim B");
    let b_token = b.first().expect("reclaimed").claim_token;
    assert_ne!(a_token, b_token);

    // Worker A finishes late: refused. Worker B's outcome stands.
    let late = rig.store.complete(op_id, a_token, &json!({"data": "late"})).await.unwrap();
    assert!(!late, "a superseded claim cannot complete the operation");
    let ok = rig.store.complete(op_id, b_token, &json!({"data": "current"})).await.unwrap();
    assert!(ok, "the live claim completes normally");
    let (state, _) = rig.row_state(op_id).await;
    assert_eq!(state, "succeeded");
}

/// #746 (P19 mode 3, cancellation half): cancelling a `queued` operation is
/// truthful and outright; cancelling a `running` one is a REQUEST and is
/// reported as exactly that — never as an accomplished cancellation.
#[tokio::test]
async fn cancellation_is_reported_truthfully() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP cancellation_truthful: no DATABASE_URL");
        return;
    };
    // Queued → cancelled outright.
    let queued = rig.plant("queued", 0).await;
    assert!(rig.store.cancel_queued(queued, "planter", None).await.unwrap());
    let (state, _) = rig.row_state(queued).await;
    assert_eq!(state, "cancelled");

    // Running → only a request; the state is untouched until the worker's next
    // safe point, and the store refuses to pretend otherwise.
    let running = rig.plant("running", 0).await;
    assert!(
        !rig.store.cancel_queued(running, "planter", None).await.unwrap(),
        "a running operation cannot be cancelled outright"
    );
    assert!(rig.store.request_cancel(running, "planter", None).await.unwrap());
    let row = rig
        .store
        .get_scoped(running, "planter", None)
        .await
        .unwrap()
        .expect("row visible to its submitter");
    assert_eq!(row.state, "running", "still running — the request is not a cancellation");
    assert!(row.cancellation_requested);
}

// ── Scoping and refusals ─────────────────────────────────────────────────────

/// Another principal's `op_id` reads as absent — status is not an existence
/// oracle, and cancel cannot cross submitters.
#[tokio::test]
async fn operations_are_scoped_to_their_submitter() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP submitter_scoping: no DATABASE_URL");
        return;
    };
    let owner = mint_token("owner-user");
    let intruder = mint_token("intruder-user");

    let (_, body) = rig
        .submit(
            "createItem",
            "mutation { createItem(label: \"scoped\") { id } }",
            json!({}),
            &owner,
            None,
        )
        .await;
    let op_id = body["op_id"].as_str().unwrap().to_string();

    let (code, _) = rig.status(&op_id, &intruder).await;
    assert_eq!(code, 404, "another submitter's operation reads as absent");

    let resp = rig
        .client
        .delete(format!("{}/operations/v1/{op_id}", rig.url))
        .bearer_auth(&intruder)
        .send()
        .await
        .expect("delete");
    assert_eq!(resp.status().as_u16(), 404, "and cannot be cancelled cross-submitter");
}

/// The submission gate: anonymous refused; a non-allowlisted operation refused;
/// a root-field/path mismatch refused.
#[tokio::test]
async fn submission_gates_refuse_loudly() {
    let Some(rig) = boot().await else {
        eprintln!("SKIP submission_gates: no DATABASE_URL");
        return;
    };
    let token = mint_token("gate-user");

    let resp = rig
        .client
        .post(format!("{}/operations/v1/createItem", rig.url))
        .json(&json!({ "query": "mutation { createItem(label: \"x\") { id } }" }))
        .send()
        .await
        .expect("anon submit");
    assert_eq!(resp.status().as_u16(), 401, "anonymous submission refused");

    let (code, _) = rig.submit("items", "query { items { id } }", json!({}), &token, None).await;
    assert_eq!(code, 404, "an operation outside the allowlist is refused");

    let (code, _) = rig
        .submit(
            "createItem",
            "mutation { failItem(label: \"x\") { id } }",
            json!({}),
            &token,
            None,
        )
        .await;
    assert_eq!(code, 400, "the document's root field must match the path operation");
}
