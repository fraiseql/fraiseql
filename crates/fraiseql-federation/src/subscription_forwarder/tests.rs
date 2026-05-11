#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;

    // ── extract_subscription_field_name tests ───────────────────────

    #[test]
    fn test_extract_simple_field() {
        let query = "subscription { postCreated { id body } }";
        assert_eq!(
            extract_subscription_field_name(query),
            Some("postCreated".to_string())
        );
    }

    #[test]
    fn test_extract_with_operation_name() {
        let query = "subscription OnPost { postCreated { id } }";
        assert_eq!(
            extract_subscription_field_name(query),
            Some("postCreated".to_string())
        );
    }

    #[test]
    fn test_extract_with_alias() {
        let query = "subscription { myAlias: postCreated { body } }";
        assert_eq!(
            extract_subscription_field_name(query),
            Some("postCreated".to_string())
        );
    }

    #[test]
    fn test_extract_with_alias_and_op_name() {
        let query = "subscription WatchPosts { myAlias: postCreated { body } }";
        assert_eq!(
            extract_subscription_field_name(query),
            Some("postCreated".to_string())
        );
    }

    #[test]
    fn test_extract_with_variables() {
        let query = "subscription ($userId: ID!) { userUpdated(userId: $userId) { name } }";
        assert_eq!(
            extract_subscription_field_name(query),
            Some("userUpdated".to_string())
        );
    }

    #[test]
    fn test_extract_not_subscription() {
        assert_eq!(extract_subscription_field_name("query { users { id } }"), None);
    }

    #[test]
    fn test_extract_empty_body() {
        assert_eq!(extract_subscription_field_name("subscription { }"), None);
    }

    #[test]
    fn test_extract_multiline() {
        let query = r"
            subscription {
                postCreated {
                    id
                    body
                }
            }
        ";
        assert_eq!(
            extract_subscription_field_name(query),
            Some("postCreated".to_string())
        );
    }

    // ── lookup_remote_subscription tests ────────────────────────────

    #[test]
    fn test_lookup_local_field() {
        let remote = HashMap::new();
        assert!(lookup_remote_subscription("localField", &remote).is_none());
    }

    #[test]
    fn test_lookup_remote_field() {
        let mut remote = HashMap::new();
        remote.insert("postCreated".to_string(), "wss://posts.internal/graphql".to_string());
        assert_eq!(
            lookup_remote_subscription("postCreated", &remote),
            Some("wss://posts.internal/graphql")
        );
    }

    #[test]
    fn test_lookup_unknown_field_is_local() {
        let mut remote = HashMap::new();
        remote.insert("postCreated".to_string(), "wss://posts.internal/graphql".to_string());
        assert!(lookup_remote_subscription("orderCreated", &remote).is_none());
    }

    // ── http_to_ws_url tests ────────────────────────────────────────

    #[test]
    fn test_http_to_ws() {
        assert_eq!(http_to_ws_url("http://example.com/graphql"), "ws://example.com/graphql");
    }

    #[test]
    fn test_https_to_wss() {
        assert_eq!(http_to_ws_url("https://example.com/graphql"), "wss://example.com/graphql");
    }

    #[test]
    fn test_ws_passthrough() {
        assert_eq!(http_to_ws_url("wss://example.com/graphql"), "wss://example.com/graphql");
    }

    // ── ForwardError display tests ──────────────────────────────────

    #[test]
    fn test_forward_error_display() {
        let err = ForwardError::SsrfBlocked("blocked".to_string());
        assert!(err.to_string().contains("SSRF"));

        let err = ForwardError::ConnectionFailed("refused".to_string());
        assert!(err.to_string().contains("connection failed"));

        let err = ForwardError::InitFailed("timeout".to_string());
        assert!(err.to_string().contains("init failed"));

        let err = ForwardError::ProtocolError("bad frame".to_string());
        assert!(err.to_string().contains("protocol error"));
    }

    // ── SubscriptionForwarder SSRF validation ───────────────────────

    #[test]
    fn test_forwarder_rejects_localhost() {
        let result = SubscriptionForwarder::new("http://localhost:4000/graphql");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ForwardError::SsrfBlocked(_)));
    }

    #[test]
    fn test_forwarder_rejects_private_ip() {
        let result = SubscriptionForwarder::new("http://192.168.1.1:4000/graphql");
        assert!(result.is_err());
    }

    // ── ForwardedEvent variants ─────────────────────────────────────

    #[test]
    fn test_forwarded_event_next() {
        let event = ForwardedEvent::Next(serde_json::json!({"data": {"postCreated": {"id": 1}}}));
        assert!(matches!(event, ForwardedEvent::Next(_)));
    }

    #[test]
    fn test_forwarded_event_error() {
        let event = ForwardedEvent::Error(serde_json::json!([{"message": "fail"}]));
        assert!(matches!(event, ForwardedEvent::Error(_)));
    }

    #[test]
    fn test_forwarded_event_complete() {
        let event = ForwardedEvent::Complete;
        assert!(matches!(event, ForwardedEvent::Complete));
    }
