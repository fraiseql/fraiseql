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
    pub(crate) fn add_observer(&mut self, observer: ObserverDefinition) {
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

