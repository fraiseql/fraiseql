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
    /// Supports filter expressions like:
    /// - `"status = 'shipped'"` - string equality
    /// - `"total > 100"` - numeric comparison
    /// - `"active = true"` - boolean comparison
    /// - `"address.city = 'Paris'"` - nested field access
    ///
    /// Returns `true` if no filter is specified or if filter matches.
    /// Returns `true` on parse errors (fail open).
    pub fn matches_filter(event: &crate::HistoricalEvent, filter: &Option<String>) -> bool {
        // If no filter specified, accept all events
        let Some(filter_str) = filter else {
            return true;
        };

        Self::evaluate_filter(&event.data, filter_str).unwrap_or(true)
    }

    /// Evaluate a filter expression against JSON data.
    fn evaluate_filter(data: &serde_json::Value, filter: &str) -> Result<bool, String> {
        let (field, op, value) = Self::parse_filter(filter)?;
        let field_value = Self::get_field_value(data, &field)?;
        Self::evaluate_comparison(&field_value, &op, &value)
    }

    /// Parse filter expression into (field, operator, value).
    ///
    /// Examples:
    /// - `"total > 50"` → `("total", ">", "50")`
    /// - `"address.city = 'Paris'"` → `("address.city", "=", "'Paris'")`
    fn parse_filter(filter: &str) -> Result<(String, String, String), String> {
        let filter = filter.trim();

        // Find the operator
        let operators = ["!=", ">=", "<=", "=", ">", "<"];
        let (op, rest) = operators
            .iter()
            .find_map(|op| {
                filter.find(op).map(|pos| {
                    let (field_part, value_part) = filter.split_at(pos);
                    (*op, (field_part, value_part))
                })
            })
            .ok_or_else(|| "No valid operator found".to_string())?;

        let field = rest.0.trim().to_string();
        let value = rest.1[op.len()..].trim().to_string();

        if field.is_empty() || value.is_empty() {
            return Err("Field or value is empty".to_string());
        }

        Ok((field, op.to_string(), value))
    }

    /// Get a field value from JSON, supporting nested access with dot notation.
    fn get_field_value(data: &serde_json::Value, path: &str) -> Result<serde_json::Value, String> {
        let mut current = data;

        for key in path.split('.') {
            current = &current[key];
            if current.is_null() {
                return Err(format!("Field '{path}' not found"));
            }
        }

        Ok(current.clone())
    }

    /// Evaluate a comparison between two values.
    fn evaluate_comparison(
        field_value: &serde_json::Value,
        op: &str,
        expected_value: &str,
    ) -> Result<bool, String> {
        // Parse expected value: strip quotes if string, parse as number, or check for boolean
        let expected = Self::parse_value(expected_value)?;

        match op {
            "=" => {
                // For numeric comparisons, use numeric comparison
                if field_value.is_number() && expected.is_number() {
                    Self::numeric_comparison(field_value, &expected, |a, b| (a - b).abs() < f64::EPSILON)
                } else {
                    Ok(field_value == &expected)
                }
            },
            "!=" => {
                if field_value.is_number() && expected.is_number() {
                    Self::numeric_comparison(field_value, &expected, |a, b| (a - b).abs() >= f64::EPSILON)
                } else {
                    Ok(field_value != &expected)
                }
            },
            ">" => Self::numeric_comparison(field_value, &expected, |a, b| a > b),
            ">=" => Self::numeric_comparison(field_value, &expected, |a, b| a >= b),
            "<" => Self::numeric_comparison(field_value, &expected, |a, b| a < b),
            "<=" => Self::numeric_comparison(field_value, &expected, |a, b| a <= b),
            _ => Err(format!("Unknown operator: {op}")),
        }
    }

    /// Parse a value string into JSON value.
    fn parse_value(value: &str) -> Result<serde_json::Value, String> {
        let value = value.trim();

        // Check if it's a string (enclosed in quotes)
        if (value.starts_with('\'') && value.ends_with('\''))
            || (value.starts_with('"') && value.ends_with('"'))
        {
            let string_value = value[1..value.len() - 1].to_string();
            return Ok(serde_json::Value::String(string_value));
        }

        // Check for boolean
        if value.eq_ignore_ascii_case("true") {
            return Ok(serde_json::Value::Bool(true));
        }
        if value.eq_ignore_ascii_case("false") {
            return Ok(serde_json::Value::Bool(false));
        }

        // Try to parse as number
        if let Ok(num) = value.parse::<f64>() {
            return Ok(serde_json::json!(num));
        }

        Err(format!("Cannot parse value: {value}"))
    }

    /// Perform numeric comparison between two values.
    fn numeric_comparison<F>(
        field_value: &serde_json::Value,
        expected: &serde_json::Value,
        compare: F,
    ) -> Result<bool, String>
    where
        F: Fn(f64, f64) -> bool,
    {
        let field_num = field_value
            .as_f64()
            .ok_or_else(|| "Field value is not a number".to_string())?;

        let expected_num = expected
            .as_f64()
            .ok_or_else(|| "Expected value is not a number".to_string())?;

        Ok(compare(field_num, expected_num))
    }

    /// Simulate sending an event to all subscribers (for testing).
    ///
    /// This is primarily useful for testing subscription functionality
    /// without requiring a live event source.
    pub async fn simulate_event(&self, event: crate::HistoricalEvent) {
        self.broadcast_event(&event);
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

    // Cycle 1: Parse Filter Expressions

    #[test]
    fn test_parse_filter_simple_equality() {
        let (field, op, value) = SubscriptionManager::parse_filter("status = 'shipped'").unwrap();
        assert_eq!(field, "status");
        assert_eq!(op, "=");
        assert_eq!(value, "'shipped'");
    }

    #[test]
    fn test_parse_filter_numeric_comparison() {
        let (field, op, value) = SubscriptionManager::parse_filter("total > 50").unwrap();
        assert_eq!(field, "total");
        assert_eq!(op, ">");
        assert_eq!(value, "50");
    }

    #[test]
    fn test_parse_filter_nested_field() {
        let (field, op, value) = SubscriptionManager::parse_filter("address.city = 'Paris'").unwrap();
        assert_eq!(field, "address.city");
        assert_eq!(op, "=");
        assert_eq!(value, "'Paris'");
    }

    #[test]
    fn test_parse_filter_all_operators() {
        let tests = vec![
            ("a = 'x'", "="),
            ("a != 'x'", "!="),
            ("a > 5", ">"),
            ("a >= 5", ">="),
            ("a < 5", "<"),
            ("a <= 5", "<="),
        ];

        for (filter, expected_op) in tests {
            let (_, op, _) = SubscriptionManager::parse_filter(filter).unwrap();
            assert_eq!(op, expected_op, "Filter: {}", filter);
        }
    }

    #[test]
    fn test_parse_filter_with_spaces() {
        let (field, op, value) = SubscriptionManager::parse_filter("  total   >   100  ").unwrap();
        assert_eq!(field, "total");
        assert_eq!(op, ">");
        assert_eq!(value, "100");
    }

    #[test]
    fn test_parse_filter_invalid_no_operator() {
        let result = SubscriptionManager::parse_filter("total");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_filter_invalid_empty_field() {
        let result = SubscriptionManager::parse_filter("= 'value'");
        assert!(result.is_err());
    }

    // Cycle 2: Evaluate Predicates Against Data

    #[test]
    fn test_evaluate_filter_string_equality() {
        let data = serde_json::json!({"status": "shipped"});
        assert!(SubscriptionManager::evaluate_filter(&data, "status = 'shipped'").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "status = 'pending'").unwrap());
    }

    #[test]
    fn test_evaluate_filter_numeric_greater_than() {
        let data = serde_json::json!({"total": 100});
        assert!(SubscriptionManager::evaluate_filter(&data, "total > 50").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "total > 150").unwrap());
    }

    #[test]
    fn test_evaluate_filter_numeric_less_than() {
        let data = serde_json::json!({"price": 25});
        assert!(SubscriptionManager::evaluate_filter(&data, "price < 30").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "price < 20").unwrap());
    }

    #[test]
    fn test_evaluate_filter_numeric_equality() {
        let data = serde_json::json!({"count": 42});
        assert!(SubscriptionManager::evaluate_filter(&data, "count = 42").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "count = 43").unwrap());
    }

    #[test]
    fn test_evaluate_filter_boolean_true() {
        let data = serde_json::json!({"active": true});
        assert!(SubscriptionManager::evaluate_filter(&data, "active = true").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "active = false").unwrap());
    }

    #[test]
    fn test_evaluate_filter_boolean_false() {
        let data = serde_json::json!({"deleted": false});
        assert!(SubscriptionManager::evaluate_filter(&data, "deleted = false").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "deleted = true").unwrap());
    }

    #[test]
    fn test_evaluate_filter_nested_field() {
        let data = serde_json::json!({
            "address": {
                "city": "Paris",
                "country": "France"
            }
        });
        assert!(SubscriptionManager::evaluate_filter(&data, "address.city = 'Paris'").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "address.city = 'London'").unwrap());
    }

    #[test]
    fn test_evaluate_filter_nested_numeric() {
        let data = serde_json::json!({
            "shipping": {
                "cost": 15.50
            }
        });
        assert!(SubscriptionManager::evaluate_filter(&data, "shipping.cost > 10").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "shipping.cost > 20").unwrap());
    }

    #[test]
    fn test_evaluate_filter_not_equal() {
        let data = serde_json::json!({"status": "pending"});
        assert!(SubscriptionManager::evaluate_filter(&data, "status != 'shipped'").unwrap());
        assert!(!SubscriptionManager::evaluate_filter(&data, "status != 'pending'").unwrap());
    }

    // Cycle 3: Test Filter Matching with Real Events

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
    }

    #[test]
    fn test_matches_filter_with_valid_filter() {
        let event = HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"total": 100, "status": "shipped"}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        assert!(SubscriptionManager::matches_filter(
            &event,
            &Some("total > 50".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_rejects_non_matching() {
        let event = HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"total": 30}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        assert!(!SubscriptionManager::matches_filter(
            &event,
            &Some("total > 50".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_with_nested_field() {
        let event = HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({
                "total": 100,
                "address": {
                    "city": "Paris"
                }
            }),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        assert!(SubscriptionManager::matches_filter(
            &event,
            &Some("address.city = 'Paris'".to_string())
        ));
        assert!(!SubscriptionManager::matches_filter(
            &event,
            &Some("address.city = 'London'".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_invalid_syntax_fails_open() {
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

        // Invalid filter should fail open (accept the event)
        assert!(SubscriptionManager::matches_filter(
            &event,
            &Some("invalid filter syntax".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_missing_field_fails_open() {
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

        // Filter for non-existent field should fail open
        assert!(SubscriptionManager::matches_filter(
            &event,
            &Some("status = 'shipped'".to_string())
        ));
    }

    #[test]
    fn test_matches_filter_in_broadcast() {
        let manager = SubscriptionManager::new();

        let mut rx1 = manager.subscribe("sub-1".to_string(), "Order".to_string(), Some("total > 50".to_string()));
        let mut rx2 = manager.subscribe("sub-2".to_string(), "Order".to_string(), Some("total <= 50".to_string()));

        let event_high = HistoricalEvent {
            id:          Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   Uuid::new_v4(),
            data:        serde_json::json!({"total": 100}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   Utc::now(),
        };

        manager.broadcast_event(&event_high);

        // rx1 should receive (total > 50)
        assert!(rx1.try_recv().ok().is_some());
        // rx2 should not receive (total <= 50 is false)
        assert!(rx2.try_recv().ok().is_none());
    }

    #[tokio::test]
    async fn test_simulate_event_broadcasts() {
        let manager = SubscriptionManager::new();
        let mut rx = manager.subscribe("sub-1".to_string(), "Order".to_string(), None);

        let event = crate::HistoricalEvent {
            id:          uuid::Uuid::new_v4(),
            event_type:  "INSERT".to_string(),
            entity_type: "Order".to_string(),
            entity_id:   uuid::Uuid::new_v4(),
            data:        serde_json::json!({"total": 100}),
            user_id:     None,
            tenant_id:   None,
            timestamp:   chrono::Utc::now(),
        };

        manager.simulate_event(event.clone()).await;
        let received = rx.try_recv();
        assert!(received.is_ok(), "subscriber should receive simulated event");
        assert_eq!(received.unwrap().entity_type, "Order");
    }
}
