//! #390 end to end: every authenticated write path records a correctly derived,
//! unforgeable actor in the change-log — from the Bearer token on the wire to the
//! `core.tb_entity_change_log` row, through the production mount.
//!
//! The executor-level stamping is pinned in `fraiseql-core`
//! (`changelog_outbox_e2e.rs`) with hand-built `SecurityContext`s, and the claim
//! derivation in `security/actor_type/tests.rs`. What only this shape can observe
//! is the full chain the binary actually runs: HS256 validation →
//! `build_security_context` (the one builder every transport calls, #858) →
//! `derive_actor` → mutation runner → in-transaction outbox stamp. It drives both
//! mounted HTTP write transports — `POST /graphql` and the REST write surface —
//! and asserts on the recorded rows, not on any intermediate struct.
//!
//! Forgery coverage: a token cannot influence its classification through claims
//! named after the framework attributes, and an unauthenticated request is
//! refused outright rather than executed unattributed.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: server`
//! suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** recreates its own `p29_actor` schema AND the shared
//! `core.tb_entity_change_log` (this suite is *about* the change-log, so unlike
//! the REST suites it cannot opt out) → run `--test-threads=1`, and never in the
//! same process as another changelog-owning binary.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable
#![allow(clippy::future_not_send)] // Reason: temp_env's env-guard makes boot() non-Send; each test runs it on its own tokio runtime

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
        MutationOperation, QueryDefinition, RestConfig, TypeDefinition,
    },
};
use fraiseql_server::server_config::{Hs256Config, ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

mod common;

use crate::common::server_harness::TestServer;

const SCHEMA: &str = "p29_actor";
const SECRET_ENV: &str = "FRAISEQL_TEST_P29_ACTOR_HS256_SECRET";
const SECRET: &str = "p29-actor-secret-0123456789-0123456789";
const ISSUER: &str = "https://actor.test.fraiseql";
const AUDIENCE: &str = "actor-test";

/// The human subject: UUID-shaped, so a delegated token's `acting_for` resolves.
const HUMAN_SUB: &str = "5a1e0000-0000-4000-8000-000000000390";

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// Fixture: the `app.mutation_response` contract types, a fresh change-log table
/// carrying the actor columns AND the domain CHECK constraint (migration 08's
/// posture — so this suite also proves every runtime-stamped token satisfies the
/// constraint), and one `createItem` mutation function.
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
             v.entity_type := 'ActorItem'; v.entity_id := n; \
             v.entity := jsonb_build_object('id', n, 'label', p_label); \
             RETURN v; END; $$"
        ),
    ];
    // #942/#982: the change-log table comes from the ONE shared provisioner
    // (migration-08 contract), one statement per exec call.
    let mut stmts = stmts;
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// One `createItem` mutation, change-log ON (the default — this suite is about
/// the outbox row), mounted on both `/graphql` and `POST /rest/v1/items`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut item = TypeDefinition::new("ActorItem", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(item);

    schema.queries.push(
        QueryDefinition::new("items", "ActorItem")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_item")),
    );

    let mut create = MutationDefinition::new("createItem", "ActorItem");
    create.sql_source = Some(format!("{SCHEMA}.fn_create_item"));
    create.operation = MutationOperation::Insert {
        table: "tb_item".to_string(),
    };
    create.arguments = vec![ArgumentDefinition::new("label", FieldType::String)];
    schema.mutations.push(create);

    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: true,
        ..RestConfig::default()
    });
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
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

/// Mint an HS256 token whose base claims are valid and whose `extra` claims are
/// merged in — the knob each test case turns.
fn mint(sub: &str, extra: &Value) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs(),
    )
    .expect("epoch seconds fit i64");
    let mut claims = json!({
        "sub": sub,
        "iss": ISSUER,
        "aud": AUDIENCE,
        "iat": now,
        "exp": now + 3600,
    });
    if let (Some(obj), Some(extra_obj)) = (claims.as_object_mut(), extra.as_object()) {
        for (k, v) in extra_obj {
            obj.insert(k.clone(), v.clone());
        }
    }
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint token")
}

struct Rig {
    server:  TestServer,
    adapter: Arc<PostgresAdapter>,
    client:  reqwest::Client,
}

async fn boot() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("adapter"));
    provision(&adapter).await;

    // A second, server-owned adapter: `Server::new` requires exclusive Arc
    // ownership for cache wrapping. The secret is scoped to construction so the
    // process environment stays clean for the rest of the binary (#907).
    let server = Box::pin(temp_env::async_with_vars(
        [(SECRET_ENV, Some(SECRET))],
        TestServer::start_with_rest_writes(
            config(),
            schema(),
            Arc::new(PostgresAdapter::new(&url).await.expect("server adapter")),
        ),
    ))
    .await;
    Some(Rig {
        server,
        adapter,
        client: reqwest::Client::new(),
    })
}

impl Rig {
    /// Run `createItem` over `POST /graphql` with the given Bearer token.
    async fn graphql_create(&self, label: &str, token: Option<&str>) -> (u16, String) {
        let body = json!({
            "query": "mutation Create($label: String) { createItem(label: $label) { id label } }",
            "variables": { "label": label },
        });
        let mut req = self
            .client
            .post(format!("{}/graphql", self.server.url))
            .header("content-type", "application/json")
            .json(&body);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.expect("graphql request");
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        (status, text)
    }

    /// Run the same mutation over the REST write surface.
    async fn rest_create(&self, label: &str, token: &str) -> (u16, String) {
        let resp = self
            .client
            .post(format!("{}/rest/v1/items", self.server.url))
            .bearer_auth(token)
            .json(&json!({ "label": label }))
            .send()
            .await
            .expect("rest request");
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        (status, text)
    }

    /// The `(actor_type, acting_for)` recorded for the change-log row whose
    /// entity payload carries `label` — `None` when no row was written.
    async fn recorded_actor(&self, label: &str) -> Option<(Option<String>, Option<String>)> {
        let rows = self
            .adapter
            .execute_raw_query(&format!(
                "SELECT actor_type, acting_for::text AS acting_for \
                 FROM core.tb_entity_change_log \
                 WHERE object_data->>'label' = '{label}'"
            ))
            .await
            .expect("read change-log");
        rows.first().map(|r| {
            let get = |k: &str| r.get(k).and_then(Value::as_str).map(ToString::to_string);
            (get("actor_type"), get("acting_for"))
        })
    }

    /// Rows among the given labels whose `actor_type` is NULL — the
    /// "unattributed action" this phase forbids on authenticated paths.
    async fn unattributed_rows(&self, labels: &[&str]) -> i64 {
        let quoted: Vec<String> = labels.iter().map(|l| format!("'{l}'")).collect();
        let rows = self
            .adapter
            .execute_raw_query(&format!(
                "SELECT count(*) AS n FROM core.tb_entity_change_log \
                 WHERE object_data->>'label' IN ({}) AND actor_type IS NULL",
                quoted.join(", ")
            ))
            .await
            .expect("scan change-log");
        rows.first()
            .and_then(|r| r.get("n"))
            .and_then(Value::as_i64)
            .or_else(|| {
                // count(*) may come back as a string depending on the JSON path.
                rows.first()
                    .and_then(|r| r.get("n"))
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse().ok())
            })
            .expect("count row")
    }
}

fn assert_mutation_succeeded(status: u16, body: &str) {
    assert_eq!(status, 200, "mutation transport must succeed, body: {body}");
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    assert!(
        v.get("errors").is_none() || v["errors"].as_array().is_some_and(Vec::is_empty),
        "mutation must not error: {body}"
    );
}

/// The headline test: one server, every token shape, both HTTP write
/// transports — and every recorded row is attributed with the derived actor.
#[tokio::test]
async fn every_write_path_records_the_derived_actor() {
    if database_url_or_skip("every_write_path_records_the_derived_actor").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();

    // 1. An ordinary human JWT → human_user, nobody acted-for.
    let (s, b) = rig.graphql_create("gql-human", Some(&mint(HUMAN_SUB, &json!({})))).await;
    assert_mutation_succeeded(s, &b);
    let (actor, acting_for) = rig.recorded_actor("gql-human").await.expect("row written");
    assert_eq!(actor.as_deref(), Some("human_user"));
    assert_eq!(acting_for, None);

    // 2. RFC 8693 delegation: an agent acting for the human subject → ai_agent, acting_for = the
    //    token's sub (the human).
    let agent = mint(HUMAN_SUB, &json!({ "act": { "sub": "agent-runner-1" } }));
    let (s, b) = rig.graphql_create("gql-agent", Some(&agent)).await;
    assert_mutation_succeeded(s, &b);
    let (actor, acting_for) = rig.recorded_actor("gql-agent").await.expect("row written");
    assert_eq!(actor.as_deref(), Some("ai_agent"));
    assert_eq!(acting_for.as_deref(), Some(HUMAN_SUB), "acting_for = the delegated human");

    // 3. A service-account token (OAuth2 `scope` claim) → service_account.
    let svc = mint("svc-batch-1", &json!({ "scope": "service_account" }));
    let (s, b) = rig.graphql_create("gql-svc", Some(&svc)).await;
    assert_mutation_succeeded(s, &b);
    let (actor, _) = rig.recorded_actor("gql-svc").await.expect("row written");
    assert_eq!(actor.as_deref(), Some("service_account"));

    // 4. The REST write surface — same extractor, same derivation, same stamp.
    let (s, b) = rig.rest_create("rest-human", &mint(HUMAN_SUB, &json!({}))).await;
    assert!(s == 200 || s == 201, "REST create must succeed (got {s}): {b}");
    let (actor, _) = rig.recorded_actor("rest-human").await.expect("row written");
    assert_eq!(actor.as_deref(), Some("human_user"), "REST write is attributed too");

    // 5. No write above produced an unattributed row.
    let n = rig
        .unattributed_rows(&["gql-human", "gql-agent", "gql-svc", "rest-human"])
        .await;
    assert_eq!(n, 0, "no authenticated write path may record an unattributed action");
}

/// A client cannot forge its classification: claims named after the framework
/// attributes (`fraiseql.actor_type` / `fraiseql.acting_for`) and a look-alike
/// bare `actor_type` claim are all ignored — the recorded actor is derived from
/// the token's *structure* (no `act`, no `service_account` scope → `human_user`).
#[tokio::test]
async fn forged_actor_claims_are_ignored() {
    if database_url_or_skip("forged_actor_claims_are_ignored").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();

    let forged = mint(
        HUMAN_SUB,
        &json!({
            "fraiseql.actor_type": "system_job",
            "fraiseql.acting_for": "9f9f0000-0000-4000-8000-000000000999",
            "actor_type": "system_job",
        }),
    );
    let (s, b) = rig.graphql_create("gql-forged", Some(&forged)).await;
    assert_mutation_succeeded(s, &b);

    let (actor, acting_for) = rig.recorded_actor("gql-forged").await.expect("row written");
    assert_eq!(
        actor.as_deref(),
        Some("human_user"),
        "forged actor claims must not change the derived classification"
    );
    assert_eq!(acting_for, None, "forged acting_for must not be recorded");
}

/// With authentication configured, an unauthenticated mutation is refused at the
/// door on both transports — never executed as an unattributed action.
#[tokio::test]
async fn unauthenticated_writes_are_refused_not_unattributed() {
    if database_url_or_skip("unauthenticated_writes_are_refused_not_unattributed").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();

    let (s, _) = rig.graphql_create("gql-anon", None).await;
    assert_eq!(s, 401, "unauthenticated /graphql mutation must be refused");

    let resp = rig
        .client
        .post(format!("{}/rest/v1/items", rig.server.url))
        .json(&json!({ "label": "rest-anon" }))
        .send()
        .await
        .expect("rest request");
    assert_eq!(resp.status().as_u16(), 401, "unauthenticated REST write must be refused");

    assert!(
        rig.recorded_actor("gql-anon").await.is_none(),
        "a refused mutation must not reach the change-log"
    );
    assert!(
        rig.recorded_actor("rest-anon").await.is_none(),
        "a refused REST write must not reach the change-log"
    );
}
