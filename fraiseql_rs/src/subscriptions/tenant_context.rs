//! Multi-tenant enforcement for subscriptions
//!
//! Implements tenant-based event routing and filtering.
//! Ensures events from one tenant never leak to another tenant's subscriptions.

use serde_json::Value;

/// Multi-tenant context for event scoping
///
/// Enforces tenant boundaries by:
/// - Routing events through tenant-scoped channels
/// - Filtering events by `tenant_id` before delivery
/// - Preventing cross-tenant subscription attempts
#[derive(Debug, Clone)]
pub struct TenantContext {
    /// Tenant ID from authenticated connection
    pub tenant_id: i64,
    /// Whether to enforce tenant filtering
    pub enforce_filtering: bool,
}

impl TenantContext {
    /// Create new tenant context with filtering enabled
    #[must_use] 
    pub const fn new(tenant_id: i64) -> Self {
        Self {
            tenant_id,
            enforce_filtering: true,
        }
    }

    /// Create tenant context with explicit filtering control
    #[must_use] 
    pub const fn with_filtering(tenant_id: i64, enforce_filtering: bool) -> Self {
        Self {
            tenant_id,
            enforce_filtering,
        }
    }

    /// Create tenant context for single-tenant mode (no filtering)
    #[must_use] 
    pub const fn single_tenant() -> Self {
        Self {
            tenant_id: 1,
            enforce_filtering: false,
        }
    }

    /// Generate tenant-scoped channel name
    ///
    /// Multi-tenant systems should use this to route events through tenant-specific channels.
    /// This prevents events from leaking between tenants at the event bus level.
    ///
    /// # Example
    /// ```text
    /// base_channel: "orders"
    /// tenant_id: 5
    /// result: "orders:tenant-5"
    /// ```
    #[must_use] 
    pub fn scoped_channel(&self, base_channel: &str) -> String {
        if self.enforce_filtering {
            format!("{}:tenant-{}", base_channel, self.tenant_id)
        } else {
            base_channel.to_string()
        }
    }

    /// Check if event data belongs to this tenant
    ///
    /// Returns true if:
    /// - Event contains `tenant_id` field matching context `tenant_id`
    /// - Or filtering is disabled (single-tenant mode)
    ///
    /// # Event Data Format
    /// Expects event.data to be a JSON object with optional `tenant_id` field:
    /// ```json
    /// {
    ///   "id": "order-123",
    ///   "user_id": 42,
    ///   "tenant_id": 5,
    ///   "amount": 100.50
    /// }
    /// ```
    #[must_use] 
    pub fn matches(&self, event_data: &Value) -> bool {
        // Single-tenant mode: accept all events
        if !self.enforce_filtering {
            return true;
        }

        // Multi-tenant mode: check tenant_id field
        event_data.get("tenant_id").map_or(false, |event_tenant| {
            // Event tenant_id must match context tenant_id
            event_tenant.as_i64() == Some(self.tenant_id)
        })
    }

    /// Check if event should be filtered (opposite of matches)
    #[must_use] 
    pub fn should_drop(&self, event_data: &Value) -> bool {
        !self.matches(event_data)
    }

    /// Validate that subscription `tenant_id` variable matches context
    ///
    /// When a subscription includes a `tenant_id` variable (explicit tenant scoping),
    /// it must match the authenticated context's `tenant_id`.
    ///
    /// # Returns
    /// - `true` if subscription is allowed (`tenant_id` matches or not specified)
    /// - `false` if subscription violates tenant boundaries
    ///
    /// # Example
    /// ```text
    /// context.tenant_id = 5
    /// subscription_variables = { "tenant_id": 5 }  // ALLOWED
    /// subscription_variables = { "tenant_id": 10 } // REJECTED
    /// subscription_variables = {}                   // ALLOWED (wildcard)
    /// ```
    #[must_use] 
    pub fn validate_subscription_variables(&self, variables: &Value) -> bool {
        // If no filtering enabled, all subscriptions allowed
        if !self.enforce_filtering {
            return true;
        }

        // Check if subscription includes explicit tenant_id variable
        variables.get("tenant_id").map_or(true, |sub_tenant_id| {
            sub_tenant_id.as_i64().map_or(false, |tenant_id| {
                // Must match context tenant_id
                tenant_id == self.tenant_id
            })
        })
    }

    /// Get all possible tenant IDs that could access a resource
    ///
    /// Used for validating multi-tenant subscriptions.
    /// Returns a set of authorized tenant IDs for this context.
    ///
    /// In a multi-tenant system, subscriptions should only receive events
    /// from authorized tenants.
    #[must_use] 
    pub fn authorized_tenants(&self) -> Vec<i64> {
        if self.enforce_filtering {
            vec![self.tenant_id]
        } else {
            // Single-tenant mode: all tenants could theoretically access
            // This should only be used in dev/test environments
            vec![]
        }
    }

    /// Get description of tenant context for logging
    #[must_use] 
    pub fn describe(&self) -> String {
        if self.enforce_filtering { format!("Tenant: {} (enforced)", self.tenant_id) } else { "Single-tenant mode (no enforcement)".to_string() }
    }

    /// Check if two tenant contexts can access shared resources
    ///
    /// Returns true if contexts are from the same tenant or filtering is disabled.
    #[must_use] 
    pub const fn is_compatible(&self, other: &Self) -> bool {
        if !self.enforce_filtering || !other.enforce_filtering {
            return true;
        }
        self.tenant_id == other.tenant_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tenant_context_creation() {
        let ctx = TenantContext::new(5);
        assert_eq!(ctx.tenant_id, 5);
        assert!(ctx.enforce_filtering);
    }

    #[test]
    fn test_tenant_context_single_tenant() {
        let ctx = TenantContext::single_tenant();
        assert_eq!(ctx.tenant_id, 1);
        assert!(!ctx.enforce_filtering);
    }

    #[test]
    fn test_tenant_context_with_filtering() {
        let ctx = TenantContext::with_filtering(10, false);
        assert_eq!(ctx.tenant_id, 10);
        assert!(!ctx.enforce_filtering);
    }

    #[test]
    fn test_scoped_channel_with_filtering() {
        let ctx = TenantContext::new(5);
        let scoped = ctx.scoped_channel("orders");
        assert_eq!(scoped, "orders:tenant-5");
    }

    #[test]
    fn test_scoped_channel_without_filtering() {
        let ctx = TenantContext::single_tenant();
        let scoped = ctx.scoped_channel("orders");
        assert_eq!(scoped, "orders");
    }

    #[test]
    fn test_scoped_channel_preserves_base_prefix() {
        let ctx = TenantContext::new(3);
        let scoped = ctx.scoped_channel("notifications:alerts");
        assert_eq!(scoped, "notifications:alerts:tenant-3");
    }

    #[test]
    fn test_matches_same_tenant_id() {
        let ctx = TenantContext::new(5);
        let event_data = json!({ "tenant_id": 5, "user_id": 100 });
        assert!(ctx.matches(&event_data));
    }

    #[test]
    fn test_matches_different_tenant_id() {
        let ctx = TenantContext::new(5);
        let event_data = json!({ "tenant_id": 10, "user_id": 100 });
        assert!(!ctx.matches(&event_data));
    }

    #[test]
    fn test_matches_missing_tenant_id() {
        let ctx = TenantContext::new(5);
        let event_data = json!({ "user_id": 100, "amount": 50.0 });
        assert!(!ctx.matches(&event_data));
    }

    #[test]
    fn test_matches_non_integer_tenant_id() {
        let ctx = TenantContext::new(5);
        let event_data = json!({ "tenant_id": "acme-corp" });
        assert!(!ctx.matches(&event_data));
    }

    #[test]
    fn test_matches_single_tenant_mode() {
        let ctx = TenantContext::single_tenant();
        let event_data = json!({ "tenant_id": 100 });
        assert!(ctx.matches(&event_data));
    }

    #[test]
    fn test_should_drop_matching_event() {
        let ctx = TenantContext::new(5);
        let event_data = json!({ "tenant_id": 5 });
        assert!(!ctx.should_drop(&event_data));
    }

    #[test]
    fn test_should_drop_mismatching_event() {
        let ctx = TenantContext::new(5);
        let event_data = json!({ "tenant_id": 10 });
        assert!(ctx.should_drop(&event_data));
    }

    #[test]
    fn test_validate_subscription_matching_tenant() {
        let ctx = TenantContext::new(5);
        let variables = json!({ "tenant_id": 5 });
        assert!(ctx.validate_subscription_variables(&variables));
    }

    #[test]
    fn test_validate_subscription_mismatching_tenant() {
        let ctx = TenantContext::new(5);
        let variables = json!({ "tenant_id": 10 });
        assert!(!ctx.validate_subscription_variables(&variables));
    }

    #[test]
    fn test_validate_subscription_wildcard() {
        let ctx = TenantContext::new(5);
        let variables = json!({ "user_id": 123 });
        assert!(ctx.validate_subscription_variables(&variables));
    }

    #[test]
    fn test_validate_subscription_empty_variables() {
        let ctx = TenantContext::new(5);
        let variables = json!({});
        assert!(ctx.validate_subscription_variables(&variables));
    }

    #[test]
    fn test_validate_subscription_non_integer_tenant_id() {
        let ctx = TenantContext::new(5);
        let variables = json!({ "tenant_id": "acme" });
        assert!(!ctx.validate_subscription_variables(&variables));
    }

    #[test]
    fn test_validate_subscription_single_tenant_mode() {
        let ctx = TenantContext::single_tenant();
        let variables = json!({ "tenant_id": 100 });
        assert!(ctx.validate_subscription_variables(&variables));
    }

    #[test]
    fn test_authorized_tenants_filtered() {
        let ctx = TenantContext::new(5);
        let authorized = ctx.authorized_tenants();
        assert_eq!(authorized, vec![5]);
    }

    #[test]
    fn test_authorized_tenants_single_tenant() {
        let ctx = TenantContext::single_tenant();
        let authorized = ctx.authorized_tenants();
        assert_eq!(authorized, Vec::<i64>::new());
    }

    #[test]
    fn test_describe_filtered() {
        let ctx = TenantContext::new(5);
        let desc = ctx.describe();
        assert_eq!(desc, "Tenant: 5 (enforced)");
    }

    #[test]
    fn test_describe_single_tenant() {
        let ctx = TenantContext::single_tenant();
        let desc = ctx.describe();
        assert_eq!(desc, "Single-tenant mode (no enforcement)");
    }

    #[test]
    fn test_is_compatible_same_tenant() {
        let ctx1 = TenantContext::new(5);
        let ctx2 = TenantContext::new(5);
        assert!(ctx1.is_compatible(&ctx2));
    }

    #[test]
    fn test_is_compatible_different_tenant() {
        let ctx1 = TenantContext::new(5);
        let ctx2 = TenantContext::new(10);
        assert!(!ctx1.is_compatible(&ctx2));
    }

    #[test]
    fn test_is_compatible_single_tenant_mode() {
        let ctx1 = TenantContext::new(5);
        let ctx2 = TenantContext::single_tenant();
        assert!(ctx1.is_compatible(&ctx2));
    }

    #[test]
    fn test_is_compatible_both_single_tenant() {
        let ctx1 = TenantContext::single_tenant();
        let ctx2 = TenantContext::single_tenant();
        assert!(ctx1.is_compatible(&ctx2));
    }

    #[test]
    fn test_complex_event_data_matching() {
        let ctx = TenantContext::new(2);
        let event_data = json!({
            "id": "order-456",
            "tenant_id": 2,
            "user_id": 100,
            "amount": 99.99,
            "items": [
                { "sku": "ABC", "qty": 1 },
                { "sku": "XYZ", "qty": 2 }
            ],
            "metadata": {
                "source": "web",
                "timestamp": 1_609_459_200
            }
        });
        assert!(ctx.matches(&event_data));
    }

    #[test]
    fn test_multiple_tenants_isolation() {
        let ctx1 = TenantContext::new(1);
        let ctx2 = TenantContext::new(2);
        let ctx3 = TenantContext::new(3);

        let event1 = json!({ "tenant_id": 1 });
        let event2 = json!({ "tenant_id": 2 });
        let event3 = json!({ "tenant_id": 3 });

        // Each context only accepts its own tenant events
        assert!(ctx1.matches(&event1));
        assert!(!ctx1.matches(&event2));
        assert!(!ctx1.matches(&event3));

        assert!(!ctx2.matches(&event1));
        assert!(ctx2.matches(&event2));
        assert!(!ctx2.matches(&event3));

        assert!(!ctx3.matches(&event1));
        assert!(!ctx3.matches(&event2));
        assert!(ctx3.matches(&event3));
    }

    #[test]
    fn test_scoped_channel_naming_consistency() {
        let ctx = TenantContext::new(5);
        let ch1 = ctx.scoped_channel("orders");
        let ch2 = ctx.scoped_channel("orders");
        assert_eq!(ch1, ch2);
    }

    #[test]
    fn test_tenant_context_null_tenant_id() {
        let ctx = TenantContext::new(0);
        let event = json!({ "tenant_id": 0 });
        assert!(ctx.matches(&event));
    }

    #[test]
    fn test_tenant_context_large_tenant_id() {
        let ctx = TenantContext::new(i64::MAX);
        let event = json!({ "tenant_id": i64::MAX });
        assert!(ctx.matches(&event));
    }
}
