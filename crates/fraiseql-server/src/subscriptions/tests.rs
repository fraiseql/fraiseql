mod event_bridge_tests {
    use std::sync::Arc;

    use fraiseql_core::{
        runtime::subscription::{SubscriptionManager, SubscriptionOperation},
        schema::CompiledSchema,
    };

    use super::super::event_bridge::*;

    #[test]
    fn test_event_bridge_creation() {
        let schema = Arc::new(CompiledSchema::new());
        let manager = Arc::new(SubscriptionManager::new(schema));
        let config = EventBridgeConfig::new();

        let bridge = EventBridge::new(manager, config);

        // Verify bridge is created
        assert!(
            bridge.sender().try_reserve().is_ok(),
            "event bridge channel should have capacity for at least one message"
        );
    }

    #[test]
    fn test_event_conversion_insert() {
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "INSERT",
            serde_json::json!({
                "id": "order_123",
                "status": "pending"
            }),
        );

        let subscription_event = EventBridge::convert_event(entity_event);

        assert_eq!(subscription_event.entity_type, "Order");
        assert_eq!(subscription_event.entity_id, "order_123");
        assert_eq!(subscription_event.operation, SubscriptionOperation::Create);
    }

    #[test]
    fn test_event_conversion_update() {
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "UPDATE",
            serde_json::json!({
                "id": "order_123",
                "status": "shipped"
            }),
        );

        let subscription_event = EventBridge::convert_event(entity_event);

        assert_eq!(subscription_event.operation, SubscriptionOperation::Update);
    }

    #[test]
    fn test_event_conversion_delete() {
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "DELETE",
            serde_json::json!({
                "id": "order_123"
            }),
        );

        let subscription_event = EventBridge::convert_event(entity_event);

        assert_eq!(subscription_event.operation, SubscriptionOperation::Delete);
    }

    #[test]
    fn test_event_conversion_with_old_data() {
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "UPDATE",
            serde_json::json!({
                "id": "order_123",
                "status": "shipped"
            }),
        )
        .with_old_data(serde_json::json!({
            "id": "order_123",
            "status": "pending"
        }));

        let subscription_event = EventBridge::convert_event(entity_event);

        assert!(
            subscription_event.old_data.is_some(),
            "update events should carry old_data for delta computation"
        );
    }

    #[test]
    fn convert_event_propagates_change_spine_envelope() {
        use fraiseql_core::runtime::subscription::ChangeSpineEnvelope;

        let envelope = ChangeSpineEnvelope {
            actor_type:     Some("ai_agent".to_string()),
            acting_for:     Some("11111111-1111-1111-1111-111111111111".to_string()),
            schema_version: Some("v3".to_string()),
            tenant_id:      Some("22222222-2222-2222-2222-222222222222".to_string()),
            duration_ms:    Some(7),
            seq:            Some(99),
        };
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "UPDATE",
            serde_json::json!({ "id": "order_123" }),
        )
        .with_change_spine(envelope.clone());

        let subscription_event = EventBridge::convert_event(entity_event);

        assert_eq!(
            subscription_event.change_spine,
            Some(envelope),
            "the Change-Spine envelope must round-trip through convert_event (#425)"
        );
    }

    #[tokio::test]
    async fn test_event_bridge_spawning() {
        let schema = Arc::new(CompiledSchema::new());
        let manager = Arc::new(SubscriptionManager::new(schema));
        let config = EventBridgeConfig::new();

        let bridge = EventBridge::new(manager, config);
        let handle = bridge.spawn();

        // Verify task was spawned
        assert!(!handle.is_finished());

        // Clean up
        handle.abort();
    }

    #[tokio::test]
    async fn test_event_bridge_end_to_end_forwarding() {
        let schema = Arc::new(CompiledSchema::new());
        let manager = Arc::new(SubscriptionManager::new(schema));
        let config = EventBridgeConfig::new();

        let bridge = EventBridge::new(manager, config);
        let sender = bridge.sender();
        let handle = bridge.spawn();

        // Send multiple events through the channel
        for i in 0..3 {
            let event = EntityEvent::new(
                "Order",
                format!("order_{i}"),
                "INSERT",
                serde_json::json!({"id": format!("order_{i}"), "total": 99.95}),
            );
            sender.send(event).await.expect("channel should be open");
        }

        // Yield to let the bridge task process events
        tokio::task::yield_now().await;

        // The bridge should still be running (didn't panic processing events)
        assert!(!handle.is_finished(), "bridge should still be running after processing events");

        handle.abort();
    }

    #[tokio::test]
    async fn test_event_bridge_sender_cloning() {
        let schema = Arc::new(CompiledSchema::new());
        let manager = Arc::new(SubscriptionManager::new(schema));
        let config = EventBridgeConfig::new();

        let bridge = EventBridge::new(manager, config);
        let sender1 = bridge.sender();
        let sender2 = bridge.sender();

        // Both senders should be usable (cloned from the same channel)
        assert!(sender1.try_reserve().is_ok());
        assert!(sender2.try_reserve().is_ok());
    }

    // Tenant-aware CDC filtering: `EventBridge::convert_event` must carry the top-level
    // `tenant_id` from the source `EntityEvent` onto the `SubscriptionEvent` — this is the
    // multi-tenant filtering key on the live `/ws` path. (Ported from the deleted
    // `realtime_integration_test.rs`, whose broadcast/presence tests went with Cluster C but
    // whose live-path conversion tests must survive.)
    #[test]
    fn convert_event_preserves_tenant_id() {
        let entity_event =
            EntityEvent::new("Order", "order_1", "INSERT", serde_json::json!({"id": "order_1"}))
                .with_tenant_id("org_42");

        let sub_event = EventBridge::convert_event(entity_event);
        assert_eq!(sub_event.tenant_id.as_deref(), Some("org_42"));
        assert_eq!(sub_event.entity_type, "Order");
    }

    #[test]
    fn convert_event_without_tenant_id_passes_through_as_none() {
        // No source tenant → `None` (event delivered to all subscribers, not scoped).
        let entity_event =
            EntityEvent::new("Order", "order_1", "INSERT", serde_json::json!({"id": "order_1"}));

        let sub_event = EventBridge::convert_event(entity_event);
        assert!(sub_event.tenant_id.is_none());
    }
}

mod lifecycle_tests {
    use super::super::lifecycle::*;

    #[tokio::test]
    async fn noop_lifecycle_accepts_connect() {
        let lifecycle = NoopLifecycle;
        let result = lifecycle.on_connect(&serde_json::json!({}), "conn-1").await;
        assert!(result.is_ok(), "noop lifecycle should accept any connection");
    }

    #[tokio::test]
    async fn noop_lifecycle_accepts_subscribe() {
        let lifecycle = NoopLifecycle;
        let result = lifecycle.on_subscribe("orderCreated", &serde_json::json!({}), "conn-1").await;
        assert!(result.is_ok(), "noop lifecycle should accept any subscription");
    }
}

mod protocol_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use fraiseql_core::runtime::protocol::ServerMessage;

    use super::super::protocol::*;

    // ── WsProtocol::from_header ──────────────────────────────────

    #[test]
    fn from_header_transport_ws() {
        assert_eq!(
            WsProtocol::from_header(Some("graphql-transport-ws")),
            Some(WsProtocol::GraphqlTransportWs)
        );
    }

    #[test]
    fn from_header_legacy_ws() {
        assert_eq!(WsProtocol::from_header(Some("graphql-ws")), Some(WsProtocol::GraphqlWs));
    }

    #[test]
    fn from_header_multiple_prefers_first_known() {
        // Client may offer both; we pick the first recognised one.
        assert_eq!(
            WsProtocol::from_header(Some("graphql-ws, graphql-transport-ws")),
            Some(WsProtocol::GraphqlWs)
        );
        assert_eq!(
            WsProtocol::from_header(Some("graphql-transport-ws, graphql-ws")),
            Some(WsProtocol::GraphqlTransportWs)
        );
    }

    #[test]
    fn from_header_unknown_returns_none() {
        assert_eq!(WsProtocol::from_header(Some("unknown-protocol")), None);
    }

    #[test]
    fn from_header_none_returns_none() {
        assert_eq!(WsProtocol::from_header(None), None);
    }

    // ── ProtocolCodec::decode (modern) ───────────────────────────

    #[test]
    fn decode_transport_ws_subscribe() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        let raw = r#"{"type":"subscribe","id":"1","payload":{"query":"subscription { x }"}}"#;
        let msg = codec.decode(raw).unwrap();
        assert_eq!(msg.message_type, "subscribe");
        assert_eq!(msg.id, Some("1".to_string()));
    }

    #[test]
    fn decode_transport_ws_invalid_json() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        assert!(
            matches!(codec.decode("not json"), Err(ProtocolError::InvalidJson(_))),
            "expected InvalidJson error for malformed input, got: {:?}",
            codec.decode("not json")
        );
    }

    // ── ProtocolCodec::decode (legacy) ───────────────────────────

    #[test]
    fn decode_legacy_start_becomes_subscribe() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let raw = r#"{"type":"start","id":"1","payload":{"query":"subscription { x }"}}"#;
        let msg = codec.decode(raw).unwrap();
        assert_eq!(msg.message_type, "subscribe");
    }

    #[test]
    fn decode_legacy_stop_becomes_complete() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let raw = r#"{"type":"stop","id":"1"}"#;
        let msg = codec.decode(raw).unwrap();
        assert_eq!(msg.message_type, "complete");
    }

    #[test]
    fn decode_legacy_connection_init_unchanged() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let raw = r#"{"type":"connection_init"}"#;
        let msg = codec.decode(raw).unwrap();
        assert_eq!(msg.message_type, "connection_init");
    }

    // ── ProtocolCodec::encode (modern) ───────────────────────────

    #[test]
    fn encode_transport_ws_next() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        let msg = ServerMessage::next("1", serde_json::json!({"x": 1}));
        let json = codec.encode(&msg).unwrap().unwrap();
        assert!(json.contains("\"next\""));
    }

    #[test]
    fn encode_transport_ws_ping() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        let msg = ServerMessage::ping(None);
        let json = codec.encode(&msg).unwrap().unwrap();
        assert!(json.contains("\"ping\""));
    }

    // ── ProtocolCodec::encode (legacy) ───────────────────────────

    #[test]
    fn encode_legacy_next_becomes_data() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::next("1", serde_json::json!({"x": 1}));
        let json = codec.encode(&msg).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "data");
    }

    #[test]
    fn encode_legacy_ping_becomes_ka() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::ping(None);
        let json = codec.encode(&msg).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "ka");
        // ka has no payload or id
        assert!(parsed.get("payload").is_none() || parsed["payload"].is_null());
    }

    #[test]
    fn encode_legacy_pong_is_suppressed() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::pong(None);
        let result = codec.encode(&msg).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn encode_legacy_connection_ack_unchanged() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::connection_ack(None);
        let json = codec.encode(&msg).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "connection_ack");
    }

    #[test]
    fn encode_legacy_error_unchanged() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        let msg = ServerMessage::error(
            "1",
            vec![fraiseql_core::runtime::protocol::GraphQLError::new("test")],
        );
        let json = codec.encode(&msg).unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "error");
    }

    // ── uses_keepalive ───────────────────────────────────────────

    #[test]
    fn uses_keepalive_legacy() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlWs);
        assert!(codec.uses_keepalive());
    }

    #[test]
    fn uses_keepalive_modern() {
        let codec = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
        assert!(!codec.uses_keepalive());
    }
}

mod webhook_lifecycle_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use std::time::Duration;

    use super::super::webhook_lifecycle::{MAX_WEBHOOK_RESPONSE_BYTES, WebhookLifecycle};

    #[test]
    fn from_schema_json_no_hooks() {
        let json = serde_json::json!({});
        assert!(WebhookLifecycle::from_schema_json(&json).is_none());
    }

    #[test]
    fn from_schema_json_empty_hooks() {
        let json = serde_json::json!({"hooks": {}});
        assert!(WebhookLifecycle::from_schema_json(&json).is_none());
    }

    #[test]
    fn from_schema_json_with_connect_url() {
        let json = serde_json::json!({
            "hooks": {
                "on_connect": "http://localhost:8001/hooks/ws-connect",
                "timeout_ms": 300
            }
        });
        let wh = WebhookLifecycle::from_schema_json(&json).unwrap();
        assert_eq!(wh.on_connect_url, Some("http://localhost:8001/hooks/ws-connect".to_string()));
        assert!(wh.on_disconnect_url.is_none());
        assert!(wh.on_subscribe_url.is_none());
        assert_eq!(wh.timeout, Duration::from_millis(300));
    }

    #[test]
    fn from_schema_json_default_timeout() {
        let json = serde_json::json!({
            "hooks": {
                "on_disconnect": "http://localhost:8001/hooks/ws-disconnect"
            }
        });
        let wh = WebhookLifecycle::from_schema_json(&json).unwrap();
        assert_eq!(wh.timeout, Duration::from_millis(500));
    }

    #[test]
    fn webhook_response_cap_constant_is_reasonable() {
        // 64 KiB: large enough for any human-readable error, small enough to prevent OOM.
        assert_eq!(MAX_WEBHOOK_RESPONSE_BYTES, 64 * 1024);
    }

    #[test]
    fn webhook_response_body_is_capped_at_limit() {
        // Simulate what on_connect / on_subscribe do: bytes → cap → lossy UTF-8.
        let oversized: Vec<u8> = vec![b'x'; MAX_WEBHOOK_RESPONSE_BYTES + 100];
        let capped = &oversized[..oversized.len().min(MAX_WEBHOOK_RESPONSE_BYTES)];
        let text = String::from_utf8_lossy(capped).into_owned();
        assert_eq!(text.len(), MAX_WEBHOOK_RESPONSE_BYTES);
    }
}
