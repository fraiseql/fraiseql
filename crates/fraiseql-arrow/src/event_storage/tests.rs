use super::*;

#[test]
fn test_historical_event_creation() {
    let event = HistoricalEvent {
        id:          Uuid::new_v4(),
        event_type:  "INSERT".to_string(),
        entity_type: "Order".to_string(),
        entity_id:   Uuid::new_v4(),
        data:        serde_json::json!({"total": 100.50}),
        user_id:     Some("user123".to_string()),
        tenant_id:   Some("tenant1".to_string()),
        timestamp:   Utc::now(),
    };

    assert_eq!(event.event_type, "INSERT");
    assert_eq!(event.entity_type, "Order");
}
