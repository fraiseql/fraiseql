//! Event types and data structures for the observer system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type of database event that triggered the observer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventKind {
    /// Entity was created
    #[serde(rename = "INSERT")]
    Created,
    /// Entity was updated
    #[serde(rename = "UPDATE")]
    Updated,
    /// Entity was deleted
    #[serde(rename = "DELETE")]
    Deleted,
    /// Custom event type
    #[serde(rename = "CUSTOM")]
    Custom,
}

impl EventKind {
    /// Convert to string representation
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            EventKind::Created => "INSERT",
            EventKind::Updated => "UPDATE",
            EventKind::Deleted => "DELETE",
            EventKind::Custom => "CUSTOM",
        }
    }
}

/// Changes to a field (old vs new value)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChanges {
    /// Old value before update
    pub old: serde_json::Value,
    /// New value after update
    pub new: serde_json::Value,
}

/// Entity event from database mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityEvent {
    /// Unique event identifier
    pub id:          Uuid,
    /// Type of event (INSERT, UPDATE, DELETE, CUSTOM)
    pub event_type:  EventKind,
    /// Entity type name (e.g., "Order", "User", "Product")
    pub entity_type: String,
    /// Entity instance ID
    pub entity_id:   Uuid,
    /// Current entity data
    pub data:        serde_json::Value,
    /// Field changes (for UPDATE events)
    pub changes:     Option<std::collections::HashMap<String, FieldChanges>>,
    /// User ID from auth context (if available)
    pub user_id:     Option<String>,
    /// When the event occurred
    pub timestamp:   DateTime<Utc>,
}

impl EntityEvent {
    /// Create a new entity event
    #[must_use]
    pub fn new(
        event_type: EventKind,
        entity_type: String,
        entity_id: Uuid,
        data: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            entity_type,
            entity_id,
            data,
            changes: None,
            user_id: None,
            timestamp: Utc::now(),
        }
    }

    /// Set the `user_id` for this event
    #[must_use]
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set field changes for UPDATE events
    #[must_use]
    pub fn with_changes(
        mut self,
        changes: std::collections::HashMap<String, FieldChanges>,
    ) -> Self {
        self.changes = Some(changes);
        self
    }

    /// Check if a field changed value
    #[must_use]
    pub fn field_changed(&self, field_name: &str) -> bool {
        self.changes.as_ref().is_some_and(|changes| changes.contains_key(field_name))
    }

    /// Check if a field changed to a specific value
    #[must_use]
    pub fn field_changed_to(&self, field_name: &str, expected_value: &serde_json::Value) -> bool {
        self.changes
            .as_ref()
            .and_then(|changes| changes.get(field_name))
            .is_some_and(|change| change.new == *expected_value)
    }

    /// Check if a field changed from a specific value
    #[must_use]
    pub fn field_changed_from(&self, field_name: &str, expected_value: &serde_json::Value) -> bool {
        self.changes
            .as_ref()
            .and_then(|changes| changes.get(field_name))
            .is_some_and(|change| change.old == *expected_value)
    }

    /// Check if this is a new entity (no old value for any field)
    #[must_use]
    pub fn is_new(&self) -> bool {
        self.event_type == EventKind::Created
    }

    /// Check if this is a delete event
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.event_type == EventKind::Deleted
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_event_kind_as_str() {
        assert_eq!(EventKind::Created.as_str(), "INSERT");
        assert_eq!(EventKind::Updated.as_str(), "UPDATE");
        assert_eq!(EventKind::Deleted.as_str(), "DELETE");
        assert_eq!(EventKind::Custom.as_str(), "CUSTOM");
    }

    #[test]
    fn test_create_entity_event() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100, "status": "pending"}),
        );

        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.data["total"], 100);
        assert!(event.is_new());
        assert!(!event.is_deleted());
    }

    #[test]
    fn test_entity_event_with_user_id() {
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}))
                .with_user_id("user123".to_string());

        assert_eq!(event.user_id, Some("user123".to_string()));
    }

    #[test]
    fn test_field_changes() {
        let entity_id = Uuid::new_v4();
        let mut changes = std::collections::HashMap::new();
        changes.insert(
            "status".to_string(),
            FieldChanges {
                old: json!("pending"),
                new: json!("shipped"),
            },
        );

        let event = EntityEvent::new(
            EventKind::Updated,
            "Order".to_string(),
            entity_id,
            json!({"status": "shipped"}),
        )
        .with_changes(changes);

        assert!(event.field_changed("status"));
        assert!(!event.field_changed("total"));
        assert!(event.field_changed_to("status", &json!("shipped")));
        assert!(event.field_changed_from("status", &json!("pending")));
    }

    #[test]
    fn test_delete_event() {
        let event =
            EntityEvent::new(EventKind::Deleted, "Order".to_string(), Uuid::new_v4(), json!({}));

        assert!(event.is_deleted());
        assert!(!event.is_new());
    }
}
