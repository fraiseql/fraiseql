//! Event filtering for subscriptions
//!
//! Implements filtering logic to determine which events match a subscription's filter criteria.

use crate::subscriptions::event_bus::Event;
use serde_json::Value;
use std::collections::HashMap;

/// Event filter for subscriptions
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// Field-based filters (field_name -> value_condition)
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
    pub fn new() -> Self {
        Self {
            field_filters: HashMap::new(),
            event_type_filter: None,
            channel_filter: None,
        }
    }

    /// Add field filter
    pub fn with_field(mut self, field: &str, condition: FilterCondition) -> Self {
        self.field_filters.insert(field.to_string(), condition);
        self
    }

    /// Add event type filter
    pub fn with_event_type(mut self, event_type: &str) -> Self {
        self.event_type_filter = Some(event_type.to_string());
        self
    }

    /// Add channel filter
    pub fn with_channel(mut self, channel: &str) -> Self {
        self.channel_filter = Some(channel.to_string());
        self
    }

    /// Check if event matches filter
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
            if !self.matches_condition(&event.data, field, condition) {
                return false;
            }
        }

        true
    }

    /// Check if a value matches a condition
    fn matches_condition(&self, data: &Value, field: &str, condition: &FilterCondition) -> bool {
        let value = self.get_field_value(data, field);

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
                if let Some(val) = value {
                    values.contains(&val)
                } else {
                    false
                }
            }

            FilterCondition::NotIn(values) => {
                if let Some(val) = value {
                    !values.contains(&val)
                } else {
                    true
                }
            }
        }
    }

    /// Get field value from nested data structure (supports dot notation)
    fn get_field_value(&self, data: &Value, field: &str) -> Option<Value> {
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
}
