#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

#[cfg(test)]
mod actions_tests {
    use std::{collections::HashMap, time::Duration};

    use serde_json::json;

    use crate::{actions::*, event::EventKind};

    #[test]
    fn test_webhook_action_creation() {
        let webhook = WebhookAction::new();
        // Just verify that the webhook action was created successfully
        let _ = webhook;
    }

    #[test]
    fn test_slack_action_creation() {
        let slack = SlackAction::new();
        // Just verify that the Slack action was created successfully
        let _ = slack;
    }

    #[test]
    fn test_email_action_creation() {
        let email = EmailAction::new();
        // Basic instantiation test
        let _result = std::mem::size_of_val(&email);
    }

    #[tokio::test]
    async fn test_email_action_execute() {
        let email = EmailAction::new();
        let event = crate::event::EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            json!({"total": 100}),
        );

        let result = email.execute("user@example.com", "Test", None, &event).await.unwrap();

        assert!(result.success);
        assert!(result.message_id.is_some());
    }

    #[test]
    fn test_webhook_render_body_template() {
        let webhook = WebhookAction::new();
        let data = json!({"status": "completed", "total": 150});
        let template = r#"{"status": "{{ status }}", "amount": {{ total }}}"#;

        let result = webhook.render_body_template(template, &data).unwrap();

        // Check that substitution happened
        let rendered_str = result.to_string();
        assert!(rendered_str.contains("completed"));
        assert!(rendered_str.contains("150"));
    }

    #[test]
    fn test_slack_render_message_template() {
        let slack = SlackAction::new();
        let data = json!({"status": "shipped", "order_id": "12345"});
        let template = "Order {{ order_id }} has been {{ status }}";

        let result = slack.render_message_template(template, &data);

        assert_eq!(result, "Order 12345 has been shipped");
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_action_execution_result() {
        let result = ActionExecutionResult {
            action_type: "webhook".to_string(),
            success:     true,
            duration_ms: 42.5,
            tracking_id: Some("abc123".to_string()),
        };

        assert_eq!(result.action_type, "webhook");
        assert!(result.success);
        assert_eq!(result.duration_ms, 42.5);
    }

    // --- Header injection tests (H11) ---

    #[test]
    fn test_validate_headers_clean_passes() {
        let mut headers = HashMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        validate_headers(&headers).unwrap_or_else(|e| panic!("clean headers should pass: {e}"));
    }

    #[test]
    fn test_validate_headers_lf_in_name_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Evil\nInjected".to_string(), "value".to_string());
        let err = validate_headers(&headers).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("header injection"), "expected injection message, got: {msg}");
    }

    #[test]
    fn test_validate_headers_cr_in_name_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Evil\rInjected".to_string(), "value".to_string());
        let result = validate_headers(&headers);
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "CR in header name should be rejected: {result:?}"
        );
    }

    #[test]
    fn test_validate_headers_lf_in_value_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Legit".to_string(), "value\r\nX-Injected: malicious".to_string());
        let result = validate_headers(&headers);
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "CRLF in header value should be rejected: {result:?}"
        );
    }

    #[test]
    fn test_validate_headers_empty_map_passes() {
        validate_headers(&HashMap::new())
            .unwrap_or_else(|e| panic!("empty headers should pass: {e}"));
    }

    #[test]
    fn test_webhook_action_with_timeout_creates_ok() {
        let _action = WebhookAction::with_timeout(Duration::from_secs(5));
    }

    // --- Additional header injection tests (14-3) ---

    #[test]
    fn test_validate_headers_nul_in_name_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Evil\0Null".to_string(), "value".to_string());
        let err = validate_headers(&headers).unwrap_err();
        assert!(
            err.to_string().contains("NUL") || err.to_string().contains("injection"),
            "expected injection message, got: {err}"
        );
    }

    #[test]
    fn test_validate_headers_nul_in_value_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Legit".to_string(), "value\0payload".to_string());
        let err = validate_headers(&headers).unwrap_err();
        assert!(err.to_string().contains("injection"), "got: {err}");
    }

    #[test]
    fn test_validate_headers_colon_in_name_rejected() {
        let mut headers = HashMap::new();
        // A colon in a header name is the name/value separator — disallowed.
        headers.insert("X-Forged: X-Real-IP".to_string(), "value".to_string());
        let err = validate_headers(&headers).unwrap_err();
        assert!(err.to_string().contains("colon"), "expected colon message, got: {err}");
    }

    #[test]
    fn test_validate_headers_colon_in_value_is_allowed() {
        // Colons are valid in header *values* (e.g. "Bearer tok:en", URLs, etc.)
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer abc:xyz".to_string());
        validate_headers(&headers)
            .unwrap_or_else(|e| panic!("colon in header value should be allowed: {e}"));
    }

    // --- HTTP status classification tests (14-4) ---

    #[test]
    fn test_200_ok_is_success() {
        let result = classify_http_status(reqwest::StatusCode::OK, 10.0);
        let response = result.unwrap_or_else(|e| panic!("200 OK should be success: {e}"));
        assert!(response.success);
    }

    #[test]
    fn test_404_is_permanent_failure() {
        let result = classify_http_status(reqwest::StatusCode::NOT_FOUND, 5.0);
        assert!(matches!(
            result,
            Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })
        ));
    }

    #[test]
    fn test_400_is_permanent_failure() {
        let result = classify_http_status(reqwest::StatusCode::BAD_REQUEST, 5.0);
        assert!(matches!(
            result,
            Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })
        ));
    }

    #[test]
    fn test_429_is_transient_failure() {
        // 429 must NOT be permanent — it should be eligible for retry.
        let result = classify_http_status(reqwest::StatusCode::TOO_MANY_REQUESTS, 5.0);
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionExecutionFailed { .. })),
            "429 must be treated as transient (retryable), not permanent"
        );
    }

    #[test]
    fn test_500_is_transient_failure() {
        let result = classify_http_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR, 5.0);
        assert!(matches!(result, Err(crate::error::ObserverError::ActionExecutionFailed { .. })));
    }

    // --- SSRF protection tests (C7) ---

    #[test]
    fn test_outbound_url_scheme_must_be_http() {
        let result = validate_outbound_url("file:///etc/passwd");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "file scheme should be rejected: {result:?}"
        );
        let result = validate_outbound_url("ftp://example.com");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "ftp scheme should be rejected: {result:?}"
        );
        let result = validate_outbound_url("example.com/hook");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "no scheme should be rejected: {result:?}"
        );
    }

    #[test]
    fn test_outbound_url_blocks_loopback() {
        let result = validate_outbound_url("http://localhost:8080");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "localhost should be blocked: {result:?}"
        );
        let result = validate_outbound_url("http://127.0.0.1/hook");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "127.0.0.1 should be blocked: {result:?}"
        );
        let result = validate_outbound_url("http://[::1]/hook");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "::1 should be blocked: {result:?}"
        );
    }

    #[test]
    fn test_outbound_url_blocks_private_ranges() {
        let result = validate_outbound_url("http://10.0.0.1/hook");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "10.x should be blocked: {result:?}"
        );
        let result = validate_outbound_url("http://172.16.0.1/hook");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "172.16.x should be blocked: {result:?}"
        );
        let result = validate_outbound_url("http://192.168.1.100/hook");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "192.168.x should be blocked: {result:?}"
        );
        // AWS metadata endpoint
        let result = validate_outbound_url("http://169.254.169.254/latest/meta-data/");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "169.254.x should be blocked: {result:?}"
        );
        // CGNAT range
        let result = validate_outbound_url("http://100.64.0.1/hook");
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "100.64.x should be blocked: {result:?}"
        );
    }

    #[test]
    fn test_outbound_url_allows_public_addresses() {
        validate_outbound_url("https://hooks.slack.com/services/xxx")
            .unwrap_or_else(|e| panic!("public slack URL should pass: {e}"));
        validate_outbound_url("https://api.example.com/webhook")
            .unwrap_or_else(|e| panic!("public API URL should pass: {e}"));
        validate_outbound_url("http://203.0.113.10/hook")
            .unwrap_or_else(|e| panic!("public IP should pass: {e}"));
    }

    // ── S24-H2: SlackAction client timeout ────────────────────────────────────

    #[test]
    fn slack_action_default_timeout_is_set() {
        // Verify the shared timeout constant is non-zero and in a sane range.
        const { assert!(DEFAULT_WEBHOOK_TIMEOUT_SECS > 0 && DEFAULT_WEBHOOK_TIMEOUT_SECS <= 120) }
    }

    #[test]
    fn slack_action_new_creates_instance() {
        // SlackAction::new() must succeed — no panics allowed from Client::builder().
        let _slack = SlackAction::new();
    }
}

#[cfg(test)]
mod actions_additional_tests {
    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        actions_additional::*,
        event::{EntityEvent, EventKind},
    };

    fn create_test_event() -> EntityEvent {
        EntityEvent::new(
            EventKind::Created,
            "User".to_string(),
            Uuid::new_v4(),
            json!({
                "id": "123",
                "name": "Test User",
                "email": "test@example.com"
            }),
        )
    }

    // SMS Action Tests
    #[test]
    fn test_sms_action_creation() {
        let _action = SmsAction::new();
    }

    #[test]
    fn test_sms_action_execute() {
        let action = SmsAction::new();
        let event = create_test_event();
        let response = action.execute("+1234567890", Some("Test notification"), &event).unwrap();

        assert!(response.success);
        assert!(response.duration_ms >= 0.0);
        assert!(response.message_id.is_some());
    }

    // Push Action Tests
    #[test]
    fn test_push_action_creation() {
        let _action = PushAction::new();
    }

    #[test]
    fn test_push_action_execute() {
        let action = PushAction::new();
        let response = action.execute("device_token_123", "Test Title", "Test Body").unwrap();

        assert!(response.success);
        assert!(response.duration_ms >= 0.0);
        assert!(response.notification_id.is_some());
    }

    // Search Action Tests
    #[test]
    fn test_search_action_creation() {
        let _action = SearchAction::new();
    }

    #[test]
    fn test_search_action_execute() {
        let action = SearchAction::new();
        let event = create_test_event();
        let response = action.execute("users", Some("user_123"), &event).unwrap();

        assert!(response.success);
        assert!(response.duration_ms >= 0.0);
        assert!(response.indexed);
    }

    // Cache Action Tests
    #[test]
    fn test_cache_action_creation() {
        let _action = CacheAction::new();
    }

    #[test]
    fn test_cache_action_execute() {
        let action = CacheAction::new();
        let response = action.execute("user:*", "invalidate").unwrap();

        assert!(response.success);
        assert!(response.duration_ms >= 0.0);
        assert_eq!(response.keys_affected, 1);
    }
}

#[cfg(test)]
mod elasticsearch_sink_tests {
    use crate::{elasticsearch_sink::*, error::ObserverError};

    #[test]
    fn test_config_default() {
        let config = ElasticsearchSinkConfig::default();
        assert_eq!(config.bulk_size, 1000);
        assert_eq!(config.flush_interval_secs, 5);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_validate_empty_url() {
        let config = ElasticsearchSinkConfig {
            url: String::new(),
            ..Default::default()
        };
        assert!(
            matches!(config.validate(), Err(ObserverError::InvalidConfig { .. })),
            "empty url must return InvalidConfig, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_config_validate_empty_prefix() {
        let config = ElasticsearchSinkConfig {
            index_prefix: String::new(),
            ..Default::default()
        };
        assert!(
            config.validate().is_err(),
            "empty index_prefix must return error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_config_validate_invalid_bulk_size() {
        let config = ElasticsearchSinkConfig {
            bulk_size: 0,
            ..Default::default()
        };
        assert!(
            matches!(config.validate(), Err(ObserverError::InvalidConfig { .. })),
            "bulk_size=0 must return InvalidConfig, got: {:?}",
            config.validate()
        );

        let config = ElasticsearchSinkConfig {
            bulk_size: 200_000,
            ..Default::default()
        };
        assert!(
            matches!(config.validate(), Err(ObserverError::InvalidConfig { .. })),
            "bulk_size=200_000 must return InvalidConfig, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_config_validate_invalid_timeout() {
        let config = ElasticsearchSinkConfig {
            flush_interval_secs: 0,
            ..Default::default()
        };
        assert!(
            matches!(config.validate(), Err(ObserverError::InvalidConfig { .. })),
            "flush_interval_secs=0 must return InvalidConfig, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_config_validate_valid() {
        let config = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            ..ElasticsearchSinkConfig::default()
        };
        config
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for valid config: {e}"));
    }

    #[test]
    fn test_is_transient_error() {
        let config = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            ..ElasticsearchSinkConfig::default()
        };
        let sink = ElasticsearchSink::new(config).unwrap();

        assert!(sink.is_transient_error("Connection refused"));
        assert!(sink.is_transient_error("timeout"));
        assert!(sink.is_transient_error("503 Service Unavailable"));
        assert!(sink.is_transient_error("502 Bad Gateway"));
        assert!(!sink.is_transient_error("Invalid index"));
    }

    #[test]
    fn test_is_transient_error_connection_reset() {
        let config = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            ..ElasticsearchSinkConfig::default()
        };
        let sink = ElasticsearchSink::new(config).unwrap();
        assert!(sink.is_transient_error("connection reset by peer"));
        assert!(!sink.is_transient_error("404 Not Found"));
        assert!(!sink.is_transient_error("400 Bad Request"));
    }

    #[test]
    fn test_config_max_bulk_size_boundary() {
        // 100_001 exceeds the upper bound
        let config = ElasticsearchSinkConfig {
            bulk_size: 100_001,
            ..Default::default()
        };
        assert!(
            matches!(config.validate(), Err(ObserverError::InvalidConfig { .. })),
            "bulk_size=100_001 must return InvalidConfig, got: {:?}",
            config.validate()
        );

        // 100_000 is the maximum valid value
        let config = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            bulk_size: 100_000,
            ..Default::default()
        };
        config
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for bulk_size=100_000: {e}"));
    }

    #[test]
    fn test_with_env_overrides_returns_valid_config() {
        // with_env_overrides() is callable and produces a consistent config.
        // Full override behaviour is tested via env-var integration; here we
        // verify the function compiles, returns Self, and produces a valid result.
        let base = ElasticsearchSinkConfig {
            url: "https://es.example.com:9200".to_string(),
            ..ElasticsearchSinkConfig::default()
        };
        let after = base.with_env_overrides();
        after
            .validate()
            .unwrap_or_else(|e| panic!("config after with_env_overrides must still be valid: {e}"));
    }

    #[test]
    fn test_config_custom_values_validate() {
        let config = ElasticsearchSinkConfig {
            url:                 "https://es.example.com:9200".to_string(),
            index_prefix:        "my-app-events".to_string(),
            bulk_size:           500,
            flush_interval_secs: 30,
            max_retries:         5,
        };
        config
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for custom valid config: {e}"));
    }

    // ── S23-H4: Elasticsearch sink timeout + bulk response cap ────────────────

    #[test]
    fn es_sink_timeout_is_set() {
        let secs = ES_SINK_REQUEST_TIMEOUT.as_secs();
        assert!(secs > 0 && secs <= 120, "ES sink timeout should be 1–120 s, got {secs}");
    }

    #[test]
    fn es_sink_bulk_response_cap_is_reasonable() {
        const { assert!(MAX_ES_BULK_RESPONSE_BYTES >= 1024 * 1024) }
        const { assert!(MAX_ES_BULK_RESPONSE_BYTES <= 500 * 1024 * 1024) }
    }

    #[tokio::test]
    async fn es_sink_oversized_bulk_response_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock = MockServer::start().await;
        let oversized = vec![b'x'; MAX_ES_BULK_RESPONSE_BYTES + 1];
        Mock::given(method("POST"))
            .and(path("/_bulk"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock)
            .await;

        let config = ElasticsearchSinkConfig {
            url:                 mock.uri(),
            index_prefix:        "test".to_string(),
            bulk_size:           10,
            flush_interval_secs: 5,
            max_retries:         1,
        };
        let sink = ElasticsearchSink::new_unchecked(config);

        // Drive the private try_bulk_index path via flush_buffer through a mock event.
        // We create a minimal event buffer and call the internal path indirectly.
        let event = crate::event::EntityEvent {
            id:          uuid::Uuid::nil(),
            event_type:  crate::event::EventKind::Created,
            entity_type: "Order".to_string(),
            entity_id:   uuid::Uuid::nil(),
            timestamp:   chrono::Utc::now(),
            data:        serde_json::json!({}),
            changes:     None,
            user_id:     None,
            tenant_id:   Some("tenant-1".to_string()),
        };
        let result = sink.try_bulk_index(&[event]).await;
        assert!(result.is_err(), "oversized bulk response must be rejected");
        let reason = match result.unwrap_err() {
            ObserverError::DatabaseError { reason } => reason,
            e => panic!("expected DatabaseError, got {e:?}"),
        };
        assert!(reason.contains("too large"), "error must mention size limit: {reason}");
    }
}

#[cfg(test)]
mod error_tests {
    use crate::error::*;

    #[test]
    fn test_error_code_is_transient() {
        assert!(ObserverErrorCode::ActionExecutionFailed.is_transient());
        assert!(ObserverErrorCode::DatabaseError.is_transient());
        assert!(ObserverErrorCode::ListenerConnectionFailed.is_transient());

        assert!(!ObserverErrorCode::InvalidConfig.is_transient());
        assert!(!ObserverErrorCode::ActionPermanentlyFailed.is_transient());
    }

    #[test]
    fn test_error_code_should_dlq() {
        assert!(ObserverErrorCode::ActionPermanentlyFailed.should_dlq());
        assert!(ObserverErrorCode::TemplateRenderingFailed.should_dlq());
        assert!(ObserverErrorCode::InvalidActionConfig.should_dlq());

        assert!(!ObserverErrorCode::ActionExecutionFailed.should_dlq());
        assert!(!ObserverErrorCode::DatabaseError.should_dlq());
    }

    #[test]
    fn test_observer_error_code_method() {
        let err = ObserverError::InvalidConfig {
            message: "test".to_string(),
        };
        assert_eq!(err.code(), ObserverErrorCode::InvalidConfig);
        assert!(!err.is_transient());
        assert!(!err.should_dlq());
    }

    #[test]
    fn test_transient_action_failure() {
        let err = ObserverError::ActionExecutionFailed {
            reason: "timeout".to_string(),
        };
        assert!(err.is_transient());
        assert!(!err.should_dlq());
    }

    #[test]
    fn test_permanent_action_failure() {
        let err = ObserverError::ActionPermanentlyFailed {
            reason: "invalid config".to_string(),
        };
        assert!(!err.is_transient());
        assert!(err.should_dlq());
    }

    #[test]
    fn test_deserialization_error_routes_to_dlq() {
        let err = ObserverError::DeserializationError {
            raw:    b"not valid json {{".to_vec(),
            reason: "invalid json: expected value at line 1 column 1".to_string(),
        };
        // Not transient — retrying the same broken bytes cannot succeed.
        assert!(!err.is_transient());
        // Should be routed to DLQ so the raw payload is preserved.
        assert!(err.should_dlq());
        assert_eq!(err.code(), ObserverErrorCode::DeserializationError);
    }

    #[test]
    fn test_deserialization_error_should_dlq_code() {
        assert!(ObserverErrorCode::DeserializationError.should_dlq());
        assert!(!ObserverErrorCode::DeserializationError.is_transient());
    }

    #[test]
    fn test_tenant_violation_error_code() {
        let err = ObserverError::TenantViolation {
            event_tenant:   Some("other-tenant".to_string()),
            required_scope: "Single(acme)".to_string(),
        };
        assert_eq!(err.code(), ObserverErrorCode::TenantViolation);
        // Not retryable — the tenant policy won't change between attempts.
        assert!(!err.is_transient());
        // Handled internally by DedupedObserverExecutor; not routed via should_dlq().
        assert!(!err.should_dlq());
    }

    #[test]
    fn test_tenant_violation_none_tenant() {
        let err = ObserverError::TenantViolation {
            event_tenant:   None,
            required_scope: "Single(acme)".to_string(),
        };
        assert_eq!(err.code(), ObserverErrorCode::TenantViolation);
        assert!(!err.is_transient());
    }
}

#[cfg(test)]
mod event_tests {
    use serde_json::json;
    use uuid::Uuid;

    use crate::event::*;

    #[test]
    fn test_event_kind_as_str() {
        assert_eq!(EventKind::Created.as_str(), "INSERT");
        assert_eq!(EventKind::Updated.as_str(), "UPDATE");
        assert_eq!(EventKind::Deleted.as_str(), "DELETE");
        assert_eq!(EventKind::Custom.as_str(), "CUSTOM");
    }

    #[test]
    fn test_create_entity_event() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100, "status": "pending"}),
        );

        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.data["total"], 100);
        assert!(event.is_new());
        assert!(!event.is_deleted());
    }

    #[test]
    fn test_entity_event_with_user_id() {
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
                .with_user_id("user123".to_string());

        assert_eq!(event.user_id, Some("user123".to_string()));
    }

    #[test]
    fn test_field_changes() {
        let entity_id = Uuid::new_v4();
        let mut changes = std::collections::HashMap::new();
        changes.insert(
            "status".to_string(),
            FieldChanges {
                old: json!("pending"),
                new: json!("shipped"),
            },
        );

        let event = EntityEvent::new(
            EventKind::Updated,
            "Order".to_string(),
            entity_id,
            json!({"status": "shipped"}),
        )
        .with_changes(changes);

        assert!(event.field_changed("status"));
        assert!(!event.field_changed("total"));
        assert!(event.field_changed_to("status", &json!("shipped")));
        assert!(event.field_changed_from("status", &json!("pending")));
    }

    #[test]
    fn test_delete_event() {
        let event =
            EntityEvent::new(EventKind::Deleted, "Order".to_string(), Uuid::new_v4(), json!({}));

        assert!(event.is_deleted());
        assert!(!event.is_new());
    }

    #[test]
    fn test_entity_event_with_tenant_id() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100}),
        )
        .with_tenant_id("tenant-123");

        assert_eq!(event.tenant_id, Some("tenant-123".to_string()));
    }

    #[test]
    fn test_entity_event_tenant_id_with_owned_string() {
        let tenant_id_owned = String::from("tenant-456");
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100}),
        )
        .with_tenant_id(tenant_id_owned);

        assert_eq!(event.tenant_id, Some("tenant-456".to_string()));
    }

    #[test]
    fn test_entity_event_multi_tenant_isolation() {
        let event1 = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100}),
        )
        .with_tenant_id("tenant-1");

        let event2 = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 200}),
        )
        .with_tenant_id("tenant-2");

        assert_ne!(event1.tenant_id, event2.tenant_id);
    }
}

#[cfg(test)]
mod factory_tests {
    use std::{collections::HashMap, sync::Arc};

    use crate::{
        ObserverRuntimeConfig,
        config::{PerformanceConfig, TransportConfig, TransportKind},
        factory::*,
        matcher::EventMatcher,
        testing::mocks::MockDeadLetterQueue,
    };

    #[tokio::test]
    async fn test_build_postgres_only_topology() {
        let config = ObserverRuntimeConfig {
            transport:               TransportConfig {
                transport: TransportKind::Postgres,
                ..Default::default()
            },
            redis:                   None, // No Redis
            clickhouse:              None,
            job_queue:               None,
            performance:             PerformanceConfig {
                enable_dedup: false,
                enable_caching: false,
                enable_concurrent: true,
                ..Default::default()
            },
            observers:               HashMap::new(),
            channel_capacity:        1000,
            max_concurrency:         50,
            overflow_policy:         crate::config::OverflowPolicy::Drop,
            backlog_alert_threshold: 500,
            shutdown_timeout:        "30s".to_string(),
            max_dlq_size:            None,
        };

        let dlq = Arc::new(MockDeadLetterQueue::new());
        let result = ExecutorFactory::build_postgres_only(&config, dlq).await;
        result.unwrap_or_else(|e| panic!("expected Ok for postgres-only topology: {e}"));
    }

    #[tokio::test]
    async fn test_build_rejects_dedup_without_redis() {
        let config = ObserverRuntimeConfig {
            transport:               TransportConfig::default(),
            redis:                   None, // No Redis but dedup enabled
            clickhouse:              None,
            job_queue:               None,
            performance:             PerformanceConfig {
                enable_dedup: true, // Invalid!
                ..Default::default()
            },
            observers:               HashMap::new(),
            channel_capacity:        1000,
            max_concurrency:         50,
            overflow_policy:         crate::config::OverflowPolicy::Drop,
            backlog_alert_threshold: 500,
            shutdown_timeout:        "30s".to_string(),
            max_dlq_size:            None,
        };

        let dlq = Arc::new(MockDeadLetterQueue::new());

        #[cfg(all(feature = "dedup", feature = "caching"))]
        {
            let result = ExecutorFactory::build(&config, dlq).await;
            assert!(
                matches!(result, Err(crate::ObserverError::InvalidConfig { .. })),
                "dedup without redis must return InvalidConfig"
            );
        }

        #[cfg(not(all(feature = "dedup", feature = "caching")))]
        {
            // Without features, should succeed (ignores config)
            let result = ExecutorFactory::build(&config, dlq).await;
            result
                .unwrap_or_else(|e| panic!("expected Ok when dedup/caching features absent: {e}"));
        }
    }

    #[tokio::test]
    async fn test_process_event_trait() {
        use serde_json::json;
        use uuid::Uuid;

        use crate::event::{EntityEvent, EventKind};

        let matcher = EventMatcher::build(HashMap::new()).unwrap();
        let dlq = Arc::new(MockDeadLetterQueue::new());
        let executor = crate::executor::ObserverExecutor::new(matcher, dlq);

        // Can use via trait object
        let processor: Arc<dyn ProcessEvent> = Arc::new(executor);

        let event =
            EntityEvent::new(EventKind::Created, "Test".to_string(), Uuid::new_v4(), json!({}));

        let summary = processor.process_event(&event).await.unwrap();
        assert!(!summary.duplicate_skipped);
    }

    #[cfg(feature = "queue")]
    #[tokio::test]
    async fn test_build_with_queue_requires_config() {
        let config = ObserverRuntimeConfig {
            transport:               TransportConfig::default(),
            redis:                   None,
            clickhouse:              None,
            job_queue:               None, // No job queue config
            performance:             PerformanceConfig {
                enable_dedup: false,
                enable_caching: false,
                enable_concurrent: true,
                ..Default::default()
            },
            observers:               HashMap::new(),
            channel_capacity:        1000,
            max_concurrency:         50,
            overflow_policy:         crate::config::OverflowPolicy::Drop,
            backlog_alert_threshold: 500,
            shutdown_timeout:        "30s".to_string(),
            max_dlq_size:            None,
        };

        let dlq = Arc::new(MockDeadLetterQueue::new());
        let result = ExecutorFactory::build_with_queue(&config, dlq).await;
        assert!(
            matches!(result, Err(crate::ObserverError::InvalidConfig { .. })),
            "missing job_queue config must return InvalidConfig"
        );
    }

    #[cfg(feature = "queue")]
    #[tokio::test]
    async fn test_job_queue_config_validation() {
        use crate::config::JobQueueConfig;

        // Valid config
        let config = JobQueueConfig::default();
        config
            .validate()
            .unwrap_or_else(|e| panic!("expected Ok for default config: {e}"));

        // Invalid: empty URL
        let config = JobQueueConfig {
            url: String::new(),
            ..JobQueueConfig::default()
        };
        assert!(
            config.validate().is_err(),
            "empty url must return error, got: {:?}",
            config.validate()
        );

        // Invalid: zero batch size
        let config = JobQueueConfig {
            batch_size: 0,
            ..JobQueueConfig::default()
        };
        assert!(
            config.validate().is_err(),
            "batch_size=0 must return error, got: {:?}",
            config.validate()
        );

        // Invalid: zero concurrency
        let config = JobQueueConfig {
            worker_concurrency: 0,
            ..JobQueueConfig::default()
        };
        assert!(
            config.validate().is_err(),
            "worker_concurrency=0 must return error, got: {:?}",
            config.validate()
        );
    }

    #[cfg(feature = "queue")]
    #[test]
    fn test_job_queue_config_defaults() {
        use crate::config::JobQueueConfig;

        let config = JobQueueConfig::default();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.batch_timeout_secs, 5);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.worker_concurrency, 10);
        assert_eq!(config.poll_interval_ms, 1000);
    }

    #[cfg(feature = "queue")]
    #[test]
    fn test_job_queue_config_env_overrides() {
        use crate::config::JobQueueConfig;

        let config = JobQueueConfig::default().with_env_overrides();

        // Should have defaults (env vars not set in test)
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.worker_concurrency, 10);
    }
}

#[cfg(test)]
mod matcher_tests {
    use std::collections::HashMap;

    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        ObserverDefinition,
        config::{ActionConfig, FailurePolicy, RetryConfig},
        event::{EntityEvent, EventKind},
        matcher::*,
    };

    fn create_observer(event_type: &str, entity: &str) -> ObserverDefinition {
        ObserverDefinition {
            event_type: event_type.to_string(),
            entity:     entity.to_string(),
            condition:  None,
            actions:    vec![ActionConfig::Webhook {
                url:           Some("https://example.com".to_string()),
                url_env:       None,
                headers:       HashMap::default(),
                body_template: Some("{}".to_string()),
            }],
            retry:      RetryConfig::default(),
            on_failure: FailurePolicy::Log,
        }
    }

    #[test]
    fn test_matcher_new() {
        let matcher = EventMatcher::new();
        assert_eq!(matcher.observer_count(), 0);
        assert_eq!(matcher.event_type_count(), 0);
        assert_eq!(matcher.entity_type_count(), 0);
    }

    #[test]
    fn test_matcher_add_observer() {
        let mut matcher = EventMatcher::new();
        let observer = create_observer("INSERT", "Order");

        matcher.add_observer(observer);

        assert_eq!(matcher.observer_count(), 1);
        assert_eq!(matcher.event_type_count(), 1);
        assert_eq!(matcher.entity_type_count(), 1);
    }

    #[test]
    fn test_matcher_find_exact_match() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher.add_observer(create_observer("INSERT", "Order"));
        evt_matcher.add_observer(create_observer("UPDATE", "Order"));
        evt_matcher.add_observer(create_observer("INSERT", "User"));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));

        let results = evt_matcher.find_matches(&event);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity, "Order");
    }

    #[test]
    fn test_matcher_find_no_match() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Updated, "Product".to_string(), Uuid::new_v4(), json!({}));

        let results = evt_matcher.find_matches(&event);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_matcher_multiple_observers_same_event() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher.add_observer(create_observer("INSERT", "Order"));
        evt_matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));

        let results = evt_matcher.find_matches(&event);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_matcher_build_from_hashmap() {
        let mut observers = HashMap::new();
        observers.insert("order_insert".to_string(), create_observer("INSERT", "Order"));
        observers.insert("user_insert".to_string(), create_observer("INSERT", "User"));
        observers.insert("order_update".to_string(), create_observer("UPDATE", "Order"));

        let matcher = EventMatcher::build(observers).unwrap();

        assert_eq!(matcher.observer_count(), 3);
        assert_eq!(matcher.event_type_count(), 2); // INSERT and UPDATE
        // entity_type_count returns total entity type entries (Order appears twice, User once)
        assert_eq!(matcher.entity_type_count(), 3);
    }

    #[test]
    fn test_matcher_find_by_event_and_entity() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher.add_observer(create_observer("INSERT", "Order"));
        evt_matcher.add_observer(create_observer("UPDATE", "Order"));

        let matching_observers = evt_matcher.find_by_event_and_entity(EventKind::Created, "Order");
        assert_eq!(matching_observers.len(), 1);

        let no_matching_observers =
            evt_matcher.find_by_event_and_entity(EventKind::Deleted, "Order");
        assert_eq!(no_matching_observers.len(), 0);
    }

    #[test]
    fn test_matcher_clear() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "User"));

        assert_eq!(matcher.observer_count(), 2);

        matcher.clear();
        assert_eq!(matcher.observer_count(), 0);
        assert_eq!(matcher.event_type_count(), 0);
    }

    #[test]
    fn test_matcher_all_observers() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "Order"));
        matcher.add_observer(create_observer("INSERT", "User"));

        let all = matcher.all_observers();
        assert_eq!(all.len(), 3);
    }

    // =========================================================================
    // Additional tests for matcher.rs coverage
    // =========================================================================

    #[test]
    fn test_no_observers_empty_result() {
        let matcher = EventMatcher::new();
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert!(results.is_empty(), "No observers should yield empty result");
    }

    #[test]
    fn test_single_observer_matches_entity_type() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert_eq!(results.len(), 1, "Single observer should match its entity type");
    }

    #[test]
    fn test_single_observer_wrong_entity_type_no_match() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Created, "User".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert!(results.is_empty(), "Observer should not match wrong entity type");
    }

    #[test]
    fn test_multiple_observers_first_matches_only() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "User"));

        // Only INSERT on Order should match
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert_eq!(results.len(), 1, "Only the matching observer should be returned");
        assert_eq!(results[0].entity, "Order");
    }

    #[test]
    fn test_multiple_observers_all_match_when_same_event_entity() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert_eq!(results.len(), 3, "All matching observers should be returned");
    }

    #[test]
    fn test_wildcard_entity_matches_all_entities() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "*"));

        let event_order =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let event_user =
            EntityEvent::new(EventKind::Created, "User".to_string(), Uuid::new_v4(), json!({}));

        let results_order = matcher.find_matches(&event_order);
        let results_user = matcher.find_matches(&event_user);

        assert_eq!(results_order.len(), 1, "Wildcard observer should match Order");
        assert_eq!(results_user.len(), 1, "Wildcard observer should match User");
    }

    #[test]
    fn test_observer_count_after_multiple_adds() {
        let mut matcher = EventMatcher::new();
        assert_eq!(matcher.observer_count(), 0);

        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "User"));
        matcher.add_observer(create_observer("DELETE", "Product"));

        assert_eq!(
            matcher.observer_count(),
            3,
            "Observer count should reflect all added observers"
        );
    }

    #[test]
    fn test_event_type_case_insensitive_matching() {
        let mut matcher = EventMatcher::new();
        // Observer defined with lowercase
        matcher.add_observer(create_observer("insert", "Order"));

        // Event uses EventKind::Created which maps to "INSERT"
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert_eq!(results.len(), 1, "Event type matching should be case-insensitive");
    }

    #[test]
    fn test_find_by_event_and_entity_with_wildcard() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "*"));
        matcher.add_observer(create_observer("INSERT", "Order"));

        // Both wildcard and exact should match
        let results = matcher.find_by_event_and_entity(EventKind::Created, "Order");
        assert_eq!(results.len(), 2, "Both exact and wildcard observers should match");
    }

    #[test]
    fn test_entity_type_count_with_multiple_event_types() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "Order"));
        matcher.add_observer(create_observer("DELETE", "Order"));

        // 3 event types × 1 entity type each = 3 total entity type entries
        assert_eq!(matcher.event_type_count(), 3, "Should have 3 distinct event types");
        assert_eq!(matcher.entity_type_count(), 3, "Should have 3 entity type entries");
    }

    /// This test uses `ObserverExecutor::with_dispatcher` to exercise the test seam
    /// and prevent the `dead_code` lint from triggering on that method.
    #[tokio::test]
    async fn test_executor_with_dispatcher_test_seam() {
        use std::sync::Arc;

        use crate::{
            ObserverExecutor,
            matcher::EventMatcher,
            testing::mocks::{MockActionDispatcher, MockDeadLetterQueue},
        };

        let matcher = EventMatcher::new();
        let dlq = Arc::new(MockDeadLetterQueue::new());
        let dispatcher = Arc::new(MockActionDispatcher::new());

        // Exercise the with_dispatcher constructor — this prevents the dead_code lint
        let _executor = ObserverExecutor::with_dispatcher(matcher, dlq, dispatcher);
        // The executor was constructed — no panics means the seam works
    }
}

#[cfg(all(test, feature = "queue"))]
mod queued_executor_tests {
    use crate::queued_executor::*;

    #[test]
    fn test_queued_summary_creation() {
        let summary = QueuedExecutionSummary::new();
        assert_eq!(summary.jobs_queued, 0);
        assert_eq!(summary.queueing_errors, 0);
        assert!(summary.is_success());
    }

    #[test]
    fn test_queued_summary_success() {
        let summary = QueuedExecutionSummary {
            jobs_queued:        5,
            queueing_errors:    0,
            conditions_skipped: 0,
            job_ids:            vec![],
            errors:             vec![],
        };
        assert!(summary.is_success());
        assert_eq!(summary.total_jobs(), 5);
    }

    #[test]
    fn test_queued_summary_with_errors() {
        let mut summary = QueuedExecutionSummary::new();
        summary.queueing_errors = 2;
        summary.errors.push("failed to connect".to_string());
        assert!(!summary.is_success());
        assert_eq!(summary.total_jobs(), 2);
    }

    #[test]
    fn test_to_execution_summary() {
        let mut summary = QueuedExecutionSummary::new();
        summary.jobs_queued = 5;
        summary.queueing_errors = 1;
        summary.conditions_skipped = 2;
        summary.errors.push("error1".to_string());

        let exec_summary = summary.to_execution_summary();
        assert_eq!(exec_summary.successful_actions, 5);
        assert_eq!(exec_summary.failed_actions, 1);
        assert_eq!(exec_summary.conditions_skipped, 2);
        assert_eq!(exec_summary.errors.len(), 1);
    }
}

#[cfg(test)]
mod storage_tests {
    #[tokio::test]
    async fn test_postgres_query_events() {
        if std::env::var("DATABASE_URL").is_err() {
            eprintln!("Skipping: DATABASE_URL not set");
            return;
        }
        // This test would require a test database setup
        // Skipping for now - integration tests will cover this
    }
}

#[cfg(test)]
mod traits_tests {
    use uuid::Uuid;

    use crate::{config::ActionConfig, event::EntityEvent, traits::*};

    #[test]
    fn test_action_result_creation() {
        let result = ActionResult {
            action_type: "email".to_string(),
            success:     true,
            message:     "Email sent".to_string(),
            duration_ms: 125.5,
        };

        assert_eq!(result.action_type, "email");
        assert!(result.success);
        assert!((result.duration_ms - 125.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dlq_item_creation() {
        let event = EntityEvent::new(
            crate::event::EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        );

        let action = ActionConfig::Email {
            to:               Some("user@example.com".to_string()),
            to_template:      None,
            subject:          Some("Test".to_string()),
            subject_template: None,
            body_template:    Some("Body".to_string()),
            reply_to:         None,
        };

        let item = DlqItem {
            id: Uuid::new_v4(),
            event,
            action,
            error_message: "SMTP failed".to_string(),
            attempts: 3,
        };

        assert_eq!(item.attempts, 3);
        assert_eq!(item.error_message, "SMTP failed");
    }
}
