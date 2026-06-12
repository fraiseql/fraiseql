//! Tests for top-level `routes/` modules.
#![allow(unused_imports)]

mod auth_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use axum::{
        Extension, Router,
        body::Body,
        http::{Request, StatusCode, header},
        routing::get,
    };
    use chrono::Utc;
    use tower::ServiceExt as _;

    use super::super::auth::*;
    use crate::{
        auth::{OidcServerClient, PkceStateStore},
        middleware::AuthUser,
    };

    fn mock_pkce_store() -> Arc<PkceStateStore> {
        Arc::new(PkceStateStore::new(600, None))
    }

    fn make_auth_user(
        user_id: &str,
        extra: std::collections::HashMap<String, serde_json::Value>,
    ) -> AuthUser {
        AuthUser(fraiseql_core::security::AuthenticatedUser {
            user_id:      fraiseql_core::types::UserId::new(user_id),
            scopes:       vec![],
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: extra,
        })
    }

    fn make_auth_user_with_identity(
        user_id: &str,
        email: Option<&str>,
        display_name: Option<&str>,
        extra: std::collections::HashMap<String, serde_json::Value>,
    ) -> AuthUser {
        AuthUser(fraiseql_core::security::AuthenticatedUser {
            user_id:      fraiseql_core::types::UserId::new(user_id),
            scopes:       vec![],
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        email.map(str::to_owned),
            display_name: display_name.map(str::to_owned),
            extra_claims: extra,
        })
    }

    fn make_me_state(expose_claims: Vec<&str>) -> Arc<AuthMeState> {
        Arc::new(AuthMeState {
            expose_claims: expose_claims.into_iter().map(str::to_owned).collect(),
        })
    }

    fn mock_oidc_client() -> Arc<OidcServerClient> {
        Arc::new(OidcServerClient::new(
            "test-client",
            "test-secret",
            "https://api.example.com/auth/callback",
            "https://provider.example.com/authorize",
            "https://provider.example.com/token",
        ))
    }

    fn auth_router() -> Router {
        let auth_state = Arc::new(AuthPkceState {
            pkce_store:              mock_pkce_store(),
            oidc_client:             mock_oidc_client(),
            http_client:             Arc::new(reqwest::Client::new()),
            post_login_redirect_uri: None,
        });
        Router::new()
            .route("/auth/start", get(auth_start))
            .route("/auth/callback", get(auth_callback))
            .with_state(auth_state)
    }

    #[tokio::test]
    async fn test_auth_me_always_returns_sub_user_id_expires_at() {
        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user("user-123", std::collections::HashMap::new())))
            .with_state(make_me_state(vec![]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["sub"], "user-123");
        assert_eq!(json["user_id"], "user-123");
        assert!(json["expires_at"].is_string(), "expires_at must be present");
    }

    #[tokio::test]
    async fn test_auth_me_expose_claims_filters_correctly() {
        let mut extra = std::collections::HashMap::new();
        extra.insert("email".to_owned(), serde_json::json!("alice@example.com"));
        extra.insert("https://myapp.com/role".to_owned(), serde_json::json!("admin"));

        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user("alice", extra)))
            .with_state(make_me_state(vec!["email"]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["email"], "alice@example.com", "listed claim must appear");
        assert!(json.get("https://myapp.com/role").is_none(), "unlisted claim must be absent");
    }

    #[tokio::test]
    async fn test_auth_me_claim_absent_from_token_silently_omitted() {
        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user("user-x", std::collections::HashMap::new())))
            .with_state(make_me_state(vec!["tenant_id"]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json.get("tenant_id").is_none(), "absent claim must not be null-padded");
        assert_eq!(json["sub"], "user-x");
    }

    #[tokio::test]
    async fn test_auth_me_namespaced_claim_in_expose_claims() {
        let mut extra = std::collections::HashMap::new();
        extra.insert("https://myapp.com/role".to_owned(), serde_json::json!("editor"));

        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user("user-y", extra)))
            .with_state(make_me_state(vec!["https://myapp.com/role"]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["https://myapp.com/role"], "editor");
    }

    #[tokio::test]
    async fn test_auth_me_returns_email_and_display_name() {
        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user_with_identity(
                "user-z",
                Some("user@corp.com"),
                Some("Jane Doe"),
                std::collections::HashMap::new(),
            )))
            .with_state(make_me_state(vec![]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["email"], "user@corp.com", "email must be a flat string");
        assert_eq!(json["display_name"], "Jane Doe", "display_name must be a flat string");
    }

    #[tokio::test]
    async fn test_auth_me_omits_null_email_and_display_name() {
        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user_with_identity(
                "user-w",
                None,
                None,
                std::collections::HashMap::new(),
            )))
            .with_state(make_me_state(vec![]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json.get("email").is_none(), "absent email must not be null-padded");
        assert!(
            json.get("display_name").is_none(),
            "absent display_name must not be null-padded"
        );
    }

    #[tokio::test]
    async fn test_auth_start_redirects_with_pkce_params() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/start?redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert!(resp.status().is_redirection(), "expected redirect, got {}", resp.status());
        let location = resp
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .expect("Location header must be present");

        assert!(location.contains("response_type=code"), "missing response_type");
        assert!(location.contains("code_challenge="), "missing code_challenge");
        assert!(location.contains("code_challenge_method=S256"), "missing challenge method");
        assert!(location.contains("state="), "missing state param");
        assert!(location.contains("client_id=test-client"), "missing client_id");
    }

    #[tokio::test]
    async fn test_auth_start_missing_redirect_uri_returns_400() {
        let app = auth_router();
        let req = Request::builder().uri("/auth/start").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(
            resp.status().is_client_error(),
            "missing redirect_uri must be a client error, got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn test_auth_callback_unknown_state_returns_400() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?code=abc&state=completely-unknown-state")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].is_string(), "error field must be a string: {json}");
    }

    #[tokio::test]
    async fn test_auth_callback_missing_code_returns_400() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?state=some-state-no-code")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_auth_start_oversized_redirect_uri_returns_400() {
        let app = auth_router();
        let long_uri = "https://example.com/".to_string() + &"a".repeat(2100);
        let encoded = urlencoding::encode(&long_uri);
        let req = Request::builder()
            .uri(format!("/auth/start?redirect_uri={encoded}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"].as_str().unwrap_or("").contains("maximum length"),
            "error must mention length: {json}"
        );
    }

    #[tokio::test]
    async fn test_auth_callback_oidc_error_returns_mapped_message() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?error=access_denied&error_description=internal+tenant+info")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error_msg = json["error"].as_str().unwrap_or("");
        assert!(
            !error_msg.contains("internal tenant info"),
            "provider description must not be reflected to client: {error_msg}"
        );
        assert_eq!(error_msg, "Access was denied");
    }

    #[tokio::test]
    async fn test_auth_callback_unknown_oidc_error_returns_generic_message() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?error=unknown_vendor_error&error_description=secret+details")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"].as_str().unwrap_or(""), "Authorization failed");
    }

    #[tokio::test]
    async fn test_auth_callback_oidc_error_no_description_uses_fallback() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?error=access_denied")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"].as_str().unwrap_or(""), "Access was denied");
    }

    #[tokio::test]
    async fn test_auth_start_to_callback_state_roundtrip_with_encryption() {
        use crate::auth::{EncryptionAlgorithm, StateEncryptionService};

        let enc = Arc::new(StateEncryptionService::from_raw_key(
            &[0u8; 32],
            EncryptionAlgorithm::Chacha20Poly1305,
        ));
        let pkce_store = Arc::new(PkceStateStore::new(600, Some(enc)));

        let auth_state = Arc::new(AuthPkceState {
            pkce_store,
            oidc_client: mock_oidc_client(),
            http_client: Arc::new(reqwest::Client::new()),
            post_login_redirect_uri: None,
        });

        let app = Router::new()
            .route("/auth/start", get(auth_start))
            .route("/auth/callback", get(auth_callback))
            .with_state(auth_state);

        let req = Request::builder()
            .uri("/auth/start?redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();

        assert!(
            resp.status().is_redirection(),
            "expected redirect from /auth/start, got {}",
            resp.status(),
        );

        let location = resp
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .expect("Location header must be set")
            .to_string();

        let parsed_location =
            reqwest::Url::parse(&location).expect("Location header must be a valid URL");
        let state_token = parsed_location
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .expect("state= must appear in the redirect Location URL");

        assert!(!state_token.is_empty(), "extracted state token must not be empty");

        let callback_uri = format!("/auth/callback?code=test_code&state={state_token}");
        let req2 = Request::builder().uri(&callback_uri).body(Body::empty()).unwrap();
        let resp2 = app.clone().oneshot(req2).await.unwrap();

        assert_ne!(
            resp2.status(),
            StatusCode::BAD_REQUEST,
            "state from /auth/start must be accepted by /auth/callback; \
             400 means the PKCE state was not found or decryption failed",
        );
        assert_eq!(
            resp2.status(),
            StatusCode::BAD_GATEWAY,
            "token exchange should fail 502 (no real OIDC provider); got {}",
            resp2.status(),
        );
    }
}

mod revoke_tests {
    //! Handler-level authorization tests for `POST /auth/revoke` and
    //! `POST /auth/revoke-all` (issue #358).
    //!
    //! Routing-level "401 without auth" is enforced by mounting the routes
    //! behind `oidc_auth_middleware` in `server/routing/auth.rs`; the
    //! handler tests below cover the additional in-handler checks:
    //!
    //! - `revoke_token` revokes the bearer-token's own `jti` (not an attacker-supplied body token).
    //! - `revoke_all_tokens` enforces caller-`sub` == `body.sub` unless the caller holds the
    //!   `admin` scope.

    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use std::sync::Arc;

    use axum::{
        Extension, Router,
        body::Body,
        http::{Request, StatusCode, header},
        routing::post,
    };
    use chrono::Utc;
    use tower::ServiceExt as _;

    use super::super::auth::{RevocationRouteState, revoke_all_tokens, revoke_token};
    use crate::{
        middleware::{AuthUser, SessionJti},
        token_revocation::{InMemoryRevocationStore, TokenRevocationManager},
    };

    fn make_manager() -> Arc<TokenRevocationManager> {
        Arc::new(TokenRevocationManager::new(
            Arc::new(InMemoryRevocationStore::new()),
            true,
            false,
            3600,
        ))
    }

    fn make_auth_user(sub: &str, scopes: Vec<&str>) -> AuthUser {
        AuthUser(fraiseql_core::security::AuthenticatedUser {
            user_id:      fraiseql_core::types::UserId::new(sub),
            scopes:       scopes.into_iter().map(str::to_owned).collect(),
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: std::collections::HashMap::new(),
        })
    }

    fn revoke_app(manager: Arc<TokenRevocationManager>) -> Router {
        let state = Arc::new(RevocationRouteState {
            revocation_manager: manager,
        });
        Router::new()
            .route("/auth/revoke", post(revoke_token))
            .route("/auth/revoke-all", post(revoke_all_tokens))
            .with_state(state)
    }

    fn post_json(uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .method("POST")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn revoke_token_uses_session_jti_not_body() {
        let manager = make_manager();
        let app = revoke_app(Arc::clone(&manager));

        // Authenticated as "alice" with jti "alice-jti-1". The request body's
        // token field carries an attacker-controlled stub that would have
        // revoked "victim-jti" under the old insecure_decode design.
        let attacker_body = r#"{"token":"eyJhbGciOiJub25lIn0.eyJqdGkiOiJ2aWN0aW0tanRpIn0."}"#;
        let req = post_json("/auth/revoke", attacker_body);

        let resp = app
            .layer(Extension(make_auth_user("alice", vec![])))
            .layer(Extension(SessionJti(Some("alice-jti-1".to_string()))))
            .oneshot(req)
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK, "revoke should succeed");
        assert!(
            manager.check_token(Some("alice-jti-1"), "alice", Some(1000)).await.is_err(),
            "alice's bearer-token jti must be the one revoked"
        );
        assert!(
            manager.check_token(Some("victim-jti"), "alice", Some(1000)).await.is_ok(),
            "attacker-supplied body.token jti must NOT be revoked"
        );
    }

    #[tokio::test]
    async fn revoke_token_returns_409_when_token_has_no_jti() {
        let manager = make_manager();
        let app = revoke_app(manager);

        let req = post_json("/auth/revoke", r"{}");
        let resp = app
            .layer(Extension(make_auth_user("alice", vec![])))
            .layer(Extension(SessionJti(None)))
            .oneshot(req)
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn revoke_all_self_records_epoch() {
        let manager = make_manager();
        let app = revoke_app(Arc::clone(&manager));

        // No epoch before the call.
        assert!(
            manager.user_revoked_after("alice").await.unwrap().is_none(),
            "no revoke-all epoch before the request"
        );

        let req = post_json("/auth/revoke-all", r#"{"sub":"alice"}"#);
        let resp = app
            .layer(Extension(make_auth_user("alice", vec![])))
            .oneshot(req)
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        // The route actually recorded an epoch (pre-fix it deleted 0 sub-keyed rows).
        assert!(
            manager.user_revoked_after("alice").await.unwrap().is_some(),
            "revoke-all must record a per-user epoch"
        );
    }

    #[tokio::test]
    async fn revoke_all_cross_user_without_admin_scope_returns_403() {
        let manager = make_manager();
        let app = revoke_app(Arc::clone(&manager));

        let req = post_json("/auth/revoke-all", r#"{"sub":"bob"}"#);
        let resp = app
            .layer(Extension(make_auth_user("alice", vec!["read", "write"])))
            .oneshot(req)
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "alice without admin scope must not revoke bob's sessions"
        );
    }

    #[tokio::test]
    async fn revoke_all_cross_user_with_admin_scope_succeeds() {
        let manager = make_manager();
        let app = revoke_app(Arc::clone(&manager));

        let req = post_json("/auth/revoke-all", r#"{"sub":"bob"}"#);
        let resp = app
            .layer(Extension(make_auth_user("alice", vec!["admin"])))
            .oneshot(req)
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn revoke_all_empty_body_sub_returns_400() {
        let manager = make_manager();
        let app = revoke_app(manager);

        let req = post_json("/auth/revoke-all", r#"{"sub":""}"#);
        let resp = app
            .layer(Extension(make_auth_user("alice", vec!["admin"])))
            .oneshot(req)
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}

mod health_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::super::health::*;

    #[test]
    fn test_determine_status_all_healthy() {
        #[cfg(feature = "federation")]
        assert_eq!(determine_status(true, None, None, None), "healthy");
        #[cfg(not(feature = "federation"))]
        assert_eq!(determine_status(true, None, None), "healthy");
    }

    #[test]
    fn test_determine_status_db_down_is_unhealthy() {
        #[cfg(feature = "federation")]
        assert_eq!(determine_status(false, None, None, None), "unhealthy");
        #[cfg(not(feature = "federation"))]
        assert_eq!(determine_status(false, None, None), "unhealthy");
    }

    #[test]
    fn test_determine_status_observers_not_running_is_degraded() {
        let observers = Some(ObserverHealth {
            running:        false,
            pending_events: 0,
            last_error:     None,
        });
        #[cfg(feature = "federation")]
        assert_eq!(determine_status(true, observers.as_ref(), None, None), "degraded");
        #[cfg(not(feature = "federation"))]
        assert_eq!(determine_status(true, observers.as_ref(), None), "degraded");
    }

    #[test]
    fn test_determine_status_secrets_disconnected_is_degraded() {
        let secrets = Some(SecretsHealth {
            connected: false,
            backend:   "vault".to_string(),
        });
        #[cfg(feature = "federation")]
        assert_eq!(determine_status(true, None, secrets.as_ref(), None), "degraded");
        #[cfg(not(feature = "federation"))]
        assert_eq!(determine_status(true, None, secrets.as_ref()), "degraded");
    }

    #[cfg(feature = "federation")]
    #[test]
    fn test_determine_status_federation_circuit_open_is_degraded() {
        use crate::federation::circuit_breaker::{CircuitHealthState, SubgraphCircuitHealth};

        let federation = Some(FederationHealth {
            configured: true,
            subgraphs:  vec![SubgraphCircuitHealth {
                subgraph: "Product".to_string(),
                state:    CircuitHealthState::Open,
            }],
        });
        assert_eq!(determine_status(true, None, None, federation.as_ref()), "degraded");
    }

    #[test]
    fn test_determine_status_db_down_overrides_degraded() {
        let secrets = Some(SecretsHealth {
            connected: false,
            backend:   "vault".to_string(),
        });
        #[cfg(feature = "federation")]
        assert_eq!(determine_status(false, None, secrets.as_ref(), None), "unhealthy");
        #[cfg(not(feature = "federation"))]
        assert_eq!(determine_status(false, None, secrets.as_ref()), "unhealthy");
    }

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            database: DatabaseStatus {
                connected:          true,
                database_type:      "PostgreSQL".to_string(),
                active_connections: Some(2),
                idle_connections:   Some(8),
            },
            observers: None,
            cache: None,
            secrets: None,
            #[cfg(feature = "federation")]
            federation: None,
            version: "2.0.0-a1".to_string(),
            schema_hash: Some("abc123def456abc1".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("PostgreSQL"));
    }

    #[cfg(feature = "federation")]
    #[test]
    fn test_health_response_omits_federation_when_none() {
        let response = HealthResponse {
            status:      "healthy".to_string(),
            database:    DatabaseStatus {
                connected:          true,
                database_type:      "PostgreSQL".to_string(),
                active_connections: None,
                idle_connections:   None,
            },
            observers:   None,
            cache:       None,
            secrets:     None,
            federation:  None,
            version:     "2.0.0".to_string(),
            schema_hash: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("federation"), "federation key must be absent when field is None");
    }

    #[cfg(feature = "federation")]
    #[test]
    fn test_health_response_includes_federation_when_present() {
        use crate::federation::circuit_breaker::{CircuitHealthState, SubgraphCircuitHealth};

        let response = HealthResponse {
            status:      "healthy".to_string(),
            database:    DatabaseStatus {
                connected:          true,
                database_type:      "PostgreSQL".to_string(),
                active_connections: None,
                idle_connections:   None,
            },
            observers:   None,
            cache:       None,
            secrets:     None,
            federation:  Some(FederationHealth {
                configured: true,
                subgraphs:  vec![SubgraphCircuitHealth {
                    subgraph: "Product".to_string(),
                    state:    CircuitHealthState::Open,
                }],
            }),
            version:     "2.0.0".to_string(),
            schema_hash: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("federation"), "federation key must be present");
        assert!(json.contains("configured"), "configured field must appear");
        assert!(json.contains("Product"), "subgraph name must appear");
        assert!(json.contains("open"), "circuit state must be serialised as snake_case");
    }
}

mod introspection_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use super::super::introspection::TypeInfo;

    #[test]
    fn test_type_info_serialization() {
        let type_info = TypeInfo {
            name:        "User".to_string(),
            description: Some("A user in the system".to_string()),
            field_count: 3,
        };

        let json = serde_json::to_string(&type_info).unwrap();
        assert!(json.contains("User"));
        assert!(json.contains("field_count"));
    }
}

mod metrics_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use super::super::metrics::MetricsResponse;

    #[test]
    fn test_metrics_response_structure() {
        let response = MetricsResponse {
            queries_total:           1000,
            queries_success:         950,
            queries_error:           50,
            avg_query_duration_ms:   12.5,
            cache_hit_ratio:         0.75,
            pool_connections_total:  20,
            pool_connections_idle:   15,
            pool_connections_active: 5,
            pool_requests_waiting:   0,
        };

        assert_eq!(response.queries_total, 1000);
        assert_eq!(response.queries_success, 950);
        assert_eq!(response.queries_error, 50);
        assert!((response.avg_query_duration_ms - 12.5).abs() < 0.001);
        assert!((response.cache_hit_ratio - 0.75).abs() < 0.001);
        assert_eq!(response.pool_connections_total, 20);
        assert_eq!(response.pool_connections_idle, 15);
        assert_eq!(response.pool_connections_active, 5);
        assert_eq!(response.pool_requests_waiting, 0);
    }

    #[test]
    fn test_metrics_response_serialization() {
        let response = MetricsResponse {
            queries_total:           100,
            queries_success:         95,
            queries_error:           5,
            avg_query_duration_ms:   5.0,
            cache_hit_ratio:         0.85,
            pool_connections_total:  10,
            pool_connections_idle:   8,
            pool_connections_active: 2,
            pool_requests_waiting:   0,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("queries_total"));
        assert!(json.contains("100"));
        assert!(json.contains("queries_success"));
        assert!(json.contains("pool_connections_total"));
        assert!(json.contains("pool_connections_idle"));
        assert!(json.contains("pool_connections_active"));
        assert!(json.contains("pool_requests_waiting"));
    }
}

mod playground_tests {
    use super::super::playground::{PlaygroundState, apollo_sandbox_html, graphiql_html};
    use crate::server_config::PlaygroundTool;

    #[test]
    fn test_graphiql_html_contains_endpoint() {
        let html = graphiql_html("/graphql");
        assert!(html.contains("/graphql"));
        assert!(html.contains("GraphiQL"));
        assert!(html.contains("graphiql.min.js"));
    }

    #[test]
    fn test_apollo_sandbox_html_contains_endpoint() {
        let html = apollo_sandbox_html("/graphql");
        assert!(html.contains("/graphql"));
        assert!(html.contains("EmbeddedSandbox"));
        assert!(html.contains("embeddable-sandbox.umd.production.min.js"));
    }

    #[test]
    fn test_playground_state_new() {
        let state = PlaygroundState::new("/graphql", PlaygroundTool::ApolloSandbox);
        assert_eq!(state.graphql_endpoint, "/graphql");
        assert_eq!(state.tool, PlaygroundTool::ApolloSandbox);
    }
}

mod realtime_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::post,
    };
    use tower::ServiceExt;

    use super::super::realtime::{BroadcastState, broadcast_handler};
    use crate::subscriptions::{BroadcastConfig, BroadcastManager};

    fn test_app() -> Router {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));
        let state = BroadcastState::new(manager);
        Router::new()
            .route("/realtime/v1/broadcast", post(broadcast_handler))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_broadcast_publish_ok() {
        let app = test_app();

        let body = serde_json::json!({
            "channel": "room:1",
            "event": "message",
            "payload": {"text": "hello"}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/realtime/v1/broadcast")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["receivers"], 0);
    }

    #[tokio::test]
    async fn test_broadcast_empty_channel_rejected() {
        let app = test_app();

        let body = serde_json::json!({
            "channel": "",
            "event": "message",
            "payload": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/realtime/v1/broadcast")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_broadcast_empty_event_rejected() {
        let app = test_app();

        let body = serde_json::json!({
            "channel": "room:1",
            "event": "",
            "payload": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/realtime/v1/broadcast")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_broadcast_with_subscriber() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));
        let state = BroadcastState::new(manager.clone());
        let app = Router::new()
            .route("/realtime/v1/broadcast", post(broadcast_handler))
            .with_state(state);

        let _rx = manager.subscribe("room:1").await.unwrap();

        let body = serde_json::json!({
            "channel": "room:1",
            "event": "message",
            "payload": {"text": "hello"}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/realtime/v1/broadcast")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["receivers"], 1);
    }
}

mod subscriptions_tests {
    use super::super::subscriptions::extract_subscription_name;

    #[test]
    fn test_extract_subscription_name_simple() {
        let query = "subscription { orderCreated { id } }";
        assert_eq!(extract_subscription_name(query), Some("orderCreated".to_string()));
    }

    #[test]
    fn test_extract_subscription_name_with_operation() {
        let query = "subscription OnOrderCreated { orderCreated { id amount } }";
        assert_eq!(extract_subscription_name(query), Some("orderCreated".to_string()));
    }

    #[test]
    fn test_extract_subscription_name_with_variables() {
        let query = "subscription ($userId: ID!) { userUpdated(userId: $userId) { id name } }";
        assert_eq!(extract_subscription_name(query), Some("userUpdated".to_string()));
    }

    #[test]
    fn test_extract_subscription_name_whitespace() {
        let query = r"
            subscription {
                orderCreated {
                    id
                }
            }
        ";
        assert_eq!(extract_subscription_name(query), Some("orderCreated".to_string()));
    }

    #[test]
    fn test_extract_subscription_name_invalid() {
        assert_eq!(extract_subscription_name("query { users { id } }"), None);
        assert_eq!(extract_subscription_name("{ users { id } }"), None);
        assert_eq!(extract_subscription_name("subscription { }"), None);
    }

    fn tenant_matches_logic(conn_tenant: Option<&str>, evt_tenant: Option<&str>) -> bool {
        match (conn_tenant, evt_tenant) {
            (Some(conn_tid), Some(evt_tid)) => conn_tid == evt_tid,
            _ => true,
        }
    }

    #[test]
    fn event_dispatch_tenant_filter_same_tenant_passes() {
        assert!(tenant_matches_logic(Some("tenant-a"), Some("tenant-a")));
    }

    #[test]
    fn event_dispatch_tenant_filter_different_tenant_blocks() {
        assert!(!tenant_matches_logic(Some("tenant-a"), Some("tenant-b")));
    }

    #[test]
    fn event_dispatch_tenant_filter_no_connection_tenant_passes() {
        assert!(tenant_matches_logic(None, Some("tenant-a")));
        assert!(tenant_matches_logic(None, None));
    }

    #[test]
    fn event_dispatch_tenant_filter_no_event_tenant_passes() {
        assert!(tenant_matches_logic(Some("tenant-a"), None));
    }
}
