//! Tests for `middleware/` modules.
#![allow(unused_imports)] // Reason: blanket re-exports for test convenience

mod admin_scope_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    use fraiseql_error::FraiseQLError;
    use super::super::admin_scope::{ADMIN_SCOPE, has_admin_scope, require_admin_scope};

    #[test]
    fn has_admin_scope_exact_match() {
        assert!(has_admin_scope(ADMIN_SCOPE));
    }

    #[test]
    fn has_admin_scope_in_list() {
        assert!(has_admin_scope("read write fraiseql:admin user:list"));
    }

    #[test]
    fn has_admin_scope_first_in_list() {
        assert!(has_admin_scope("fraiseql:admin read write"));
    }

    #[test]
    fn has_admin_scope_missing() {
        assert!(!has_admin_scope("read write user:list"));
    }

    #[test]
    fn has_admin_scope_empty() {
        assert!(!has_admin_scope(""));
    }

    #[test]
    fn has_admin_scope_partial_match_rejected() {
        // "fraiseql:admin_readonly" should NOT match "fraiseql:admin"
        assert!(!has_admin_scope("fraiseql:admin_readonly"));
    }

    #[test]
    fn require_admin_scope_accepts_valid() {
        require_admin_scope("fraiseql:admin").unwrap();
    }

    #[test]
    fn require_admin_scope_rejects_missing() {
        let err = require_admin_scope("read write").unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Authorization { .. }),
            "Expected Authorization error, got: {err:?}"
        );
        assert!(err.to_string().contains("fraiseql:admin"));
    }

    #[test]
    fn require_admin_scope_rejects_empty() {
        assert!(require_admin_scope("").is_err());
    }
}

mod auth_tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::cast_precision_loss)]
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::cast_possible_truncation)]
    #![allow(clippy::cast_possible_wrap)]
    #![allow(clippy::missing_panics_doc)]
    #![allow(clippy::missing_errors_doc)]
    #![allow(missing_docs)]
    #![allow(clippy::items_after_statements)]

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tower::ServiceExt;

    use super::super::auth::{BearerAuthState, bearer_auth_middleware, constant_time_compare};

    async fn protected_handler() -> &'static str {
        "secret data"
    }

    fn create_test_app(token: &str) -> Router {
        let auth_state = BearerAuthState::new(token.to_string());

        Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
    }

    #[tokio::test]
    async fn test_valid_token_allows_access() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer secret-token-12345")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_missing_auth_header_returns_401() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder().uri("/protected").body(Body::empty()).unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().contains_key("www-authenticate"));
    }

    #[tokio::test]
    async fn test_invalid_auth_format_returns_401() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Basic dXNlcjpwYXNz") // Basic auth, not Bearer
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_wrong_token_returns_403() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_empty_bearer_token_returns_403() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer ")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_constant_time_compare_equal() {
        assert!(constant_time_compare("hello", "hello"));
        assert!(constant_time_compare("", ""));
        assert!(constant_time_compare("a-long-token-123", "a-long-token-123"));
    }

    #[test]
    fn test_constant_time_compare_not_equal() {
        assert!(!constant_time_compare("hello", "world"));
        assert!(!constant_time_compare("hello", "hello!"));
        assert!(!constant_time_compare("hello", "hell"));
        assert!(!constant_time_compare("abc", "abd"));
    }

    #[test]
    fn test_constant_time_compare_different_lengths() {
        assert!(!constant_time_compare("short", "longer-string"));
        assert!(!constant_time_compare("", "notempty"));
    }

    #[test]
    fn test_subtle_compare_identical_tokens() {
        assert!(constant_time_compare("x", "x"));
        assert!(constant_time_compare(
            "super-secret-32-char-admin-token",
            "super-secret-32-char-admin-token"
        ));
    }

    #[test]
    fn test_subtle_compare_off_by_one_byte() {
        assert!(!constant_time_compare("token-abc", "token-abd")); // last byte differs
        assert!(!constant_time_compare("Aoken-abc", "token-abc")); // first byte differs
    }

    #[test]
    fn test_subtle_compare_empty_strings() {
        assert!(constant_time_compare("", ""));
        assert!(!constant_time_compare("", "a"));
        assert!(!constant_time_compare("a", ""));
    }

    // ── S48 brute-force rate limiter tests ──────────────────────────────────

    #[test]
    fn test_failure_limiter_not_blocked_initially() {
        let limiter = super::super::auth::FailureLimiter::new(3);
        assert!(!limiter.is_blocked("1.2.3.4"));
    }

    #[test]
    fn test_failure_limiter_blocks_after_max_failures() {
        let limiter = super::super::auth::FailureLimiter::new(3);
        assert!(!limiter.record_failure("1.2.3.4")); // 1st → not blocked
        assert!(!limiter.record_failure("1.2.3.4")); // 2nd → not blocked
        assert!(limiter.record_failure("1.2.3.4")); // 3rd → now blocked
        assert!(limiter.is_blocked("1.2.3.4"));
    }

    #[test]
    fn test_failure_limiter_success_resets_counter() {
        let limiter = super::super::auth::FailureLimiter::new(3);
        limiter.record_failure("1.2.3.4");
        limiter.record_failure("1.2.3.4");
        assert_eq!(limiter.failure_count("1.2.3.4"), 2);
        limiter.record_success("1.2.3.4");
        assert_eq!(limiter.failure_count("1.2.3.4"), 0);
        assert!(!limiter.is_blocked("1.2.3.4"));
    }

    #[test]
    fn test_failure_limiter_independent_per_ip() {
        let limiter = super::super::auth::FailureLimiter::new(2);
        limiter.record_failure("10.0.0.1");
        limiter.record_failure("10.0.0.1");
        assert!(limiter.is_blocked("10.0.0.1"));
        // Different IP should not be blocked.
        assert!(!limiter.is_blocked("10.0.0.2"));
    }

    #[tokio::test]
    async fn test_middleware_returns_429_after_max_failures() {
        // Use max_failures = 2 so the test does not send too many requests.
        let auth_state = BearerAuthState::with_max_failures("correct-token".to_string(), 2);
        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                auth_state,
                bearer_auth_middleware,
            ));

        // Two bad attempts (from unknown peer since ConnectInfo not wired in tests).
        for _ in 0..2 {
            let req = Request::builder()
                .uri("/protected")
                .header("Authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap();
            let _ = app.clone().oneshot(req).await.unwrap();
        }

        // Third attempt should be 429 (already blocked after 2 failures).
        let req = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_middleware_resets_counter_on_success() {
        let auth_state = BearerAuthState::with_max_failures("good-token".to_string(), 2);
        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                auth_state,
                bearer_auth_middleware,
            ));

        // One bad attempt, then a successful one.
        let bad_req = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer bad-token")
            .body(Body::empty())
            .unwrap();
        let r = app.clone().oneshot(bad_req).await.unwrap();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);

        let good_req = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer good-token")
            .body(Body::empty())
            .unwrap();
        let r = app.clone().oneshot(good_req).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);

        // After success the counter should have been reset; one more bad attempt
        // should be 403, not 429.
        let bad_req2 = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer bad-token")
            .body(Body::empty())
            .unwrap();
        let r = app.oneshot(bad_req2).await.unwrap();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_admin_auth_max_failures_default_is_ten() {
        use crate::server_config::ServerConfig;
        let cfg = ServerConfig::default();
        assert_eq!(cfg.admin_auth_max_failures, 10);
    }
}

mod content_type_tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::cast_precision_loss)]
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::cast_possible_truncation)]
    #![allow(clippy::cast_possible_wrap)]
    #![allow(clippy::missing_panics_doc)]
    #![allow(clippy::missing_errors_doc)]
    #![allow(missing_docs)]
    #![allow(clippy::items_after_statements)]

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode, header::CONTENT_TYPE},
        middleware,
        routing::post,
    };
    use tower::ServiceExt;

    use super::super::content_type::require_json_content_type;

    async fn echo_handler() -> &'static str {
        "ok"
    }

    fn app() -> Router {
        Router::new()
            .route("/graphql", post(echo_handler))
            .layer(middleware::from_fn(require_json_content_type))
    }

    #[tokio::test]
    async fn text_plain_rejected_with_415() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .header(CONTENT_TYPE, "text/plain")
                    .body(Body::from(r#"{"query":"{ __typename }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn form_urlencoded_rejected_with_415() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("query=%7B+__typename+%7D"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn application_json_passes() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"query":"{ __typename }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn application_json_with_charset_passes() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .header(CONTENT_TYPE, "application/json; charset=utf-8")
                    .body(Body::from(r#"{"query":"{ __typename }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_request_passes_without_content_type() {
        let app = Router::new()
            .route("/graphql", axum::routing::get(echo_handler))
            .layer(middleware::from_fn(require_json_content_type));

        let res = app
            .oneshot(Request::get("/graphql").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_content_type_rejected() {
        let res = app()
            .oneshot(
                Request::post("/graphql")
                    .body(Body::from(r#"{"query":"{ __typename }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}

mod cors_tests {
    #![allow(clippy::unwrap_used)]

    use super::super::cors::{cors_layer, cors_layer_restricted, security_headers_middleware};

    #[test]
    fn test_cors_layer_creation() {
        let _layer = cors_layer();
    }

    #[test]
    fn test_cors_layer_restricted() {
        let origins = vec!["https://example.com".to_string()];
        let _layer = cors_layer_restricted(&origins);
    }

    #[test]
    fn test_cors_layer_restricted_empty_origins() {
        let origins = vec![];
        let _layer = cors_layer_restricted(&origins);
    }

    #[test]
    fn test_cors_layer_restricted_invalid_origin() {
        let origins = vec![
            "not-a-valid-url".to_string(),
            "https://valid.com".to_string(),
        ];
        let layer = cors_layer_restricted(&origins);
        let _ = layer;
    }

    use axum::{Router, body::Body, http::Request, middleware, routing::get};
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    fn sec_app() -> Router {
        Router::new()
            .route("/", get(ok_handler))
            .layer(middleware::from_fn(security_headers_middleware))
    }

    #[tokio::test]
    async fn test_security_headers_nosniff_present() {
        let resp = sec_app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.headers().get("x-content-type-options").unwrap(), "nosniff");
    }

    #[tokio::test]
    async fn test_security_headers_frame_options_deny() {
        let resp = sec_app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.headers().get("x-frame-options").unwrap(), "DENY");
    }

    #[tokio::test]
    async fn test_security_headers_xss_protection_zero() {
        let resp = sec_app()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.headers().get("x-xss-protection").unwrap(),
            "0",
            "X-XSS-Protection must be 0 (legacy auditor disabled)"
        );
    }

    #[test]
    fn test_cors_layer_config_comprehensive() {
        let origins = vec![
            "https://example.com".to_string(),
            "https://app.example.com".to_string(),
        ];
        let _ = cors_layer_restricted(&origins);
    }
}

mod header_limits_tests {
    use axum::{Router, body::Body, middleware, routing::get};
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::super::header_limits::header_limits_middleware;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    fn test_app(max_count: usize, max_bytes: usize) -> Router {
        Router::new()
            .route("/", get(ok_handler))
            .layer(middleware::from_fn(move |req, next| {
                header_limits_middleware(req, next, max_count, max_bytes)
            }))
    }

    #[tokio::test]
    async fn accepts_request_within_limits() {
        let app = test_app(10, 4096);
        let req = Request::builder()
            .uri("/")
            .header("x-test", "value")
            .body(Body::empty())
            .expect("Reason: test request builder should not fail");

        let resp = app.oneshot(req).await.expect("Reason: oneshot should not fail in test");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_too_many_headers() {
        let app = test_app(3, 65_536);
        let mut builder = Request::builder().uri("/");
        for i in 0..10 {
            builder = builder.header(format!("x-test-{i}"), "value");
        }
        let req = builder
            .body(Body::empty())
            .expect("Reason: test request builder should not fail");

        let resp = app.oneshot(req).await.expect("Reason: oneshot should not fail in test");
        assert_eq!(resp.status(), StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
    }

    #[tokio::test]
    async fn rejects_headers_too_large() {
        let app = test_app(100, 64); // 64-byte total limit
        let req = Request::builder()
            .uri("/")
            .header("x-large", "a]".repeat(100))
            .body(Body::empty())
            .expect("Reason: test request builder should not fail");

        let resp = app.oneshot(req).await.expect("Reason: oneshot should not fail in test");
        assert_eq!(resp.status(), StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
    }

    #[tokio::test]
    async fn accepts_at_exact_count_limit() {
        let app = test_app(5, 65_536);
        let mut builder = Request::builder().uri("/");
        for i in 0..5 {
            builder = builder.header(format!("x-h-{i}"), "v");
        }
        let req = builder
            .body(Body::empty())
            .expect("Reason: test request builder should not fail");

        let resp = app.oneshot(req).await.expect("Reason: oneshot should not fail in test");
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

mod hs256_auth_tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::Arc;

    use fraiseql_core::security::{AuthConfig, AuthMiddleware};

    use super::super::hs256_auth::{Hs256AuthState, hs256_auth_middleware as _};

    #[test]
    fn hs256_auth_state_is_cloneable() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<Hs256AuthState>();

        let mw = AuthMiddleware::from_config(AuthConfig::with_hs256("test-secret-123"));
        let _state = Hs256AuthState::new(Arc::new(mw), "test".to_string());
    }
}

mod metrics_tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::cast_precision_loss)]
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::cast_possible_truncation)]
    #![allow(clippy::cast_possible_wrap)]
    #![allow(clippy::missing_panics_doc)]
    #![allow(clippy::missing_errors_doc)]
    #![allow(missing_docs)]
    #![allow(clippy::items_after_statements)]

    use std::sync::{Arc, atomic::Ordering};

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tower::ServiceExt;

    use crate::metrics_server::MetricsCollector;
    use super::super::metrics::metrics_middleware;

    async fn ok_handler() -> StatusCode {
        StatusCode::OK
    }

    async fn bad_request_handler() -> StatusCode {
        StatusCode::BAD_REQUEST
    }

    async fn internal_error_handler() -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    #[tokio::test]
    async fn test_metrics_middleware_counts_requests() {
        let metrics = Arc::new(MetricsCollector::new());

        let app = Router::new()
            .route("/ok", get(ok_handler))
            .layer(middleware::from_fn_with_state(metrics.clone(), metrics_middleware));

        let request = Request::builder().uri("/ok").body(Body::empty()).unwrap();

        let _response = app.oneshot(request).await.unwrap();

        assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http_responses_2xx.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_metrics_middleware_tracks_4xx() {
        let metrics = Arc::new(MetricsCollector::new());

        let app = Router::new()
            .route("/bad", get(bad_request_handler))
            .layer(middleware::from_fn_with_state(metrics.clone(), metrics_middleware));

        let request = Request::builder().uri("/bad").body(Body::empty()).unwrap();

        let _response = app.oneshot(request).await.unwrap();

        assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http_responses_4xx.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http_responses_2xx.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_metrics_middleware_tracks_5xx() {
        let metrics = Arc::new(MetricsCollector::new());

        let app = Router::new()
            .route("/error", get(internal_error_handler))
            .layer(middleware::from_fn_with_state(metrics.clone(), metrics_middleware));

        let request = Request::builder().uri("/error").body(Body::empty()).unwrap();

        let _response = app.oneshot(request).await.unwrap();

        assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http_responses_5xx.load(Ordering::Relaxed), 1);
    }
}

mod oidc_auth_tests {
    #![allow(clippy::unwrap_used)]

    use axum::http::header;

    use super::super::oidc_auth::{AuthUser, OidcAuthState, extract_access_token_cookie};

    #[test]
    fn test_auth_user_clone() {
        use chrono::Utc;
        use fraiseql_core::security::AuthenticatedUser;

        let user = AuthenticatedUser {
            user_id:      fraiseql_core::types::UserId::new("user123"),
            scopes:       vec!["read".to_string()],
            expires_at:   Utc::now(),
            email:        None,
            display_name: None,
            extra_claims: std::collections::HashMap::new(),
        };

        let auth_user = AuthUser(user);
        let cloned = auth_user.clone();

        assert_eq!(auth_user.0.user_id, cloned.0.user_id);
    }

    #[test]
    fn test_oidc_auth_state_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<OidcAuthState>();
    }

    #[test]
    fn test_cookie_fallback_extracts_token() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "__Host-access_token=my.jwt.token; Path=/; SameSite=Strict".parse().unwrap(),
        );

        let token = extract_access_token_cookie(&headers);
        assert_eq!(token.as_deref(), Some("my.jwt.token"));
    }

    #[test]
    fn test_cookie_fallback_strips_rfc6265_quotes() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::COOKIE, "__Host-access_token=\"my.jwt.token\"".parse().unwrap());

        let token = extract_access_token_cookie(&headers);
        assert_eq!(token.as_deref(), Some("my.jwt.token"));
    }

    #[test]
    fn test_cookie_fallback_absent_returns_none() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::COOKIE, "session=abc; other=xyz".parse().unwrap());

        let token = extract_access_token_cookie(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_cookie_fallback_no_cookie_header_returns_none() {
        let headers = axum::http::HeaderMap::new();
        let token = extract_access_token_cookie(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_cookie_fallback_multiple_cookies_finds_correct_one() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "session=abc; __Host-access_token=correct.token; csrf=xyz".parse().unwrap(),
        );

        let token = extract_access_token_cookie(&headers);
        assert_eq!(token.as_deref(), Some("correct.token"));
    }
}

mod tenant_tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::cast_precision_loss)]
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::cast_possible_truncation)]
    #![allow(clippy::cast_possible_wrap)]
    #![allow(clippy::missing_panics_doc)]
    #![allow(clippy::missing_errors_doc)]
    #![allow(missing_docs)]
    #![allow(clippy::items_after_statements)]

    use super::super::tenant::TenantContext;

    #[test]
    fn test_tenant_context_scoped() {
        let ctx = TenantContext {
            org_id: Some("org-123".to_string()),
        };
        assert!(ctx.is_tenant_scoped());
        assert_eq!(ctx.get_org_id(), Some("org-123"));
    }

    #[test]
    fn test_tenant_context_unscoped() {
        let ctx = TenantContext { org_id: None };
        assert!(!ctx.is_tenant_scoped());
        assert_eq!(ctx.get_org_id(), None);
    }

    #[test]
    fn test_require_org_id_success() {
        let ctx = TenantContext {
            org_id: Some("org-123".to_string()),
        };
        assert_eq!(ctx.require_org_id().unwrap(), "org-123");
    }

    #[test]
    fn test_require_org_id_failure() {
        let ctx = TenantContext { org_id: None };
        assert!(
            ctx.require_org_id().is_err(),
            "expected Err when org_id is None, got: {:?}",
            ctx.require_org_id()
        );
        assert_eq!(
            ctx.require_org_id().unwrap_err(),
            "Request must be tenant-scoped (missing org_id)"
        );
    }
}

mod trace_tests {
    use super::super::trace::trace_layer;

    #[test]
    fn test_trace_layer_creation() {
        let _layer = trace_layer();
    }
}
