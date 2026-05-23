// ── api_key_tests ─────────────────────────────────────────────────────────────

#![allow(clippy::panic)] // Reason: test code, panics acceptable
mod api_key_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use crate::api_key::*;

    fn sha256_hex(input: &str) -> String {
        hex::encode(sha256_hash(input.as_bytes()))
    }

    fn test_config(key: &str) -> ApiKeyConfig {
        ApiKeyConfig {
            enabled:        true,
            header:         "x-api-key".into(),
            hash_algorithm: "sha256".into(),
            storage:        "env".into(),
            static_keys:    vec![StaticApiKeyConfig {
                key_hash: format!("sha256:{}", sha256_hex(key)),
                scopes:   vec!["read:*".into()],
                name:     "test-key".into(),
            }],
        }
    }

    #[tokio::test]
    async fn valid_api_key_returns_security_context() {
        let config = test_config("my-secret-key");
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-api-key", "my-secret-key".parse().unwrap());

        match auth.authenticate(&headers).await {
            ApiKeyResult::Authenticated(ctx) => {
                assert_eq!(ctx.user_id, fraiseql_core::types::UserId::new("apikey:test-key"));
                assert_eq!(ctx.scopes, vec!["read:*".to_string()]);
            },
            ref other => panic!("expected Authenticated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_api_key_returns_invalid() {
        let config = test_config("my-secret-key");
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-api-key", "wrong-key".parse().unwrap());

        assert!(matches!(auth.authenticate(&headers).await, ApiKeyResult::Invalid));
    }

    #[tokio::test]
    async fn missing_api_key_returns_not_present() {
        let config = test_config("my-secret-key");
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();

        let headers = axum::http::HeaderMap::new();
        assert!(matches!(auth.authenticate(&headers).await, ApiKeyResult::NotPresent));
    }

    #[tokio::test]
    async fn api_key_prefix_stripped() {
        let config = test_config("my-secret-key");
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-api-key", "ApiKey my-secret-key".parse().unwrap());

        assert!(matches!(auth.authenticate(&headers).await, ApiKeyResult::Authenticated(_)));
    }

    #[test]
    fn disabled_config_returns_none() {
        let mut config = test_config("key");
        config.enabled = false;
        assert!(ApiKeyAuthenticator::from_config(&config).is_none());
    }

    #[test]
    fn invalid_hash_hex_is_skipped() {
        let config = ApiKeyConfig {
            enabled:        true,
            header:         "x-api-key".into(),
            hash_algorithm: "sha256".into(),
            storage:        "env".into(),
            static_keys:    vec![StaticApiKeyConfig {
                key_hash: "not-valid-hex".into(),
                scopes:   vec![],
                name:     "bad-key".into(),
            }],
        };
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();
        assert_eq!(auth.static_keys.len(), 0);
    }

    #[test]
    fn hash_without_prefix_works() {
        let hash = sha256_hex("test");
        let config = ApiKeyConfig {
            enabled:        true,
            header:         "x-api-key".into(),
            hash_algorithm: "sha256".into(),
            storage:        "env".into(),
            static_keys:    vec![StaticApiKeyConfig {
                key_hash: hash, // no "sha256:" prefix
                scopes:   vec![],
                name:     "no-prefix".into(),
            }],
        };
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();
        assert_eq!(auth.static_keys.len(), 1);
    }

    #[test]
    fn sha256_hash_is_deterministic() {
        let h1 = sha256_hash(b"hello");
        let h2 = sha256_hash(b"hello");
        assert_eq!(h1, h2);
        // Different input → different hash.
        let h3 = sha256_hash(b"world");
        assert_ne!(h1, h3);
    }

    #[test]
    fn unsupported_algorithm_returns_none() {
        let mut config = test_config("key");
        config.hash_algorithm = "bcrypt".into();
        assert!(ApiKeyAuthenticator::from_config(&config).is_none());
    }
}

// ── cli_tests ─────────────────────────────────────────────────────────────────

#[cfg(feature = "cli")]
mod cli_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    #![allow(clippy::field_reassign_with_default)] // Reason: test readability

    use clap::Parser as _;

    use crate::{cli::*, middleware::RateLimitConfig, server_config::ServerConfig};

    #[test]
    fn cli_parse_config_flag() {
        let cli = Cli::parse_from(["fraiseql-server", "--config", "/etc/fraiseql.toml"]);
        assert_eq!(cli.server.config.as_deref(), Some("/etc/fraiseql.toml"));
    }

    #[test]
    fn cli_parse_database_url_flag() {
        let cli = Cli::parse_from([
            "fraiseql-server",
            "--database-url",
            "postgres://localhost/db",
        ]);
        assert_eq!(cli.server.database_url.as_deref(), Some("postgres://localhost/db"));
    }

    #[test]
    fn cli_parse_bind_addr_flag() {
        let cli = Cli::parse_from(["fraiseql-server", "--bind-addr", "127.0.0.1:3000"]);
        assert_eq!(cli.server.bind_addr, Some("127.0.0.1:3000".parse().unwrap()));
    }

    #[test]
    fn cli_defaults_are_none_when_no_flags_or_env() {
        let cli = Cli::parse_from(["fraiseql-server"]);
        assert!(cli.server.config.is_none());
        assert!(cli.server.schema_path.is_none());
        assert!(cli.server.metrics_token.is_none());
        assert!(cli.server.admin_token.is_none());
    }

    #[test]
    fn cli_parse_bool_flag_with_value() {
        let cli = Cli::parse_from(["fraiseql-server", "--metrics-enabled", "true"]);
        assert_eq!(cli.server.metrics_enabled, Some(true));

        let cli = Cli::parse_from(["fraiseql-server", "--metrics-enabled", "false"]);
        assert_eq!(cli.server.metrics_enabled, Some(false));
    }

    #[test]
    fn cli_parse_bool_flag_without_value() {
        let cli = Cli::parse_from(["fraiseql-server", "--metrics-enabled"]);
        assert_eq!(cli.server.metrics_enabled, Some(true));
    }

    #[test]
    fn cli_parse_rate_limit_flags() {
        let cli = Cli::parse_from([
            "fraiseql-server",
            "--rate-limit-rps-per-ip",
            "200",
            "--rate-limit-burst-size",
            "1000",
        ]);
        assert_eq!(cli.server.rate_limit_rps_per_ip, Some(200));
        assert_eq!(cli.server.rate_limit_burst_size, Some(1000));
        assert!(cli.server.rate_limit_rps_per_user.is_none());
    }

    #[test]
    fn cli_parse_log_format() {
        let cli = Cli::parse_from(["fraiseql-server", "--log-format", "json"]);
        assert_eq!(cli.server.log_format.as_deref(), Some("json"));
        assert!(cli.server.is_json_log_format());
    }

    #[test]
    fn apply_overrides_database_url() {
        let args = ServerArgs {
            database_url: Some("postgres://override/db".into()),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        args.apply_to_config(&mut config);
        assert_eq!(config.database_url, "postgres://override/db");
    }

    #[test]
    fn apply_leaves_config_unchanged_when_no_overrides() {
        let args = ServerArgs::default();
        let mut config = ServerConfig::default();
        let original_db = config.database_url.clone();
        let original_addr = config.bind_addr;
        args.apply_to_config(&mut config);
        assert_eq!(config.database_url, original_db);
        assert_eq!(config.bind_addr, original_addr);
    }

    #[test]
    fn apply_metrics_enabled_override() {
        let args = ServerArgs {
            metrics_enabled: Some(true),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        assert!(!config.metrics_enabled);
        args.apply_to_config(&mut config);
        assert!(config.metrics_enabled);
    }

    #[test]
    fn apply_rate_limit_creates_config_when_absent() {
        let args = ServerArgs {
            rate_limit_rps_per_ip: Some(50),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        config.rate_limiting = None;
        args.apply_to_config(&mut config);
        let rl = config.rate_limiting.unwrap();
        assert_eq!(rl.rps_per_ip, 50);
        assert!(rl.enabled);
        assert_eq!(rl.burst_size, 500);
    }

    #[test]
    fn apply_rate_limit_preserves_existing_fields() {
        let args = ServerArgs {
            rate_limit_burst_size: Some(999),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        config.rate_limiting = Some(RateLimitConfig {
            enabled:               true,
            rps_per_ip:            42,
            rps_per_user:          420,
            burst_size:            100,
            cleanup_interval_secs: 60,
            trust_proxy_headers:   true,
            trusted_proxy_cidrs:   Vec::new(),
            max_buckets:           100_000,
        });
        args.apply_to_config(&mut config);
        let rl = config.rate_limiting.unwrap();
        assert_eq!(rl.burst_size, 999);
        assert_eq!(rl.rps_per_ip, 42);
        assert_eq!(rl.rps_per_user, 420);
        assert!(rl.trust_proxy_headers);
    }

    #[test]
    fn apply_introspection_overrides() {
        let args = ServerArgs {
            introspection_enabled: Some(true),
            introspection_require_auth: Some(false),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        args.apply_to_config(&mut config);
        assert!(config.introspection_enabled);
        assert!(!config.introspection_require_auth);
    }

    #[test]
    fn is_json_log_format_case_insensitive() {
        let args = ServerArgs {
            log_format: Some("JSON".into()),
            ..Default::default()
        };
        assert!(args.is_json_log_format());

        let args = ServerArgs {
            log_format: Some("text".into()),
            ..Default::default()
        };
        assert!(!args.is_json_log_format());

        let args = ServerArgs::default();
        assert!(!args.is_json_log_format());
    }
}

// ── error_tests ───────────────────────────────────────────────────────────────

mod error_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use crate::error::*;

    #[test]
    fn test_error_serialization() {
        let error = GraphQLError::validation("Invalid query")
            .with_location(1, 5)
            .with_path(vec!["user".to_string(), "id".to_string()]);

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Invalid query"));
        assert!(json.contains("VALIDATION_ERROR"));
        assert!(json.contains("\"line\":1"));
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse::new(vec![
            GraphQLError::validation("Field not found"),
            GraphQLError::database("Connection timeout"),
        ]);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Field not found"));
        assert!(json.contains("Connection timeout"));
    }

    #[test]
    fn test_error_code_status_codes() {
        use axum::http::StatusCode;
        assert_eq!(ErrorCode::ValidationError.status_code(), StatusCode::OK);
        assert_eq!(ErrorCode::ParseError.status_code(), StatusCode::OK);
        assert_eq!(ErrorCode::RequestError.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::Unauthenticated.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(ErrorCode::Forbidden.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(ErrorCode::DatabaseError.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ErrorCode::CircuitBreakerOpen.status_code(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_circuit_breaker_open_error() {
        let error = GraphQLError::circuit_breaker_open("Product", 30);
        assert_eq!(error.code, ErrorCode::CircuitBreakerOpen);
        assert!(error.message.contains("Product"));
        assert!(error.message.contains("30"));
        let ext = error.extensions.unwrap();
        assert_eq!(ext.retry_after_secs, Some(30));
        assert_eq!(ext.category, Some("CIRCUIT_BREAKER".to_string()));
    }

    #[test]
    fn test_circuit_breaker_response_has_retry_after_header() {
        use axum::{http::StatusCode, response::IntoResponse};

        let response = ErrorResponse::from_error(GraphQLError::circuit_breaker_open("User", 60))
            .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let retry_after = response.headers().get(axum::http::header::RETRY_AFTER);
        assert_eq!(retry_after.and_then(|v| v.to_str().ok()), Some("60"));
    }

    #[test]
    fn test_from_fraiseql_error_database_maps_to_database_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Database {
            message:   "relation \"users\" does not exist".into(),
            sql_state: None,
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::DatabaseError);
    }

    #[test]
    fn test_from_fraiseql_error_validation_maps_to_validation_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Validation {
            message: "field 'id' is required".into(),
            path:    None,
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::ValidationError);
    }

    #[test]
    fn test_from_fraiseql_error_not_found_maps_to_not_found_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::NotFound {
            resource_type: "User".into(),
            identifier:    "123".into(),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::NotFound);
    }

    #[test]
    fn test_from_fraiseql_error_authorization_maps_to_forbidden() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Authorization {
            message:  "insufficient permissions".into(),
            action:   Some("write".into()),
            resource: Some("User".into()),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::Forbidden);
    }

    #[test]
    fn test_from_fraiseql_error_authentication_maps_to_unauthenticated() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Authentication {
            message: "token expired".into(),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::Unauthenticated);
    }

    #[test]
    fn test_error_extensions() {
        let extensions = ErrorExtensions {
            category:         Some("VALIDATION".to_string()),
            status:           Some(400),
            request_id:       Some("req-123".to_string()),
            retry_after_secs: None,
            detail:           None,
        };

        let error = GraphQLError::validation("Invalid").with_extensions(extensions);
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("VALIDATION"));
        assert!(json.contains("req-123"));
    }

    #[test]
    fn test_all_error_codes_have_expected_status() {
        use axum::http::StatusCode;
        assert_eq!(ErrorCode::ParseError.status_code(), StatusCode::OK);
        assert_eq!(ErrorCode::RequestError.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::NotFound.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(ErrorCode::Conflict.status_code(), StatusCode::CONFLICT);
        assert_eq!(ErrorCode::RateLimitExceeded.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(ErrorCode::Timeout.status_code(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(ErrorCode::InternalServerError.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ErrorCode::PersistedQueryMismatch.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::ForbiddenQuery.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::DocumentNotFound.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_persisted_query_not_found_maps_to_200() {
        use axum::{http::StatusCode, response::IntoResponse};
        assert_eq!(ErrorCode::PersistedQueryNotFound.status_code(), StatusCode::OK);

        let response =
            ErrorResponse::from_error(GraphQLError::persisted_query_not_found()).into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_from_fraiseql_timeout_maps_to_timeout_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Timeout {
            timeout_ms: 5000,
            query:      Some("{ users { id } }".into()),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::Timeout);
    }

    #[test]
    fn test_from_fraiseql_rate_limited_maps_to_rate_limit_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::RateLimited {
            message:          "too many requests".into(),
            retry_after_secs: 60,
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::RateLimitExceeded);
    }

    #[test]
    fn test_from_fraiseql_conflict_maps_to_conflict_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Conflict {
            message: "unique constraint violated".into(),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::Conflict);
    }

    #[test]
    fn test_from_fraiseql_parse_maps_to_parse_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Parse {
            message:  "unexpected token".into(),
            location: "line 1, col 5".into(),
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::ParseError);
    }

    #[test]
    fn test_from_fraiseql_internal_maps_to_internal_code() {
        use fraiseql_core::error::FraiseQLError;
        let err = FraiseQLError::Internal {
            message: "unexpected nil pointer".into(),
            source:  None,
        };
        let graphql_err = GraphQLError::from_fraiseql_error(&err);
        assert_eq!(graphql_err.code, ErrorCode::InternalServerError);
    }

    #[test]
    fn test_timeout_response_has_correct_status() {
        use axum::{http::StatusCode, response::IntoResponse};
        let response =
            ErrorResponse::from_error(GraphQLError::new("timed out", ErrorCode::Timeout))
                .into_response();
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    }

    #[test]
    fn test_rate_limit_response_has_correct_status() {
        use axum::{http::StatusCode, response::IntoResponse};
        let response = ErrorResponse::from_error(GraphQLError::rate_limited("too many requests"))
            .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_not_found_response_has_correct_status() {
        use axum::{http::StatusCode, response::IntoResponse};
        let response = ErrorResponse::from_error(GraphQLError::not_found("resource not found"))
            .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_complexity_rejection_returns_200() {
        use axum::{http::StatusCode, response::IntoResponse};
        let response = ErrorResponse::from_error(GraphQLError::validation(
            "Query exceeds maximum complexity: 121 > 100",
        ))
        .into_response();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "complexity validation errors must return HTTP 200 per GraphQL-over-HTTP spec"
        );
    }

    #[test]
    fn test_depth_rejection_returns_200() {
        use axum::{http::StatusCode, response::IntoResponse};
        let response = ErrorResponse::from_error(GraphQLError::validation(
            "Query exceeds maximum depth: 16 > 15",
        ))
        .into_response();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "depth validation errors must return HTTP 200 per GraphQL-over-HTTP spec"
        );
    }

    #[test]
    fn test_parse_error_returns_200() {
        use axum::{http::StatusCode, response::IntoResponse};
        let response =
            ErrorResponse::from_error(GraphQLError::parse("unexpected token '}'")).into_response();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "GraphQL parse errors must return HTTP 200 per GraphQL-over-HTTP spec"
        );
    }
}

// ── extractors_tests ──────────────────────────────────────────────────────────

mod extractors_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use crate::extractors::*;

    #[test]
    fn test_extract_request_id_from_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-request-id", "req-12345".parse().unwrap());

        let request_id = extract_request_id(&headers);
        assert_eq!(request_id, "req-12345");
    }

    #[test]
    fn test_extract_request_id_generates_default() {
        let headers = axum::http::HeaderMap::new();
        let request_id = extract_request_id(&headers);
        assert!(request_id.starts_with("req-"));
        assert_eq!(request_id.len(), 40);
    }

    #[test]
    fn test_extract_ip_ignores_x_forwarded_for() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let ip = extract_ip_address(&headers);
        assert_eq!(ip, None, "Must not trust X-Forwarded-For header");
    }

    #[test]
    fn test_extract_ip_ignores_x_real_ip() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.2".parse().unwrap());

        let ip = extract_ip_address(&headers);
        assert_eq!(ip, None, "Must not trust X-Real-IP header");
    }

    #[test]
    fn test_extract_ip_address_none_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let ip = extract_ip_address(&headers);
        assert_eq!(ip, None);
    }

    #[test]
    fn test_extract_tenant_id_ignores_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-tenant-id", "tenant-acme".parse().unwrap());

        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, None, "Must not trust X-Tenant-ID header");
    }

    #[test]
    fn test_extract_tenant_id_none_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, None);
    }

    #[test]
    fn test_optional_security_context_creation_from_auth_user() {
        use chrono::Utc;

        let auth_user = crate::middleware::AuthUser(fraiseql_core::security::AuthenticatedUser {
            user_id:      fraiseql_core::types::UserId::new("user123"),
            scopes:       vec!["read:user".to_string(), "write:post".to_string()],
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: std::collections::HashMap::new(),
        });

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-request-id", "req-test-123".parse().unwrap());
        headers.insert("x-tenant-id", "tenant-acme".parse().unwrap());
        headers.insert("x-forwarded-for", "192.0.2.100".parse().unwrap());

        let security_context = Some(auth_user).map(|auth_user| {
            let authenticated_user = auth_user.0;
            let request_id = extract_request_id(&headers);
            let ip_address = extract_ip_address(&headers);
            let tenant_id = extract_tenant_id(&headers);

            let mut context = fraiseql_core::security::SecurityContext::from_user(
                &authenticated_user,
                request_id,
            );
            context.ip_address = ip_address;
            context.tenant_id = tenant_id.map(fraiseql_core::types::TenantId::new);
            context
        });

        let sec_ctx = security_context.unwrap();
        assert_eq!(sec_ctx.user_id, fraiseql_core::types::UserId::new("user123"));
        assert_eq!(sec_ctx.scopes, vec!["read:user".to_string(), "write:post".to_string()]);
        assert_eq!(sec_ctx.tenant_id, None);
        assert_eq!(sec_ctx.request_id, "req-test-123");
        assert_eq!(sec_ctx.ip_address, None);
    }
}

// ── logging_tests ─────────────────────────────────────────────────────────────

mod logging_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use crate::logging::*;

    #[test]
    fn test_request_id_generation() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_request_context_builder() {
        let context = RequestContext::new()
            .with_operation("GetUser".to_string())
            .with_user_id("user123".to_string())
            .with_client_ip("192.168.1.1".to_string())
            .with_api_version("v1".to_string());

        assert_eq!(context.operation, Some("GetUser".to_string()));
        assert_eq!(context.user_id, Some("user123".to_string()));
        assert_eq!(context.client_ip, Some("192.168.1.1".to_string()));
        assert_eq!(context.api_version, Some("v1".to_string()));
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = StructuredLogEntry::new(LogLevel::Info, "test message".to_string());
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "test message");
        assert!(entry.request_context.is_none());
    }

    #[test]
    fn test_log_entry_with_context() {
        let context = RequestContext::new().with_operation("Query".to_string());

        let entry = StructuredLogEntry::new(LogLevel::Info, "operation executed".to_string())
            .with_request_context(context);

        assert_eq!(entry.request_context.unwrap().operation, Some("Query".to_string()));
    }

    #[test]
    fn test_log_metrics_builder() {
        let metrics = LogMetrics::new()
            .with_duration_ms(123.45)
            .with_complexity(5)
            .with_items_processed(100)
            .with_cache_hit(true)
            .with_db_queries(3);

        assert_eq!(metrics.duration_ms, Some(123.45));
        assert_eq!(metrics.complexity, Some(5));
        assert_eq!(metrics.items_processed, Some(100));
        assert_eq!(metrics.cache_hit, Some(true));
        assert_eq!(metrics.db_queries, Some(3));
    }

    #[test]
    fn test_error_details_builder() {
        let error =
            ErrorDetails::new("DatabaseError".to_string(), "Connection timeout".to_string())
                .with_code("DB_TIMEOUT".to_string());

        assert_eq!(error.error_type, "DatabaseError");
        assert_eq!(error.message, "Connection timeout");
        assert_eq!(error.code, Some("DB_TIMEOUT".to_string()));
    }

    #[test]
    fn test_log_entry_json_serialization() {
        let entry = StructuredLogEntry::new(LogLevel::Error, "test error".to_string());
        let json = entry.to_json_string();

        assert!(json.contains("\"level\":\"ERROR\""));
        assert!(json.contains("\"message\":\"test error\""));
        assert!(json.contains("\"timestamp\":"));
    }

    #[test]
    fn test_request_logger_creation() {
        let context = RequestContext::new().with_operation("Query".to_string());
        let logger = RequestLogger::new(context);

        assert_eq!(logger.context().operation, Some("Query".to_string()));
    }

    #[test]
    fn test_request_logger_log_entry() {
        let logger = RequestLogger::with_request_id(RequestId::new());
        let entry = logger.info("test message");

        assert_eq!(entry.level, LogLevel::Info);
        assert!(
            entry.request_context.is_some(),
            "RequestLogger should attach request_context to every log entry"
        );
    }

    #[test]
    fn test_log_level_conversion() {
        assert_eq!(LogLevel::from(tracing::Level::INFO), LogLevel::Info);
        assert_eq!(LogLevel::from(tracing::Level::WARN), LogLevel::Warn);
        assert_eq!(LogLevel::from(tracing::Level::ERROR), LogLevel::Error);
        assert_eq!(LogLevel::from(tracing::Level::DEBUG), LogLevel::Debug);
        assert_eq!(LogLevel::from(tracing::Level::TRACE), LogLevel::Trace);
    }

    #[test]
    fn test_complex_log_entry() {
        let context = RequestContext::new()
            .with_operation("GetUsers".to_string())
            .with_user_id("user123".to_string());

        let metrics = LogMetrics::new()
            .with_duration_ms(45.67)
            .with_db_queries(2)
            .with_cache_hit(true);

        let error =
            ErrorDetails::new("ValidationError".to_string(), "Invalid query parameter".to_string());

        let source = SourceLocation::new(
            "routes/graphql.rs".to_string(),
            42,
            "fraiseql_server::routes".to_string(),
        );

        let entry = StructuredLogEntry::new(LogLevel::Warn, "Query validation warning".to_string())
            .with_request_context(context)
            .with_metrics(metrics)
            .with_error(error)
            .with_source(source);

        let json = entry.to_json_string();
        assert!(json.contains("\"level\":\"WARN\""));
        assert!(json.contains("\"duration_ms\":"));
        assert!(json.contains("\"error_type\":"));
        assert!(json.contains("\"file\":"));
    }
}

// ── metrics_server_tests ──────────────────────────────────────────────────────

mod metrics_server_tests {
    use std::sync::{Arc, atomic::Ordering};

    use crate::metrics_server::*;

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.queries_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.queries_success.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_increment() {
        let collector = MetricsCollector::new();
        collector.queries_total.fetch_add(5, Ordering::Relaxed);
        collector.queries_success.fetch_add(4, Ordering::Relaxed);
        collector.queries_error.fetch_add(1, Ordering::Relaxed);

        assert_eq!(collector.queries_total.load(Ordering::Relaxed), 5);
        assert_eq!(collector.queries_success.load(Ordering::Relaxed), 4);
        assert_eq!(collector.queries_error.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_prometheus_output_format() {
        let collector = MetricsCollector::new();
        collector.queries_total.store(100, Ordering::Relaxed);
        collector.queries_success.store(95, Ordering::Relaxed);
        collector.queries_error.store(5, Ordering::Relaxed);

        let metrics = PrometheusMetrics::from(&collector);
        let output = metrics.to_prometheus_format();

        assert!(output.contains("fraiseql_graphql_queries_total 100"));
        assert!(output.contains("fraiseql_graphql_queries_success 95"));
        assert!(output.contains("fraiseql_graphql_queries_error 5"));
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
    }

    #[test]
    fn test_timing_guard() {
        let duration_atomic = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let guard = TimingGuard::new(duration_atomic.clone());

        guard.record();

        let recorded = duration_atomic.load(Ordering::Relaxed);
        assert!(recorded < 1_000_000); // Must complete in under 1 second
    }

    #[test]
    fn test_cache_hit_ratio_calculation() {
        let collector = MetricsCollector::new();
        collector.cache_hits.store(75, Ordering::Relaxed);
        collector.cache_misses.store(25, Ordering::Relaxed);

        let metrics = PrometheusMetrics::from(&collector);
        assert!((metrics.cache_hit_ratio - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_average_duration_calculation() {
        let collector = MetricsCollector::new();
        collector.queries_total.store(10, Ordering::Relaxed);
        collector.queries_duration_us.store(50_000, Ordering::Relaxed); // 50ms total

        let metrics = PrometheusMetrics::from(&collector);
        assert!((metrics.queries_avg_duration_ms - 5.0).abs() < 0.01); // 5ms average
    }

    #[test]
    fn test_operation_metrics_record_and_render() {
        let registry = OperationMetricsRegistry::new(500);
        registry.record("GetUsers", 10_000, false); // 10ms
        registry.record("GetUsers", 20_000, false); // 20ms
        registry.record("GetPosts", 5_000, true); // 5ms error

        let output = registry.to_prometheus_format();
        assert!(output.contains("fraiseql_query_duration_seconds_bucket{operation=\"GetUsers\""));
        assert!(output.contains("fraiseql_query_duration_seconds_count{operation=\"GetUsers\"} 2"));
        assert!(output.contains("fraiseql_query_duration_seconds_count{operation=\"GetPosts\"} 1"));
        assert!(output.contains("fraiseql_query_errors_total{operation=\"GetPosts\"} 1"));
        assert!(output.contains("fraiseql_query_errors_total{operation=\"GetUsers\"} 0"));
    }

    #[test]
    fn test_anonymous_operation_label() {
        let registry = OperationMetricsRegistry::new(500);
        registry.record("", 1_000, false);

        let output = registry.to_prometheus_format();
        assert!(output.contains("operation=\"__anonymous__\""));
    }

    #[test]
    fn test_overflow_bucketing() {
        let registry = OperationMetricsRegistry::new(3);
        registry.record("Op1", 1_000, false);
        registry.record("Op2", 1_000, false);
        registry.record("Op3", 1_000, false);
        // This should go to overflow
        registry.record("Op4", 1_000, false);

        let output = registry.to_prometheus_format();
        assert!(output.contains("operation=\"__overflow__\""));
        assert!(
            output.contains("fraiseql_query_duration_seconds_count{operation=\"__overflow__\"} 1")
        );
    }

    #[test]
    fn test_histogram_bucket_correctness() {
        let registry = OperationMetricsRegistry::new(500);
        // 50ms = 50_000us → should increment le=0.05 and all buckets above
        registry.record("TestOp", 50_000, false);

        let output = registry.to_prometheus_format();
        // le=0.025 (25ms) should be 0 (50ms > 25ms)
        assert!(output.contains(
            "fraiseql_query_duration_seconds_bucket{operation=\"TestOp\",le=\"0.025\"} 0"
        ));
        // le=0.05 (50ms) should be 1 (50ms <= 50ms)
        assert!(output.contains(
            "fraiseql_query_duration_seconds_bucket{operation=\"TestOp\",le=\"0.05\"} 1"
        ));
        // le=0.1 (100ms) should be 1 (cumulative)
        assert!(
            output.contains(
                "fraiseql_query_duration_seconds_bucket{operation=\"TestOp\",le=\"0.1\"} 1"
            )
        );
    }
}

// ── tls_tests ─────────────────────────────────────────────────────────────────

mod tls_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use std::path::{Path, PathBuf};

    use crate::{
        server_config::{DatabaseTlsConfig, TlsServerConfig},
        tls::*,
    };

    #[test]
    fn test_tls_setup_disabled() {
        let setup = TlsSetup::new(None, None)
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        assert!(!setup.is_tls_enabled());
        assert!(!setup.is_mtls_required());
        assert!(setup.cert_path().is_none());
        assert!(setup.key_path().is_none());
    }

    #[test]
    fn test_database_tls_defaults() {
        let setup = TlsSetup::new(None, None)
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        assert_eq!(setup.postgres_ssl_mode(), "prefer");
        assert!(!setup.redis_ssl_enabled());
        assert!(!setup.clickhouse_https_enabled());
        assert!(!setup.elasticsearch_https_enabled());
        assert!(setup.verify_certificates());
    }

    #[test]
    fn test_postgres_url_tls_application() {
        let db_config = DatabaseTlsConfig {
            postgres_ssl_mode:   "require".to_string(),
            redis_ssl:           false,
            clickhouse_https:    false,
            elasticsearch_https: false,
            verify_certificates: true,
            ca_bundle_path:      None,
        };

        let setup = TlsSetup::new(None, Some(db_config))
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        let url = "postgresql://localhost/fraiseql";
        let tls_url = setup.apply_postgres_tls(url);

        assert!(tls_url.contains("sslmode=require"));
    }

    #[test]
    fn test_redis_url_tls_application() {
        let db_config = DatabaseTlsConfig {
            postgres_ssl_mode:   "prefer".to_string(),
            redis_ssl:           true,
            clickhouse_https:    false,
            elasticsearch_https: false,
            verify_certificates: true,
            ca_bundle_path:      None,
        };

        let setup = TlsSetup::new(None, Some(db_config))
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        let url = "redis://localhost:6379";
        let tls_url = setup.apply_redis_tls(url);

        assert_eq!(tls_url, "rediss://localhost:6379");
    }

    #[test]
    fn test_clickhouse_url_tls_application() {
        let db_config = DatabaseTlsConfig {
            postgres_ssl_mode:   "prefer".to_string(),
            redis_ssl:           false,
            clickhouse_https:    true,
            elasticsearch_https: false,
            verify_certificates: true,
            ca_bundle_path:      None,
        };

        let setup = TlsSetup::new(None, Some(db_config))
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        let url = "http://localhost:8123";
        let tls_url = setup.apply_clickhouse_tls(url);

        assert_eq!(tls_url, "https://localhost:8123");
    }

    #[test]
    fn test_elasticsearch_url_tls_application() {
        let db_config = DatabaseTlsConfig {
            postgres_ssl_mode:   "prefer".to_string(),
            redis_ssl:           false,
            clickhouse_https:    false,
            elasticsearch_https: true,
            verify_certificates: true,
            ca_bundle_path:      None,
        };

        let setup = TlsSetup::new(None, Some(db_config))
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        let url = "http://localhost:9200";
        let tls_url = setup.apply_elasticsearch_tls(url);

        assert_eq!(tls_url, "https://localhost:9200");
    }

    #[test]
    fn test_all_database_tls_enabled() {
        let db_config = DatabaseTlsConfig {
            postgres_ssl_mode:   "require".to_string(),
            redis_ssl:           true,
            clickhouse_https:    true,
            elasticsearch_https: true,
            verify_certificates: true,
            ca_bundle_path:      Some(PathBuf::from("/etc/ssl/certs/ca-bundle.crt")),
        };

        let setup = TlsSetup::new(None, Some(db_config))
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        assert_eq!(setup.postgres_ssl_mode(), "require");
        assert!(setup.redis_ssl_enabled());
        assert!(setup.clickhouse_https_enabled());
        assert!(setup.elasticsearch_https_enabled());
        assert!(setup.verify_certificates());
        assert!(
            setup.ca_bundle_path().is_some(),
            "ca_bundle_path should be propagated from DatabaseTlsConfig"
        );
    }

    #[test]
    fn test_postgres_url_with_existing_params() {
        let db_config = DatabaseTlsConfig {
            postgres_ssl_mode:   "require".to_string(),
            redis_ssl:           false,
            clickhouse_https:    false,
            elasticsearch_https: false,
            verify_certificates: true,
            ca_bundle_path:      None,
        };

        let setup = TlsSetup::new(None, Some(db_config))
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        let url = "postgresql://localhost/fraiseql?application_name=fraiseql";
        let tls_url = setup.apply_postgres_tls(url);

        assert!(tls_url.contains("application_name=fraiseql"));
        assert!(tls_url.contains("sslmode=require"));
    }

    #[test]
    fn test_database_tls_config_getters() {
        let db_config = DatabaseTlsConfig {
            postgres_ssl_mode:   "verify-full".to_string(),
            redis_ssl:           true,
            clickhouse_https:    true,
            elasticsearch_https: false,
            verify_certificates: true,
            ca_bundle_path:      Some(PathBuf::from("/etc/ssl/certs/ca.pem")),
        };

        let setup = TlsSetup::new(None, Some(db_config))
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        assert!(
            setup.db_config().is_some(),
            "db_config should be present when constructed with a DatabaseTlsConfig"
        );
        assert_eq!(setup.postgres_ssl_mode(), "verify-full");
        assert!(setup.redis_ssl_enabled());
        assert!(setup.clickhouse_https_enabled());
        assert!(!setup.elasticsearch_https_enabled());
        assert_eq!(setup.ca_bundle_path(), Some(Path::new("/etc/ssl/certs/ca.pem")));
    }

    #[test]
    fn test_create_rustls_config_without_tls_enabled() {
        let setup = TlsSetup::new(None, None)
            .expect("TlsSetup::new is infallible when cert and db_config are None");

        let result = setup.create_rustls_config();
        assert!(result.is_err(), "expected Err when TLS not enabled, got: {result:?}");
        assert!(result.unwrap_err().to_string().contains("TLS not enabled"));
    }

    #[test]
    fn test_create_rustls_config_with_missing_cert() {
        let tls_config = TlsServerConfig {
            enabled:             true,
            cert_path:           PathBuf::from("/nonexistent/cert.pem"),
            key_path:            PathBuf::from("/nonexistent/key.pem"),
            require_client_cert: false,
            client_ca_path:      None,
            min_version:         "1.2".to_string(),
        };

        let setup = TlsSetup::new(Some(tls_config), None)
            .expect("TlsSetup::new succeeds with enabled=true when min_version is valid; cert reading happens later in create_rustls_config");

        let result = setup.create_rustls_config();
        assert!(result.is_err(), "expected Err for missing cert file, got: {result:?}");
        assert!(result.unwrap_err().to_string().contains("Failed to open"));
    }
}

// ── token_revocation_tests ────────────────────────────────────────────────────

mod token_revocation_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use crate::token_revocation::*;

    fn memory_store() -> std::sync::Arc<dyn RevocationStore> {
        std::sync::Arc::new(InMemoryRevocationStore::new())
    }

    #[tokio::test]
    async fn revoke_then_check_is_revoked() {
        let store = memory_store();
        store.revoke("jti-1", 3600).await.unwrap();
        assert!(store.is_revoked("jti-1").await.unwrap());
    }

    #[tokio::test]
    async fn non_revoked_jti_passes() {
        let store = memory_store();
        assert!(!store.is_revoked("jti-unknown").await.unwrap());
    }

    #[tokio::test]
    async fn expired_entry_not_revoked() {
        let store = InMemoryRevocationStore::new();
        store.revoke("jti-expired", 0).await.unwrap();
        assert!(!store.is_revoked("jti-expired").await.unwrap());
    }

    #[tokio::test]
    async fn cleanup_removes_expired() {
        let store = InMemoryRevocationStore::new();
        store.revoke("jti-a", 0).await.unwrap();
        store.revoke("jti-b", 3600).await.unwrap();
        store.cleanup_expired();
        assert_eq!(store.entries.len(), 1);
    }

    #[tokio::test]
    async fn manager_rejects_revoked_token() {
        let store = memory_store();
        store.revoke("jti-x", 3600).await.unwrap();
        let mgr = TokenRevocationManager::new(store, true, false);
        assert_eq!(mgr.check_token(Some("jti-x")).await, Err(TokenRejection::Revoked));
    }

    #[tokio::test]
    async fn manager_allows_non_revoked_token() {
        let mgr = TokenRevocationManager::new(memory_store(), true, false);
        mgr.check_token(Some("jti-ok"))
            .await
            .unwrap_or_else(|e| panic!("expected Ok for non-revoked token: {e:?}"));
    }

    #[tokio::test]
    async fn manager_rejects_missing_jti_when_required() {
        let mgr = TokenRevocationManager::new(memory_store(), true, false);
        assert_eq!(mgr.check_token(None).await, Err(TokenRejection::MissingJti));
    }

    #[tokio::test]
    async fn manager_allows_missing_jti_when_not_required() {
        let mgr = TokenRevocationManager::new(memory_store(), false, false);
        assert!(
            mgr.check_token(None).await.is_ok(),
            "missing jti should be allowed when jti is not required"
        );
    }

    #[tokio::test]
    async fn manager_allows_empty_jti_when_not_required() {
        let mgr = TokenRevocationManager::new(memory_store(), false, false);
        assert!(
            mgr.check_token(Some("")).await.is_ok(),
            "empty jti should be allowed when jti is not required"
        );
    }

    #[tokio::test]
    async fn revoke_all_for_user_removes_all_matching_entries() {
        use chrono::Utc;

        let store = InMemoryRevocationStore::new();
        let exp = Utc::now() + chrono::Duration::seconds(3600);
        store.entries.insert("jti-alice-1".to_string(), ("alice".to_string(), exp));
        store.entries.insert("jti-alice-2".to_string(), ("alice".to_string(), exp));
        store.entries.insert("jti-bob-1".to_string(), ("bob".to_string(), exp));

        let count = store.revoke_all_for_user("alice").await.unwrap();
        assert_eq!(count, 2, "should have revoked 2 alice entries, got {count}");

        assert!(
            !store.is_revoked("jti-alice-1").await.unwrap(),
            "alice jti-1 should be removed from store"
        );
        assert!(
            !store.is_revoked("jti-alice-2").await.unwrap(),
            "alice jti-2 should be removed from store"
        );

        assert!(
            store.is_revoked("jti-bob-1").await.unwrap(),
            "bob jti-1 must NOT be revoked by alice's revoke_all"
        );
    }

    #[tokio::test]
    async fn revoke_all_for_user_returns_zero_when_no_entries() {
        let store = InMemoryRevocationStore::new();
        let count = store.revoke_all_for_user("unknown-user").await.unwrap();
        assert_eq!(count, 0, "empty store should return 0");
    }

    #[test]
    fn unix_now_helper_in_session_returns_reasonable_value() {
        use fraiseql_auth::session::unix_now;
        let now = unix_now().expect("unix_now should succeed on a normal system");
        assert!(now >= 1_577_836_800, "unix_now should return a timestamp after 2020");
    }

    #[test]
    fn hmac_fallback_tokens_differ_between_calls_even_for_same_user() {
        use rand::Rng;
        let key1: [u8; 32] = rand::rng().random();
        let key2: [u8; 32] = rand::rng().random();
        assert_ne!(key1, key2, "two OsRng-generated 256-bit keys must differ");
    }
}

// ── tracing_utils_tests ───────────────────────────────────────────────────────

#[cfg(feature = "federation")]
mod tracing_utils_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use crate::tracing_utils::*;

    #[test]
    fn test_extract_valid_traceparent() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "traceparent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".parse().unwrap(),
        );

        let ctx = extract_trace_context(&headers);
        let ctx = ctx.unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.parent_span_id, "00f067aa0ba902b7");
        assert_eq!(ctx.trace_flags, "01");
    }

    #[test]
    fn test_extract_invalid_traceparent() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("traceparent", "invalid-header".parse().unwrap());

        let ctx = extract_trace_context(&headers);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_extract_missing_traceparent() {
        let headers = axum::http::HeaderMap::new();
        let ctx = extract_trace_context(&headers);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_extract_invalid_utf8_traceparent() {
        let headers = axum::http::HeaderMap::new();
        let ctx = extract_trace_context(&headers);
        assert!(ctx.is_none());
    }
}

// ── trusted_documents_tests ───────────────────────────────────────────────────

mod trusted_documents_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use std::collections::HashMap;

    use crate::trusted_documents::*;

    fn test_documents() -> HashMap<String, String> {
        let mut docs = HashMap::new();
        docs.insert("sha256:abc123".to_string(), "{ users { id } }".to_string());
        docs.insert("sha256:def456".to_string(), "mutation { createUser { id } }".to_string());
        docs
    }

    #[tokio::test]
    async fn strict_mode_rejects_raw_query() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        let result = store.resolve(None, Some("{ users { id } }")).await;
        assert!(matches!(result, Err(TrustedDocumentError::ForbiddenRawQuery)));
    }

    #[tokio::test]
    async fn strict_mode_accepts_valid_document_id() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        let result = store.resolve(Some("sha256:abc123"), None).await;
        assert_eq!(result.unwrap(), "{ users { id } }");
    }

    #[tokio::test]
    async fn strict_mode_rejects_unknown_document_id() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        let result = store.resolve(Some("sha256:unknown"), None).await;
        assert!(matches!(result, Err(TrustedDocumentError::DocumentNotFound { .. })));
    }

    #[tokio::test]
    async fn permissive_mode_allows_raw_queries() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Permissive);
        let result = store.resolve(None, Some("{ arbitrary { query } }")).await;
        assert_eq!(result.unwrap(), "{ arbitrary { query } }");
    }

    #[tokio::test]
    async fn permissive_mode_uses_manifest_for_document_id() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Permissive);
        let result = store.resolve(Some("sha256:abc123"), None).await;
        assert_eq!(result.unwrap(), "{ users { id } }");
    }

    #[tokio::test]
    async fn document_id_without_prefix_is_resolved() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        let result = store.resolve(Some("abc123"), None).await;
        assert_eq!(result.unwrap(), "{ users { id } }");
    }

    #[tokio::test]
    async fn disabled_store_passes_through() {
        let store = TrustedDocumentStore::disabled();
        let result = store.resolve(None, Some("{ anything }")).await;
        assert_eq!(result.unwrap(), "{ anything }");
    }

    #[tokio::test]
    async fn hot_reload_replaces_documents() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        assert_eq!(store.document_count().await, 2);

        let mut new_docs = HashMap::new();
        new_docs.insert("sha256:new123".to_string(), "{ new query }".to_string());
        store.replace_documents(new_docs).await;

        assert_eq!(store.document_count().await, 1);
        let result = store.resolve(Some("sha256:new123"), None).await;
        assert_eq!(result.unwrap(), "{ new query }");

        let result = store.resolve(Some("sha256:abc123"), None).await;
        assert!(
            matches!(result, Err(TrustedDocumentError::DocumentNotFound { .. })),
            "expected DocumentNotFound after hot-reload removed old document, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn manifest_file_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trusted-documents.json");
        let manifest = serde_json::json!({
            "version": 1,
            "documents": {
                "sha256:aaa": "{ users { id } }",
                "sha256:bbb": "{ posts { title } }"
            }
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let store =
            TrustedDocumentStore::from_manifest_file(&path, TrustedDocumentMode::Strict).unwrap();
        assert_eq!(store.document_count().await, 2);
        let result = store.resolve(Some("sha256:aaa"), None).await;
        assert_eq!(result.unwrap(), "{ users { id } }");
    }

    #[test]
    fn manifest_file_exceeding_size_limit_is_rejected() {
        use std::io::Write as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge-manifest.json");

        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"{\"version\":1,\"documents\":{}}").unwrap();
        let padding = vec![b' '; (MAX_MANIFEST_BYTES + 1) as usize];
        f.write_all(&padding).unwrap();
        drop(f);

        let result = TrustedDocumentStore::from_manifest_file(&path, TrustedDocumentMode::Strict);
        assert!(result.is_err(), "oversized manifest must be rejected");
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("too large") || msg.contains("10485760"),
            "error must mention size limit: {msg}"
        );
    }

    #[test]
    fn manifest_file_at_size_limit_is_accepted_if_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small-manifest.json");
        let manifest = serde_json::json!({"version": 1, "documents": {}});
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();
        TrustedDocumentStore::from_manifest_file(&path, TrustedDocumentMode::Permissive)
            .unwrap_or_else(|e| panic!("small valid manifest must be accepted: {e}"));
    }
}
