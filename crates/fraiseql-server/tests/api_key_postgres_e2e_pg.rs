//! #627: the Postgres-backed API-key store against a real database.
//!
//! Before this store existed, `[security.api_keys] storage = "postgres"` was
//! parsed and never read — an authenticator with zero keys that authenticated
//! nothing, silently (prevented only by a CLI-side compile bail). These tests
//! execute the DDL against real PostgreSQL (#748 precedent), then prove the
//! full lifecycle: a created key authenticates with its scopes, revocation and
//! expiry are enforced, a wrong verifier under a valid selector is rejected in
//! constant-time-compare fashion, and rotation invalidates every copy of the
//! old secret while the key identity survives.
//!
//! Self-skips when no `DATABASE_URL` is set (inert in the database-free `test`
//! leg; runs in the Dagger `integration: server` suite).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `fraiseql_p26_*` databases → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use axum::http::HeaderMap;
use fraiseql_server::api_key::{
    ApiKeyAuthenticator, ApiKeyConfig, ApiKeyResult,
    postgres::{ApiKeyStoreError, PgApiKeyStore},
};
use fraiseql_test_support::try_database_url;
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Scratch-database plumbing (same shape as rbac_admin_e2e_pg)
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

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// An authenticator wired to the given store, reading the default header.
fn authenticator(store: PgApiKeyStore) -> ApiKeyAuthenticator {
    let config = ApiKeyConfig {
        enabled:        true,
        header:         "x-api-key".into(),
        hash_algorithm: "sha256".into(),
        storage:        "postgres".into(),
        static_keys:    vec![],
    };
    ApiKeyAuthenticator::from_config(&config)
        .expect("valid config builds")
        .with_postgres(store)
}

fn headers_with_key(key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", key.parse().unwrap());
    headers
}

// ---------------------------------------------------------------------------
// The lifecycle, end to end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ddl_executes_and_a_created_key_authenticates_with_its_scopes() {
    let Some(url) = database_url_or_skip("ddl_executes_and_a_created_key_authenticates") else {
        return;
    };
    let db = "fraiseql_p26_lifecycle";
    let pool = scratch_pool(&url, db).await;

    let store = PgApiKeyStore::new(pool);
    store.ensure_schema().await.expect("DDL must execute against real PostgreSQL");
    // Idempotency: a second boot must not fail.
    store.ensure_schema().await.expect("DDL is idempotent");

    let (full_key, record) = store
        .create_key("ci-reporter", &["read:metrics".to_string()], None)
        .await
        .expect("create key");
    assert!(full_key.starts_with("fqlk_"), "key carries the fqlk prefix: {full_key}");
    assert!(
        full_key.contains(&record.selector),
        "the full key embeds the selector so revocation can be targeted"
    );

    let auth = authenticator(store);
    match auth.authenticate(&headers_with_key(&full_key)).await {
        ApiKeyResult::Authenticated(ctx) => {
            assert!(
                ctx.scopes.iter().any(|s| s == "read:metrics"),
                "scopes flow into the SecurityContext: {:?}",
                ctx.scopes
            );
            assert!(
                ctx.user_id.as_str().contains("ci-reporter"),
                "the key name is the audit identity: {}",
                ctx.user_id.as_str()
            );
        },
        other => panic!("a freshly created key must authenticate, got {other:?}"),
    }

    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn revoked_expired_and_wrong_verifier_keys_are_rejected() {
    let Some(url) = database_url_or_skip("revoked_expired_and_wrong_verifier") else {
        return;
    };
    let db = "fraiseql_p26_reject";
    let pool = scratch_pool(&url, db).await;

    let store = PgApiKeyStore::new(pool);
    store.ensure_schema().await.expect("DDL");

    // Revocation.
    let (revoked_key, revoked_record) =
        store.create_key("to-revoke", &[], None).await.expect("create");
    store.revoke(&revoked_record.selector).await.expect("revoke");
    // Idempotent revoke keeps the original timestamp semantics.
    store.revoke(&revoked_record.selector).await.expect("revoke twice");

    // Expiry: created already-expired.
    let (expired_key, _) = store
        .create_key("expired", &[], Some(chrono::Utc::now() - chrono::Duration::seconds(5)))
        .await
        .expect("create expired");

    // Wrong verifier under a valid selector.
    let (valid_key, valid_record) = store.create_key("valid", &[], None).await.expect("create");
    let forged_key = {
        let tail = "0".repeat(48);
        format!("fqlk_{}_{tail}", valid_record.selector)
    };

    let auth = authenticator(store);
    for (label, key) in [
        ("revoked", &revoked_key),
        ("expired", &expired_key),
        ("forged", &forged_key),
    ] {
        assert!(
            matches!(auth.authenticate(&headers_with_key(key)).await, ApiKeyResult::Invalid),
            "{label} key must be rejected"
        );
    }
    // The untouched valid key still works (the rejections are not a dead store).
    assert!(
        matches!(
            auth.authenticate(&headers_with_key(&valid_key)).await,
            ApiKeyResult::Authenticated(_)
        ),
        "the valid key is the counterweight"
    );

    // An unknown selector must be indistinguishable from a bad verifier.
    let unknown = format!("fqlk_{}_{}", "e".repeat(24), "f".repeat(48));
    assert!(
        matches!(auth.authenticate(&headers_with_key(&unknown)).await, ApiKeyResult::Invalid),
        "unknown selector rejects like a bad verifier"
    );

    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn rotation_invalidates_the_old_secret_and_the_new_one_works() {
    let Some(url) = database_url_or_skip("rotation_invalidates_the_old_secret") else {
        return;
    };
    let db = "fraiseql_p26_rotate";
    let pool = scratch_pool(&url, db).await;

    let store = PgApiKeyStore::new(pool);
    store.ensure_schema().await.expect("DDL");

    let (old_key, record) = store
        .create_key("rotating", &["write:orders".to_string()], None)
        .await
        .expect("create");
    let new_key = store.rotate(&record.selector).await.expect("rotate");
    assert_ne!(old_key, new_key, "rotation must mint a new secret");

    let auth = authenticator(store.clone());
    assert!(
        matches!(auth.authenticate(&headers_with_key(&old_key)).await, ApiKeyResult::Invalid),
        "every copy of the old secret stops working"
    );
    match auth.authenticate(&headers_with_key(&new_key)).await {
        ApiKeyResult::Authenticated(ctx) => {
            assert!(
                ctx.scopes.iter().any(|s| s == "write:orders"),
                "the key identity (scopes) survives rotation"
            );
        },
        other => panic!("the rotated key must authenticate, got {other:?}"),
    }

    // A revoked key refuses rotation — rotating it would silently un-revoke.
    store.revoke(&record.selector).await.expect("revoke");
    assert!(
        matches!(store.rotate(&record.selector).await, Err(ApiKeyStoreError::NotFound)),
        "rotating a revoked key must refuse"
    );

    drop_scratch(&url, db).await;
}

#[tokio::test]
async fn listing_shows_metadata_and_never_secret_material() {
    let Some(url) = database_url_or_skip("listing_shows_metadata") else {
        return;
    };
    let db = "fraiseql_p26_list";
    let pool = scratch_pool(&url, db).await;

    let store = PgApiKeyStore::new(pool);
    store.ensure_schema().await.expect("DDL");
    let (full_key, _) = store.create_key("listed", &[], None).await.expect("create");

    let keys = store.list_keys().await.expect("list");
    assert_eq!(keys.len(), 1);
    let listed = serde_json::to_string(&keys[0]).expect("record serializes");
    let verifier = full_key.rsplit('_').next().unwrap();
    assert!(
        !listed.contains(verifier),
        "the listing must never contain the verifier: {listed}"
    );
    assert!(listed.contains(&keys[0].selector), "the selector is the public handle");

    drop_scratch(&url, db).await;
}

// ---------------------------------------------------------------------------
// The full loop through the shipped binary's mount: boot DDL, admin REST
// management, header authentication — one server, one store.
// ---------------------------------------------------------------------------

/// 38 characters — comfortably over the configured admin-token minimum.
const ADMIN_TOKEN: &str = "p26-admin-token-at-least-32-chars-long";

#[tokio::test]
async fn server_mounts_management_api_and_the_managed_keys_authenticate() {
    use std::sync::Arc;

    use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
    use fraiseql_server::{Server, server_config::ServerConfig};

    let Some(url) = database_url_or_skip("server_mounts_management_api") else {
        return;
    };
    let db = "fraiseql_p26_server";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let schema: CompiledSchema = serde_json::from_value(serde_json::json!({
        "version": "2.0.0",
        "types": [],
        "queries": [],
        "mutations": [],
        "security": {
            "api_keys": { "enabled": true, "storage": "postgres" }
        },
    }))
    .expect("compiled schema");

    let config = ServerConfig {
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        database_url: scratch_url.clone(),
        admin_api_enabled: true,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        ..ServerConfig::default()
    };
    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("PostgresAdapter::new"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();

    let server = Box::pin(Server::new(config, schema, adapter, Some(pool.clone())))
        .await
        .expect("Server::new with postgres api-keys and a pool must succeed");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    // Unauthenticated management access is refused.
    let resp = client
        .get(format!("{base}/api/v1/admin/api-keys"))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status().as_u16(), 401, "management requires the admin bearer");

    // Create a key over the admin REST surface (boot ran the DDL).
    let resp = client
        .post(format!("{base}/api/v1/admin/api-keys"))
        .bearer_auth(ADMIN_TOKEN)
        .json(&serde_json::json!({ "name": "e2e-key", "scopes": ["read:things"] }))
        .send()
        .await
        .expect("create request");
    assert_eq!(resp.status().as_u16(), 201, "create must succeed");
    let body: serde_json::Value = resp.json().await.expect("json");
    let full_key = body["key"].as_str().expect("full key returned once").to_string();
    let selector = body["record"]["selector"].as_str().expect("selector").to_string();

    // The managed key authenticates a real request through the same server.
    // /api/v1/schema/metadata sits behind auth when introspection_require_auth
    // defaults on; the simplest authenticated probe is the management list with
    // the API key withheld and the bearer present — instead, verify through the
    // authenticator-facing surface: a GraphQL request carrying the key header
    // must NOT be rejected as an invalid key (an unknown key would 401).
    let resp = client
        .post(format!("{base}/graphql"))
        .header("x-api-key", &full_key)
        .json(&serde_json::json!({ "query": "{ __typename }" }))
        .send()
        .await
        .expect("graphql request");
    assert_ne!(
        resp.status().as_u16(),
        401,
        "a managed key must be accepted by the authenticator"
    );

    // Revoke it over REST; the same request is now refused.
    let resp = client
        .post(format!("{base}/api/v1/admin/api-keys/{selector}/revoke"))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .expect("revoke request");
    assert_eq!(resp.status().as_u16(), 200, "revoke must succeed");

    let resp = client
        .post(format!("{base}/graphql"))
        .header("x-api-key", &full_key)
        .json(&serde_json::json!({ "query": "{ __typename }" }))
        .send()
        .await
        .expect("graphql request after revoke");
    assert_eq!(
        resp.status().as_u16(),
        401,
        "a revoked key must be rejected by the live authenticator"
    );

    let _ = tx.send(());
    let _ = handle.await;
    drop_scratch(&url, db).await;
}
