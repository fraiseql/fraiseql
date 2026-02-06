//! Event storage interface for historical observer event querying.
//!
//! This module provides a trait for querying historical observer events
//! that have been stored in a persistent backend. Implementations can be
//! provided by the application (e.g., wrapping fraiseql-observers' storage).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An observer event record for Arrow Flight querying.
///
/// This is a simplified representation suitable for analytics queries.
/// The full EntityEvent structure is in fraiseql-observers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalEvent {
    /// Unique event ID
    pub id: Uuid,
    /// Event type: "INSERT", "UPDATE", "DELETE", or "CUSTOM"
    pub event_type: String,
    /// Entity type (e.g., "Order", "User")
    pub entity_type: String,
    /// Entity instance ID
    pub entity_id: Uuid,
    /// Event data as JSON
    pub data: serde_json::Value,
    /// User who triggered the event (if available)
    pub user_id: Option<String>,
    /// Tenant ID for multi-tenant systems (if applicable)
    pub tenant_id: Option<String>,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
}

/// Trait for querying historical events from storage.
///
/// Implementations should handle date range filtering, entity type filtering,
/// and result limiting efficiently (with appropriate database indexes).
///
/// # Example Implementations
///
/// - PostgreSQL: Query from a `fraiseql_events` table
/// - DuckDB: Query from Parquet files in cloud storage
/// - ClickHouse: Query from a distributed events table
#[async_trait]
pub trait EventStorage: Send + Sync {
    /// Query historical events by entity type and optional date range.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - Filter events for this entity type (e.g., "Order", "User")
    /// * `start_date` - Optional start of date range (inclusive)
    /// * `end_date` - Optional end of date range (inclusive)
    /// * `limit` - Maximum number of events to return
    ///
    /// # Returns
    ///
    /// Events matching the filters, sorted by timestamp descending (most recent first)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails or events cannot be deserialized.
    async fn query_events(
        &self,
        entity_type: &str,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<HistoricalEvent>, String>;

    /// Count events matching the filter criteria.
    ///
    /// Useful for pagination and understanding result sizes.
    async fn count_events(
        &self,
        entity_type: &str,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> Result<usize, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_historical_event_creation() {
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

        assert_eq!(event.event_type, "INSERT");
        assert_eq!(event.entity_type, "Order");
    }
}
