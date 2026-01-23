//! Event-to-observer matcher for efficient event routing.
//!
//! This module implements an event matcher that maps events to observers
//! in O(1) time using a hashmap-based index. Observers are indexed by:
//! 1. Event type (INSERT, UPDATE, DELETE, CUSTOM)
//! 2. Entity type (e.g., "Order", "User")
//!
//! This allows fast lookups when an event occurs.

use crate::config::ObserverDefinition;
use crate::error::Result;
use crate::event::{EntityEvent, EventKind};
use std::collections::HashMap;

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
    pub fn build(observers: HashMap<String, ObserverDefinition>) -> Result<Self> {
        let mut matcher = Self::new();

        for (_name, definition) in observers {
            matcher.add_observer(definition)?;
        }

        Ok(matcher)
    }

    /// Add a single observer to the matcher
    fn add_observer(&mut self, observer: ObserverDefinition) -> Result<()> {
        let event_type = observer.event_type.to_uppercase();
        let entity_type = observer.entity.clone();

        self.index
            .entry(event_type)
            .or_default()
            .entry(entity_type)
            .or_default()
            .push(observer);

        Ok(())
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
        self.index
            .values()
            .map(std::collections::HashMap::len)
            .sum()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActionConfig, FailurePolicy, RetryConfig};
    use serde_json::json;
    use uuid::Uuid;

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

        matcher.add_observer(observer).unwrap();

        assert_eq!(matcher.observer_count(), 1);
        assert_eq!(matcher.event_type_count(), 1);
        assert_eq!(matcher.entity_type_count(), 1);
    }

    #[test]
    fn test_matcher_find_exact_match() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher
            .add_observer(create_observer("INSERT", "Order"))
            .unwrap();
        evt_matcher
            .add_observer(create_observer("UPDATE", "Order"))
            .unwrap();
        evt_matcher
            .add_observer(create_observer("INSERT", "User"))
            .unwrap();

        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({}),
        );

        let results = evt_matcher.find_matches(&event);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity, "Order");
    }

    #[test]
    fn test_matcher_find_no_match() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher
            .add_observer(create_observer("INSERT", "Order"))
            .unwrap();

        let event = EntityEvent::new(
            EventKind::Updated,
            "Product".to_string(),
            Uuid::new_v4(),
            json!({}),
        );

        let results = evt_matcher.find_matches(&event);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_matcher_multiple_observers_same_event() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher
            .add_observer(create_observer("INSERT", "Order"))
            .unwrap();
        evt_matcher
            .add_observer(create_observer("INSERT", "Order"))
            .unwrap();

        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({}),
        );

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
        assert_eq!(matcher.event_type_count(), 2);  // INSERT and UPDATE
        // entity_type_count returns total entity type entries (Order appears twice, User once)
        assert_eq!(matcher.entity_type_count(), 3);
    }

    #[test]
    fn test_matcher_find_by_event_and_entity() {
        let mut evt_matcher = EventMatcher::new();
        evt_matcher
            .add_observer(create_observer("INSERT", "Order"))
            .unwrap();
        evt_matcher
            .add_observer(create_observer("UPDATE", "Order"))
            .unwrap();

        let matching_observers = evt_matcher.find_by_event_and_entity(EventKind::Created, "Order");
        assert_eq!(matching_observers.len(), 1);

        let no_matching_observers = evt_matcher.find_by_event_and_entity(EventKind::Deleted, "Order");
        assert_eq!(no_matching_observers.len(), 0);
    }

    #[test]
    fn test_matcher_clear() {
        let mut matcher = EventMatcher::new();
        matcher
            .add_observer(create_observer("INSERT", "Order"))
            .unwrap();
        matcher
            .add_observer(create_observer("UPDATE", "User"))
            .unwrap();

        assert_eq!(matcher.observer_count(), 2);

        matcher.clear();
        assert_eq!(matcher.observer_count(), 0);
        assert_eq!(matcher.event_type_count(), 0);
    }

    #[test]
    fn test_matcher_all_observers() {
        let mut matcher = EventMatcher::new();
        matcher
            .add_observer(create_observer("INSERT", "Order"))
            .unwrap();
        matcher
            .add_observer(create_observer("UPDATE", "Order"))
            .unwrap();
        matcher
            .add_observer(create_observer("INSERT", "User"))
            .unwrap();

        let all = matcher.all_observers();
        assert_eq!(all.len(), 3);
    }
}
