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
        max_log_entries:  3, // Only allow 3 logs
    };

    let mut store = StoreData::new(event, limits);

    // Log more than the limit
    store.log(LogLevel::Info, "log 1");
    store.log(LogLevel::Info, "log 2");
    store.log(LogLevel::Info, "log 3");
    store.log(LogLevel::Info, "log 4 (should be dropped)");
    store.log(LogLevel::Info, "log 5 (should be dropped)");

    // Only 3 logs should be stored
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
