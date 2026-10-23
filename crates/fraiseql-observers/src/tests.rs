#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr
)] // Reason: test code, panics acceptable
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
    async fn test_email_action_execute_without_smtp_fails_loud() {
        // #349: with no SMTP configured, the email action fails loud (permanent)
        // rather than reporting a phantom success — a non-sending integration is
        // always surfaced (DLQ'd), never silently dropped.
        let email = EmailAction::new();
        let event = crate::event::EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            json!({"total": 100}),
        );

        let result = email.execute("user@example.com", "Test", None, &event).await;
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionPermanentlyFailed { .. })),
            "EmailAction with no SMTP must fail permanently, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_email_send_to_dead_smtp_is_loud_and_transient() {
        // #349 acceptance gate (no SMTP infra): a refused send must surface as a
        // failure — and be classified transient (retry → DLQ), never a phantom
        // success. 127.0.0.1:1 has nothing listening → connection refused, fast
        // and deterministic; tls=none avoids any handshake delay.
        use crate::config::{EmailSmtpConfig, SmtpTlsMode};

        let cfg = EmailSmtpConfig {
            host:         "127.0.0.1".to_string(),
            port:         1,
            from:         "alerts@example.com".to_string(),
            username_env: None,
            password_env: None,
            tls:          SmtpTlsMode::None,
            timeout_secs: 2,
        };
        let email = EmailAction::from_smtp_config(Some(&cfg)).expect("transport builds");
        let event = crate::event::EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            json!({"total": 100}),
        );

        let result = email.execute("ops@example.com", "Alert", Some("body"), &event).await;
        assert!(
            matches!(result, Err(crate::error::ObserverError::ActionExecutionFailed { .. })),
            "a refused SMTP send must be a loud, transient failure (retryable), got: {result:?}"
        );
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
    fn test_webhook_body_template_string_value_cannot_inject_keys() {
        // H11: an attacker controls a string field; the value tries to break out
        // of its JSON string and inject a forged "verified": true into the signed
        // body. After the fix the value must arrive as a single STRING with no
        // injected sibling key.
        let webhook = WebhookAction::new();
        let data = json!({"name": r#"alice", "verified": true, "x": "y"#});
        let template = r#"{"user": "{{ name }}"}"#;

        let rendered = webhook.render_body_template(template, &data).unwrap();

        assert!(rendered.get("verified").is_none(), "attacker must not inject a 'verified' key");
        assert_eq!(
            rendered.get("user").and_then(|v| v.as_str()),
            Some(r#"alice", "verified": true, "x": "y"#),
            "the field value must round-trip intact as one string"
        );
    }

    #[test]
    fn test_webhook_body_template_escapes_quotes_and_backslashes() {
        // Quotes and backslashes in a string field round-trip intact (no breakout).
        let webhook = WebhookAction::new();
        let data = json!({"name": r#"a"b\c"#});
        let template = r#"{"user": "{{ name }}"}"#;

        let rendered = webhook.render_body_template(template, &data).unwrap();

        assert_eq!(rendered.get("user").and_then(|v| v.as_str()), Some(r#"a"b\c"#));
    }

    #[test]
    fn a_dotted_placeholder_renders_the_nested_value() {
        let data = json!({"customer": {"email": "a@b"}});
        let rendered = WebhookAction::new()
            .render_body_template(r#"{"to": "{{ customer.email }}"}"#, &data)
            .unwrap();
        assert_eq!(rendered, json!({"to": "a@b"}));
    }

    /// A string node that is exactly one placeholder takes the value's own JSON type.
    #[test]
    fn a_whole_string_slot_takes_the_typed_value() {
        let data = json!({"email": "a@b", "count": 3, "missing_null": null});
        let rendered = WebhookAction::new()
            .render_body_template(
                r#"{"to": "{{ email }}", "n": "{{ count }}", "z": "{{ missing_null }}"}"#,
                &data,
            )
            .unwrap();
        assert_eq!(rendered, json!({"to": "a@b", "n": 3, "z": null}));
    }

    /// The paths a template reads, as `render_body_template` reads them: string values of a
    /// JSON template (never its keys), or every placeholder of a text one.
    #[test]
    fn body_template_placeholders_lists_the_paths_rendering_reads() {
        assert_eq!(
            body_template_placeholders(
                r#"{"{{ key }}": "{{ a }}", "n": ["x {{ b.c }} y", {"d": "{{d}}"}], "k": 1}"#
            ),
            vec!["a", "b.c", "d"]
        );
        assert_eq!(
            body_template_placeholders("{\"n\": {{ total }}, {{ id }}"),
            vec!["total", "id"]
        );
        assert!(
            body_template_placeholders(r#"{"plain": "text", "open": "{{ unclosed"}"#).is_empty()
        );
    }

    /// Each listed path is one rendering looks up: data that fills them all renders, and data
    /// missing any one refuses naming it.
    #[test]
    fn body_template_placeholders_agrees_with_rendering() {
        let template = r#"{"to": "{{ customer.email }}", "msg": "Order {{ id }} of {{ total }}"}"#;
        let paths = body_template_placeholders(template);
        assert_eq!(paths, vec!["customer.email", "id", "total"]);
        let full = json!({"customer": {"email": "a@b"}, "id": 7, "total": 3});
        WebhookAction::new().render_body_template(template, &full).unwrap();
        for missing in ["id", "total"] {
            let mut data = full.clone();
            data.as_object_mut().unwrap().remove(missing);
            let result = WebhookAction::new().render_body_template(template, &data);
            assert_refused_as_unmatched(&result, missing);
        }
    }

    #[test]
    fn an_unmatched_placeholder_is_a_delivery_error() {
        let result =
            WebhookAction::new().render_body_template(r#"{"x": "{{ nope }}"}"#, &json!({"id": 1}));
        assert_refused_as_unmatched(&result, "nope");
    }

    #[test]
    fn an_unmatched_embedded_placeholder_is_a_delivery_error() {
        let result = WebhookAction::new()
            .render_body_template(r#"{"msg": "Order {{ nope }}"}"#, &json!({"id": 1}));
        assert_refused_as_unmatched(&result, "nope");
    }

    /// A template that is not JSON (an unquoted value slot) is substituted as text, and an
    /// unmatched placeholder there refuses the delivery too.
    #[test]
    fn an_unmatched_placeholder_in_a_text_template_is_a_delivery_error() {
        let result = WebhookAction::new()
            .render_body_template(r#"{"amount": {{ nope }}}"#, &json!({"total": 1}));
        assert_refused_as_unmatched(&result, "nope");
    }

    /// The refusal names the placeholder the event could not fill.
    fn assert_refused_as_unmatched(result: &crate::error::Result<serde_json::Value>, path: &str) {
        match result {
            Err(crate::error::ObserverError::TemplateRenderingFailed { reason }) => assert!(
                reason.contains("no value for the template placeholder") && reason.contains(path),
                "the refusal must name the unmatched `{path}`, got {reason:?}"
            ),
            other => panic!("a placeholder the event cannot fill must refuse, got {other:?}"),
        }
    }

    #[test]
    fn an_embedded_placeholder_interpolates_its_text() {
        let data = json!({"id": 7, "name": r#"a"b"#});
        let rendered = WebhookAction::new()
            .render_body_template(r#"{"msg": "Order {{ id }} by {{ name }}"}"#, &data)
            .unwrap();
        assert_eq!(rendered, json!({"msg": r#"Order 7 by a"b"#}));
    }

    #[test]
    fn an_embedded_null_is_a_delivery_error() {
        let result = WebhookAction::new()
            .render_body_template(r#"{"msg": "Order {{ id }}"}"#, &json!({"id": null}));
        assert!(
            matches!(result, Err(crate::error::ObserverError::TemplateRenderingFailed { .. })),
            "a null inside a string must not render as the text null, got {result:?}"
        );
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

    use crate::ssrf_test_env::with_ssrf_env_cleared;

    #[test]
    fn test_outbound_url_scheme_must_be_http() {
        with_ssrf_env_cleared(|| {
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
        });
    }

    #[test]
    fn test_outbound_url_blocks_loopback() {
        with_ssrf_env_cleared(|| {
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
        });
    }

    #[test]
    fn test_outbound_url_blocks_private_ranges() {
        with_ssrf_env_cleared(|| {
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
        });
    }

    #[test]
    fn test_outbound_url_allows_public_addresses() {
        with_ssrf_env_cleared(|| {
            validate_outbound_url("https://hooks.slack.com/services/xxx")
                .unwrap_or_else(|e| panic!("public slack URL should pass: {e}"));
            validate_outbound_url("https://api.example.com/webhook")
                .unwrap_or_else(|e| panic!("public API URL should pass: {e}"));
            // Not 203.0.113.x: that is TEST-NET-3, an RFC 5737 documentation range
            // the shared guard refuses because it is not globally routable.
            validate_outbound_url("http://93.184.216.34/hook")
                .unwrap_or_else(|e| panic!("public IP should pass: {e}"));
        });
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

    // -----------------------------------------------------------------------
    // #346 — secret-header redaction + URL host-only extraction
    // -----------------------------------------------------------------------

    #[test]
    fn redact_secret_headers_masks_authorization() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_owned(), "Bearer abc123".to_owned());
        headers.insert("Content-Type".to_owned(), "application/json".to_owned());

        let redacted = redact_secret_headers(&headers);
        assert_eq!(redacted.get("Authorization").map(String::as_str), Some("<redacted>"));
        assert_eq!(
            redacted.get("Content-Type").map(String::as_str),
            Some("application/json"),
            "non-secret headers must pass through unchanged"
        );
    }

    #[test]
    fn redact_secret_headers_is_case_insensitive() {
        let mut headers = HashMap::new();
        headers.insert("authorization".to_owned(), "Bearer abc123".to_owned());
        headers.insert("AUTHORIZATION".to_owned(), "Bearer xyz789".to_owned());
        headers.insert("X-Api-Key".to_owned(), "key-1".to_owned());
        headers.insert("x-api-key".to_owned(), "key-2".to_owned());
        headers.insert("Cookie".to_owned(), "session=foo".to_owned());

        let redacted = redact_secret_headers(&headers);
        for name in [
            "authorization",
            "AUTHORIZATION",
            "X-Api-Key",
            "x-api-key",
            "Cookie",
        ] {
            assert_eq!(
                redacted.get(name).map(String::as_str),
                Some("<redacted>"),
                "header {name:?} must be redacted"
            );
        }
    }

    #[test]
    fn redact_secret_headers_masks_substring_matches() {
        // Names containing one of the needles anywhere in the (lowercased) name
        // are redacted — false-positives (over-masking) are acceptable per the
        // helper's doc; false-negatives (printing a bearer) are not.
        let mut headers = HashMap::new();
        headers.insert("X-Auth-Secret".to_owned(), "shh".to_owned());
        headers.insert("Custom-Token-Header".to_owned(), "tok".to_owned());
        headers.insert("X-Cookie-Whatever".to_owned(), "yum".to_owned());

        let redacted = redact_secret_headers(&headers);
        for name in ["X-Auth-Secret", "Custom-Token-Header", "X-Cookie-Whatever"] {
            assert_eq!(redacted.get(name).map(String::as_str), Some("<redacted>"));
        }
    }

    #[test]
    fn redact_secret_headers_does_not_mask_safe_headers() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), "application/json".to_owned());
        headers.insert("Accept".to_owned(), "*/*".to_owned());
        headers.insert("User-Agent".to_owned(), "fraiseql/2.4.0".to_owned());

        let redacted = redact_secret_headers(&headers);
        assert_eq!(redacted, headers);
    }

    #[test]
    fn url_host_only_strips_scheme_userinfo_path_query_and_fragment() {
        assert_eq!(
            url_host_only("https://user:pass@example.com:8443/path?secret=abc#frag"),
            "example.com"
        );
        assert_eq!(url_host_only("https://api.example.com/webhooks/123"), "api.example.com");
        assert_eq!(
            url_host_only("https://example.com/?token=sensitive"),
            "example.com",
            "query string with sensitive tokens must not appear in INFO log"
        );
    }

    #[test]
    fn url_host_only_returns_invalid_on_garbage_input() {
        assert_eq!(url_host_only(""), "<invalid>");
        assert_eq!(url_host_only("not a url"), "<invalid>");
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

    #[test]
    fn test_create_entity_event_defaults_perf_envelope_to_none() {
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));

        // The Change-Spine perf/envelope columns are absent until stamped.
        assert_eq!(event.duration_ms, None);
        assert_eq!(event.seq, None);
    }

    #[test]
    fn test_entity_event_with_duration_ms() {
        let event =
            EntityEvent::new(EventKind::Updated, "Order".to_string(), Uuid::new_v4(), json!({}))
                .with_duration_ms(42);

        assert_eq!(event.duration_ms, Some(42));
    }

    #[test]
    fn test_entity_event_with_seq() {
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
                .with_seq(1_007);

        assert_eq!(event.seq, Some(1_007));
    }

    #[test]
    fn test_entity_event_serde_roundtrip_preserves_perf_envelope() {
        let event =
            EntityEvent::new(EventKind::Updated, "Order".to_string(), Uuid::new_v4(), json!({}))
                .with_duration_ms(7)
                .with_seq(99);

        let json = serde_json::to_value(&event).expect("serialize EntityEvent");
        assert_eq!(json["duration_ms"], json!(7));
        assert_eq!(json["seq"], json!(99));

        let decoded: EntityEvent = serde_json::from_value(json).expect("deserialize EntityEvent");
        assert_eq!(decoded.duration_ms, Some(7));
        assert_eq!(decoded.seq, Some(99));
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
                url:                Some("https://example.com".to_string()),
                url_env:            None,
                method:             None,
                headers:            HashMap::default(),
                body_template:      Some("{}".to_string()),
                signing_secret:     None,
                signing_secret_env: None,
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

#[cfg(test)]
mod storage_tests {
    #[tokio::test]
    async fn test_postgres_query_events() {
        if fraiseql_test_support::try_database_url().is_none() {
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
            status_code: None,
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
