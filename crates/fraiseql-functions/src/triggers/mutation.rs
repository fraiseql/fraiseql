//! Mutation triggers: `after:mutation` and `before:mutation`.

use crate::types::EventPayload;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub const fn as_str(&self) -> &'static str {
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
///
/// Represents a mutation event from the database. Used by the observer pipeline
/// to dispatch to `after:mutation` triggers asynchronously.
///
/// # Dispatch Semantics
///
/// - Fire after mutation completes (mutation response already sent)
/// - Async dispatch: doesn't block mutation response
/// - Failure doesn't affect mutation (error logged only)
/// - Execution order: in declaration order from schema
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
///
/// When a mutation completes, the observer pipeline emits an `EntityEvent`.
/// If an `AfterMutationTrigger` matches the entity type and event kind,
/// the corresponding function is invoked asynchronously without blocking
/// the mutation response.
///
/// # Matching
///
/// - Must match `entity_type` exactly
/// - If `event_filter` is `None`, matches all event kinds (Insert/Update/Delete)
/// - If `event_filter` is `Some`, matches only that specific event kind
///
/// # Dispatch
///
/// - Invoked in declaration order from `schema.compiled.json`
/// - Spawned as an async task (mutation response returns immediately)
/// - Function execution timeout: 5s default (can be overridden per function)
/// - Failure doesn't affect mutation (error logged to tracing subscriber)
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
                .is_none_or(|filter| filter == event_kind)
    }

    /// Build an `EventPayload` from an entity event.
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
///
/// # Semantics
///
/// - `Proceed`: Allows mutation to continue with the provided input
///   - Input may be modified from original
///   - Passed to next trigger in chain (if any)
/// - `Abort`: Prevents mutation from executing
///   - Returns error to client immediately
///   - Short-circuits remaining triggers in chain
///   - Side-effects from aborted triggers are NOT rolled back
///
/// # Important: Side-Effects Not Rolled Back
///
/// If a `before:mutation` trigger abort is triggered, any side-effects
/// (HTTP calls, storage writes, logs) from earlier triggers in the chain
/// are NOT rolled back. Only the mutation itself is prevented.
///
/// This is by design: function side-effects are intended to be independent
/// of mutation success. For example, if a function logs an audit entry and
/// then a later trigger aborts, the audit entry remains.
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
///
/// Executes multiple `before:mutation` triggers in declaration order.
/// Each trigger can modify the input and pass it to the next trigger,
/// or abort the mutation by returning an error.
///
/// # Execution Semantics
///
/// - Synchronous: blocks the mutation (execution is on the hot path)
/// - Sequential: triggers execute in declaration order
/// - Propagating: each trigger receives the modified input from previous trigger
/// - Short-circuit: first abort stops the chain immediately
/// - Default timeout: 500ms per trigger (shorter than general 5s default)
/// - Side-effects: any side-effects from aborted triggers are NOT rolled back
///
/// # Example
///
/// ```ignore
/// let chain = BeforeMutationChain {
///     triggers: vec![
///         validateInput,  // checks required fields
///         checkDuplicates, // checks uniqueness
///         auditLog,       // logs the attempt
///     ]
/// };
///
/// let result = chain.execute(input, &observer).await?;
/// match result {
///     Proceed(modified) => { /* mutation continues */ }
///     Abort(error) => { /* mutation cancelled */ }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BeforeMutationChain {
    /// Triggers in declaration order.
    pub triggers: Vec<BeforeMutationTrigger>,
}

/// Matcher for efficiently finding triggers by (`entity_type`, `event_kind`).
///
/// Uses a nested `HashMap` for O(1) lookup:
/// - `entity_type` → `event_kind` → `Vec<AfterMutationTrigger>`
/// - When `event_kind` is None (matches all), stored separately for fallback
///
/// # Integration with `FunctionObserver`
///
/// When the `FunctionObserver` receives an `EntityEvent` from the mutation pipeline,
/// it calls `find()` to get all matching `AfterMutationTrigger`s. For each matching
/// trigger, the observer spawns an async task to invoke the function without blocking
/// the mutation response. Task completion is tracked to prevent leaks on shutdown.
///
/// # Example
///
/// ```ignore
/// let mut matcher = TriggerMatcher::new();
/// matcher.add(AfterMutationTrigger {
///     function_name: "onUserCreated".to_string(),
///     entity_type: "User".to_string(),
///     event_filter: Some(EventKind::Insert),
/// });
///
/// // Later, when a User insert occurs:
/// let triggers = matcher.find("User", EventKind::Insert);
/// for trigger in triggers {
///     // Spawn async task to invoke function
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TriggerMatcher {
    /// Map of `entity_type` → `event_kind` → triggers
    specific: HashMap<String, HashMap<String, Vec<AfterMutationTrigger>>>,
    /// Map of `entity_type` → triggers that match all event kinds
    all_kinds: HashMap<String, Vec<AfterMutationTrigger>>,
}

impl TriggerMatcher {
    /// Create a new empty trigger matcher.
    pub fn new() -> Self {
        Self {
            specific: HashMap::new(),
            all_kinds: HashMap::new(),
        }
    }

    /// Add a trigger to the matcher.
    pub fn add(&mut self, trigger: AfterMutationTrigger) {
        match trigger.event_filter {
            Some(event_kind) => {
                self.specific
                    .entry(trigger.entity_type.clone())
                    .or_default()
                    .entry(event_kind.as_str().to_string())
                    .or_default()
                    .push(trigger);
            }
            None => {
                self.all_kinds
                    .entry(trigger.entity_type.clone())
                    .or_default()
                    .push(trigger);
            }
        }
    }

    /// Find all triggers matching the given entity and event kind.
    pub fn find(&self, entity: &str, event_kind: EventKind) -> Vec<AfterMutationTrigger> {
        let event_str = event_kind.as_str();
        let mut result = Vec::new();

        // Get specific triggers for this event kind
        if let Some(entity_map) = self.specific.get(entity) {
            if let Some(triggers) = entity_map.get(event_str) {
                result.extend(triggers.clone());
            }
        }

        // Get all-kinds triggers for this entity
        if let Some(triggers) = self.all_kinds.get(entity) {
            result.extend(triggers.clone());
        }

        result
    }
}

impl Default for TriggerMatcher {
    fn default() -> Self {
        Self::new()
    }
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

    #[test]
    fn test_trigger_matcher_empty() {
        let matcher = TriggerMatcher::new();
        let results = matcher.find("User", EventKind::Insert);
        assert!(results.is_empty());
    }

    #[test]
    fn test_trigger_matcher_specific_event_kind() {
        let mut matcher = TriggerMatcher::new();
        let trigger = AfterMutationTrigger {
            function_name: "onUserCreated".to_string(),
            entity_type: "User".to_string(),
            event_filter: Some(EventKind::Insert),
        };

        matcher.add(trigger);
        let results = matcher.find("User", EventKind::Insert);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].function_name, "onUserCreated");

        // Should not match other event kinds
        let results = matcher.find("User", EventKind::Update);
        assert!(results.is_empty());
    }

    #[test]
    fn test_trigger_matcher_all_kinds() {
        let mut matcher = TriggerMatcher::new();
        let trigger = AfterMutationTrigger {
            function_name: "onUserChanged".to_string(),
            entity_type: "User".to_string(),
            event_filter: None,
        };

        matcher.add(trigger);
        assert_eq!(matcher.find("User", EventKind::Insert).len(), 1);
        assert_eq!(matcher.find("User", EventKind::Update).len(), 1);
        assert_eq!(matcher.find("User", EventKind::Delete).len(), 1);
    }

    #[test]
    fn test_trigger_matcher_mixed_specific_and_all() {
        let mut matcher = TriggerMatcher::new();

        // Add specific triggers
        matcher.add(AfterMutationTrigger {
            function_name: "onUserCreated".to_string(),
            entity_type: "User".to_string(),
            event_filter: Some(EventKind::Insert),
        });

        // Add all-kinds trigger
        matcher.add(AfterMutationTrigger {
            function_name: "onUserChanged".to_string(),
            entity_type: "User".to_string(),
            event_filter: None,
        });

        // Insert should return both
        let results = matcher.find("User", EventKind::Insert);
        assert_eq!(results.len(), 2);

        // Update should return only all-kinds
        let results = matcher.find("User", EventKind::Update);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].function_name, "onUserChanged");
    }

    #[test]
    fn test_trigger_matcher_multiple_entities() {
        let mut matcher = TriggerMatcher::new();

        matcher.add(AfterMutationTrigger {
            function_name: "onUserCreated".to_string(),
            entity_type: "User".to_string(),
            event_filter: Some(EventKind::Insert),
        });

        matcher.add(AfterMutationTrigger {
            function_name: "onPostCreated".to_string(),
            entity_type: "Post".to_string(),
            event_filter: Some(EventKind::Insert),
        });

        let user_results = matcher.find("User", EventKind::Insert);
        assert_eq!(user_results.len(), 1);
        assert_eq!(user_results[0].function_name, "onUserCreated");

        let post_results = matcher.find("Post", EventKind::Insert);
        assert_eq!(post_results.len(), 1);
        assert_eq!(post_results[0].function_name, "onPostCreated");
    }

    #[test]
    fn test_trigger_matcher_no_cross_entity_match() {
        let mut matcher = TriggerMatcher::new();

        matcher.add(AfterMutationTrigger {
            function_name: "onUserCreated".to_string(),
            entity_type: "User".to_string(),
            event_filter: Some(EventKind::Insert),
        });

        let post_results = matcher.find("Post", EventKind::Insert);
        assert!(post_results.is_empty());
    }
}
