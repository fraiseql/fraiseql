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

/// Timestamps are written as stored documents hold them today: the fraction padded to
/// `.120` / `.500`, none at a whole second, and a `Z` suffix. A document read and written
/// again keeps those bytes; webhooks, idempotency keys and JSONB comparisons depend on them.
mod stored_time_format {
    use crate::event_storage::HistoricalEvent;

    #[test]
    fn a_historical_event_keeps_its_stored_timestamp() {
        let doc = serde_json::json!({"id":"00000000-0000-0000-0000-000000000001","event_type":"INSERT","entity_type":"Order","entity_id":"00000000-0000-0000-0000-000000000001","data":{},"user_id":null,"tenant_id":null,"timestamp":"2025-11-20T12:00:00.120Z"});
        let read: HistoricalEvent =
            serde_json::from_value(doc.clone()).expect("the stored document reads");
        assert_eq!(serde_json::to_value(&read).expect("it writes"), doc);
    }
}
