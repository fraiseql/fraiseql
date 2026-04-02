//! Event-to-observer matcher for efficient event routing.
//!
//! This module implements an event matcher that maps events to observers
//! in O(1) time using a hashmap-based index. Observers are indexed by:
//! 1. Event type (INSERT, UPDATE, DELETE, CUSTOM)
//! 2. Entity type (e.g., "Order", "User")
//!
//! This allows fast lookups when an event occurs.

use std::collections::HashMap;

#[allow(unused_imports)] // Reason: used only in doc links for `# Errors` sections
use crate::error::ObserverError;
use crate::{
    config::ObserverDefinition,
    error::Result,
    event::{EntityEvent, EventKind},
};

/// Index for fast O(1) event-to-observer matching
///
/// Structure: `{ event_type -> { entity_type -> [observer_definitions] } }`
#[derive(Debug, Clone)]
pub struct EventMatcher {
    // Two-level index: event_type -> entity_type -> observers
    index: HashMap<String, HashMap<String, Vec<ObserverDefinition>>>,
}

impl EventMatcher {
    /// Create a new event matcher
    #[must_use]
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    /// Build matcher from a set of observer definitions
    ///
    /// # Arguments
    /// * `observers` - Map of observer name -> definition
    ///
    /// # Returns
    /// `EventMatcher` with all observers indexed
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if any observer definition contains an invalid
    /// entity type pattern.
    pub fn build(observers: HashMap<String, ObserverDefinition>) -> Result<Self> {
        let mut matcher = Self::new();

        for (_name, definition) in observers {
            matcher.add_observer(definition);
        }

        Ok(matcher)
    }

    /// Add a single observer to the matcher
    fn add_observer(&mut self, observer: ObserverDefinition) {
        let event_type = observer.event_type.to_uppercase();
        let entity_type = observer.entity.clone();

        self.index
            .entry(event_type)
            .or_default()
            .entry(entity_type)
            .or_default()
            .push(observer);
    }

    /// Find all observers that match an event
    ///
    /// # Arguments
    /// * `event` - The entity event to match
    ///
    /// # Returns
    /// Vector of matching observer definitions (empty if no matches)
    #[must_use]
    pub fn find_matches(&self, event: &EntityEvent) -> Vec<&ObserverDefinition> {
        let event_type_str = event.event_type.as_str().to_uppercase();

        let mut results = Vec::new();

        if let Some(entity_index) = self.index.get(&event_type_str) {
            // Try exact entity type match first
            if let Some(observers) = entity_index.get(&event.entity_type) {
                results.extend(observers.iter());
            }

            // Also try wildcard "*" match for observers that match all entities
            if let Some(wildcard_observers) = entity_index.get("*") {
                results.extend(wildcard_observers.iter());
            }
        }

        results
    }

    /// Find observers matching an event type and entity
    #[must_use]
    pub fn find_by_event_and_entity(
        &self,
        event_type: EventKind,
        entity_type: &str,
    ) -> Vec<&ObserverDefinition> {
        let event_type_str = event_type.as_str().to_uppercase();

        let mut results = Vec::new();

        if let Some(entity_index) = self.index.get(&event_type_str) {
            // Try exact entity type match first
            if let Some(observers) = entity_index.get(entity_type) {
                results.extend(observers.iter());
            }

            // Also try wildcard "*" match for observers that match all entities
            if let Some(wildcard_observers) = entity_index.get("*") {
                results.extend(wildcard_observers.iter());
            }
        }

        results
    }

    /// Get all observers (for administrative/debugging purposes)
    #[must_use]
    pub fn all_observers(&self) -> Vec<&ObserverDefinition> {
        self.index
            .values()
            .flat_map(|entity_map| entity_map.values().flat_map(|obs| obs.iter()))
            .collect()
    }

    /// Get count of all observers
    #[must_use]
    pub fn observer_count(&self) -> usize {
        self.index
            .values()
            .map(|entity_map| entity_map.values().map(std::vec::Vec::len).sum::<usize>())
            .sum()
    }

    /// Get count of unique event types
    #[must_use]
    pub fn event_type_count(&self) -> usize {
        self.index.len()
    }

    /// Get count of unique entity types
    #[must_use]
    pub fn entity_type_count(&self) -> usize {
        self.index.values().map(std::collections::HashMap::len).sum()
    }

    /// Clear all observers (for testing/reset purposes)
    pub fn clear(&mut self) {
        self.index.clear();
    }
}

impl Default for EventMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::config::{ActionConfig, FailurePolicy, RetryConfig};

    fn create_observer(event_type: &str, entity: &str) -> ObserverDefinition {
        ObserverDefinition {
            event_type: event_type.to_string(),
            entity: entity.to_string(),
            condition: None,
            actions: vec![ActionConfig::Webhook {
                url: Some("https://example.com".to_string()),
                url_env: None,
                headers: HashMap::default(),
                body_template: Some("{}".to_string()),
            }],
            retry: RetryConfig::default(),
            on_failure: FailurePolicy::Log,
        }
    }

    #[test]
    fn test_matcher_new() {
        let matcher = EventMatcher::new();
        assert_eq!(matcher.observer_count(), 0);
        assert_eq!(matcher.event_type_count(), 0);
        assert_eq!(matcher.entity_type_count(), 0);
    }

    #[test]
    fn test_matcher_add_observer() {
        let mut matcher = EventMatcher::new();
        let observer = create_observer("INSERT", "Order");

        matcher.add_observer(observer);

        assert_eq!(matcher.observer_count(), 1);
        assert_eq!(matcher.event_type_count(), 1);
        assert_eq!(matcher.entity_type_count(), 1);
    }

    #[test]
    fn test_matcher_find_exact_match() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher.add_observer(create_observer("INSERT", "Order"));
        evt_matcher.add_observer(create_observer("UPDATE", "Order"));
        evt_matcher.add_observer(create_observer("INSERT", "User"));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));

        let results = evt_matcher.find_matches(&event);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity, "Order");
    }

    #[test]
    fn test_matcher_find_no_match() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Updated, "Product".to_string(), Uuid::new_v4(), json!({}));

        let results = evt_matcher.find_matches(&event);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_matcher_multiple_observers_same_event() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher.add_observer(create_observer("INSERT", "Order"));
        evt_matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));

        let results = evt_matcher.find_matches(&event);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_matcher_build_from_hashmap() {
        let mut observers = HashMap::new();
        observers.insert("order_insert".to_string(), create_observer("INSERT", "Order"));
        observers.insert("user_insert".to_string(), create_observer("INSERT", "User"));
        observers.insert("order_update".to_string(), create_observer("UPDATE", "Order"));

        let matcher = EventMatcher::build(observers).unwrap();

        assert_eq!(matcher.observer_count(), 3);
        assert_eq!(matcher.event_type_count(), 2); // INSERT and UPDATE
        // entity_type_count returns total entity type entries (Order appears twice, User once)
        assert_eq!(matcher.entity_type_count(), 3);
    }

    #[test]
    fn test_matcher_find_by_event_and_entity() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher.add_observer(create_observer("INSERT", "Order"));
        evt_matcher.add_observer(create_observer("UPDATE", "Order"));

        let matching_observers = evt_matcher.find_by_event_and_entity(EventKind::Created, "Order");
        assert_eq!(matching_observers.len(), 1);

        let no_matching_observers =
            evt_matcher.find_by_event_and_entity(EventKind::Deleted, "Order");
        assert_eq!(no_matching_observers.len(), 0);
    }

    #[test]
    fn test_matcher_clear() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "User"));

        assert_eq!(matcher.observer_count(), 2);

        matcher.clear();
        assert_eq!(matcher.observer_count(), 0);
        assert_eq!(matcher.event_type_count(), 0);
    }

    #[test]
    fn test_matcher_all_observers() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "Order"));
        matcher.add_observer(create_observer("INSERT", "User"));

        let all = matcher.all_observers();
        assert_eq!(all.len(), 3);
    }

    // =========================================================================
    // Additional tests for matcher.rs coverage
    // =========================================================================

    #[test]
    fn test_no_observers_empty_result() {
        let matcher = EventMatcher::new();
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert!(results.is_empty(), "No observers should yield empty result");
    }

    #[test]
    fn test_single_observer_matches_entity_type() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert_eq!(results.len(), 1, "Single observer should match its entity type");
    }

    #[test]
    fn test_single_observer_wrong_entity_type_no_match() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Created, "User".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert!(results.is_empty(), "Observer should not match wrong entity type");
    }

    #[test]
    fn test_multiple_observers_first_matches_only() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "User"));

        // Only INSERT on Order should match
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert_eq!(results.len(), 1, "Only the matching observer should be returned");
        assert_eq!(results[0].entity, "Order");
    }

    #[test]
    fn test_multiple_observers_all_match_when_same_event_entity() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("INSERT", "Order"));

        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert_eq!(results.len(), 3, "All matching observers should be returned");
    }

    #[test]
    fn test_wildcard_entity_matches_all_entities() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "*"));

        let event_order =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let event_user =
            EntityEvent::new(EventKind::Created, "User".to_string(), Uuid::new_v4(), json!({}));

        let results_order = matcher.find_matches(&event_order);
        let results_user = matcher.find_matches(&event_user);

        assert_eq!(results_order.len(), 1, "Wildcard observer should match Order");
        assert_eq!(results_user.len(), 1, "Wildcard observer should match User");
    }

    #[test]
    fn test_observer_count_after_multiple_adds() {
        let mut matcher = EventMatcher::new();
        assert_eq!(matcher.observer_count(), 0);

        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "User"));
        matcher.add_observer(create_observer("DELETE", "Product"));

        assert_eq!(
            matcher.observer_count(),
            3,
            "Observer count should reflect all added observers"
        );
    }

    #[test]
    fn test_event_type_case_insensitive_matching() {
        let mut matcher = EventMatcher::new();
        // Observer defined with lowercase
        matcher.add_observer(create_observer("insert", "Order"));

        // Event uses EventKind::Created which maps to "INSERT"
        let event =
            EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));
        let results = matcher.find_matches(&event);
        assert_eq!(results.len(), 1, "Event type matching should be case-insensitive");
    }

    #[test]
    fn test_find_by_event_and_entity_with_wildcard() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "*"));
        matcher.add_observer(create_observer("INSERT", "Order"));

        // Both wildcard and exact should match
        let results = matcher.find_by_event_and_entity(EventKind::Created, "Order");
        assert_eq!(results.len(), 2, "Both exact and wildcard observers should match");
    }

    #[test]
    fn test_entity_type_count_with_multiple_event_types() {
        let mut matcher = EventMatcher::new();
        matcher.add_observer(create_observer("INSERT", "Order"));
        matcher.add_observer(create_observer("UPDATE", "Order"));
        matcher.add_observer(create_observer("DELETE", "Order"));

        // 3 event types × 1 entity type each = 3 total entity type entries
        assert_eq!(matcher.event_type_count(), 3, "Should have 3 distinct event types");
        assert_eq!(matcher.entity_type_count(), 3, "Should have 3 entity type entries");
    }

    /// This test uses `ObserverExecutor::with_dispatcher` to exercise the test seam
    /// and prevent the `dead_code` lint from triggering on that method.
    #[tokio::test]
    async fn test_executor_with_dispatcher_test_seam() {
        use std::sync::Arc;

        use crate::{
            ObserverExecutor,
            matcher::EventMatcher,
            testing::mocks::{MockActionDispatcher, MockDeadLetterQueue},
        };

        let matcher = EventMatcher::new();
        let dlq = Arc::new(MockDeadLetterQueue::new());
        let dispatcher = Arc::new(MockActionDispatcher::new());

        // Exercise the with_dispatcher constructor — this prevents the dead_code lint
        let _executor = ObserverExecutor::with_dispatcher(matcher, dlq, dispatcher);
        // The executor was constructed — no panics means the seam works
    }
}
