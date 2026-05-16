#![allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions

use super::*;

#[test]
fn test_store_data_creation() {
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };
    let limits = ResourceLimits::default();

    let store = StoreData::new(event, limits);

    assert_eq!(store.event_payload.trigger_type, "test");
    assert_eq!(store.logs.len(), 0);
    assert_eq!(store.memory_peak_bytes, 0);
}

#[test]
fn test_store_data_log_respects_limit() {
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };
    let limits = ResourceLimits {
        max_memory_bytes: 128 * 1024 * 1024,
        max_duration:     std::time::Duration::from_secs(5),
        max_log_entries:  3,
    };

    let mut store = StoreData::new(event, limits);

    store.log_message(LogLevel::Info, "log 1");
    store.log_message(LogLevel::Info, "log 2");
    store.log_message(LogLevel::Info, "log 3");
    store.log_message(LogLevel::Info, "log 4 (should be dropped)");
    store.log_message(LogLevel::Info, "log 5 (should be dropped)");

    assert_eq!(store.logs.len(), 3);
    assert_eq!(store.logs[0].message, "log 1");
    assert_eq!(store.logs[1].message, "log 2");
    assert_eq!(store.logs[2].message, "log 3");
}

#[test]
fn test_store_data_get_event_payload() {
    let event = EventPayload {
        trigger_type: "mutation".to_string(),
        entity:       "User".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"id": 42}),
        timestamp:    chrono::Utc::now(),
    };
    let store = StoreData::new(event, ResourceLimits::default());

    let json = store.get_event_payload_json().expect("serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");

    assert_eq!(parsed["trigger_type"], "mutation");
    assert_eq!(parsed["entity"], "User");
    assert_eq!(parsed["event_kind"], "created");
    assert_eq!(parsed["data"]["id"], 42);
}

#[test]
fn test_store_data_host_context_typed() {
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };
    let mut store = StoreData::new(event.clone(), ResourceLimits::default());

    // Verify host_context starts as None
    assert!(store.host_context.is_none());

    // Set host context using NoopHostContext
    let noop = Arc::new(crate::host::NoopHostContext::new(event));
    store.set_host_context(noop);

    // Verify it's set and callable without downcasting
    let host = store.require_host_context().expect("host context set");
    assert_eq!(host.event_payload().trigger_type, "test");
}
