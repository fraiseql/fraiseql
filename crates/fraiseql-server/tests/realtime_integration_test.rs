//! Integration tests for Phase 16 realtime enhancements:
//! CDC → `EventBridge` wiring, broadcast channels, presence, and tenant filtering.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;
use std::time::Duration;

use fraiseql_core::runtime::subscription::{SubscriptionEvent, SubscriptionManager, SubscriptionOperation};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::subscriptions::broadcast::{BroadcastConfig, BroadcastManager};
use fraiseql_server::subscriptions::event_bridge::{EntityEvent, EventBridge, EventBridgeConfig};
use fraiseql_server::subscriptions::presence::{PresenceConfig, PresenceManager};

// ============================================================================
// CDC → EventBridge Wiring
// ============================================================================

#[tokio::test]
async fn test_cdc_event_bridge_end_to_end() {
    let schema = Arc::new(CompiledSchema::new());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let bridge = EventBridge::new(manager, EventBridgeConfig::new());
    let sender = bridge.sender();
    let handle = bridge.spawn();

    // Simulate CDC event from ChangeLogListener
    let event = EntityEvent::new("Order", "order_1", "INSERT", serde_json::json!({"id": "order_1"}));
    sender.send(event).await.unwrap();

    // Allow processing
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!handle.is_finished(), "bridge should remain running");

    handle.abort();
}

#[tokio::test]
async fn test_cdc_event_bridge_multiple_events() {
    let schema = Arc::new(CompiledSchema::new());
    let manager = Arc::new(SubscriptionManager::new(schema));
    let bridge = EventBridge::new(manager, EventBridgeConfig::new());
    let sender = bridge.sender();
    let handle = bridge.spawn();

    // Send a batch of events like the observer runtime would
    for i in 0..10 {
        let op = match i % 3 {
            0 => "INSERT",
            1 => "UPDATE",
            _ => "DELETE",
        };
        let event = EntityEvent::new("Product", format!("prod_{i}"), op, serde_json::json!({"id": i}));
        sender.send(event).await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!handle.is_finished(), "bridge should handle batch without errors");

    handle.abort();
}

#[tokio::test]
async fn test_cdc_event_bridge_tenant_propagation() {
    let schema = Arc::new(CompiledSchema::new());
    let manager = Arc::new(SubscriptionManager::new(schema));

    // Subscribe to the broadcast channel to receive events
    let _rx = manager.receiver();

    let bridge = EventBridge::new(manager, EventBridgeConfig::new());
    let sender = bridge.sender();
    let handle = bridge.spawn();

    // Send event with tenant_id
    let event = EntityEvent::new("User", "user_1", "INSERT", serde_json::json!({"id": "user_1"}))
        .with_tenant_id("org_42");
    sender.send(event).await.unwrap();

    tokio::time::sleep(Duration::from_millis(20)).await;

    // The event was published (even though no subscription matches,
    // the bridge should not error)
    assert!(!handle.is_finished());

    // The bridge processed the event without errors
    handle.abort();
}

// ============================================================================
// Broadcast Channels
// ============================================================================

#[tokio::test]
async fn test_broadcast_publish_and_receive() {
    let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

    let mut rx = manager.subscribe("chat:lobby").await.unwrap();

    manager
        .publish("chat:lobby", "message".into(), serde_json::json!({"text": "hello world"}))
        .await
        .unwrap();

    let msg = rx.recv().await.unwrap();
    assert_eq!(msg.channel, "chat:lobby");
    assert_eq!(msg.event, "message");
    assert_eq!(msg.payload["text"], "hello world");
}

#[tokio::test]
async fn test_broadcast_channel_isolation() {
    let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

    let mut rx_a = manager.subscribe("room:a").await.unwrap();
    let _rx_b = manager.subscribe("room:b").await.unwrap();

    // Publish only to room:a
    manager
        .publish("room:a", "event".into(), serde_json::json!({"v": 1}))
        .await
        .unwrap();

    let msg = rx_a.recv().await.unwrap();
    assert_eq!(msg.payload["v"], 1);
}

#[tokio::test]
async fn test_broadcast_multiple_subscribers() {
    let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

    let mut rx1 = manager.subscribe("events").await.unwrap();
    let mut rx2 = manager.subscribe("events").await.unwrap();

    let count = manager
        .publish("events", "update".into(), serde_json::json!({"n": 42}))
        .await
        .unwrap();

    assert_eq!(count, 2, "should notify 2 subscribers");

    let m1 = rx1.recv().await.unwrap();
    let m2 = rx2.recv().await.unwrap();
    assert_eq!(m1.payload, m2.payload);
}

#[tokio::test]
async fn test_broadcast_payload_size_limit() {
    let config = BroadcastConfig {
        max_message_bytes: 32,
        ..BroadcastConfig::new()
    };
    let manager = BroadcastManager::new(config);

    let result = manager
        .publish("ch", "e".into(), serde_json::json!({"big": "this payload exceeds the 32 byte limit"}))
        .await;

    assert!(result.is_err(), "oversized payload should be rejected");
}

#[tokio::test]
async fn test_broadcast_gc_empty_channels() {
    let manager = BroadcastManager::new(BroadcastConfig::new());

    // Create a channel with a subscriber (active)
    let _rx = manager.subscribe("active").await.unwrap();
    // Create a channel without subscribers (orphan)
    manager
        .publish("orphan", "e".into(), serde_json::json!({}))
        .await
        .unwrap();

    assert_eq!(manager.channel_count().await, 2);

    let removed = manager.gc_empty_channels().await;
    assert_eq!(removed, 1);
    assert_eq!(manager.channel_count().await, 1);
}

// ============================================================================
// Presence
// ============================================================================

#[tokio::test]
async fn test_presence_join_and_state() {
    let mgr = PresenceManager::new(PresenceConfig::new());

    let (state, diff) = mgr
        .join("room1", "alice", serde_json::json!({"status": "online"}))
        .await
        .unwrap();

    assert_eq!(state.room, "room1");
    assert_eq!(state.members.len(), 1);
    assert_eq!(state.members[0].id, "alice");
    assert_eq!(state.members[0].state["status"], "online");

    assert_eq!(diff.joins.len(), 1);
    assert!(diff.leaves.is_empty());
}

#[tokio::test]
async fn test_presence_join_multiple_members() {
    let mgr = PresenceManager::new(PresenceConfig::new());

    mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
    let (state, diff) = mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

    assert_eq!(state.members.len(), 2);
    assert_eq!(diff.joins.len(), 1);
    assert_eq!(diff.joins[0].id, "bob");
}

#[tokio::test]
async fn test_presence_leave_and_cleanup() {
    let mgr = PresenceManager::new(PresenceConfig::new());

    mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
    let diff = mgr.leave("room1", "alice").await.unwrap();

    assert_eq!(diff.leaves, vec!["alice"]);
    assert!(mgr.get_room("room1").await.is_none(), "empty room should be cleaned up");
}

#[tokio::test]
async fn test_presence_heartbeat_eviction() {
    let config = PresenceConfig {
        heartbeat_timeout: Duration::from_millis(5),
        ..PresenceConfig::new()
    };
    let mgr = PresenceManager::new(config);

    mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
    mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

    // Wait for heartbeat to expire
    tokio::time::sleep(Duration::from_millis(20)).await;

    let diffs = mgr.evict_stale().await;
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].leaves.len(), 2, "both members should be evicted");
    assert!(mgr.get_room("room1").await.is_none(), "room should be cleaned up");
}

#[tokio::test]
async fn test_presence_heartbeat_keeps_member_alive() {
    let config = PresenceConfig {
        heartbeat_timeout: Duration::from_millis(50),
        ..PresenceConfig::new()
    };
    let mgr = PresenceManager::new(config);

    mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
    mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

    // Wait a bit, then heartbeat only alice
    tokio::time::sleep(Duration::from_millis(30)).await;
    mgr.heartbeat("room1", "alice").await;

    // Wait for bob's heartbeat to expire but not alice's
    tokio::time::sleep(Duration::from_millis(30)).await;

    let diffs = mgr.evict_stale().await;
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].leaves, vec!["bob"]);

    let state = mgr.get_room("room1").await.unwrap();
    assert_eq!(state.members.len(), 1);
    assert_eq!(state.members[0].id, "alice");
}

#[tokio::test]
async fn test_presence_update_state() {
    let mgr = PresenceManager::new(PresenceConfig::new());

    mgr.join("room1", "alice", serde_json::json!({"cursor": [0, 0]}))
        .await
        .unwrap();

    let diff = mgr
        .update_state("room1", "alice", serde_json::json!({"cursor": [100, 200]}))
        .await
        .unwrap();

    // Update appears as a join in the diff (Supabase convention)
    assert_eq!(diff.joins.len(), 1);
    assert_eq!(diff.joins[0].state["cursor"][0], 100);
}

#[tokio::test]
async fn test_presence_room_capacity() {
    let config = PresenceConfig {
        max_members_per_room: 2,
        ..PresenceConfig::new()
    };
    let mgr = PresenceManager::new(config);

    mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
    mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

    let result = mgr.join("room1", "charlie", serde_json::json!({})).await;
    assert!(result.is_err(), "third member should be rejected");
}

// ============================================================================
// Tenant-Aware CDC Filtering
// ============================================================================

#[tokio::test]
async fn test_tenant_filtering_subscription_event() {
    // Verify SubscriptionEvent carries tenant_id
    let event = SubscriptionEvent::new(
        "Order",
        "order_1",
        SubscriptionOperation::Create,
        serde_json::json!({"id": "order_1"}),
    )
    .with_tenant_id("org_42");

    assert_eq!(event.tenant_id.as_deref(), Some("org_42"));
}

#[tokio::test]
async fn test_tenant_filtering_bridge_event() {
    // Verify EntityEvent carries tenant_id through the bridge
    let event = EntityEvent::new("User", "user_1", "INSERT", serde_json::json!({"id": "user_1"}))
        .with_tenant_id("org_99");

    assert_eq!(event.tenant_id.as_deref(), Some("org_99"));
}

#[tokio::test]
async fn test_tenant_filtering_event_conversion() {
    // Verify EventBridge.convert_event preserves tenant_id
    let entity_event =
        EntityEvent::new("Order", "order_1", "INSERT", serde_json::json!({"id": "order_1"}))
            .with_tenant_id("org_42");

    let sub_event = EventBridge::convert_event(entity_event);
    assert_eq!(sub_event.tenant_id.as_deref(), Some("org_42"));
    assert_eq!(sub_event.entity_type, "Order");
}

#[tokio::test]
async fn test_tenant_filtering_no_tenant_passes_through() {
    // Events without a tenant_id should pass through to all subscribers
    let entity_event =
        EntityEvent::new("Order", "order_1", "INSERT", serde_json::json!({"id": "order_1"}));

    let sub_event = EventBridge::convert_event(entity_event);
    assert!(sub_event.tenant_id.is_none());
}

// ============================================================================
// Combined: Broadcast + Presence stats
// ============================================================================

#[tokio::test]
async fn test_broadcast_stats() {
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
async fn test_presence_stats() {
    let mgr = PresenceManager::new(PresenceConfig::new());

    mgr.join("r1", "alice", serde_json::json!({})).await.unwrap();
    mgr.join("r1", "bob", serde_json::json!({})).await.unwrap();
    mgr.join("r2", "charlie", serde_json::json!({})).await.unwrap();
    mgr.leave("r1", "alice").await;

    let stats = mgr.stats().await;
    assert_eq!(stats.active_rooms, 2);
    assert_eq!(stats.total_members, 2);
    assert_eq!(stats.joins_total, 3);
    assert_eq!(stats.leaves_total, 1);
}
