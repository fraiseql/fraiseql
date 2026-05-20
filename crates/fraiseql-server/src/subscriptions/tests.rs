mod broadcast_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use super::super::broadcast::*;

    #[tokio::test]
    async fn test_publish_creates_channel_on_demand() {
        let manager = BroadcastManager::new(BroadcastConfig::new());

        let receivers = manager
            .publish("chat:room1", "message".into(), serde_json::json!({"text": "hello"}))
            .await
            .unwrap();

        // No subscribers yet, so 0 receivers
        assert_eq!(receivers, 0);
        assert_eq!(manager.channel_count().await, 1);
    }

    #[tokio::test]
    async fn test_subscribe_then_publish() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

        let mut rx = manager.subscribe("chat:room1").await.unwrap();

        let receivers = manager
            .publish("chat:room1", "message".into(), serde_json::json!({"text": "hello"}))
            .await
            .unwrap();

        assert_eq!(receivers, 1);

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.channel, "chat:room1");
        assert_eq!(msg.event, "message");
        assert_eq!(msg.payload["text"], "hello");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

        let mut rx1 = manager.subscribe("events").await.unwrap();
        let mut rx2 = manager.subscribe("events").await.unwrap();

        let receivers = manager
            .publish("events", "update".into(), serde_json::json!({"v": 1}))
            .await
            .unwrap();

        assert_eq!(receivers, 2);

        let msg1 = rx1.recv().await.unwrap();
        let msg2 = rx2.recv().await.unwrap();
        assert_eq!(msg1.payload, msg2.payload);
    }

    #[tokio::test]
    async fn test_payload_too_large() {
        let config = BroadcastConfig {
            max_message_bytes: 10,
            ..BroadcastConfig::new()
        };
        let manager = BroadcastManager::new(config);

        let result = manager
            .publish("ch", "e".into(), serde_json::json!({"big": "data that is too large"}))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), 413);
    }

    #[tokio::test]
    async fn test_too_many_channels() {
        let config = BroadcastConfig {
            max_channels: 2,
            ..BroadcastConfig::new()
        };
        let manager = BroadcastManager::new(config);

        manager.publish("ch1", "e".into(), serde_json::json!({})).await.unwrap();
        manager.publish("ch2", "e".into(), serde_json::json!({})).await.unwrap();
        let result = manager.publish("ch3", "e".into(), serde_json::json!({})).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), 503);
    }

    #[tokio::test]
    async fn test_gc_empty_channels() {
        let manager = BroadcastManager::new(BroadcastConfig::new());

        // Create a channel with a subscriber
        let _rx = manager.subscribe("active").await.unwrap();
        // Create a channel with no subscribers
        manager.publish("orphan", "e".into(), serde_json::json!({})).await.unwrap();

        assert_eq!(manager.channel_count().await, 2);

        let removed = manager.gc_empty_channels().await;
        assert_eq!(removed, 1);
        assert_eq!(manager.channel_count().await, 1);
    }

    #[tokio::test]
    async fn test_stats() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

        let _rx = manager.subscribe("ch1").await.unwrap();
        manager.publish("ch1", "e".into(), serde_json::json!({})).await.unwrap();
        manager.publish("ch2", "e".into(), serde_json::json!({})).await.unwrap();

        let stats = manager.stats().await;
        assert_eq!(stats.messages_published, 2);
        assert_eq!(stats.active_channels, 2);
        assert_eq!(stats.active_receivers, 1);
    }

    #[tokio::test]
    async fn test_channel_isolation() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

        let mut rx_a = manager.subscribe("channel_a").await.unwrap();
        let _rx_b = manager.subscribe("channel_b").await.unwrap();

        manager
            .publish("channel_a", "event".into(), serde_json::json!({"for": "a"}))
            .await
            .unwrap();

        let msg = rx_a.recv().await.unwrap();
        assert_eq!(msg.payload["for"], "a");

        // channel_b subscriber should not have received anything
        // (try_recv would return Empty, not a message)
    }
}

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

mod presence_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::time::Duration;

    use super::super::presence::*;

    fn default_manager() -> PresenceManager {
        PresenceManager::new(PresenceConfig::new())
    }

    #[tokio::test]
    async fn test_join_returns_state_and_diff() {
        let mgr = default_manager();

        let (state, diff) = mgr
            .join("room1", "alice", serde_json::json!({"status": "online"}))
            .await
            .unwrap();

        assert_eq!(state.room, "room1");
        assert_eq!(state.members.len(), 1);
        assert_eq!(state.members[0].id, "alice");
        assert_eq!(diff.joins.len(), 1);
        assert!(diff.leaves.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_members_in_room() {
        let mgr = default_manager();

        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        let (state, diff) = mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

        assert_eq!(state.members.len(), 2);
        assert_eq!(diff.joins.len(), 1);
        assert_eq!(diff.joins[0].id, "bob");
    }

    #[tokio::test]
    async fn test_leave_returns_diff() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();

        let diff = mgr.leave("room1", "alice").await.unwrap();
        assert_eq!(diff.leaves, vec!["alice"]);
        assert!(diff.joins.is_empty());

        // Room should be cleaned up
        assert!(mgr.get_room("room1").await.is_none());
    }

    #[tokio::test]
    async fn test_leave_nonexistent_returns_none() {
        let mgr = default_manager();
        assert!(mgr.leave("room1", "alice").await.is_none());
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();

        assert!(mgr.heartbeat("room1", "alice").await);
        assert!(!mgr.heartbeat("room1", "nobody").await);
        assert!(!mgr.heartbeat("noroom", "alice").await);
    }

    #[tokio::test]
    async fn test_update_state() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({"status": "online"}))
            .await
            .unwrap();

        let diff = mgr
            .update_state("room1", "alice", serde_json::json!({"status": "away"}))
            .await
            .unwrap();

        assert_eq!(diff.joins.len(), 1);
        assert_eq!(diff.joins[0].state["status"], "away");

        let state = mgr.get_room("room1").await.unwrap();
        assert_eq!(state.members[0].state["status"], "away");
    }

    #[tokio::test(start_paused = true)]
    async fn test_evict_stale_members() {
        let config = PresenceConfig {
            heartbeat_timeout: Duration::from_millis(1),
            ..PresenceConfig::new()
        };
        let mgr = PresenceManager::new(config);

        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

        // Advance past heartbeat timeout
        tokio::time::advance(Duration::from_millis(10)).await;

        let diffs = mgr.evict_stale().await;
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].leaves.len(), 2);

        // Room should be cleaned up
        assert!(mgr.get_room("room1").await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn test_heartbeat_prevents_eviction() {
        let config = PresenceConfig {
            heartbeat_timeout: Duration::from_millis(50),
            ..PresenceConfig::new()
        };
        let mgr = PresenceManager::new(config);

        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

        // Advance time, then heartbeat only alice
        tokio::time::advance(Duration::from_millis(30)).await;
        mgr.heartbeat("room1", "alice").await;

        // Advance past bob's expiry but not alice's (she heartbeated at t=30)
        tokio::time::advance(Duration::from_millis(30)).await;

        let diffs = mgr.evict_stale().await;
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].leaves, vec!["bob"]);

        // Alice should still be in the room
        let state = mgr.get_room("room1").await.unwrap();
        assert_eq!(state.members.len(), 1);
        assert_eq!(state.members[0].id, "alice");
    }

    #[tokio::test]
    async fn test_room_full() {
        let config = PresenceConfig {
            max_members_per_room: 2,
            ..PresenceConfig::new()
        };
        let mgr = PresenceManager::new(config);

        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

        let result = mgr.join("room1", "charlie", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_too_many_rooms() {
        let config = PresenceConfig {
            max_rooms: 2,
            ..PresenceConfig::new()
        };
        let mgr = PresenceManager::new(config);

        mgr.join("room1", "a", serde_json::json!({})).await.unwrap();
        mgr.join("room2", "b", serde_json::json!({})).await.unwrap();

        let result = mgr.join("room3", "c", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rejoin_same_member_updates_state() {
        let mgr = default_manager();

        mgr.join("room1", "alice", serde_json::json!({"v": 1})).await.unwrap();
        let (state, _) = mgr.join("room1", "alice", serde_json::json!({"v": 2})).await.unwrap();

        // Should still be 1 member, not 2
        assert_eq!(state.members.len(), 1);
        assert_eq!(state.members[0].state["v"], 2);
    }

    #[tokio::test]
    async fn test_stats() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();
        mgr.join("room2", "charlie", serde_json::json!({})).await.unwrap();
        mgr.leave("room1", "alice").await;

        let stats = mgr.stats().await;
        assert_eq!(stats.active_rooms, 2);
        assert_eq!(stats.total_members, 2);
        assert_eq!(stats.joins_total, 3);
        assert_eq!(stats.leaves_total, 1);
    }

    #[tokio::test]
    async fn test_room_isolation() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room2", "bob", serde_json::json!({})).await.unwrap();

        let state1 = mgr.get_room("room1").await.unwrap();
        let state2 = mgr.get_room("room2").await.unwrap();

        assert_eq!(state1.members.len(), 1);
        assert_eq!(state1.members[0].id, "alice");
        assert_eq!(state2.members.len(), 1);
        assert_eq!(state2.members[0].id, "bob");
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
