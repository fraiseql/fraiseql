//! The storage policy admin surface, through the production mount (#974).
//!
//! These build a real [`Server`], mount it with
//! `mount_base_and_admin_routes` — the same call `build_router` makes — and
//! drive `/api/v1/admin/storage/{bucket}/policies` over `oneshot`. Nothing here
//! reimplements the mount, so a route that stops being registered, or a token
//! split that stops being applied, fails here rather than at a customer's first
//! `PUT`.
//!
//! They need a real PostgreSQL: the policy store is the durable half of the
//! feature, and an admin endpoint tested against a mock store proves only that
//! the mock works. The Dagger `integration` leg names this module explicitly;
//! the database-less `test` leg skips it by name rather than letting it
//! self-skip into a green that asserted nothing.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test module.

use std::{collections::HashMap, sync::Arc};

use axum::{Router, body::Body};
use fraiseql_core::{cache::CachedDatabaseAdapter, schema::CompiledSchema};
use fraiseql_storage::{
    BucketAccess, BucketConfig, LocalBackend, PolicyMethod, PolicyPrincipal, PolicyRule,
    StorageBackend, StorageMetadataRepo, StoragePolicyStore, StorageRlsEvaluator, StorageState,
    UploadSessionRepo,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

const WRITE_TOKEN: &str = "admin-write-token-that-is-long-enough-1234";
const READ_TOKEN: &str = "admin-readonly-token-that-is-long-enough-5678";
const BUCKET: &str = "docs";

/// A bucket whose configured policy permits authenticated reads under `a/`.
fn configured_policy() -> fraiseql_storage::BucketPolicy {
    fraiseql_storage::BucketPolicy {
        rules: vec![PolicyRule {
            methods:           vec![PolicyMethod::Read],
            principal:         PolicyPrincipal::Authenticated,
            key_prefix:        Some("a/".to_string()),
            not_before:        None,
            not_after:         None,
            require_unexpired: false,
            require_claims:    fraiseql_storage::ClaimValues::new(),
        }],
    }
}

/// The rules body an operator would `PUT`.
fn rules_body(prefix: &str) -> serde_json::Value {
    serde_json::json!({
        "rules": [
            { "methods": ["read"], "principal": "authenticated", "key_prefix": prefix },
        ]
    })
}

struct Rig {
    app:   Router,
    state: StorageState,
    // Held for the test's lifetime: the temp backend dir and the harness
    // service guard.
    _keep: (tempfile::TempDir, fraiseql_test_support::Service),
}

/// Build a server whose admin API is split-token and whose storage carries a
/// configured policy, then mount it exactly as `build_router` does.
async fn rig() -> Rig {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let pool = PgPool::connect(svc.url()).await.unwrap();
    for stmt in fraiseql_storage::migrations::storage_migration_sql().split(';') {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(&pool).await.unwrap();
        }
    }
    // The suite shares one database and runs single-threaded; start from no
    // stored policies so a previous test cannot decide this one.
    sqlx::query("DELETE FROM _fraiseql_storage_policies")
        .execute(&pool)
        .await
        .unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let mut buckets = HashMap::new();
    buckets.insert(
        BUCKET.to_string(),
        BucketConfig {
            name: BUCKET.to_string(),
            access: BucketAccess::Private,
            policies: Some(configured_policy()),
            ..BucketConfig::default()
        },
    );
    let state = StorageState::new(
        Arc::new(StorageBackend::Local(LocalBackend::new(tmp.path().to_str().unwrap()))),
        Arc::new(StorageMetadataRepo::new(pool.clone())),
        StorageRlsEvaluator::new(),
        buckets,
        Arc::new(UploadSessionRepo::new(pool.clone())),
        Arc::new(StoragePolicyStore::new(pool)),
    );

    let config = ServerConfig {
        cors_enabled: false,
        admin_api_enabled: true,
        admin_token: Some(WRITE_TOKEN.to_string()),
        admin_readonly_token: Some(READ_TOKEN.to_string()),
        ..ServerConfig::default()
    };
    let server: Server<CachedDatabaseAdapter<FailingAdapter>> = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("Server::new should succeed for an empty schema")
    .with_storage_state(state.clone());

    let app_state = server.build_app_state();
    let app = server.mount_base_and_admin_routes(Router::new(), &app_state);

    Rig {
        app,
        state,
        _keep: (tmp, svc),
    }
}

fn request(method: &str, token: Option<&str>, body: Option<serde_json::Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(format!("/api/v1/admin/storage/{BUCKET}/policies"));
    if let Some(token) = token {
        builder = builder.header(http::header::AUTHORIZATION, format!("Bearer {token}"));
    }
    match body {
        Some(json) => builder
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&json).unwrap()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn json_of(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

// ---------------------------------------------------------------------------

/// A policy this server would not accept at boot is refused at the request —
/// and the policy already in force is still the policy in force.
///
/// This is the guarantee the issue names: a pushed policy cannot refuse to
/// boot, so the refusal has to happen here, and it has to be inert. The
/// read-back is not a separate record of intent — `GET` reports the live bucket
/// map, the same map every storage request reads.
#[tokio::test]
async fn an_unparseable_policy_is_refused_and_leaves_the_running_one_in_place() {
    let rig = rig().await;

    // Establish a stored policy first, so the refusal has something other than
    // the configured policy to preserve.
    let ok = rig
        .app
        .clone()
        .oneshot(request("PUT", Some(WRITE_TOKEN), Some(rules_body("live/"))))
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);

    // The offending rule pairs a VALID method with a misspelt one. A parser
    // that dropped the unknown spelling instead of refusing would leave a
    // perfectly serviceable `read` rule behind and answer 200 — so this shape,
    // unlike a rule whose only method is misspelt, tells the two apart.
    let bad = serde_json::json!({
        "rules": [
            { "methods": ["read"], "principal": "authenticated" },
            { "methods": ["read", "reed"], "principal": "authenticated" },
        ]
    });
    let response = rig
        .app
        .clone()
        .oneshot(request("PUT", Some(WRITE_TOKEN), Some(bad)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_of(response).await;
    assert_eq!(body["error"], "invalid_policy");
    assert_eq!(body["rule_index"], 1, "the refusal must name the offending rule");
    assert_eq!(body["policy_in_force"], "unchanged");

    // The live map still carries the accepted policy — not the refused one, and
    // not nothing.
    let effective = rig.state.buckets.load().get(BUCKET).unwrap().policies.clone().unwrap();
    assert_eq!(
        effective.rules.first().unwrap().key_prefix.as_deref(),
        Some("live/"),
        "a refused push must not disturb the policy in force"
    );

    // And the store was not written either, so a restart agrees with what is
    // being enforced.
    let stored = rig.state.policy_store.get(BUCKET).await.unwrap().unwrap();
    assert_eq!(
        stored.parse().unwrap().rules.first().unwrap().key_prefix.as_deref(),
        Some("live/"),
    );
}

/// A rule carrying a field this build does not know is refused as an invalid
/// policy, not silently stripped of it.
#[tokio::test]
async fn a_rule_with_an_unknown_field_is_refused() {
    let rig = rig().await;
    let body = serde_json::json!({
        "rules": [
            { "methods": ["read"], "principal": "authenticated", "require_unexpird": true },
        ]
    });
    let response = rig
        .app
        .clone()
        .oneshot(request("PUT", Some(WRITE_TOKEN), Some(body)))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "a misspelt condition must not become a rule that silently stops narrowing"
    );
    let json = json_of(response).await;
    assert_eq!(json["error"], "invalid_policy");
    assert!(
        json["message"].as_str().unwrap().contains("require_unexpird"),
        "the refusal must name the offending field: {json}"
    );
    assert!(
        rig.state.policy_store.get(BUCKET).await.unwrap().is_none(),
        "nothing was persisted"
    );
}

/// The stored policy replaces the configured one wholesale, `GET` says which
/// source governs, and `DELETE` hands the bucket back.
#[tokio::test]
async fn put_get_delete_round_trip_reports_the_governing_source() {
    let rig = rig().await;

    // Before any push: the configured policy governs.
    let response = rig.app.clone().oneshot(request("GET", Some(READ_TOKEN), None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["source"], "config_file");
    assert_eq!(body["access"], "private");
    assert_eq!(body["rules"][0]["key_prefix"], "a/");
    assert!(body["updated_at"].is_null(), "a configured policy has no stored timestamp");

    // Push: the store now governs, and the configured rule is gone rather than
    // joined.
    let response = rig
        .app
        .clone()
        .oneshot(request("PUT", Some(WRITE_TOKEN), Some(rules_body("b/"))))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["source"], "store");
    assert!(body["updated_at"].is_string());
    assert_eq!(body["rules"].as_array().unwrap().len(), 1, "the push replaced, it did not add");
    assert_eq!(body["rules"][0]["key_prefix"], "b/");

    let body =
        json_of(rig.app.clone().oneshot(request("GET", Some(READ_TOKEN), None)).await.unwrap())
            .await;
    assert_eq!(body["source"], "store");
    assert_eq!(body["rules"][0]["key_prefix"], "b/");

    // Delete: back to the configured policy, and the response says so.
    let response = rig
        .app
        .clone()
        .oneshot(request("DELETE", Some(WRITE_TOKEN), None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["source"], "config_file");
    assert_eq!(body["rules"][0]["key_prefix"], "a/");
    assert!(rig.state.policy_store.get(BUCKET).await.unwrap().is_none());
}

/// The token split is real in both directions: reading which policy governs is
/// a read-token operation, changing it is not.
///
/// This is the reason the endpoint lives on `/api/v1/admin` rather than beside
/// the Studio storage browser. If the split did not hold, an operator handing
/// out the read-only token would be handing out the ability to rewrite every
/// bucket's access control.
#[tokio::test]
async fn the_read_token_can_inspect_but_cannot_change_a_policy() {
    let rig = rig().await;

    assert_eq!(
        rig.app
            .clone()
            .oneshot(request("GET", Some(READ_TOKEN), None))
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "the read token inspects"
    );
    // A well-formed bearer carrying the wrong token is 403, as everywhere else
    // on the admin API; 401 is reserved for a caller who presented nothing.
    assert_eq!(
        rig.app
            .clone()
            .oneshot(request("PUT", Some(READ_TOKEN), Some(rules_body("b/"))))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN,
        "the read token must not be able to replace a policy"
    );
    assert_eq!(
        rig.app
            .clone()
            .oneshot(request("DELETE", Some(READ_TOKEN), None))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN,
        "nor delete one"
    );
    assert!(
        rig.state.policy_store.get(BUCKET).await.unwrap().is_none(),
        "and neither refusal wrote anything"
    );
    assert_eq!(
        rig.app.clone().oneshot(request("GET", None, None)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED,
        "and an unauthenticated caller cannot even read"
    );
}

/// A bucket this server does not configure answers 404 on every verb, and a
/// push to it stores nothing — an orphan row would govern nothing while looking
/// like it did.
#[tokio::test]
async fn an_unknown_bucket_is_refused_on_every_verb() {
    let rig = rig().await;
    let uri = "/api/v1/admin/storage/nope/policies";
    for (method, body) in [
        ("GET", None),
        ("PUT", Some(rules_body("b/"))),
        ("DELETE", None),
    ] {
        let token = if method == "GET" {
            READ_TOKEN
        } else {
            WRITE_TOKEN
        };
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
            .header(http::header::CONTENT_TYPE, "application/json");
        let payload =
            body.map_or_else(Body::empty, |json| Body::from(serde_json::to_vec(&json).unwrap()));
        let response = rig.app.clone().oneshot(builder.body(payload).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} on an unknown bucket");
    }
    assert!(rig.state.policy_store.get("nope").await.unwrap().is_none());
    assert!(rig.state.policy_store.list().await.unwrap().is_empty());
}

/// An empty rule list is a lock-down an operator can express, and it is not the
/// same as deleting the policy.
#[tokio::test]
async fn an_empty_rule_list_locks_the_bucket_down() {
    let rig = rig().await;
    let response = rig
        .app
        .clone()
        .oneshot(request("PUT", Some(WRITE_TOKEN), Some(serde_json::json!({"rules": []}))))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["source"], "store");
    assert_eq!(body["rules"].as_array().unwrap().len(), 0);

    let effective = rig.state.buckets.load().get(BUCKET).unwrap().policies.clone().unwrap();
    assert!(
        effective.rules.is_empty(),
        "an empty list is a policy that permits nothing, not an absent policy"
    );
}

/// The endpoint is not mounted at all when the deployment configures no
/// storage, rather than existing and answering 404 for every bucket.
#[tokio::test]
async fn the_endpoint_is_absent_without_storage() {
    let config = ServerConfig {
        cors_enabled: false,
        admin_api_enabled: true,
        admin_token: Some(WRITE_TOKEN.to_string()),
        admin_readonly_token: Some(READ_TOKEN.to_string()),
        ..ServerConfig::default()
    };
    let server: Server<CachedDatabaseAdapter<FailingAdapter>> = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("Server::new should succeed for an empty schema");
    let app_state = server.build_app_state();
    let app = server.mount_base_and_admin_routes(Router::new(), &app_state);

    let response = app.oneshot(request("GET", Some(READ_TOKEN), None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
