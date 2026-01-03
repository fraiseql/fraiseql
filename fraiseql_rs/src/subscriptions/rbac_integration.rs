//! RBAC (Role-Based Access Control) integration for subscriptions
//!
//! Integrates FraiseQL's RBAC system with subscription field access control.
//! Validates that users have permission to access all requested subscription fields.

use std::collections::HashMap;

/// RBAC context for subscription field access control
///
/// Validates that subscription requests only access fields the authenticated user
/// has permission to access. This prevents privilege escalation where a user
/// subscribes to fields they don't have permission to view.
#[derive(Debug, Clone)]
pub struct RBACContext {
    /// Authenticated user ID (UUID as i64 or string)
    pub user_id: String,
    /// Authenticated tenant ID (UUID as i64 or string)
    pub tenant_id: Option<String>,
    /// Requested subscription fields to validate
    pub requested_fields: Vec<String>,
    /// Whether to enforce RBAC checks
    pub enforce_rbac: bool,
}

impl RBACContext {
    /// Create new RBAC context with enforcement enabled
    pub fn new(user_id: String, tenant_id: Option<String>, requested_fields: Vec<String>) -> Self {
        Self {
            user_id,
            tenant_id,
            requested_fields,
            enforce_rbac: true,
        }
    }

    /// Create RBAC context with explicit enforcement control
    pub fn with_enforcement(
        user_id: String,
        tenant_id: Option<String>,
        requested_fields: Vec<String>,
        enforce_rbac: bool,
    ) -> Self {
        Self {
            user_id,
            tenant_id,
            requested_fields,
            enforce_rbac,
        }
    }

    /// Create RBAC context for testing (no enforcement)
    pub fn test_mode(
        user_id: String,
        tenant_id: Option<String>,
        requested_fields: Vec<String>,
    ) -> Self {
        Self {
            user_id,
            tenant_id,
            requested_fields,
            enforce_rbac: false,
        }
    }

    /// Get permission resource name for a field
    ///
    /// Converts GraphQL field names to RBAC permission resources:
    /// - `orders` → `order:read`
    /// - `users` → `user:read`
    /// - `payments` → `payment:read`
    ///
    /// This is a default naming convention. Applications can override
    /// via a custom mapping if needed.
    pub fn field_to_resource(field: &str) -> String {
        // Convert plural to singular (basic heuristic)
        let singular = if field.ends_with('s') && !field.ends_with("ss") {
            &field[..field.len() - 1]
        } else {
            field
        };

        // Format as resource:action
        format!("{}:read", singular)
    }

    /// Validate field access permissions
    ///
    /// Returns Ok(()) if all requested fields are allowed, Err with details if denied.
    ///
    /// # Field Permission Mapping
    /// Each GraphQL subscription field maps to an RBAC permission:
    /// - Field: `messages` → Permission: `message:read`
    /// - Field: `orders` → Permission: `order:read`
    /// - Field: `users` → Permission: `user:read`
    ///
    /// The validator checks that all fields can be accessed by the user.
    /// In a real implementation, this would query the PermissionResolver.
    pub fn validate_fields(&self, allowed_fields: &HashMap<String, bool>) -> Result<(), String> {
        // If enforcement disabled, allow all fields
        if !self.enforce_rbac {
            return Ok(());
        }

        // Check each requested field
        for field in &self.requested_fields {
            // Get resource name for this field
            let resource = Self::field_to_resource(field);

            // Check if this field is allowed
            if !allowed_fields.get(field).copied().unwrap_or(false) {
                return Err(format!(
                    "User {} is not authorized to access field '{}' (requires permission: {})",
                    self.user_id, field, resource
                ));
            }
        }

        Ok(())
    }

    /// Check if user has access to a single field
    pub fn can_access_field(&self, field: &str, allowed_fields: &HashMap<String, bool>) -> bool {
        if !self.enforce_rbac {
            return true;
        }
        allowed_fields.get(field).copied().unwrap_or(false)
    }

    /// Get list of accessible fields from requested set
    pub fn filter_accessible_fields(&self, allowed_fields: &HashMap<String, bool>) -> Vec<String> {
        self.requested_fields
            .iter()
            .filter(|field| self.can_access_field(field, allowed_fields))
            .cloned()
            .collect()
    }

    /// Get list of denied fields from requested set
    pub fn filter_denied_fields(&self, allowed_fields: &HashMap<String, bool>) -> Vec<String> {
        self.requested_fields
            .iter()
            .filter(|field| !self.can_access_field(field, allowed_fields))
            .cloned()
            .collect()
    }

    /// Get description of RBAC context for logging
    pub fn describe(&self) -> String {
        if self.enforce_rbac {
            format!(
                "RBAC enforced: user={}, tenant={:?}, fields=[{}]",
                self.user_id,
                self.tenant_id,
                self.requested_fields.join(", ")
            )
        } else {
            format!(
                "RBAC disabled (test mode): user={}, fields=[{}]",
                self.user_id,
                self.requested_fields.join(", ")
            )
        }
    }

    /// Get description of field access decision
    pub fn describe_field_access(
        &self,
        field: &str,
        allowed_fields: &HashMap<String, bool>,
    ) -> String {
        let resource = Self::field_to_resource(field);
        if self.can_access_field(field, allowed_fields) {
            format!(
                "User {} can access '{}' (permission: {})",
                self.user_id, field, resource
            )
        } else {
            format!(
                "User {} cannot access '{}' (requires: {})",
                self.user_id, field, resource
            )
        }
    }
}

/// RBAC check result with audit information
#[derive(Debug, Clone)]
pub struct RBACCheckResult {
    /// Whether the check passed
    pub allowed: bool,

    /// Fields that were denied access
    pub denied_fields: Vec<String>,

    /// Fields that were allowed access
    pub allowed_fields: Vec<String>,

    /// Audit reason (for logging/compliance)
    pub reason: String,
}

impl RBACCheckResult {
    /// Create successful check result
    pub fn allowed(allowed_fields: Vec<String>) -> Self {
        Self {
            allowed: true,
            denied_fields: Vec::new(),
            allowed_fields,
            reason: "All requested fields are accessible".to_string(),
        }
    }

    /// Create failed check result
    pub fn denied(denied_fields: Vec<String>, reason: String) -> Self {
        Self {
            allowed: false,
            denied_fields,
            allowed_fields: Vec::new(),
            reason,
        }
    }

    /// Create partial result (some fields allowed, some denied)
    pub fn partial(allowed_fields: Vec<String>, denied_fields: Vec<String>) -> Self {
        let allowed_count = allowed_fields.len();
        let denied_count = denied_fields.len();
        Self {
            allowed: false,
            denied_fields,
            allowed_fields,
            reason: format!(
                "Partial access: {} allowed, {} denied",
                allowed_count, denied_count
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbac_context_creation() {
        let ctx = RBACContext::new(
            "user-123".to_string(),
            Some("tenant-5".to_string()),
            vec!["orders".to_string(), "users".to_string()],
        );
        assert_eq!(ctx.user_id, "user-123");
        assert_eq!(ctx.tenant_id, Some("tenant-5".to_string()));
        assert_eq!(ctx.requested_fields.len(), 2);
        assert!(ctx.enforce_rbac);
    }

    #[test]
    fn test_rbac_context_test_mode() {
        let ctx = RBACContext::test_mode(
            "user-123".to_string(),
            Some("tenant-5".to_string()),
            vec!["orders".to_string()],
        );
        assert!(!ctx.enforce_rbac);
    }

    #[test]
    fn test_field_to_resource_singular() {
        assert_eq!(RBACContext::field_to_resource("order"), "order:read");
        assert_eq!(RBACContext::field_to_resource("user"), "user:read");
        assert_eq!(RBACContext::field_to_resource("message"), "message:read");
    }

    #[test]
    fn test_field_to_resource_plural() {
        assert_eq!(RBACContext::field_to_resource("orders"), "order:read");
        assert_eq!(RBACContext::field_to_resource("users"), "user:read");
        assert_eq!(RBACContext::field_to_resource("messages"), "message:read");
    }

    #[test]
    fn test_field_to_resource_double_s() {
        assert_eq!(RBACContext::field_to_resource("class"), "class:read");
        assert_eq!(RBACContext::field_to_resource("grass"), "grass:read");
    }

    #[test]
    fn test_validate_fields_all_allowed() {
        let ctx = RBACContext::new(
            "user-123".to_string(),
            None,
            vec!["orders".to_string(), "users".to_string()],
        );

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), true);
        allowed.insert("users".to_string(), true);

        assert!(ctx.validate_fields(&allowed).is_ok());
    }

    #[test]
    fn test_validate_fields_denied() {
        let ctx = RBACContext::new(
            "user-123".to_string(),
            None,
            vec!["orders".to_string(), "users".to_string()],
        );

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), true);
        allowed.insert("users".to_string(), false);

        assert!(ctx.validate_fields(&allowed).is_err());
    }

    #[test]
    fn test_validate_fields_disabled() {
        let ctx = RBACContext::test_mode("user-123".to_string(), None, vec!["orders".to_string()]);

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), false);

        assert!(ctx.validate_fields(&allowed).is_ok());
    }

    #[test]
    fn test_can_access_field() {
        let ctx = RBACContext::new("user-123".to_string(), None, vec!["orders".to_string()]);

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), true);
        allowed.insert("users".to_string(), false);

        assert!(ctx.can_access_field("orders", &allowed));
        assert!(!ctx.can_access_field("users", &allowed));
    }

    #[test]
    fn test_filter_accessible_fields() {
        let ctx = RBACContext::new(
            "user-123".to_string(),
            None,
            vec![
                "orders".to_string(),
                "users".to_string(),
                "products".to_string(),
            ],
        );

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), true);
        allowed.insert("users".to_string(), false);
        allowed.insert("products".to_string(), true);

        let accessible = ctx.filter_accessible_fields(&allowed);
        assert_eq!(accessible.len(), 2);
        assert!(accessible.contains(&"orders".to_string()));
        assert!(accessible.contains(&"products".to_string()));
    }

    #[test]
    fn test_filter_denied_fields() {
        let ctx = RBACContext::new(
            "user-123".to_string(),
            None,
            vec![
                "orders".to_string(),
                "users".to_string(),
                "products".to_string(),
            ],
        );

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), true);
        allowed.insert("users".to_string(), false);
        allowed.insert("products".to_string(), false);

        let denied = ctx.filter_denied_fields(&allowed);
        assert_eq!(denied.len(), 2);
        assert!(denied.contains(&"users".to_string()));
        assert!(denied.contains(&"products".to_string()));
    }

    #[test]
    fn test_describe() {
        let ctx = RBACContext::new(
            "user-123".to_string(),
            Some("tenant-5".to_string()),
            vec!["orders".to_string()],
        );
        let desc = ctx.describe();
        assert!(desc.contains("user-123"));
        assert!(desc.contains("tenant-5"));
        assert!(desc.contains("orders"));
    }

    #[test]
    fn test_describe_test_mode() {
        let ctx = RBACContext::test_mode("user-123".to_string(), None, vec!["orders".to_string()]);
        let desc = ctx.describe();
        assert!(desc.contains("test mode"));
    }

    #[test]
    fn test_describe_field_access_allowed() {
        let ctx = RBACContext::new("user-123".to_string(), None, vec!["orders".to_string()]);

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), true);

        let desc = ctx.describe_field_access("orders", &allowed);
        assert!(desc.contains("can access"));
        assert!(desc.contains("user-123"));
    }

    #[test]
    fn test_describe_field_access_denied() {
        let ctx = RBACContext::new("user-123".to_string(), None, vec!["orders".to_string()]);

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), false);

        let desc = ctx.describe_field_access("orders", &allowed);
        assert!(desc.contains("cannot access"));
    }

    #[test]
    fn test_rbac_check_result_allowed() {
        let result = RBACCheckResult::allowed(vec!["orders".to_string(), "users".to_string()]);
        assert!(result.allowed);
        assert_eq!(result.allowed_fields.len(), 2);
        assert_eq!(result.denied_fields.len(), 0);
    }

    #[test]
    fn test_rbac_check_result_denied() {
        let result =
            RBACCheckResult::denied(vec!["orders".to_string()], "Access denied".to_string());
        assert!(!result.allowed);
        assert_eq!(result.denied_fields.len(), 1);
        assert_eq!(result.allowed_fields.len(), 0);
    }

    #[test]
    fn test_rbac_check_result_partial() {
        let result =
            RBACCheckResult::partial(vec!["orders".to_string()], vec!["users".to_string()]);
        assert!(!result.allowed);
        assert_eq!(result.allowed_fields.len(), 1);
        assert_eq!(result.denied_fields.len(), 1);
    }

    #[test]
    fn test_rbac_context_with_enforcement() {
        let ctx = RBACContext::with_enforcement(
            "user-123".to_string(),
            None,
            vec!["orders".to_string()],
            false,
        );
        assert!(!ctx.enforce_rbac);
    }

    #[test]
    fn test_validate_fields_missing_field_in_map() {
        let ctx = RBACContext::new("user-123".to_string(), None, vec!["orders".to_string()]);

        let allowed = HashMap::new();

        assert!(ctx.validate_fields(&allowed).is_err());
    }

    #[test]
    fn test_can_access_field_missing_field() {
        let ctx = RBACContext::new("user-123".to_string(), None, vec!["orders".to_string()]);

        let allowed = HashMap::new();

        assert!(!ctx.can_access_field("orders", &allowed));
    }

    #[test]
    fn test_can_access_field_test_mode() {
        let ctx = RBACContext::test_mode("user-123".to_string(), None, vec!["orders".to_string()]);

        let allowed = HashMap::new();

        assert!(ctx.can_access_field("orders", &allowed));
    }

    #[test]
    fn test_rbac_context_multiple_fields() {
        let fields = vec![
            "orders".to_string(),
            "users".to_string(),
            "products".to_string(),
            "reviews".to_string(),
        ];

        let ctx = RBACContext::new(
            "user-123".to_string(),
            Some("tenant-5".to_string()),
            fields.clone(),
        );

        assert_eq!(ctx.requested_fields.len(), 4);

        let mut allowed = HashMap::new();
        allowed.insert("orders".to_string(), true);
        allowed.insert("users".to_string(), true);
        allowed.insert("products".to_string(), false);
        allowed.insert("reviews".to_string(), true);

        let accessible = ctx.filter_accessible_fields(&allowed);
        assert_eq!(accessible.len(), 3);

        let denied = ctx.filter_denied_fields(&allowed);
        assert_eq!(denied.len(), 1);
    }
}
