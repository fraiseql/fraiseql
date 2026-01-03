//! Row-level filtering for subscriptions
//!
//! Implements user_id and tenant_id based filtering on subscription events.

use serde_json::Value;

/// Subscription filter context
///
/// Contains user and tenant information used to filter events before delivery
/// to subscription clients. Events that don't match the filter are dropped.
#[derive(Debug, Clone)]
pub struct RowFilterContext {
    /// User ID from authenticated connection
    pub user_id: Option<i64>,
    /// Tenant ID from authenticated connection
    pub tenant_id: Option<i64>,
    /// Whether to apply user_id filtering
    pub filter_by_user: bool,
    /// Whether to apply tenant_id filtering
    pub filter_by_tenant: bool,
}

impl RowFilterContext {
    /// Create new filter context from user credentials
    pub fn new(user_id: Option<i64>, tenant_id: Option<i64>) -> Self {
        let filter_by_user = user_id.is_some();
        let filter_by_tenant = tenant_id.is_some();

        Self {
            user_id,
            tenant_id,
            filter_by_user,
            filter_by_tenant,
        }
    }

    /// Create filter context with no filtering (for testing)
    pub fn no_filter() -> Self {
        Self {
            user_id: None,
            tenant_id: None,
            filter_by_user: false,
            filter_by_tenant: false,
        }
    }

    /// Create filter context that only filters by user_id
    pub fn user_only(user_id: i64) -> Self {
        Self {
            user_id: Some(user_id),
            tenant_id: None,
            filter_by_user: true,
            filter_by_tenant: false,
        }
    }

    /// Create filter context that only filters by tenant_id
    pub fn tenant_only(tenant_id: i64) -> Self {
        Self {
            user_id: None,
            tenant_id: Some(tenant_id),
            filter_by_user: false,
            filter_by_tenant: true,
        }
    }

    /// Check if event passes the row-level filter
    ///
    /// Returns true if the event data contains matching user_id and/or tenant_id values.
    /// If no filtering is enabled, always returns true.
    ///
    /// # Filtering Logic
    /// - If `filter_by_user` is true: event.data.user_id must equal context.user_id
    /// - If `filter_by_tenant` is true: event.data.tenant_id must equal context.tenant_id
    /// - Both filters must pass if both are enabled (AND logic)
    ///
    /// # Event Data Format
    /// Expects event.data to be a JSON object with optional user_id and tenant_id fields:
    /// ```json
    /// {
    ///   "id": "order-123",
    ///   "user_id": 42,
    ///   "tenant_id": 5,
    ///   "amount": 100.50
    /// }
    /// ```
    pub fn matches(&self, event_data: &Value) -> bool {
        // If no filtering enabled, accept all events
        if !self.filter_by_user && !self.filter_by_tenant {
            return true;
        }

        // Check user_id filter if enabled
        if self.filter_by_user {
            match (self.user_id, event_data.get("user_id")) {
                (Some(expected_user), Some(actual_user)) => {
                    // Event user_id must match context user_id
                    if actual_user.as_i64() != Some(expected_user) {
                        return false;
                    }
                }
                (Some(_), None) => {
                    // Filter requires user_id but event doesn't have it
                    return false;
                }
                (None, _) => {
                    // Context has no user_id, can't filter
                    return false;
                }
            }
        }

        // Check tenant_id filter if enabled
        if self.filter_by_tenant {
            match (self.tenant_id, event_data.get("tenant_id")) {
                (Some(expected_tenant), Some(actual_tenant)) => {
                    // Event tenant_id must match context tenant_id
                    if actual_tenant.as_i64() != Some(expected_tenant) {
                        return false;
                    }
                }
                (Some(_), None) => {
                    // Filter requires tenant_id but event doesn't have it
                    return false;
                }
                (None, _) => {
                    // Context has no tenant_id, can't filter
                    return false;
                }
            }
        }

        true
    }

    /// Check if event should be filtered (opposite of matches)
    pub fn should_drop(&self, event_data: &Value) -> bool {
        !self.matches(event_data)
    }

    /// Get description of filter for logging
    pub fn describe(&self) -> String {
        match (self.filter_by_user, self.filter_by_tenant) {
            (false, false) => "No filtering".to_string(),
            (true, false) => format!("user_id={}", self.user_id.unwrap_or(0)),
            (false, true) => format!("tenant_id={}", self.tenant_id.unwrap_or(0)),
            (true, true) => format!(
                "user_id={} AND tenant_id={}",
                self.user_id.unwrap_or(0),
                self.tenant_id.unwrap_or(0)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_no_filter_accepts_all() {
        let filter = RowFilterContext::no_filter();
        let event_data = json!({ "user_id": 123, "tenant_id": 1 });
        assert!(filter.matches(&event_data));
    }

    #[test]
    fn test_user_filter_matching() {
        let filter = RowFilterContext::user_only(123);
        let event_data = json!({ "user_id": 123, "other_field": "value" });
        assert!(filter.matches(&event_data));
    }

    #[test]
    fn test_user_filter_mismatching() {
        let filter = RowFilterContext::user_only(123);
        let event_data = json!({ "user_id": 456, "other_field": "value" });
        assert!(!filter.matches(&event_data));
    }

    #[test]
    fn test_user_filter_missing_field() {
        let filter = RowFilterContext::user_only(123);
        let event_data = json!({ "other_field": "value" });
        assert!(!filter.matches(&event_data));
    }

    #[test]
    fn test_tenant_filter_matching() {
        let filter = RowFilterContext::tenant_only(5);
        let event_data = json!({ "tenant_id": 5, "user_id": 100 });
        assert!(filter.matches(&event_data));
    }

    #[test]
    fn test_tenant_filter_mismatching() {
        let filter = RowFilterContext::tenant_only(5);
        let event_data = json!({ "tenant_id": 10, "user_id": 100 });
        assert!(!filter.matches(&event_data));
    }

    #[test]
    fn test_combined_user_and_tenant_filter() {
        let filter = RowFilterContext::new(Some(123), Some(5));

        // Both match
        let event = json!({ "user_id": 123, "tenant_id": 5 });
        assert!(filter.matches(&event));

        // User matches, tenant doesn't
        let event = json!({ "user_id": 123, "tenant_id": 10 });
        assert!(!filter.matches(&event));

        // Tenant matches, user doesn't
        let event = json!({ "user_id": 456, "tenant_id": 5 });
        assert!(!filter.matches(&event));

        // Neither match
        let event = json!({ "user_id": 456, "tenant_id": 10 });
        assert!(!filter.matches(&event));
    }

    #[test]
    fn test_should_drop() {
        let filter = RowFilterContext::user_only(123);
        let matching_event = json!({ "user_id": 123 });
        let mismatching_event = json!({ "user_id": 456 });

        assert!(!filter.should_drop(&matching_event));
        assert!(filter.should_drop(&mismatching_event));
    }

    #[test]
    fn test_filter_describe() {
        assert_eq!(
            RowFilterContext::no_filter().describe(),
            "No filtering"
        );
        assert_eq!(
            RowFilterContext::user_only(123).describe(),
            "user_id=123"
        );
        assert_eq!(
            RowFilterContext::tenant_only(5).describe(),
            "tenant_id=5"
        );
        assert_eq!(
            RowFilterContext::new(Some(123), Some(5)).describe(),
            "user_id=123 AND tenant_id=5"
        );
    }

    #[test]
    fn test_filter_with_complex_event_data() {
        let filter = RowFilterContext::new(Some(100), Some(2));
        let event_data = json!({
            "id": "order-456",
            "user_id": 100,
            "tenant_id": 2,
            "amount": 99.99,
            "items": [
                { "sku": "ABC", "qty": 1 },
                { "sku": "XYZ", "qty": 2 }
            ],
            "metadata": {
                "source": "web",
                "timestamp": 1609459200
            }
        });

        assert!(filter.matches(&event_data));
    }

    #[test]
    fn test_filter_with_non_integer_user_id() {
        let filter = RowFilterContext::user_only(123);
        let event_data = json!({ "user_id": "not-an-int" });
        assert!(!filter.matches(&event_data));
    }

    #[test]
    fn test_filter_with_null_ids() {
        let filter = RowFilterContext::user_only(123);
        let event_data = json!({ "user_id": null });
        assert!(!filter.matches(&event_data));
    }
}
