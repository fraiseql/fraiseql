//! #749 regression: no Studio admin endpoint may report success for work it did not do.
//!
//! Five write endpoints answered `{"success": true}` while performing no side effect —
//! session revocation, function-secret set and delete, row mutation, and user
//! invitation — and six read endpoints answered with a hard-coded empty collection,
//! which asserts "there are none" rather than "this is not wired". The module already
//! had the right convention (`invoke`, `presign`, `delete_object` and `mfa` all
//! returned `501`), so these had *drifted* from a correct sibling.
//!
//! **Why a corpus test rather than a case per endpoint.** The defect class is "one
//! more handler drifts from the convention", so a hand-picked selection of endpoints
//! would keep passing while the next one drifts. The route list here is derived from
//! `server/routing/admin.rs` **at compile time** via `include_str!`: adding a route
//! without adding its expectation fails this test, and it cannot silently skip the
//! way a runtime path lookup can.
//!
//! The two live cases (the RBAC-style ones) are here too: the revoke endpoint must
//! perform a real revocation when a store is configured, asserted by reading the
//! revocation table directly rather than by trusting the response body.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_p05_studio*` databases → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::BTreeSet, sync::Arc};

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use sqlx::PgPool;

/// The routing source, embedded at compile time.
///
/// `include_str!` rather than a runtime read: a path that stops resolving is a build
/// error, not a test that quietly stops checking anything. Three suites in this
/// repository had never executed because a runtime `path.exists()` guard resolved
/// relative to the crate directory and always returned false.
const ROUTING_SRC: &str = include_str!("../src/server/routing/admin.rs");

const ADMIN_TOKEN: &str = "p05-studio-admin-token-32-chars-min";

/// What an endpoint is allowed to answer for an authenticated admin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Performs real work and answers 2xx.
    Real,
    /// Cannot perform the operation and says so with `501 not_implemented`.
    NotImplemented,
    /// Mounted only under a configuration this test does not use.
    NotMountedHere,
}

/// Every `/admin/v1/*` route, with the method used to reach it and what it must answer.
///
/// Keep this in the order routes are declared in `mount_studio_admin_api`.
const CORPUS: &[(&str, &str, &str, Expect)] = &[
    // (method, request path, route literal as written in the router, expectation)
    ("GET", "/admin/v1/schema", "/admin/v1/schema", Expect::Real),
    ("GET", "/admin/v1/health/detailed", "/admin/v1/health/detailed", Expect::Real),
    (
        "POST",
        "/admin/v1/data/Widget/query",
        "/admin/v1/data/{entity}/query",
        Expect::NotImplemented,
    ),
    (
        "POST",
        "/admin/v1/data/Widget/mutate",
        "/admin/v1/data/{entity}/mutate",
        Expect::NotImplemented,
    ),
    ("GET", "/admin/v1/users", "/admin/v1/users", Expect::NotImplemented),
    (
        "POST",
        "/admin/v1/users/invite",
        "/admin/v1/users/invite",
        Expect::NotImplemented,
    ),
    // Real when a revocation store is configured; this server has none, so it must
    // refuse rather than report the revocation it cannot perform. The configured
    // case is asserted separately in `revoke_actually_revokes_when_a_store_is_configured`.
    (
        "POST",
        "/admin/v1/users/u1/revoke",
        "/admin/v1/users/{id}/revoke",
        Expect::NotImplemented,
    ),
    (
        "GET",
        "/admin/v1/users/u1/mfa",
        "/admin/v1/users/{id}/mfa",
        Expect::NotImplemented,
    ),
    (
        "GET",
        "/admin/v1/storage/buckets",
        "/admin/v1/storage/buckets",
        Expect::NotImplemented,
    ),
    (
        "GET",
        "/admin/v1/storage/objects?bucket=b",
        "/admin/v1/storage/objects",
        Expect::NotImplemented,
    ),
    (
        "POST",
        "/admin/v1/storage/objects/sign",
        "/admin/v1/storage/objects/sign",
        Expect::NotImplemented,
    ),
    (
        "DELETE",
        "/admin/v1/storage/objects",
        "/admin/v1/storage/objects",
        Expect::NotImplemented,
    ),
    ("GET", "/admin/v1/functions", "/admin/v1/functions", Expect::NotImplemented),
    (
        "POST",
        "/admin/v1/functions/f/invoke",
        "/admin/v1/functions/{name}/invoke",
        Expect::NotImplemented,
    ),
    (
        "GET",
        "/admin/v1/functions/f/logs",
        "/admin/v1/functions/{name}/logs",
        Expect::NotImplemented,
    ),
    (
        "GET",
        "/admin/v1/functions/f/secrets",
        "/admin/v1/functions/{name}/secrets",
        Expect::NotImplemented,
    ),
    (
        "PUT",
        "/admin/v1/functions/f/secrets/K",
        "/admin/v1/functions/{name}/secrets/{key}",
        Expect::NotImplemented,
    ),
    (
        "DELETE",
        "/admin/v1/functions/f/secrets/K",
        "/admin/v1/functions/{name}/secrets/{key}",
        Expect::NotImplemented,
    ),
    ("GET", "/admin/v1/metrics/summary", "/admin/v1/metrics/summary", Expect::Real),
    // Mounted by `mount_jwks_refresh`, which additionally requires an OIDC validator.
    (
        "POST",
        "/admin/v1/auth/refresh-jwks",
        "/admin/v1/auth/refresh-jwks",
        Expect::NotMountedHere,
    ),
];

/// A request body for the endpoints that need one, so a `422` cannot masquerade as a
/// refusal to act.
fn body_for(path: &str) -> Option<Value> {
    if path.ends_with("/query") {
        Some(json!({"page": 1, "page_size": 10}))
    } else if path.ends_with("/mutate") {
        Some(json!({"operation": "insert", "data": {"id": 1}}))
    } else if path.ends_with("/invite") {
        Some(json!({"email": "a@b.test"}))
    } else if path.ends_with("/invoke") {
        Some(json!({"event": {}}))
    } else if path.ends_with("/sign") {
        Some(json!({"bucket": "b", "key": "k", "expires_in_secs": 60}))
    } else if path.contains("/secrets/") {
        Some(json!({"value": "s3cr3t"}))
    } else if path == "/admin/v1/storage/objects" {
        Some(json!({"bucket": "b", "key": "k"}))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

async fn scratch_pool(admin_url: &str, db: &str) -> PgPool {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    sqlx::raw_sql(&format!("CREATE DATABASE {db}"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    admin.close().await;
    PgPool::connect(&with_database(admin_url, db))
        .await
        .expect("connect to scratch database")
}

async fn drop_scratch(admin_url: &str, db: &str) {
    let Ok(admin) = PgPool::connect(admin_url).await else {
        return;
    };
    let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await;
    admin.close().await;
}

/// A compiled schema carrying one type, so `/admin/v1/data/Widget/*` reaches the
/// handler rather than the entity-not-found guard.
fn schema_json(token_revocation: Option<Value>) -> Value {
    let mut security = json!({});
    if let Some(cfg) = token_revocation {
        security["token_revocation"] = cfg;
    }
    json!({
        "version": "2.0.0",
        "types": [{
            "name": "Widget",
            "sql_source": "v_widget",
            "fields": [{"name": "id", "field_type": "Int", "nullable": false}],
        }],
        "queries": [],
        "mutations": [],
        "security": security,
    })
}

struct StudioServer {
    base:      String,
    admin_url: String,
    db:        &'static str,
    pool:      PgPool,
    shutdown:  Option<tokio::sync::oneshot::Sender<()>>,
    handle:    tokio::task::JoinHandle<Result<(), fraiseql_server::ServerError>>,
    client:    reqwest::Client,
}

impl StudioServer {
    async fn start(admin_url: &str, db: &'static str, schema: Value) -> Self {
        let pool = scratch_pool(admin_url, db).await;
        let scratch_url = with_database(admin_url, db);
        let compiled: CompiledSchema = serde_json::from_value(schema).expect("compiled schema");

        let config = ServerConfig {
            // #874: production validate() refuses cors_enabled=true + empty origins
            cors_enabled: false,
            database_url: scratch_url.clone(),
            admin_api_enabled: true,
            admin_token: Some(ADMIN_TOKEN.to_string()),
            ..ServerConfig::default()
        };

        let adapter =
            Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));

        let listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();

        let mut server =
            Box::pin(Server::new(config, compiled.clone(), adapter, Some(pool.clone())))
                .await
                .expect("Server::new");

        // The PostgreSQL runtime path installs this explicitly (main.rs does the same);
        // it is `None` from `revocation_manager_from_schema` for the postgres backend.
        if let Some(manager) = fraiseql_server::token_revocation::build_postgres_revocation_manager(
            &scratch_url,
            &compiled,
        )
        .await
        .expect("revocation manager")
        {
            server = server.with_revocation_manager(manager);
        }

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            server
                .serve_on_listener(listener, async {
                    let _ = rx.await;
                })
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        Self {
            base: format!("http://127.0.0.1:{port}"),
            admin_url: admin_url.to_string(),
            db,
            pool,
            shutdown: Some(tx),
            handle,
            client: reqwest::Client::new(),
        }
    }

    async fn request(&self, method: &str, path: &str, token: Option<&str>) -> (u16, Value) {
        let url = format!("{}{path}", self.base);
        let method = reqwest::Method::from_bytes(method.as_bytes()).expect("http method");
        let mut req = self.client.request(method, url);
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body_for(path) {
            req = req.json(&body);
        }
        let resp = req.send().await.expect("request");
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        (status, serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    async fn shutdown(mut self) {
        drop(self.shutdown.take());
        let _ = (&mut self.handle).await;
        self.pool.close().await;
        drop_scratch(&self.admin_url, self.db).await;
    }
}

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

// ---------------------------------------------------------------------------
// The corpus covers the router
// ---------------------------------------------------------------------------

/// Extract every `"/admin/v1/…"` string literal from the routing source.
fn declared_routes() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = ROUTING_SRC;
    while let Some(start) = rest.find("\"/admin/v1") {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else {
            break;
        };
        found.insert(after[..end].to_string());
        rest = &after[end..];
    }
    found
}

/// No route may be mounted without a corpus entry saying what it is allowed to answer.
///
/// This is the part that makes the phase's fix durable: the next handler added to
/// `mount_studio_admin_api` has to declare whether it does real work, and if it does
/// not, the case below proves it says so.
#[test]
fn the_corpus_covers_every_mounted_admin_route() {
    let declared = declared_routes();
    assert!(
        !declared.is_empty(),
        "no /admin/v1 route literals found in server/routing/admin.rs — the parser or the \
         file layout changed, and this gate is no longer checking anything"
    );

    let covered: BTreeSet<String> =
        CORPUS.iter().map(|(_, _, route, _)| (*route).to_string()).collect();

    let uncovered: Vec<&String> = declared.difference(&covered).collect();
    assert!(
        uncovered.is_empty(),
        "these routes are mounted but have no corpus entry, so nothing checks whether they \
         fabricate success: {uncovered:?}"
    );

    let stale: Vec<&String> = covered.difference(&declared).collect();
    assert!(
        stale.is_empty(),
        "these corpus entries name routes that are no longer mounted: {stale:?}"
    );
}

// ---------------------------------------------------------------------------
// The behaviour
// ---------------------------------------------------------------------------

/// Every admin endpoint either does its job or refuses; none reports success for a
/// no-op, and none answers an empty collection in place of data it cannot read.
#[tokio::test]
async fn no_admin_endpoint_fabricates_success() {
    let Some(url) = database_url_or_skip("no_admin_endpoint_fabricates_success") else {
        return;
    };
    let server = StudioServer::start(&url, "fraiseql_p05_studio", schema_json(None)).await;

    for (method, path, route, expect) in CORPUS {
        // Everything under /admin/v1 is behind the admin bearer token.
        let (status, _) = server.request(method, path, None).await;
        if *expect == Expect::NotMountedHere {
            assert_eq!(status, 404, "{method} {route} should not be mounted in this config");
            continue;
        }
        assert_eq!(status, 401, "{method} {route} must require the admin token, got {status}");

        let (status, body) = server.request(method, path, Some(ADMIN_TOKEN)).await;
        match expect {
            Expect::Real => {
                assert!(
                    (200..300).contains(&status),
                    "{method} {route} claims to do real work but answered {status}: {body}"
                );
            },
            Expect::NotImplemented => {
                assert_eq!(
                    status, 501,
                    "{method} {route} performs no operation, so it must answer 501 rather than \
                     a success. Got {status}: {body}"
                );
                assert_eq!(
                    body["error"].as_str(),
                    Some("not_implemented"),
                    "{method} {route} must use the shared refusal shape; got {body}"
                );
                assert!(
                    body["feature"].as_str().is_some_and(|f| !f.is_empty()),
                    "{method} {route} must name the missing feature; got {body}"
                );
            },
            // Handled before the authenticated request is issued.
            Expect::NotMountedHere => {},
        }

        // The specific lie #749 is about, asserted independently of the status code:
        // no response from an endpoint that performs nothing may contain success:true.
        if *expect != Expect::Real {
            assert_ne!(
                body["success"].as_bool(),
                Some(true),
                "{method} {route} reported success without performing an operation: {body}"
            );
        }
    }

    server.shutdown().await;
}

/// The `Real` endpoints must report measured values, not plausible-looking zeros.
///
/// `uptime_secs` was `SystemTime::now() - UNIX_EPOCH` — the current Unix timestamp,
/// so a server four seconds old claimed ~1.8 billion seconds of uptime. The pool and
/// subscription figures were hard `0`, which is what a saturated pool and a dead
/// subscription fleet also look like.
#[tokio::test]
async fn reported_health_and_metrics_are_measured_or_null() {
    let Some(url) = database_url_or_skip("reported_health_and_metrics_are_measured_or_null") else {
        return;
    };
    let server = StudioServer::start(&url, "fraiseql_p05_studio_health", schema_json(None)).await;

    let (status, health) =
        server.request("GET", "/admin/v1/health/detailed", Some(ADMIN_TOKEN)).await;
    assert_eq!(status, 200, "{health}");
    let uptime = health["uptime_secs"].as_u64().expect("uptime_secs");
    assert!(
        uptime < 3600,
        "uptime_secs must be time since boot, not a Unix timestamp; got {uptime}"
    );
    for field in ["pool_active", "pool_idle", "pool_max"] {
        assert!(
            health[field].is_null(),
            "{field} is not measurable here and must be null, not a fabricated number: {health}"
        );
    }

    let (status, metrics) =
        server.request("GET", "/admin/v1/metrics/summary", Some(ADMIN_TOKEN)).await;
    assert_eq!(status, 200, "{metrics}");
    assert!(metrics["pool"].is_null(), "unmeasured pool stats must be null: {metrics}");
    assert!(
        metrics["subscriptions"].is_null(),
        "unmeasured subscription count must be null: {metrics}"
    );
    for window in ["rate_5m", "rate_1h", "rate_24h"] {
        assert!(
            metrics["errors"][window].is_null(),
            "{window} is not tracked; reporting the lifetime rate under a window name is the \
             misinformation this phase removes: {metrics}"
        );
    }
    assert!(
        metrics["errors"]["lifetime"].is_number(),
        "the lifetime rate is real: {metrics}"
    );

    server.shutdown().await;
}

/// With a revocation store configured, `POST /admin/v1/users/{id}/revoke` must revoke.
///
/// The side effect is read straight out of the revocation table. Asserting on the
/// response body is exactly what would have passed against the broken handler, which
/// answered `{"success": true, "message": "All sessions revoked"}` while no store was
/// touched — during an account-compromise response, with the attacker's tokens still
/// validating.
#[tokio::test]
async fn revoke_actually_revokes_when_a_store_is_configured() {
    let Some(url) = database_url_or_skip("revoke_actually_revokes_when_a_store_is_configured")
    else {
        return;
    };
    let schema = schema_json(Some(json!({
        "enabled": true,
        "backend": "postgres",
        "require_jti": false,
        "fail_open": false,
        "revoke_all_ttl_secs": 3600,
    })));
    let server = StudioServer::start(&url, "fraiseql_p05_studio_revoke", schema).await;

    let sub = "compromised-user";
    let before: Option<i64> = sqlx::query_scalar(
        "SELECT revoked_after FROM fraiseql_revoked_users WHERE sub = $1 AND expires_at > NOW()",
    )
    .bind(sub)
    .fetch_optional(&server.pool)
    .await
    .expect("revocation probe");
    assert_eq!(before, None, "nothing is revoked before the call");

    let (status, body) = server
        .request("POST", &format!("/admin/v1/users/{sub}/revoke"), Some(ADMIN_TOKEN))
        .await;
    assert_eq!(status, 200, "revoke must succeed with a store configured: {body}");
    assert_eq!(body["success"].as_bool(), Some(true), "{body}");

    let after: Option<i64> = sqlx::query_scalar(
        "SELECT revoked_after FROM fraiseql_revoked_users WHERE sub = $1 AND expires_at > NOW()",
    )
    .bind(sub)
    .fetch_optional(&server.pool)
    .await
    .expect("revocation probe");
    assert!(
        after.is_some(),
        "the response said all sessions were revoked; the revocation store disagrees"
    );

    server.shutdown().await;
}
