//! Event subscription manager for real-time streaming.
//!
//! Manages active subscriptions and streams events to subscribers with filtering.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::info;

use crate::EventStorage;

/// A single subscriber's event stream
#[derive(Clone)]
pub struct EventSubscription {
    /// Subscription ID (correlation ID from request)
    pub subscription_id: String,
    /// Entity type filter
    pub entity_type:     String,
    /// Optional filter expression (for future use)
    pub filter:          Option<String>,
    /// Sender for pushing events to this subscriber
    pub tx:              mpsc::UnboundedSender<crate::HistoricalEvent>,
}

/// Manages active subscriptions and event routing.
///
/// This manager maintains a set of active subscriptions and routes events
/// to matching subscribers. It's designed for in-memory subscriptions and
/// can be extended to support persistent subscriptions.
pub struct SubscriptionManager {
    /// Map of subscription_id -> EventSubscription
    subscriptions: Arc<DashMap<String, EventSubscription>>,
    /// Reference to event storage for historical queries (optional)
    event_storage: Option<Arc<dyn EventStorage>>,
}

impl SubscriptionManager {
    /// Create a new subscription manager.
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(DashMap::new()),
            event_storage: None,
        }
    }

    /// Create a new subscription manager with event storage.
    pub fn with_event_storage(event_storage: Arc<dyn EventStorage>) -> Self {
        Self {
            subscriptions: Arc::new(DashMap::new()),
            event_storage: Some(event_storage),
        }
    }

    /// Subscribe to events for a specific entity type.
    ///
    /// Returns a receiver that will emit events matching the filter.
    pub fn subscribe(
        &self,
        subscription_id: String,
        entity_type: String,
        filter: Option<String>,
    ) -> mpsc::UnboundedReceiver<crate::HistoricalEvent> {
        let (tx, rx) = mpsc::unbounded_channel();

        let subscription = EventSubscription {
            subscription_id: subscription_id.clone(),
            entity_type,
            filter,
            tx,
        };

        self.subscriptions.insert(subscription_id, subscription);

        info!("New subscription created");

        rx
    }

    /// Unsubscribe a client by subscription ID.
    pub fn unsubscribe(&self, subscription_id: &str) -> bool {
        let removed = self.subscriptions.remove(subscription_id).is_some();
        if removed {
            info!(subscription_id = %subscription_id, "Subscription closed");
        }
        removed
    }

    /// Get count of active subscriptions.
    #[must_use]
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Broadcast an event to all matching subscriptions.
    ///
    /// Sends the event to all subscribers whose entity type matches.
    pub fn broadcast_event(&self, event: &crate::HistoricalEvent) {
        for subscription in self.subscriptions.iter() {
            // Only send to subscriptions matching the entity type
            if subscription.entity_type == event.entity_type {
                // If filter matches, send the event
                if Self::matches_filter(event, &subscription.filter) {
                    let _ = subscription.tx.send(event.clone());
                }
            }
        }
    }

    /// Check if an event matches a filter.
    ///
    /// For now, this is a simple implementation that accepts all events
    /// if no filter is specified. In future phases, this could support
    /// more sophisticated filter expressions.
    fn matches_filter(_event: &crate::HistoricalEvent, filter: &Option<String>) -> bool {
        // If no filter specified, accept all events
        if filter.is_none() {
            return true;
        }

        // TODO: Implement filter matching logic
        // Could support expressions like:
        // - "status = 'shipped'"
        // - "total > 100"
        // - "changes.status.from = 'pending'"
        true
    }

    /// Simulate sending an event to all subscribers (for testing).
    ///
    /// This is primarily useful for testing subscription functionality
    /// without requiring a live event source.
    pub async fn simulate_event(&self, _event: crate::HistoricalEvent) {
        // TODO: Implement event simulation for testing
        // For now, this is a placeholder that accepts an event
        // self.broadcast_event(&_event);
    }

    /// Get reference to event storage if available.
    pub fn event_storage(&self) -> Option<&Arc<dyn EventStorage>> {
        self.event_storage.as_ref()
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::HistoricalEvent;

    #[test]
    fn test_create_subscription() {
        let manager = SubscriptionManager::new();

        let rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

        assert_eq!(manager.subscription_count(), 1);
        drop(rx);
    }

    #[test]
    fn test_unsubscribe() {
        let manager = SubscriptionManager::new();

        let rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
        assert_eq!(manager.subscription_count(), 1);

        manager.unsubscribe("sub-1");
        assert_eq!(manager.subscription_count(), 0);

        drop(rx);
    }

    #[test]
    fn test_unsubscribe_nonexistent() {
        let manager = SubscriptionManager::new();

        let result = manager.unsubscribe("nonexistent");
        assert!(!result);
        assert_eq!(manager.subscription_count(), 0);
    }

    #[test]
    fn test_broadcast_event_filters_by_entity_type() {
        let manager = SubscriptionManager::new();

        let mut rx1 = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
        let mut rx2 = manager.subscribe("sub-2".to_string(), "User".to_string(), None);

        let event = HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"total": 100}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        manager.broadcast_event(&event);

        // rx1 should receive the event
        let received = rx1.try_recv().ok();
        assert!(received.is_some());

        // rx2 should not receive it
        let received = rx2.try_recv().ok();
        assert!(received.is_none());
    }

    #[test]
    fn test_multiple_subscriptions_same_type() {
        let manager = SubscriptionManager::new();

        let mut rx1 = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);
        let mut rx2 = manager.subscribe("sub-2".to_string(), "Order".to_string(), None);

        let event = HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"total": 100}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        manager.broadcast_event(&event);

        // Both should receive the event
        assert!(rx1.try_recv().ok().is_some());
        assert!(rx2.try_recv().ok().is_some());
    }

    #[test]
    fn test_matches_filter_no_filter() {
        let event = HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"total": 100}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        assert!(SubscriptionManager::matches_filter(&event, &None));
        assert!(SubscriptionManager::matches_filter(&event, &Some("total > 50".to_string()))); // TODO: Implement actual filter matching
    }
}
