//! #865 regression: the REST **write** surface must be mounted by the server the
//! binary actually serves, authenticated, and described by an `OpenAPI` document that
//! matches it.
//!
//! `rest_router` — the router carrying POST/PUT/PATCH/DELETE and the collection-level
//! bulk routes — had **no production caller**. The only mount was `rest_query_router`,
//! read-only. Meanwhile the served `OpenAPI` document was generated from the full route
//! table regardless of which router was built, so it advertised every write path the
//! server answered with `405`. That is a published contract the server never honoured,
//! and it is a regression of the already-closed #227.
//!
//! **Why this suite drives `Server::serve_on_listener` rather than `rest_router`.**
//! Every pre-existing REST test calls `rest_router(&state, …)` directly. A defect whose
//! entire content is "nothing calls this function" is invisible to a test that calls the
//! function — which is exactly how #865 and #812 each survived two releases. The
//! assertions here are only meaningful against the real mount, so that is what they use.
//!
//! Four properties, each of which was false before this phase:
//!
//! 1. A write reaches its handler and **changes the row** — not merely "is not 405".
//! 2. The read-only mount still answers `405` for writes, so property 1 is caused by the mount and
//!    not by some unconditional fallback.
//! 3. The mounted write surface is **authenticated**. `route_layer` does not survive
//!    `Router::merge` (#812), so a write surface merged without passing through
//!    `Server::attach_auth` would accept anonymous mutations. This is the single thing mounting the
//!    surface must not get wrong.
//! 4. The served document advertises **exactly** what the router answers, in both postures. #918
//!    was one instance of that drift; deriving the document from the registration loop is what
//!    removes the class.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p13_mount` schema → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)]
// Reason: test code
// Reason: `temp_env::async_with_vars` holds a non-Send guard across the await that
// constructs the server. These tests run on a current-thread runtime, so `Send` is not
// required — the same allow the wiremock and MinIO integration suites carry.
#![allow(clippy::future_not_send)]

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

const SCHEMA: &str = "p13_mount";
const BASE: &str = "/rest/v1";

/// 32-byte HS256 secret — meets the minimum key-length requirement.
const SECRET: &str = "fraiseql-p13-secret-exactly-32by";
const SECRET_ENV: &str = "FRAISEQL_P13_MOUNT_HS256_SECRET";
const ISSUER: &str = "https://p13.fraiseql.test";
const AUDIENCE: &str = "fraiseql-p13-api";

/// Rows seeded before each test, so a created row is distinguishable by count.
const SEEDED: usize = 2;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

async fn seed(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        // The `app.mutation_response` contract. Provisioned here, idempotently, rather than
        // assumed: the shared seed (`tests/sql/postgres/init.sql`, mounted by both the
        // Dagger service and the local compose rig) does not create it. Any suite relying
        // on it being present was relying on a *previous* suite in the same database having
        // created it — which is green until the run order changes or the volume is reset,
        // and is not a property CI can depend on. `make db-reset` reproduces the failure
        // exactly (SQLSTATE 3F000, invalid_schema_name).
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$"
            .to_string(),
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_item (id uuid PRIMARY KEY, label text NOT NULL, status \
             text NOT NULL DEFAULT 'active')"
        ),
    ];

    for n in 0..SEEDED {
        stmts.push(format!(
            "INSERT INTO {SCHEMA}.tb_item VALUES ('{}', 'seeded-{n}', 'active')",
            uuid_for(n)
        ));
    }

    stmts.push(format!(
        "CREATE VIEW {SCHEMA}.v_item AS SELECT id, jsonb_build_object('id', id, 'label', label, \
         'status', status) AS data FROM {SCHEMA}.tb_item ORDER BY label"
    ));
    stmts.push(format!(
        "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_create_item(p_label text) \
         RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
         DECLARE v app.mutation_response; n uuid; BEGIN \
         n := gen_random_uuid(); \
         INSERT INTO {SCHEMA}.tb_item (id, label) VALUES (n, p_label); \
         v.succeeded := true; v.state_changed := true; v.message := 'created'; \
         v.entity_type := 'P13MountItem'; v.entity_id := n; \
         v.entity := jsonb_build_object('id', n, 'label', p_label); \
         RETURN v; END; $$"
    ));
    stmts.push(format!(
        "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_update_item(p_id uuid, p_label text, p_status \
         text) RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
         DECLARE v app.mutation_response; BEGIN \
         UPDATE {SCHEMA}.tb_item SET label = p_label, status = p_status WHERE id = p_id; \
         v.succeeded := true; v.state_changed := true; v.message := 'updated'; \
         v.entity_type := 'P13MountItem'; v.entity_id := p_id; \
         v.entity := jsonb_build_object('id', p_id, 'label', p_label); \
         RETURN v; END; $$"
    ));
    stmts.push(format!(
        "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_delete_item(p_id uuid) \
         RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
         DECLARE v app.mutation_response; BEGIN \
         DELETE FROM {SCHEMA}.tb_item WHERE id = p_id; \
         v.succeeded := true; v.state_changed := true; v.message := 'deleted'; \
         v.entity_type := 'P13MountItem'; v.entity_id := p_id; \
         v.entity := jsonb_build_object('id', p_id); \
         RETURN v; END; $$"
    ));

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

fn uuid_for(n: usize) -> String {
    format!("00000000-0000-0000-0000-{n:012}")
}

/// A schema whose REST derivation yields at least one route per write method.
/// Opt every fixture mutation out of the change-log outbox.
///
/// These suites are about REST write semantics, not the change spine. Left on (the
/// default), each mutation INSERTs into `core.tb_entity_change_log` — a table neither
/// database seed creates, and which *other* suites in this crate create with differing
/// column sets. That made the result depend on which binary had run first: green alone,
/// `column "updated_fields" does not exist` after a full-crate run, `relation ... does
/// not exist` after `make db-reset`. Dropping the dependency is what makes these suites
/// order-independent, which is the only form in which they can be a CI gate.
const fn without_changelog(mut m: MutationDefinition) -> MutationDefinition {
    m.changelog = false;
    m
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut item = TypeDefinition::new("P13MountItem", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
        FieldDefinition::new("status", FieldType::String),
    ];
    schema.types.push(item);

    let mut items = QueryDefinition::new("items", "P13MountItem")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_item"));
    items.auto_params.has_where = true;
    items.auto_params.has_limit = true;
    schema.queries.push(items);

    // A by-id query, so derivation yields `GET /items/{id}`. Without it the `Location`
    // a 201 advertises would point at a path the router does not serve — and asserting
    // that Location *resolves* is the only version of that test worth having.
    let mut item =
        QueryDefinition::new("item", "P13MountItem").with_sql_source(format!("{SCHEMA}.v_item"));
    item.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    schema.queries.push(item);

    // Insert → POST /items (201).
    let mut create = MutationDefinition::new("createItem", "P13MountItem");
    create.sql_source = Some(format!("{SCHEMA}.fn_create_item"));
    create.operation = MutationOperation::Insert {
        table: "tb_item".to_string(),
    };
    create.arguments = vec![ArgumentDefinition::new("label", FieldType::String)];
    schema.mutations.push(without_changelog(create));

    // Update covering every writable field → PUT and PATCH /items/{id} (200).
    let mut update = MutationDefinition::new("updateItem", "P13MountItem");
    update.sql_source = Some(format!("{SCHEMA}.fn_update_item"));
    update.operation = MutationOperation::Update {
        table: "tb_item".to_string(),
    };
    update.arguments = vec![
        ArgumentDefinition::new("id", FieldType::String),
        ArgumentDefinition::new("label", FieldType::String),
        ArgumentDefinition::new("status", FieldType::String),
    ];
    schema.mutations.push(without_changelog(update));

    // Delete → DELETE /items/{id}.
    let mut delete = MutationDefinition::new("deleteItem", "P13MountItem");
    delete.sql_source = Some(format!("{SCHEMA}.fn_delete_item"));
    delete.operation = MutationOperation::Delete {
        table: "tb_item".to_string(),
    };
    delete.arguments = vec![ArgumentDefinition::new("id", FieldType::String)];
    schema.mutations.push(without_changelog(delete));

    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

fn auth_config() -> ServerConfig {
    ServerConfig {
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

fn token() -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs();

    let claims = json!({
        "sub": "p13-mount-user",
        "iss": ISSUER,
        "aud": AUDIENCE,
        "iat": now,
        "exp": now + 3600,
    });

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("encode token")
}

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

struct Rig {
    server:  TestServer,
    adapter: Arc<PostgresAdapter>,
    client:  reqwest::Client,
}

impl Rig {
    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (reqwest::StatusCode, String) {
        let mut req = self.client.request(method, format!("{}{path}", self.server.url));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let response = req.send().await.expect("request");
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        (status, text)
    }

    /// Send a request and return only its status, **without reading the body**.
    ///
    /// The parity probe below hits every advertised operation, and one of them is the
    /// SSE stream endpoint: `reqwest` returns as soon as the response head arrives, but
    /// reading the body of an event stream blocks until the server closes it, which for
    /// a heartbeat stream is never. Reading it is what made the first version of that
    /// test hang instead of fail.
    async fn probe_status(
        &self,
        method: reqwest::Method,
        path: &str,
        token: &str,
    ) -> reqwest::StatusCode {
        self.client
            .request(method, format!("{}{path}", self.server.url))
            .bearer_auth(token)
            .json(&json!({}))
            .send()
            .await
            .expect("request")
            .status()
    }

    /// Labels currently in the table, read straight from it. Every write assertion
    /// goes through here rather than through the response body: a fabricated success
    /// is precisely what a response-body assertion cannot see.
    async fn labels(&self) -> Vec<String> {
        let rows: Vec<std::collections::HashMap<String, Value>> = self
            .adapter
            .execute_raw_query(&format!("SELECT label FROM {SCHEMA}.tb_item ORDER BY label"))
            .await
            .expect("read back");
        rows.iter()
            .filter_map(|r| r.get("label").and_then(Value::as_str).map(ToString::to_string))
            .collect()
    }
}

/// An HTTP client with a hard per-request deadline.
///
/// A safety net, not a nicety: the parity probe walks every advertised operation, and a
/// request that never completes would hang the whole binary rather than fail it. A
/// timeout turns that class of mistake back into a test failure.
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("build client")
}

/// A server with the write surface mounted, exactly as the binary's PostgreSQL boot
/// path builds it.
async fn rig_with_writes() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    // The secret is read once, by `Hs256Config::load_secret` during `Server::new`, so
    // the variable only needs to exist for the construction. Scoping it keeps the
    // process environment clean for the rest of the binary — #907 is a live example of
    // what a leaked `FRAISEQL_*` variable does to an unrelated suite.
    let server = Box::pin(temp_env::async_with_vars(
        [(SECRET_ENV, Some(SECRET))],
        TestServer::start_with_rest_writes(
            auth_config(),
            schema(),
            Arc::new(PostgresAdapter::new(&url).await.expect("connect")),
        ),
    ))
    .await;

    Some(Rig {
        server,
        adapter,
        client: client(),
    })
}

/// A server built the read-only way — the posture a `SqliteAdapter` deployment gets.
async fn rig_read_only() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let server = Box::pin(temp_env::async_with_vars(
        [(SECRET_ENV, Some(SECRET))],
        TestServer::start_with_config(
            auth_config(),
            schema(),
            Arc::new(PostgresAdapter::new(&url).await.expect("connect")),
        ),
    ))
    .await;

    Some(Rig {
        server,
        adapter,
        client: client(),
    })
}

// ---------------------------------------------------------------------------
// #865 — the surface is mounted, and it writes
// ---------------------------------------------------------------------------

/// The headline regression. `POST /rest/v1/items` through the production mount must
/// create a row.
///
/// The assertion is on the **table**, not on the status code alone: "not 405" would be
/// satisfied by a 500, and a reported `affected_rows` is exactly the number a
/// fabricated success gets right.
#[tokio::test]
async fn a_post_through_the_production_mount_creates_a_row() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let before = rig.labels().await;
    assert_eq!(before.len(), SEEDED, "fixture precondition");

    let (status, body) = rig
        .request(
            reqwest::Method::POST,
            &format!("{BASE}/items"),
            Some(&token()),
            Some(json!({"label": "created-by-rest"})),
        )
        .await;

    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "POST must reach the create handler and report 201; body: {body}"
    );

    let after = rig.labels().await;
    assert_eq!(
        after.len(),
        SEEDED + 1,
        "the row must actually exist after a 201; labels: {after:?}"
    );
    assert!(
        after.contains(&"created-by-rest".to_string()),
        "the created row must carry the posted label; labels: {after:?}"
    );
}

/// The control for the test above: without `with_rest_write_surface`, the same request
/// must be `405`.
///
/// Without this, a mount that unconditionally served writes — ignoring the builder
/// entirely — would look identical. It also pins the posture read-only adapters get.
#[tokio::test]
async fn the_read_only_mount_still_refuses_writes() {
    let Some(rig) = rig_read_only().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let (status, _) = rig
        .request(
            reqwest::Method::POST,
            &format!("{BASE}/items"),
            Some(&token()),
            Some(json!({"label": "must-not-be-created"})),
        )
        .await;

    assert_eq!(
        status,
        reqwest::StatusCode::METHOD_NOT_ALLOWED,
        "a read-only mount must answer 405, not silently accept the write"
    );
    assert_eq!(rig.labels().await.len(), SEEDED, "no row may be created on a read-only mount");
}

/// #812 applied to the write half: an anonymous `POST` must be rejected, and must not
/// write.
///
/// `route_layer` does not survive `Router::merge`, so a write router merged without
/// passing through `Server::attach_auth` would reach its handler with no principal.
/// Mounting writes onto an unauthenticated transport is the one thing this phase must
/// not do, so it is asserted against the table as well as the status.
///
/// ⚠ The 401 alone does **not** prove the write route exists: the read-only router also
/// registers `/items` (for `GET`), so its auth layer matches the path and answers 401
/// before axum ever reports `405`. Verified — with the mount neutered, the anonymous
/// half of this test still passed. The authenticated request at the end is what ties
/// the refusal to a route that is actually there, so this test cannot be satisfied by a
/// server with no write surface at all.
#[tokio::test]
async fn an_anonymous_write_is_rejected_and_changes_nothing() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig
        .request(
            reqwest::Method::POST,
            &format!("{BASE}/items"),
            None,
            Some(json!({"label": "anonymous-write"})),
        )
        .await;

    assert_eq!(
        status,
        reqwest::StatusCode::UNAUTHORIZED,
        "an anonymous write must be refused with 401; body: {body}"
    );

    let after = rig.labels().await;
    assert_eq!(after.len(), SEEDED, "an anonymous write must not reach the database");
    assert!(
        !after.contains(&"anonymous-write".to_string()),
        "the anonymous row must not exist; labels: {after:?}"
    );

    // The same request, authenticated, must succeed — otherwise the 401 above could be
    // a route that does not exist rather than a credential that was demanded.
    let (status, body) = rig
        .request(
            reqwest::Method::POST,
            &format!("{BASE}/items"),
            Some(&token()),
            Some(json!({"label": "authenticated-write"})),
        )
        .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "the authenticated equivalent must reach the handler; body: {body}"
    );
    assert!(
        rig.labels().await.contains(&"authenticated-write".to_string()),
        "the authenticated write must land in the table"
    );
}

/// A `DELETE` through the production mount must remove the row it names — and only it.
#[tokio::test]
async fn a_delete_through_the_production_mount_removes_exactly_one_row() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig
        .request(
            reqwest::Method::DELETE,
            &format!("{BASE}/items/{}", uuid_for(0)),
            Some(&token()),
            None,
        )
        .await;

    assert!(status.is_success(), "DELETE must reach its handler; got {status}, body: {body}");

    let after = rig.labels().await;
    assert_eq!(after, vec!["seeded-1".to_string()], "exactly the named row must be gone");
}

// ---------------------------------------------------------------------------
// The served document must describe the mounted router
// ---------------------------------------------------------------------------

/// Fetch the served `OpenAPI` document and return every `(path, method)` it advertises,
/// excluding the `/openapi.json` self-reference.
async fn advertised_operations(rig: &Rig) -> Vec<(String, String)> {
    let (status, body) = rig
        .request(reqwest::Method::GET, &format!("{BASE}/openapi.json"), Some(&token()), None)
        .await;
    assert_eq!(status, reqwest::StatusCode::OK, "openapi.json must be served; body: {body}");

    let doc: Value = serde_json::from_str(&body).expect("openapi.json must be JSON");
    let paths = doc.get("paths").and_then(Value::as_object).expect("document must carry paths");

    let mut out = Vec::new();
    for (path, item) in paths {
        if path == "/openapi.json" {
            continue;
        }
        for method in item.as_object().into_iter().flatten().map(|(m, _)| m) {
            out.push((path.clone(), method.clone()));
        }
    }
    out.sort();
    out
}

/// Turn an `OpenAPI` templated path into a concrete request path.
fn concrete(path: &str) -> String {
    let mut out = String::new();
    let mut in_param = false;
    for ch in path.chars() {
        match ch {
            '{' => in_param = true,
            '}' => {
                in_param = false;
                out.push_str(&uuid_for(0));
            },
            c if !in_param => out.push(c),
            _ => {},
        }
    }
    out
}

/// Every operation the document advertises must be answered by the router.
///
/// #918 was one instance of this drift: an item-level `PATCH /items/{id}/rename`
/// suppressed the collection-level bulk PATCH route while `add_bulk_operations`
/// advertised it regardless, so the document promised a method the router answered with
/// `405`. Asserting the whole document against the whole router removes the class
/// rather than that instance.
#[tokio::test]
async fn every_advertised_operation_is_answered_by_the_mounted_router() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let advertised = advertised_operations(&rig).await;
    assert!(!advertised.is_empty(), "the document must advertise something to be meaningful");

    let auth = token();
    let mut unanswered = Vec::new();
    for (path, method) in &advertised {
        let verb = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
            .expect("advertised method must be a valid HTTP verb");
        let status = rig.probe_status(verb, &format!("{BASE}{}", concrete(path)), &auth).await;
        if status == reqwest::StatusCode::NOT_FOUND
            || status == reqwest::StatusCode::METHOD_NOT_ALLOWED
        {
            unanswered.push(format!("{method} {path} -> {status}"));
        }
    }

    assert!(
        unanswered.is_empty(),
        "the served document advertises operations the router does not answer: {unanswered:?}"
    );
}

/// The converse, and the half #865 named: a read-only mount must not advertise writes.
///
/// The document was generated from the full route table regardless of which router was
/// built, so the read-only deployment published a complete write API it answered with
/// `405` on every path.
#[tokio::test]
async fn the_read_only_mount_advertises_no_write_operations() {
    let Some(rig) = rig_read_only().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let advertised = advertised_operations(&rig).await;
    assert!(
        advertised.iter().any(|(_, m)| m == "get"),
        "a read-only document must still advertise its reads; got {advertised:?}"
    );

    let writes: Vec<_> = advertised
        .iter()
        .filter(|(_, m)| matches!(m.as_str(), "post" | "put" | "patch" | "delete"))
        .collect();
    assert!(
        writes.is_empty(),
        "a read-only mount must not advertise write operations it answers with 405: {writes:?}"
    );
}

/// #846, server half: a `rest_path` override must produce the route it names, through
/// the real mount.
///
/// The CLI half — that an authored `rest` block reaches `QueryDefinition::rest_path` at
/// all — is pinned in `fraiseql-cli/tests/rest_annotation_round_trip_test.rs`. Both
/// halves were needed: the compiler dropped the annotation, and no test had ever driven
/// an override through a served router, only through the route table. `detect_conflicts`
/// tells operators to "use `rest_path` override to resolve" a collision, so the whole
/// path from annotation to answered request has to work.
#[tokio::test]
async fn an_authored_rest_path_serves_at_the_path_it_names() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let mut overridden = schema();
    overridden
        .queries
        .iter_mut()
        .find(|q| q.name == "items")
        .expect("items query")
        .rest_path = Some("/api/v1/orders".to_string());
    overridden.build_indexes();

    let server = Box::pin(temp_env::async_with_vars(
        [(SECRET_ENV, Some(SECRET))],
        TestServer::start_with_rest_writes(
            auth_config(),
            overridden,
            Arc::new(PostgresAdapter::new(&url).await.expect("connect")),
        ),
    ))
    .await;
    let rig = Rig {
        server,
        adapter,
        client: client(),
    };

    let (status, body) = rig
        .request(reqwest::Method::GET, &format!("{BASE}/api/v1/orders"), Some(&token()), None)
        .await;
    assert_eq!(status, reqwest::StatusCode::OK, "the overridden path must serve; body: {body}");

    // And the derived path it replaced must be gone — an override that merely *adds* a
    // route would leave the collision `detect_conflicts` sends operators here to resolve.
    let derived = rig.probe_status(reqwest::Method::GET, &format!("{BASE}/items"), &token()).await;
    assert_eq!(
        derived,
        reqwest::StatusCode::METHOD_NOT_ALLOWED,
        "the derived path must be replaced by the override, not supplemented by it"
    );
}

/// And the write mount must advertise its writes — so the test above cannot be satisfied
/// by a document that lost its write operations everywhere.
#[tokio::test]
async fn the_write_mount_advertises_its_write_operations() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let advertised = advertised_operations(&rig).await;
    for verb in ["post", "put", "patch", "delete"] {
        assert!(
            advertised.iter().any(|(_, m)| m == verb),
            "the write mount must advertise {verb}; got {advertised:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// #873 — the low-severity checklist, asserted on the wire
// ---------------------------------------------------------------------------

/// #873.1: `Prefer: handling=lenient` must actually ignore an unknown parameter.
///
/// The preference was parsed, merged across repeated headers, and advertised in the
/// served document with the example summary "Ignore unknown parameters" — while having
/// no reader anywhere outside `prefer.rs`. A client whose proxy appends `utm_source`
/// followed the published guidance and still got a 400.
#[tokio::test]
async fn a_lenient_handling_preference_ignores_an_unknown_query_parameter() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let uri = format!("{BASE}/items?utm_source=email");

    // Strict (the default) still refuses — otherwise the lenient case below would prove
    // nothing about the preference.
    let (strict, body) = rig.request(reqwest::Method::GET, &uri, Some(&token()), None).await;
    assert_eq!(
        strict,
        reqwest::StatusCode::BAD_REQUEST,
        "an unknown parameter must still be refused by default; body: {body}"
    );
    assert!(
        body.contains("utm_source"),
        "the refusal must name the offending parameter; body: {body}"
    );

    let response = rig
        .client
        .get(format!("{}{uri}", rig.server.url))
        .bearer_auth(token())
        .header("prefer", "handling=lenient")
        .send()
        .await
        .expect("lenient request");
    let status = response.status();
    let applied = response
        .headers()
        .get("preference-applied")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response.text().await.unwrap_or_default();

    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "handling=lenient must ignore the unknown parameter; body: {body}"
    );
    assert!(
        applied.contains("handling=lenient"),
        "an applied preference must be echoed; Preference-Applied: {applied:?}"
    );
}

/// #873.1, the boundary: lenient tolerates parameters the server does not recognise, it
/// does not make the server misread ones it does. A bad bracket operator is still a 400.
#[tokio::test]
async fn a_lenient_handling_preference_does_not_excuse_a_malformed_parameter() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let response = rig
        .client
        .get(format!("{}{BASE}/items?label[nonsense]=x", rig.server.url))
        .bearer_auth(token())
        .header("prefer", "handling=lenient")
        .send()
        .await
        .expect("request");
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    assert_eq!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "a malformed operator on a known field must still fail; body: {body}"
    );
    assert!(
        body.contains("nonsense"),
        "the refusal must name the bad operator; body: {body}"
    );
}

/// #873.3: `RestConfig::etag` defaults to `true`, and the served document promises a
/// `304` on GET — so an `ETag` must be emitted and `If-None-Match` must be honoured.
///
/// `RestResponseFormatter` implemented all of it and had no production caller, so no
/// `ETag` was ever returned: a client implementing the documented conditional-GET cache
/// had nothing to store and re-transferred the whole payload on every poll.
#[tokio::test]
async fn a_get_emits_an_etag_and_honours_if_none_match() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let response = rig
        .client
        .get(format!("{}{BASE}/items", rig.server.url))
        .bearer_auth(token())
        .send()
        .await
        .expect("first request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .expect("a GET must carry an ETag when [rest] etag = true")
        .to_string();

    let conditional = rig
        .client
        .get(format!("{}{BASE}/items", rig.server.url))
        .bearer_auth(token())
        .header("if-none-match", &etag)
        .send()
        .await
        .expect("conditional request");

    assert_eq!(
        conditional.status(),
        reqwest::StatusCode::NOT_MODIFIED,
        "a matching If-None-Match must yield 304, as the served document promises"
    );
    assert_eq!(
        conditional.headers().get("etag").and_then(|v| v.to_str().ok()),
        Some(etag.as_str()),
        "a 304 must repeat the ETag it matched"
    );
    assert!(
        conditional.text().await.unwrap_or_default().is_empty(),
        "a 304 must carry no body"
    );
}

/// And a changed resource must change the `ETag`, or the cache would serve stale data
/// forever — the failure mode a conditional-GET bug actually produces.
#[tokio::test]
async fn an_etag_changes_when_the_underlying_rows_change() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let etag_of = || async {
        rig.client
            .get(format!("{}{BASE}/items", rig.server.url))
            .bearer_auth(token())
            .send()
            .await
            .expect("request")
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string)
    };

    let before = etag_of().await.expect("ETag before");

    let (status, body) = rig
        .request(
            reqwest::Method::POST,
            &format!("{BASE}/items"),
            Some(&token()),
            Some(json!({"label": "etag-buster"})),
        )
        .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "setup write failed: {body}");

    let after = etag_of().await.expect("ETag after");
    assert_ne!(before, after, "the ETag must change when the collection changes");
}

/// #873.3: a `201` must carry the `Location` header the served document advertises.
#[tokio::test]
async fn a_created_resource_reports_its_location() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let response = rig
        .client
        .post(format!("{}{BASE}/items", rig.server.url))
        .bearer_auth(token())
        .json(&json!({"label": "located"}))
        .send()
        .await
        .expect("create");

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .expect("a 201 must carry Location, as the served document promises")
        .to_string();

    assert!(
        location.starts_with(&format!("{BASE}/items/")),
        "Location must point at the created resource; got {location:?}"
    );

    // And it must resolve — a Location naming a URL that 404s is worse than none.
    let (status, body) = rig.request(reqwest::Method::GET, &location, Some(&token()), None).await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "the advertised Location must resolve; body: {body}"
    );
}

/// #873.4: the SSE endpoint must not look healthy while it can never deliver an event.
///
/// `RestState::event_transport` is `None` at every construction — the struct is private
/// and has no setter — so the stream emitted `event: ping` forever and nothing else,
/// while the served document described it as carrying `insert`/`update`/`delete`. A
/// dashboard saw a healthy connection, so its reconnect and error handling never fired
/// and it showed stale data indefinitely. Enabling the `observers` feature turned an
/// honest `501` into a silent no-op.
#[tokio::test]
async fn the_sse_stream_refuses_rather_than_pretending_to_deliver_events() {
    let Some(rig) = rig_with_writes().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let response = rig
        .client
        .get(format!("{}{BASE}/items/stream", rig.server.url))
        .bearer_auth(token())
        .header("accept", "text/event-stream")
        .send()
        .await
        .expect("stream request");

    assert_eq!(
        response.status(),
        reqwest::StatusCode::NOT_IMPLEMENTED,
        "an SSE endpoint with no event transport must say so, not stream heartbeats"
    );
}
