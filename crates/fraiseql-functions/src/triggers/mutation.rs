//! Mutation triggers: `after:mutation` and `before:mutation`.

use crate::types::EventPayload;
use serde::{Deserialize, Serialize};

/// Types of mutations that can trigger events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EventKind {
    /// Insert operation.
    Insert,
    /// Update operation.
    Update,
    /// Delete operation.
    Delete,
}

impl EventKind {
    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::Insert => "insert",
            EventKind::Update => "update",
            EventKind::Delete => "delete",
        }
    }
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Entity event with old and new row data.
#[derive(Debug, Clone)]
pub struct EntityEvent {
    /// Entity type (e.g., "User", "Post").
    pub entity: String,
    /// Kind of mutation.
    pub event_kind: EventKind,
    /// Old row data (None for Insert).
    pub old: Option<serde_json::Value>,
    /// New row data (None for Delete).
    pub new: Option<serde_json::Value>,
    /// Timestamp of the event.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Trigger that fires after a mutation completes.
#[derive(Debug, Clone)]
pub struct AfterMutationTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Entity type to trigger on (e.g., "User").
    pub entity_type: String,
    /// Optional filter on event kind (None = all).
    pub event_filter: Option<EventKind>,
}

impl AfterMutationTrigger {
    /// Check if this trigger matches the given entity and event.
    pub fn matches(&self, entity: &str, event_kind: EventKind) -> bool {
        self.entity_type == entity
            && self
                .event_filter
                .map(|filter| filter == event_kind)
                .unwrap_or(true)
    }

    /// Build an EventPayload from an entity event.
    pub fn build_payload(&self, event: &EntityEvent) -> EventPayload {
        EventPayload {
            trigger_type: format!("after:mutation:{}", self.function_name),
            entity: event.entity.clone(),
            event_kind: event.event_kind.to_string(),
            data: serde_json::json!({
                "event_kind": event.event_kind.as_str(),
                "old": event.old,
                "new": event.new,
            }),
            timestamp: event.timestamp,
        }
    }
}

/// Result of a before-mutation trigger execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BeforeMutationResult {
    /// Proceed with the mutation using the provided (possibly modified) input.
    Proceed(serde_json::Value),
    /// Abort the mutation with an error message.
    Abort(String),
}

/// Trigger that fires before a mutation executes.
#[derive(Debug, Clone)]
pub struct BeforeMutationTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Name of the mutation to trigger on (e.g., "createUser").
    pub mutation_name: String,
}

impl BeforeMutationTrigger {
    /// Check if this trigger matches the given mutation.
    pub fn matches(&self, mutation: &str) -> bool {
        self.mutation_name == mutation
    }
}

/// Chain of before-mutation triggers for a single mutation.
#[derive(Debug, Clone)]
pub struct BeforeMutationChain {
    /// Triggers in declaration order.
    pub triggers: Vec<BeforeMutationTrigger>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_kind_as_str() {
        assert_eq!(EventKind::Insert.as_str(), "insert");
        assert_eq!(EventKind::Update.as_str(), "update");
        assert_eq!(EventKind::Delete.as_str(), "delete");
    }

    #[test]
    fn test_after_mutation_trigger_matches() {
        let trigger = AfterMutationTrigger {
            function_name: "onUserCreated".to_string(),
            entity_type: "User".to_string(),
            event_filter: Some(EventKind::Insert),
        };

        assert!(trigger.matches("User", EventKind::Insert));
        assert!(!trigger.matches("User", EventKind::Update));
        assert!(!trigger.matches("Post", EventKind::Insert));
    }

    #[test]
    fn test_after_mutation_trigger_matches_all_kinds() {
        let trigger = AfterMutationTrigger {
            function_name: "onUserChanged".to_string(),
            entity_type: "User".to_string(),
            event_filter: None,
        };

        assert!(trigger.matches("User", EventKind::Insert));
        assert!(trigger.matches("User", EventKind::Update));
        assert!(trigger.matches("User", EventKind::Delete));
        assert!(!trigger.matches("Post", EventKind::Insert));
    }

    #[test]
    fn test_after_mutation_trigger_builds_payload() {
        let trigger = AfterMutationTrigger {
            function_name: "onUserCreated".to_string(),
            entity_type: "User".to_string(),
            event_filter: Some(EventKind::Insert),
        };

        let event = EntityEvent {
            entity: "User".to_string(),
            event_kind: EventKind::Insert,
            old: None,
            new: Some(serde_json::json!({ "id": 1, "name": "Alice" })),
            timestamp: chrono::Utc::now(),
        };

        let payload = trigger.build_payload(&event);
        assert_eq!(payload.trigger_type, "after:mutation:onUserCreated");
        assert_eq!(payload.entity, "User");
        assert_eq!(payload.event_kind, "insert");
        assert_eq!(payload.data["event_kind"], "insert");
        assert_eq!(payload.data["old"], serde_json::Value::Null);
        assert!(payload.data["new"].is_object());
    }

    #[test]
    fn test_before_mutation_trigger_matches() {
        let trigger = BeforeMutationTrigger {
            function_name: "validateUserInput".to_string(),
            mutation_name: "createUser".to_string(),
        };

        assert!(trigger.matches("createUser"));
        assert!(!trigger.matches("updateUser"));
    }
}
