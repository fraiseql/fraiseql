//! #379 — `[security] persisted_queries_only` refuses ad-hoc operations on
//! **every** HTTP method the GraphQL endpoint serves.
//!
//! These drive the **real** `build_graphql_router` (the mount, not the
//! library): POST, GET, and the opt-in QUERY method all funnel through
//! `stages::resolve_query_body`, and this pins that an ad-hoc document is
//! refused on each of them — a persisted-only mode one transport ignores is
//! not a mode. The out-of-band transports are pinned elsewhere: MCP and REST
//! never execute client-authored documents, subscriptions honor only the root
//! field name, and the Flight service refuses GraphQL outright (no executor is
//! ever attached).
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::body::Body;
use fraiseql_core::{cache::CachedDatabaseAdapter, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

/// One persisted document, so the store is non-empty and the manifest loads.
const PERSISTED_DOC: &str = "{ users { id } }";

/// Build a server whose compiled schema declares `persisted_queries_only` with
/// a real manifest file, exactly as an operator would ship it.
async fn persisted_only_server(
    dir: &tempfile::TempDir,
) -> Server<CachedDatabaseAdapter<FailingAdapter>> {
    use sha2::Digest as _;
    let hash = hex::encode(sha2::Sha256::digest(PERSISTED_DOC.as_bytes()));
    let manifest = serde_json::json!({
        "version": 1,
        "documents": { format!("sha256:{hash}"): PERSISTED_DOC }
    });
    let manifest_path = dir.path().join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    let mut schema = CompiledSchema::new();
    let mut security = fraiseql_core::schema::SecurityConfig::new();
    security
        .additional
        .insert("persisted_queries_only".to_string(), serde_json::json!(true));
    security.additional.insert(
        "trusted_documents".to_string(),
        serde_json::json!({
            "enabled": true,
            "mode": "permissive",
            "manifest_path": manifest_path.to_str().unwrap(),
        }),
    );
    schema.security = Some(security);

    let config = ServerConfig {
        cors_enabled: false,
        enable_http_query: true,
        ..ServerConfig::default()
    };
    Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), None))
        .await
        .expect("Server::new should succeed with a trusted-documents manifest")
}

/// Status of one ad-hoc request at the real router.
async fn adhoc_status(
    server: &Server<CachedDatabaseAdapter<FailingAdapter>>,
    method: &str,
) -> StatusCode {
    let state = server.build_app_state();
    let app = server.build_graphql_router(&state);
    let request = if method == "GET" {
        let uri = format!("/graphql?query={}", urlencoding::encode("{ adhoc { id } }"));
        Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap()
    } else {
        Request::builder()
            .method(method)
            .uri("/graphql")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"query":"{ adhoc { id } }"}"#))
            .unwrap()
    };
    app.oneshot(request).await.unwrap().status()
}

/// An ad-hoc (non-persisted) operation must be refused on POST, GET, and the
/// QUERY method alike — HTTP 400 `FORBIDDEN_QUERY`, never executed.
#[tokio::test]
async fn adhoc_operations_are_refused_on_every_http_method() {
    let dir = tempfile::tempdir().unwrap();
    let server = persisted_only_server(&dir).await;

    for method in ["POST", "GET", "QUERY"] {
        assert_eq!(
            adhoc_status(&server, method).await,
            StatusCode::BAD_REQUEST,
            "persisted-only mode must refuse an ad-hoc document over {method}"
        );
    }
}

/// The persisted document itself stays servable by `documentId` — the mode
/// refuses ad-hoc text, not the allow-listed operations.
#[tokio::test]
async fn persisted_document_id_still_dispatches() {
    use sha2::Digest as _;
    let dir = tempfile::tempdir().unwrap();
    let server = persisted_only_server(&dir).await;
    let state = server.build_app_state();
    let app = server.build_graphql_router(&state);

    let hash = hex::encode(sha2::Sha256::digest(PERSISTED_DOC.as_bytes()));
    let body = serde_json::json!({ "documentId": format!("sha256:{hash}") });
    let request = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let status = app.oneshot(request).await.unwrap().status();
    // The empty fixture schema cannot resolve `users`, so execution answers
    // with a GraphQL error — the point is the documentId was ACCEPTED (not
    // refused as FORBIDDEN_QUERY / 400).
    assert_ne!(
        status,
        StatusCode::BAD_REQUEST,
        "an allow-listed documentId must dispatch in persisted-only mode"
    );
}
