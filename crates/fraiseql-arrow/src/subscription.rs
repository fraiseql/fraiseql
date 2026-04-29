//! Event subscription manager for real-time streaming.
//!
//! Manages active subscriptions and streams events to subscribers with filtering.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::info;

use crate::ArrowEventStorage;

/// A single subscriber's event stream
#[derive(Clone)]
pub struct EventSubscription {
    /// Subscription ID (correlation ID from request)
    pub subscription_id: String,
    /// Entity type filter
    pub entity_type:     String,
    /// Optional filter expression (for future use)
    pub filter:          Option<String>,
    /// Sender for pushing events to this subscriber (bounded channel).
    pub tx:              mpsc::Sender<crate::HistoricalEvent>,
}

/// Default per-subscription channel buffer depth.
///
/// At this capacity each subscriber can absorb ~256 events before the sender
/// starts dropping events for that subscriber (via `try_send`).  This bounds
/// per-subscription memory to `256 × sizeof(HistoricalEvent)` instead of
/// growing without limit.
const DEFAULT_SUBSCRIPTION_BUFFER: usize = 256;

/// Manages active subscriptions and event routing.
///
/// This manager maintains a set of active subscriptions and routes events
/// to matching subscribers. It's designed for in-memory subscriptions and
/// can be extended to support persistent subscriptions.
pub struct SubscriptionManager {
    /// Map of `subscription_id` -> `EventSubscription`
    subscriptions:           Arc<DashMap<String, EventSubscription>>,
    /// Reference to event storage for historical queries (optional)
    event_storage:           Option<Arc<dyn ArrowEventStorage>>,
    /// Per-subscription channel buffer depth.
    per_subscription_buffer: usize,
}

impl SubscriptionManager {
    /// Create a new subscription manager.
    pub fn new() -> Self {
        Self {
            subscriptions:           Arc::new(DashMap::new()),
            event_storage:           None,
            per_subscription_buffer: DEFAULT_SUBSCRIPTION_BUFFER,
        }
    }

    /// Create a new subscription manager with event storage.
    pub fn with_event_storage(event_storage: Arc<dyn ArrowEventStorage>) -> Self {
        Self {
            subscriptions:           Arc::new(DashMap::new()),
            event_storage:           Some(event_storage),
            per_subscription_buffer: DEFAULT_SUBSCRIPTION_BUFFER,
        }
    }

    /// Set the per-subscription channel buffer depth.
    ///
    /// Each subscriber's internal channel holds at most `capacity` events.
    /// When full, new events are dropped with a warning (slow-subscriber
    /// protection).  Must be called before any [`subscribe`](Self::subscribe)
    /// calls to take effect.
    #[must_use]
    pub const fn with_subscription_buffer(mut self, capacity: usize) -> Self {
        self.per_subscription_buffer = capacity;
        self
    }

    /// Subscribe to events for a specific entity type.
    ///
    /// Returns a bounded receiver that will emit events matching the filter.
    /// Events are dropped for this subscriber when its buffer is full (see
    /// [`with_subscription_buffer`](Self::with_subscription_buffer)).
    pub fn subscribe(
        &self,
        subscription_id: String,
        entity_type: String,
        filter: Option<String>,
    ) -> mpsc::Receiver<crate::HistoricalEvent> {
        let (tx, rx) = mpsc::channel(self.per_subscription_buffer);

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
    /// Events are dropped for a slow subscriber whose buffer is full; a
    /// warning is emitted so operators can tune the buffer or detect
    /// stuck clients.
    pub fn broadcast_event(&self, event: &crate::HistoricalEvent) {
        for subscription in self.subscriptions.iter() {
            // Only send to subscriptions matching the entity type
            if subscription.entity_type == event.entity_type {
                // If filter matches, send the event
                if Self::matches_filter(event, &subscription.filter) {
                    match subscription.tx.try_send(event.clone()) {
                        Ok(()) => {},
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            tracing::warn!(
                                subscription_id = %subscription.subscription_id,
                                entity_type = %subscription.entity_type,
                                "Arrow Flight subscription buffer full — event dropped for slow subscriber"
                            );
                        },
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            // Receiver dropped — subscriber disconnected.  Cleanup
                            // happens when the SubscriptionManager removes the entry.
                        },
                    }
                }
            }
        }
    }

    /// Check if an event matches a filter expression.
    ///
    /// Supports equality (`field = 'value'`) and inequality (`field != 'value'`)
    /// operators against the event's JSON data. Returns `true` if no filter is
    /// specified or the filter matches. Returns `false` for missing fields or
    /// unparseable filters.
    fn matches_filter(event: &crate::HistoricalEvent, filter: &Option<String>) -> bool {
        let Some(filter_str) = filter.as_deref() else {
            return true;
        };

        let filter_str = filter_str.trim();
        if filter_str.is_empty() {
            return true;
        }

        // Try != first (longer operator)
        if let Some((field, expected)) = filter_str.split_once(" != ") {
            let expected = expected.trim().trim_matches('\'');
            return event.data.get(field.trim()).and_then(|v| v.as_str()) != Some(expected);
        }

        // Then try =
        if let Some((field, expected)) = filter_str.split_once(" = ") {
            let expected = expected.trim().trim_matches('\'');
            return event.data.get(field.trim()).and_then(|v| v.as_str()) == Some(expected);
        }

        // Unparseable filter — log a warning and treat as no-match (conservative: reject).
        tracing::warn!(
            filter = %filter_str,
            "Arrow Flight subscription filter could not be parsed — treating as no filter match"
        );
        false
    }

    /// Broadcast a simulated event to all matching subscribers.
    ///
    /// Useful for testing subscription functionality without requiring
    /// a live event source.
    pub fn simulate_event(&self, event: crate::HistoricalEvent) {
        self.broadcast_event(&event);
    }

    /// Get reference to event storage if available.
    pub fn event_storage(&self) -> Option<&Arc<dyn ArrowEventStorage>> {
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
        let event = make_event(serde_json::json!({"status": "shipped"}));
        assert!(SubscriptionManager::matches_filter(&event, &None));
    }

    #[test]
    fn test_matches_filter_equality() {
        let event = make_event(serde_json::json!({"status": "shipped", "region": "us-east"}));
        assert!(SubscriptionManager::matches_filter(
            &event,
            &Some("status = 'shipped'".to_string())
        ));
        assert!(!SubscriptionManager::matches_filter(
            &event,
            &Some("status = 'pending'".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_inequality() {
        let event = make_event(serde_json::json!({"status": "shipped"}));
        assert!(SubscriptionManager::matches_filter(
            &event,
            &Some("status != 'pending'".to_string())
        ));
        assert!(!SubscriptionManager::matches_filter(
            &event,
            &Some("status != 'shipped'".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_missing_field() {
        let event = make_event(serde_json::json!({"status": "shipped"}));
        assert!(!SubscriptionManager::matches_filter(
            &event,
            &Some("missing_field = 'value'".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_unparseable() {
        let event = make_event(serde_json::json!({"status": "shipped"}));
        assert!(!SubscriptionManager::matches_filter(
            &event,
            &Some("garbage filter".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_empty_string() {
        let event = make_event(serde_json::json!({"status": "shipped"}));
        assert!(SubscriptionManager::matches_filter(&event, &Some(String::new())));
    }

    #[test]
    fn test_simulate_event_broadcasts() {
        let manager = SubscriptionManager::new();
        let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

        let event = make_event(serde_json::json!({"total": 100}));
        manager.simulate_event(event);

        assert!(rx.try_recv().ok().is_some());
    }

    #[test]
    fn test_broadcast_with_filter() {
        let manager = SubscriptionManager::new();
        let mut rx_match = manager.subscribe(
            "sub-1".to_string(),
            "Order".to_string(),
            Some("status = 'shipped'".to_string()),
        );
        let mut rx_no_match = manager.subscribe(
            "sub-2".to_string(),
            "Order".to_string(),
            Some("status = 'pending'".to_string()),
        );

        let event = make_event(serde_json::json!({"status": "shipped"}));
        manager.broadcast_event(&event);

        assert!(rx_match.try_recv().ok().is_some());
        assert!(rx_no_match.try_recv().ok().is_none());
    }

    fn make_event(data: serde_json::Value) -> HistoricalEvent {
        HistoricalEvent {
            id: Uuid::new_v4(),
            event_type: "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id: Uuid::new_v4(),
            data,
            user_id: None,
            tenant_id: None,
            timestamp: Utc::now(),
        }
    }

    // ─── bounded-channel tests (S34) ─────────────────────────────────────────

    #[test]
    fn test_subscription_uses_bounded_channel() {
        // with_subscription_buffer(2): only 2 events can queue; a 3rd is dropped
        let manager = SubscriptionManager::new().with_subscription_buffer(2);
        let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

        let e1 = make_event(serde_json::json!({"n": 1}));
        let e2 = make_event(serde_json::json!({"n": 2}));
        let e3 = make_event(serde_json::json!({"n": 3}));

        // First two fit in the buffer; third overflows silently (logged as warn)
        manager.broadcast_event(&e1);
        manager.broadcast_event(&e2);
        manager.broadcast_event(&e3); // dropped — buffer full

        // Drain: expect exactly 2 events (the first two)
        assert!(rx.try_recv().is_ok(), "first event should be in buffer");
        assert!(rx.try_recv().is_ok(), "second event should be in buffer");
        assert!(
            rx.try_recv().is_err(),
            "third event must be dropped when buffer is full (bounded channel)"
        );
    }

    #[test]
    fn test_subscription_buffer_default_allows_256_events() {
        let manager = SubscriptionManager::new();
        assert_eq!(manager.per_subscription_buffer, super::DEFAULT_SUBSCRIPTION_BUFFER);
        assert_eq!(manager.per_subscription_buffer, 256);
    }

    #[test]
    fn test_with_subscription_buffer_overrides_default() {
        let manager = SubscriptionManager::new().with_subscription_buffer(64);
        assert_eq!(manager.per_subscription_buffer, 64);
    }

    #[test]
    fn test_subscription_channel_is_not_unbounded() {
        // Verify that we cannot queue more than the configured capacity.
        // capacity=1: only one event fits; the second is dropped.
        let manager = SubscriptionManager::new().with_subscription_buffer(1);
        let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

        manager.broadcast_event(&make_event(serde_json::json!({"n": 1})));
        manager.broadcast_event(&make_event(serde_json::json!({"n": 2}))); // must be dropped

        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err(), "channel must be bounded to capacity=1");
    }
}
