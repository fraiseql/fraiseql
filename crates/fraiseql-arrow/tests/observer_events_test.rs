//! Tests for Observer Events (do_get) Arrow Flight support

use chrono::Utc;
use fraiseql_arrow::{EventStorage, HistoricalEvent};
use std::sync::Arc;
use uuid::Uuid;

/// Mock event storage for testing
struct MockEventStorage {
    events: Vec<HistoricalEvent>,
}

impl MockEventStorage {
    fn new() -> Self {
        let now = Utc::now();
        let events = vec![
            HistoricalEvent {
                id: Uuid::new_v4(),
                event_type: "INSERT".to_string(),
                entity_type: "Order".to_string(),
                entity_id: Uuid::new_v4(),
                data: serde_json::json!({"total": 100.50}),
                user_id: Some("user123".to_string()),
                tenant_id: Some("tenant1".to_string()),
                timestamp: now,
            },
            HistoricalEvent {
                id: Uuid::new_v4(),
                event_type: "UPDATE".to_string(),
                entity_type: "Order".to_string(),
                entity_id: Uuid::new_v4(),
                data: serde_json::json!({"total": 150.00, "status": "shipped"}),
                user_id: Some("user456".to_string()),
                tenant_id: Some("tenant1".to_string()),
                timestamp: now,
            },
        ];
        Self { events }
    }
}

#[async_trait::async_trait]
impl EventStorage for MockEventStorage {
    async fn query_events(
        &self,
        entity_type: &str,
        _start_date: Option<chrono::DateTime<Utc>>,
        _end_date: Option<chrono::DateTime<Utc>>,
        _limit: Option<usize>,
    ) -> Result<Vec<HistoricalEvent>, String> {
        Ok(self
            .events
            .iter()
            .filter(|e| e.entity_type == entity_type)
            .cloned()
            .collect())
    }

    async fn count_events(
        &self,
        entity_type: &str,
        _start_date: Option<chrono::DateTime<Utc>>,
        _end_date: Option<chrono::DateTime<Utc>>,
    ) -> Result<usize, String> {
        Ok(self
            .events
            .iter()
            .filter(|e| e.entity_type == entity_type)
            .count())
    }
}

#[tokio::test]
async fn test_mock_event_storage() {
    let storage = Arc::new(MockEventStorage::new());

    // Query for Order events
    let events = storage
        .query_events("Order", None, None, None)
        .await
        .expect("should query events");

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].entity_type, "Order");
    assert_eq!(events[0].event_type, "INSERT");
}

#[tokio::test]
async fn test_event_storage_count() {
    let storage = Arc::new(MockEventStorage::new());

    let count = storage
        .count_events("Order", None, None)
        .await
        .expect("should count events");

    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_event_storage_filter_non_matching_type() {
    let storage = Arc::new(MockEventStorage::new());

    let events = storage
        .query_events("Product", None, None, None)
        .await
        .expect("should query events");

    assert_eq!(events.len(), 0);
}

#[test]
fn test_historical_event_serialization() {
    let event = HistoricalEvent {
        id: Uuid::new_v4(),
        event_type: "INSERT".to_string(),
        entity_type: "Order".to_string(),
        entity_id: Uuid::new_v4(),
        data: serde_json::json!({"total": 100.50}),
        user_id: Some("user123".to_string()),
        tenant_id: Some("tenant1".to_string()),
        timestamp: Utc::now(),
    };

    // Should serialize to JSON without errors
    let json = serde_json::to_string(&event).expect("should serialize");
    assert!(json.contains("\"event_type\":\"INSERT\""));
    assert!(json.contains("\"entity_type\":\"Order\""));
}

#[test]
fn test_historical_event_with_nulls() {
    let event = HistoricalEvent {
        id: Uuid::new_v4(),
        event_type: "DELETE".to_string(),
        entity_type: "User".to_string(),
        entity_id: Uuid::new_v4(),
        data: serde_json::json!({"id": "user123"}),
        user_id: None,
        tenant_id: None,
        timestamp: Utc::now(),
    };

    assert_eq!(event.event_type, "DELETE");
    assert_eq!(event.entity_type, "User");
    assert!(event.user_id.is_none());
    assert!(event.tenant_id.is_none());
}
