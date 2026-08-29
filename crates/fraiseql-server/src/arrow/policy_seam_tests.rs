//! #954 — the Flight GraphQL path is refused **by policy**, not by accident.
//!
//! The companion to `server/routing/persisted_only_transport_tests.rs`, which pins
//! the same mode across POST/GET/QUERY. Flight is the transport that had no such
//! test: it refused every GraphQL request, but only because nothing had ever
//! attached an executor. That refusal is indistinguishable from a policy decision
//! and disappears the moment someone wires one — which is the whole of #954.
//!
//! So these assert on **which** refusal happens. A test that accepted any error
//! would have passed before this seam existed, and would keep passing if the seam
//! were replaced by a bare `ExecutorQueryAdapter` tomorrow.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_arrow::QueryExecutor;
use fraiseql_core::{
    cache::CachedDatabaseAdapter, schema::CompiledSchema, security::SecurityContext, types::UserId,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use super::policy_seam::PolicyGatedExecutor;
use crate::{server::Server, server_config::ServerConfig};

/// The one document the manifest allows.
const PERSISTED_DOC: &str = "{ users { id } }";
/// A document the manifest does not contain — what every Flight client must send,
/// since the transport has no persisted-document channel.
const ADHOC_DOC: &str = "{ adhoc { id } }";

/// A server in `persisted_queries_only` mode with a real manifest file, exactly as
/// an operator would ship it (the same fixture as the HTTP transport tests).
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
    schema.security = Some(fraiseql_core::schema::SecurityConfig {
        persisted_queries_only: true,
        trusted_documents: Some(fraiseql_core::schema::TrustedDocumentsConfig {
            enabled: true,
            mode: fraiseql_core::schema::TrustedDocumentMode::Permissive,
            manifest_path: Some(manifest_path.to_str().unwrap().to_string()),
            ..Default::default()
        }),
        ..fraiseql_core::schema::SecurityConfig::default()
    });

    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), None))
        .await
        .expect("Server::new should succeed with a trusted-documents manifest")
}

/// The context a Flight `do_exchange` builds after validating a session token —
/// `SecurityContext::from_user` on the session's subject, which is what the handler
/// does.
fn flight_session_context() -> SecurityContext {
    let user = fraiseql_core::security::auth_middleware::AuthenticatedUser {
        user_id:      UserId::new("flight-policy-user"),
        scopes:       vec!["user".to_string()],
        expires_at:   chrono::Utc::now() + chrono::Duration::hours(1),
        email:        None,
        display_name: None,
        extra_claims: std::collections::HashMap::new(),
    };
    SecurityContext::from_user(&user, "req-flight-1".to_string())
}

/// An ad-hoc document over Flight is refused **because it is not persisted** — not
/// because no executor is configured.
///
/// The message assertion is the point. "No executor configured" was the old
/// behaviour and would satisfy any is-error check, while meaning the transport is
/// simply unimplemented; this pins that a *mode* refused the request.
#[tokio::test]
async fn an_adhoc_document_is_refused_by_the_persisted_only_policy() {
    let dir = tempfile::tempdir().unwrap();
    let server = persisted_only_server(&dir).await;
    let seam = PolicyGatedExecutor::new(server.build_app_state());

    let error = seam
        .execute_with_security(ADHOC_DOC, None, &flight_session_context())
        .await
        .expect_err("persisted-only mode must refuse an ad-hoc document over Flight too");

    // #1201: the refusal is typed now, so this asserts the *kind* as well as the
    // text. `Authorization` is what makes the transport answer gRPC
    // PERMISSION_DENIED instead of a retryable INTERNAL — a forbidden document
    // does not become allowed by asking again.
    assert!(
        matches!(error, fraiseql_core::error::FraiseQLError::Authorization { .. }),
        "a policy refusal is an authorization decision, got: {error:?}"
    );
    let message = error.to_string();
    assert!(
        message.contains("persisted queries only"),
        "the refusal must name the policy that refused, got: {message}"
    );
    assert!(
        !message.contains("No executor configured"),
        "an unwired transport is not a policy decision, got: {message}"
    );
}

/// Under `persisted_queries_only`, **every** Flight GraphQL request is refused —
/// including one whose text is byte-for-byte an allow-listed document.
///
/// This is a consequence operators need stated rather than discovered: a trusted-
/// document store in strict mode resolves by document **ID**, and Flight has no
/// channel to carry one, so matching text is not enough and cannot be made enough
/// without inventing a Flight-only bypass. The mode therefore turns Flight GraphQL
/// off, which is the fail-closed reading of "persisted queries only" and the one
/// this seam implements.
#[tokio::test]
async fn under_persisted_only_even_the_allow_listed_text_is_refused_over_flight() {
    let dir = tempfile::tempdir().unwrap();
    let server = persisted_only_server(&dir).await;
    let seam = PolicyGatedExecutor::new(server.build_app_state());

    let error = seam
        .execute_with_security(PERSISTED_DOC, None, &flight_session_context())
        .await
        .expect_err("strict mode resolves by document ID; matching text is not a substitute");

    assert!(
        matches!(error, fraiseql_core::error::FraiseQLError::Authorization { .. }),
        "a policy refusal is an authorization decision, got: {error:?}"
    );
    assert!(
        error.to_string().contains("persisted queries only"),
        "the refusal must name the policy, got: {error}"
    );
}

/// The counterweight: the seam is not simply refusing everything.
///
/// With no trusted-document store configured, the same document reaches execution.
/// It still fails there — `FailingAdapter` is the point of the fixture, and the
/// empty schema cannot resolve `users` — but it must fail **past** the policy.
/// Without this, a seam that refused unconditionally would satisfy every other test
/// in this file.
#[tokio::test]
async fn without_a_trusted_document_store_the_query_reaches_execution() {
    let server = Box::pin(Server::new(
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        },
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("Server::new should succeed without trusted documents");
    let seam = PolicyGatedExecutor::new(server.build_app_state());

    let outcome = seam.execute_with_security(PERSISTED_DOC, None, &flight_session_context()).await;

    if let Err(error) = outcome {
        let message = error.to_string();
        assert!(
            !message.contains("persisted queries only"),
            "with no store configured nothing may be refused as unpersisted, got: {message}"
        );
        // #1201 removed the `Tenant resolution failed:` / `Tenant dispatch
        // refused:` prefixes — they were the `format!` that destroyed the typed
        // error. The property they stood for is asserted positively instead: the
        // request got *past* tenancy and failed in execution, which the empty
        // schema's own message is the evidence for.
        assert!(
            !message.to_lowercase().contains("tenant"),
            "an unconfigured single-tenant deployment must not be refused by tenancy, got: \
             {message}"
        );
        assert!(
            message.contains("users"),
            "the failure must come from executing the document, not from a gate before it, \
             got: {message}"
        );
    }
}

/// The seam is what a service ends up holding — the wiring, not just the type.
///
/// `create_flight_service` deliberately builds the service with no executor; the
/// policy seam is attached at serve time, when `AppState` first exists. This pins
/// that the attach is possible and observable, so "Flight executes GraphQL" and
/// "Flight enforces policy" cannot come apart silently.
#[tokio::test]
async fn the_policy_seam_attaches_to_a_flight_service() {
    let dir = tempfile::tempdir().unwrap();
    let server = persisted_only_server(&dir).await;

    let mut service = fraiseql_arrow::FraiseQLFlightService::new();
    assert!(
        !service.has_executor(),
        "precondition: a freshly built Flight service refuses GraphQL for want of an executor"
    );

    service.set_executor(super::policy_seam::policy_gated_executor(server.build_app_state()));

    assert!(
        service.has_executor(),
        "after the serve-time attach the Flight service executes GraphQL — through the seam"
    );
}
