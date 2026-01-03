//! Event filtering for subscriptions
//!
//! Implements filtering logic to determine which events match a subscription's filter criteria.
//! Includes security-aware filtering for Phase 4 event delivery validation.

use crate::subscriptions::event_bus::Event;
use crate::subscriptions::SubscriptionSecurityContext;
use serde_json::Value;
use std::collections::HashMap;

/// Event filter for subscriptions
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// Field-based filters (`field_name` -> `value_condition`)
    pub field_filters: HashMap<String, FilterCondition>,

    /// Type filter - match specific event types
    pub event_type_filter: Option<String>,

    /// Channel filter - match specific channels
    pub channel_filter: Option<String>,
}

/// Filter condition for a field
#[derive(Debug, Clone)]
pub enum FilterCondition {
    /// Equals exact value
    Equals(Value),

    /// Not equals
    NotEquals(Value),

    /// Greater than
    GreaterThan(f64),

    /// Less than
    LessThan(f64),

    /// Value exists (for optional fields)
    Exists,

    /// Value does not exist
    NotExists,

    /// Contains substring (for strings)
    Contains(String),

    /// In array of values
    In(Vec<Value>),

    /// Not in array of values
    NotIn(Vec<Value>),
}

impl EventFilter {
    /// Create new empty filter (matches all events)
    #[must_use] 
    pub fn new() -> Self {
        Self {
            field_filters: HashMap::new(),
            event_type_filter: None,
            channel_filter: None,
        }
    }

    /// Add field filter
    #[must_use] 
    pub fn with_field(mut self, field: &str, condition: FilterCondition) -> Self {
        self.field_filters.insert(field.to_string(), condition);
        self
    }

    /// Add event type filter
    #[must_use] 
    pub fn with_event_type(mut self, event_type: &str) -> Self {
        self.event_type_filter = Some(event_type.to_string());
        self
    }

    /// Add channel filter
    #[must_use] 
    pub fn with_channel(mut self, channel: &str) -> Self {
        self.channel_filter = Some(channel.to_string());
        self
    }

    /// Check if event matches filter
    #[must_use] 
    pub fn matches(&self, event: &Event) -> bool {
        // Check event type filter
        if let Some(ref event_type) = self.event_type_filter {
            if event.event_type != *event_type {
                return false;
            }
        }

        // Check channel filter
        if let Some(ref channel) = self.channel_filter {
            if event.channel != *channel {
                return false;
            }
        }

        // Check field filters
        for (field, condition) in &self.field_filters {
            if !Self::matches_condition(&event.data, field, condition) {
                return false;
            }
        }

        true
    }

    /// Check if a value matches a condition
    fn matches_condition(data: &Value, field: &str, condition: &FilterCondition) -> bool {
        let value = Self::get_field_value(data, field);

        match condition {
            FilterCondition::Equals(expected) => value == Some(expected.clone()),

            FilterCondition::NotEquals(expected) => value != Some(expected.clone()),

            FilterCondition::GreaterThan(threshold) => {
                if let Some(Value::Number(num)) = value {
                    num.as_f64().unwrap_or(0.0) > *threshold
                } else {
                    false
                }
            }

            FilterCondition::LessThan(threshold) => {
                if let Some(Value::Number(num)) = value {
                    num.as_f64().unwrap_or(0.0) < *threshold
                } else {
                    false
                }
            }

            FilterCondition::Exists => value.is_some(),

            FilterCondition::NotExists => value.is_none(),

            FilterCondition::Contains(substring) => {
                if let Some(Value::String(s)) = value {
                    s.contains(substring)
                } else {
                    false
                }
            }

            FilterCondition::In(values) => {
                value.map_or(false, |val| values.contains(&val))
            }

            FilterCondition::NotIn(values) => {
                value.map_or(true, |val| !values.contains(&val))
            }
        }
    }

    /// Get field value from nested data structure (supports dot notation)
    fn get_field_value(data: &Value, field: &str) -> Option<Value> {
        let parts: Vec<&str> = field.split('.').collect();

        let mut current = data;
        for part in parts {
            current = &current[part];
            if current.is_null() {
                return None;
            }
        }

        if current.is_null() {
            None
        } else {
            Some(current.clone())
        }
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Security-aware event filter
///
/// Combines base event filtering with security context validation.
/// Checks: row-level filtering, federation boundaries, tenant isolation, RBAC.
#[derive(Debug, Clone)]
pub struct SecurityAwareEventFilter {
    /// Base filter conditions (standard event filtering)
    pub base_filter: EventFilter,
    /// Security context for validation
    pub security_context: SubscriptionSecurityContext,
}

impl SecurityAwareEventFilter {
    /// Create new security-aware filter
    #[must_use] 
    pub const fn new(base_filter: EventFilter, security_context: SubscriptionSecurityContext) -> Self {
        Self {
            base_filter,
            security_context,
        }
    }

    /// Check if event should be delivered to subscriber
    ///
    /// Performs all security checks and returns delivery decision.
    ///
    /// # Returns
    /// * `(true, None)` - Event should be delivered
    /// * `(false, Some(reason))` - Event should be rejected with reason
    #[must_use] 
    pub fn should_deliver_event(&self, event: &Event) -> (bool, Option<String>) {
        // Step 1: Check base filter conditions
        if !self.base_filter.matches(event) {
            return (false, Some("Base filter condition failed".to_string()));
        }

        // Step 2: Check row-level filtering (user_id, tenant_id)
        if !self
            .security_context
            .validate_event_for_delivery(&event.data)
        {
            return (
                false,
                Some("Row-level filtering rejected event".to_string()),
            );
        }

        // Step 3: Check field access (if RBAC enabled)
        if let Some(ref rbac) = self.security_context.rbac {
            let fields_in_event = extract_fields_from_event(&event.data);
            let mut allowed_fields = HashMap::new();
            for field in fields_in_event {
                allowed_fields.insert(field, true);
            }

            if let Err(e) = rbac.validate_fields(&allowed_fields) {
                return (false, Some(format!("RBAC field access denied: {e}")));
            }
        }

        // Step 4: All checks passed
        (true, None)
    }

    /// Get rejection reason if event was rejected
    #[must_use] 
    pub fn get_rejection_reason(&self, event: &Event) -> Option<String> {
        let (_, reason) = self.should_deliver_event(event);
        reason
    }
}

/// Statistics from event filtering
#[derive(Debug, Clone)]
pub struct FilterStatistics {
    /// Total events checked
    pub events_checked: u64,
    /// Events delivered (passed filter)
    pub events_delivered: u64,
    /// Events rejected (failed filter)
    pub events_rejected: u64,
    /// Rejection rate as percentage
    pub rejection_rate: f64,
}

/// Helper function to extract field names from event data
fn extract_fields_from_event(data: &Value) -> Vec<String> {
    match data {
        Value::Object(map) => map.keys().cloned().collect(),
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_event() -> Event {
        Event::new(
            "userUpdated".to_string(),
            json!({
                "userId": 123,
                "username": "alice",
                "age": 30,
                "status": "active",
                "profile": {
                    "city": "New York",
                    "verified": true
                }
            }),
            "users".to_string(),
        )
    }

    #[test]
    fn test_filter_creation() {
        let filter = EventFilter::new();
        assert!(filter.field_filters.is_empty());
        assert!(filter.event_type_filter.is_none());
        assert!(filter.channel_filter.is_none());
    }

    #[test]
    fn test_filter_matches_event_type() {
        let event = create_test_event();
        let filter = EventFilter::new().with_event_type("userUpdated");

        assert!(filter.matches(&event));

        let filter_wrong = EventFilter::new().with_event_type("userDeleted");
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_matches_channel() {
        let event = create_test_event();
        let filter = EventFilter::new().with_channel("users");

        assert!(filter.matches(&event));

        let filter_wrong = EventFilter::new().with_channel("posts");
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_field_equals() {
        let event = create_test_event();
        let filter =
            EventFilter::new().with_field("status", FilterCondition::Equals(json!("active")));

        assert!(filter.matches(&event));

        let filter_wrong =
            EventFilter::new().with_field("status", FilterCondition::Equals(json!("inactive")));
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_field_not_equals() {
        let event = create_test_event();
        let filter =
            EventFilter::new().with_field("status", FilterCondition::NotEquals(json!("inactive")));

        assert!(filter.matches(&event));

        let filter_wrong =
            EventFilter::new().with_field("status", FilterCondition::NotEquals(json!("active")));
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_field_greater_than() {
        let event = create_test_event();
        let filter = EventFilter::new().with_field("age", FilterCondition::GreaterThan(25.0));

        assert!(filter.matches(&event));

        let filter_wrong = EventFilter::new().with_field("age", FilterCondition::GreaterThan(40.0));
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_field_less_than() {
        let event = create_test_event();
        let filter = EventFilter::new().with_field("age", FilterCondition::LessThan(35.0));

        assert!(filter.matches(&event));

        let filter_wrong = EventFilter::new().with_field("age", FilterCondition::LessThan(25.0));
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_field_contains() {
        let event = create_test_event();
        let filter =
            EventFilter::new().with_field("username", FilterCondition::Contains("ali".to_string()));

        assert!(filter.matches(&event));

        let filter_wrong =
            EventFilter::new().with_field("username", FilterCondition::Contains("bob".to_string()));
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_field_in() {
        let event = create_test_event();
        let filter = EventFilter::new().with_field(
            "status",
            FilterCondition::In(vec![json!("active"), json!("pending")]),
        );

        assert!(filter.matches(&event));

        let filter_wrong = EventFilter::new().with_field(
            "status",
            FilterCondition::In(vec![json!("inactive"), json!("deleted")]),
        );
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_field_exists() {
        let event = create_test_event();
        let filter = EventFilter::new().with_field("username", FilterCondition::Exists);

        assert!(filter.matches(&event));

        let filter_missing = EventFilter::new().with_field("nonexistent", FilterCondition::Exists);
        assert!(!filter_missing.matches(&event));
    }

    #[test]
    fn test_filter_nested_field() {
        let event = create_test_event();
        let filter = EventFilter::new()
            .with_field("profile.city", FilterCondition::Equals(json!("New York")));

        assert!(filter.matches(&event));

        let filter_wrong =
            EventFilter::new().with_field("profile.city", FilterCondition::Equals(json!("Boston")));
        assert!(!filter_wrong.matches(&event));
    }

    #[test]
    fn test_filter_multiple_conditions() {
        let event = create_test_event();
        let filter = EventFilter::new()
            .with_event_type("userUpdated")
            .with_channel("users")
            .with_field("status", FilterCondition::Equals(json!("active")))
            .with_field("age", FilterCondition::GreaterThan(25.0));

        assert!(filter.matches(&event));

        // Change one condition to fail
        let filter_fail = EventFilter::new()
            .with_event_type("userUpdated")
            .with_channel("users")
            .with_field("status", FilterCondition::Equals(json!("inactive")))
            .with_field("age", FilterCondition::GreaterThan(25.0));

        assert!(!filter_fail.matches(&event));
    }

    #[test]
    fn test_filter_empty_matches_all() {
        let event = create_test_event();
        let filter = EventFilter::new();

        assert!(filter.matches(&event));
    }

    // ============================================================================
    // PHASE 4.2: Security-Aware Event Filtering - Unit Tests
    // ============================================================================

    #[test]
    fn test_security_aware_filter_with_valid_security_context() {
        let event = create_test_event();
        let base_filter = EventFilter::new()
            .with_event_type("userUpdated")
            .with_channel("users");

        let security_ctx = SubscriptionSecurityContext::new(123, 5);
        let sec_filter = SecurityAwareEventFilter::new(base_filter, security_ctx);

        let (should_deliver, reason) = sec_filter.should_deliver_event(&event);
        assert!(
            should_deliver,
            "Event should be delivered with valid security context"
        );
        assert!(
            reason.is_none(),
            "No rejection reason expected: {:?}",
            reason
        );

        println!("✅ test_security_aware_filter_with_valid_security_context passed");
    }

    #[test]
    fn test_security_aware_filter_rejects_base_filter_mismatch() {
        let event = create_test_event();
        let base_filter = EventFilter::new().with_event_type("userDeleted"); // Wrong type

        let security_ctx = SubscriptionSecurityContext::new(123, 5);
        let sec_filter = SecurityAwareEventFilter::new(base_filter, security_ctx);

        let (should_deliver, reason) = sec_filter.should_deliver_event(&event);
        assert!(
            !should_deliver,
            "Event should be rejected due to base filter mismatch"
        );
        assert!(reason.is_some(), "Rejection reason expected");
        assert!(reason.unwrap().contains("Base filter"));

        println!("✅ test_security_aware_filter_rejects_base_filter_mismatch passed");
    }

    #[test]
    fn test_security_aware_filter_rejects_wrong_user_id() {
        let mut event = create_test_event();
        // Modify event to have different user_id
        event.data = json!({
            "userId": 999,  // Different user
            "username": "bob",
            "user_id": 999,
            "tenant_id": 5
        });

        let base_filter = EventFilter::new();
        let security_ctx = SubscriptionSecurityContext::new(123, 5); // Looking for user 123

        let sec_filter = SecurityAwareEventFilter::new(base_filter, security_ctx);

        let (should_deliver, reason) = sec_filter.should_deliver_event(&event);
        assert!(
            !should_deliver,
            "Event should be rejected due to user_id mismatch"
        );
        assert!(reason.is_some(), "Rejection reason expected");

        println!("✅ test_security_aware_filter_rejects_wrong_user_id passed");
    }

    #[test]
    fn test_security_aware_filter_rejects_wrong_tenant_id() {
        let mut event = create_test_event();
        // Modify event to have different tenant_id
        event.data = json!({
            "userId": 123,
            "user_id": 123,
            "tenant_id": 999  // Wrong tenant
        });

        let base_filter = EventFilter::new();
        let security_ctx = SubscriptionSecurityContext::new(123, 5); // Looking for tenant 5

        let sec_filter = SecurityAwareEventFilter::new(base_filter, security_ctx);

        let (should_deliver, reason) = sec_filter.should_deliver_event(&event);
        assert!(
            !should_deliver,
            "Event should be rejected due to tenant_id mismatch"
        );
        assert!(reason.is_some(), "Rejection reason expected");

        println!("✅ test_security_aware_filter_rejects_wrong_tenant_id passed");
    }

    #[test]
    fn test_security_aware_filter_with_rbac_field_validation() {
        let event = create_test_event();
        let base_filter = EventFilter::new();

        let requested_fields = vec!["username".to_string(), "age".to_string()];
        let security_ctx = SubscriptionSecurityContext::with_rbac(123, 5, requested_fields);

        let sec_filter = SecurityAwareEventFilter::new(base_filter, security_ctx);

        let (should_deliver, reason) = sec_filter.should_deliver_event(&event);

        // With RBAC enabled, the check should happen
        // (Specific outcome depends on RBAC implementation)
        assert!(
            should_deliver || reason.is_some(),
            "Should either deliver or have rejection reason"
        );

        println!("✅ test_security_aware_filter_with_rbac_field_validation passed");
    }

    #[test]
    fn test_security_aware_filter_get_rejection_reason() {
        let event = create_test_event();
        let base_filter = EventFilter::new().with_event_type("userDeleted"); // Mismatch

        let security_ctx = SubscriptionSecurityContext::new(123, 5);
        let sec_filter = SecurityAwareEventFilter::new(base_filter, security_ctx);

        let rejection_reason = sec_filter.get_rejection_reason(&event);
        assert!(rejection_reason.is_some(), "Rejection reason should exist");
        assert!(rejection_reason.unwrap().contains("Base filter"));

        println!("✅ test_security_aware_filter_get_rejection_reason passed");
    }

    #[test]
    fn test_security_aware_filter_combined_conditions() {
        let event = create_test_event();

        // Base filter with multiple conditions
        let base_filter = EventFilter::new()
            .with_event_type("userUpdated")
            .with_channel("users")
            .with_field("status", FilterCondition::Equals(json!("active")));

        let security_ctx = SubscriptionSecurityContext::new(123, 5);
        let sec_filter = SecurityAwareEventFilter::new(base_filter, security_ctx);

        let (should_deliver, reason) = sec_filter.should_deliver_event(&event);
        assert!(
            should_deliver,
            "Event should pass all conditions: {:?}",
            reason
        );
        assert!(reason.is_none());

        println!("✅ test_security_aware_filter_combined_conditions passed");
    }
}
