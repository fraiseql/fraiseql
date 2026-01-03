//! Subscription scope verification
//!
//! Validates that subscription query variables match authenticated user context.
//! Prevents users from subscribing to events outside their authorized scope.

use serde_json::Value;
use std::collections::HashMap;

/// Subscription scope context for authenticated user
///
/// Validates that subscription variables (user_id, tenant_id) match the
/// authenticated context, preventing privilege escalation or data access violations.
#[derive(Debug, Clone)]
pub struct ScopeValidator {
    /// Authenticated user ID
    pub user_id: i64,
    /// Authenticated tenant ID
    pub tenant_id: i64,
    /// Whether to enforce scope validation
    pub enforce_validation: bool,
}

impl ScopeValidator {
    /// Create new scope validator with enforcement enabled
    pub fn new(user_id: i64, tenant_id: i64) -> Self {
        Self {
            user_id,
            tenant_id,
            enforce_validation: true,
        }
    }

    /// Create scope validator with explicit enforcement control
    pub fn with_enforcement(user_id: i64, tenant_id: i64, enforce_validation: bool) -> Self {
        Self {
            user_id,
            tenant_id,
            enforce_validation,
        }
    }

    /// Create scope validator for testing (no enforcement)
    pub fn test_mode(user_id: i64, tenant_id: i64) -> Self {
        Self {
            user_id,
            tenant_id,
            enforce_validation: false,
        }
    }

    /// Validate subscription variables match authenticated context
    ///
    /// Returns Ok(()) if subscription is allowed, Err with reason if denied.
    ///
    /// Validation rules:
    /// - If enforcement disabled: always allow (testing mode)
    /// - If user_id in variables: must match context user_id
    /// - If tenant_id in variables: must match context tenant_id
    /// - If no restricting variables: allow (wildcard subscription)
    /// - Both checks must pass if both variables present
    ///
    /// # Examples
    /// ```text
    /// context.user_id = 123, context.tenant_id = 5
    ///
    /// Valid subscriptions:
    /// - {} (wildcard, no scope variables)
    /// - { user_id: 123 } (matches user)
    /// - { tenant_id: 5 } (matches tenant)
    /// - { user_id: 123, tenant_id: 5 } (both match)
    ///
    /// Rejected subscriptions:
    /// - { user_id: 456 } (user mismatch)
    /// - { tenant_id: 10 } (tenant mismatch)
    /// - { user_id: 123, tenant_id: 10 } (tenant mismatch)
    /// ```
    pub fn validate(&self, variables: &HashMap<String, Value>) -> Result<(), String> {
        // If enforcement disabled, allow all subscriptions
        if !self.enforce_validation {
            return Ok(());
        }

        // Check user_id variable if present
        if let Some(user_id_value) = variables.get("user_id") {
            match user_id_value.as_i64() {
                Some(requested_user_id) => {
                    if requested_user_id != self.user_id {
                        return Err(format!(
                            "User scope mismatch: requested user_id {} does not match authenticated user_id {}",
                            requested_user_id, self.user_id
                        ));
                    }
                }
                None => {
                    return Err("user_id variable must be an integer".to_string());
                }
            }
        }

        // Check tenant_id variable if present
        if let Some(tenant_id_value) = variables.get("tenant_id") {
            match tenant_id_value.as_i64() {
                Some(requested_tenant_id) => {
                    if requested_tenant_id != self.tenant_id {
                        return Err(format!(
                            "Tenant scope mismatch: requested tenant_id {} does not match authenticated tenant_id {}",
                            requested_tenant_id, self.tenant_id
                        ));
                    }
                }
                None => {
                    return Err("tenant_id variable must be an integer".to_string());
                }
            }
        }

        // All validation passed
        Ok(())
    }

    /// Check if subscription is allowed (convenience method)
    pub fn is_allowed(&self, variables: &HashMap<String, Value>) -> bool {
        self.validate(variables).is_ok()
    }

    /// Get scope restriction level from variables
    ///
    /// Returns ScopeLevel indicating how restricted this subscription is:
    /// - None: No scope restrictions (wildcard subscription)
    /// - User: Restricted to authenticated user
    /// - Tenant: Restricted to authenticated tenant
    /// - Both: Restricted to specific user AND tenant
    pub fn scope_level(&self, variables: &HashMap<String, Value>) -> ScopeLevel {
        let has_user = variables.contains_key("user_id");
        let has_tenant = variables.contains_key("tenant_id");

        match (has_user, has_tenant) {
            (false, false) => ScopeLevel::None,
            (true, false) => ScopeLevel::User,
            (false, true) => ScopeLevel::Tenant,
            (true, true) => ScopeLevel::Both,
        }
    }

    /// Extract scope context from variables
    ///
    /// Returns a description of what scope restrictions are applied.
    pub fn describe(&self) -> String {
        if self.enforce_validation {
            format!(
                "Scope validation enforced (user_id: {}, tenant_id: {})",
                self.user_id, self.tenant_id
            )
        } else {
            "Scope validation disabled (test mode)".to_string()
        }
    }

    /// Describe the scope of a specific subscription
    pub fn describe_subscription(&self, variables: &HashMap<String, Value>) -> String {
        match self.scope_level(variables) {
            ScopeLevel::None => "Wildcard subscription (no scope restriction)".to_string(),
            ScopeLevel::User => format!(
                "User-scoped subscription (user_id: {})",
                variables
                    .get("user_id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
            ),
            ScopeLevel::Tenant => format!(
                "Tenant-scoped subscription (tenant_id: {})",
                variables
                    .get("tenant_id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
            ),
            ScopeLevel::Both => format!(
                "Dual-scoped subscription (user_id: {}, tenant_id: {})",
                variables
                    .get("user_id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                variables
                    .get("tenant_id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
            ),
        }
    }
}

/// Subscription scope restriction level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeLevel {
    /// No scope restrictions (wildcard subscription)
    None,

    /// Restricted to authenticated user
    User,

    /// Restricted to authenticated tenant
    Tenant,

    /// Restricted to specific user AND tenant
    Both,
}

impl ScopeLevel {
    /// Check if this scope level is more restrictive than another
    pub fn is_more_restrictive(&self, other: &ScopeLevel) -> bool {
        match (self, other) {
            (ScopeLevel::Both, _) => true,
            (ScopeLevel::User, ScopeLevel::None) => true,
            (ScopeLevel::User, ScopeLevel::Tenant) => false,
            (ScopeLevel::Tenant, ScopeLevel::None) => true,
            (ScopeLevel::Tenant, ScopeLevel::User) => false,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_scope_validator_creation() {
        let validator = ScopeValidator::new(123, 5);
        assert_eq!(validator.user_id, 123);
        assert_eq!(validator.tenant_id, 5);
        assert!(validator.enforce_validation);
    }

    #[test]
    fn test_scope_validator_test_mode() {
        let validator = ScopeValidator::test_mode(123, 5);
        assert!(!validator.enforce_validation);
    }

    #[test]
    fn test_validate_empty_variables() {
        let validator = ScopeValidator::new(123, 5);
        let variables = HashMap::new();
        assert!(validator.validate(&variables).is_ok());
    }

    #[test]
    fn test_validate_matching_user_id() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        assert!(validator.validate(&variables).is_ok());
    }

    #[test]
    fn test_validate_mismatching_user_id() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(456));
        assert!(validator.validate(&variables).is_err());
    }

    #[test]
    fn test_validate_matching_tenant_id() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("tenant_id".to_string(), json!(5));
        assert!(validator.validate(&variables).is_ok());
    }

    #[test]
    fn test_validate_mismatching_tenant_id() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("tenant_id".to_string(), json!(10));
        assert!(validator.validate(&variables).is_err());
    }

    #[test]
    fn test_validate_both_ids_matching() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        variables.insert("tenant_id".to_string(), json!(5));
        assert!(validator.validate(&variables).is_ok());
    }

    #[test]
    fn test_validate_both_ids_user_mismatch() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(456));
        variables.insert("tenant_id".to_string(), json!(5));
        assert!(validator.validate(&variables).is_err());
    }

    #[test]
    fn test_validate_both_ids_tenant_mismatch() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        variables.insert("tenant_id".to_string(), json!(10));
        assert!(validator.validate(&variables).is_err());
    }

    #[test]
    fn test_validate_non_integer_user_id() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!("user-123"));
        assert!(validator.validate(&variables).is_err());
    }

    #[test]
    fn test_validate_non_integer_tenant_id() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("tenant_id".to_string(), json!("acme-corp"));
        assert!(validator.validate(&variables).is_err());
    }

    #[test]
    fn test_validate_disabled_enforcement() {
        let validator = ScopeValidator::test_mode(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(999));
        variables.insert("tenant_id".to_string(), json!(999));
        assert!(validator.validate(&variables).is_ok());
    }

    #[test]
    fn test_is_allowed() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        assert!(validator.is_allowed(&variables));
    }

    #[test]
    fn test_is_not_allowed() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(999));
        assert!(!validator.is_allowed(&variables));
    }

    #[test]
    fn test_scope_level_none() {
        let validator = ScopeValidator::new(123, 5);
        let variables = HashMap::new();
        assert_eq!(validator.scope_level(&variables), ScopeLevel::None);
    }

    #[test]
    fn test_scope_level_user() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        assert_eq!(validator.scope_level(&variables), ScopeLevel::User);
    }

    #[test]
    fn test_scope_level_tenant() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("tenant_id".to_string(), json!(5));
        assert_eq!(validator.scope_level(&variables), ScopeLevel::Tenant);
    }

    #[test]
    fn test_scope_level_both() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        variables.insert("tenant_id".to_string(), json!(5));
        assert_eq!(validator.scope_level(&variables), ScopeLevel::Both);
    }

    #[test]
    fn test_describe() {
        let validator = ScopeValidator::new(123, 5);
        let desc = validator.describe();
        assert!(desc.contains("123"));
        assert!(desc.contains("5"));
    }

    #[test]
    fn test_describe_test_mode() {
        let validator = ScopeValidator::test_mode(123, 5);
        let desc = validator.describe();
        assert!(desc.contains("disabled"));
    }

    #[test]
    fn test_describe_subscription_wildcard() {
        let validator = ScopeValidator::new(123, 5);
        let variables = HashMap::new();
        let desc = validator.describe_subscription(&variables);
        assert!(desc.contains("Wildcard"));
    }

    #[test]
    fn test_describe_subscription_user_scoped() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        let desc = validator.describe_subscription(&variables);
        assert!(desc.contains("User-scoped"));
        assert!(desc.contains("123"));
    }

    #[test]
    fn test_describe_subscription_tenant_scoped() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("tenant_id".to_string(), json!(5));
        let desc = validator.describe_subscription(&variables);
        assert!(desc.contains("Tenant-scoped"));
        assert!(desc.contains("5"));
    }

    #[test]
    fn test_describe_subscription_dual_scoped() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        variables.insert("tenant_id".to_string(), json!(5));
        let desc = validator.describe_subscription(&variables);
        assert!(desc.contains("Dual-scoped"));
        assert!(desc.contains("123"));
        assert!(desc.contains("5"));
    }

    #[test]
    fn test_scope_level_is_more_restrictive() {
        assert!(ScopeLevel::Both.is_more_restrictive(&ScopeLevel::None));
        assert!(ScopeLevel::Both.is_more_restrictive(&ScopeLevel::User));
        assert!(ScopeLevel::Both.is_more_restrictive(&ScopeLevel::Tenant));
        assert!(ScopeLevel::User.is_more_restrictive(&ScopeLevel::None));
        assert!(ScopeLevel::Tenant.is_more_restrictive(&ScopeLevel::None));
        assert!(!ScopeLevel::User.is_more_restrictive(&ScopeLevel::Tenant));
        assert!(!ScopeLevel::Tenant.is_more_restrictive(&ScopeLevel::User));
        assert!(!ScopeLevel::None.is_more_restrictive(&ScopeLevel::User));
    }

    #[test]
    fn test_validate_extra_variables() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123));
        variables.insert("tenant_id".to_string(), json!(5));
        variables.insert("filter".to_string(), json!("active"));
        variables.insert("limit".to_string(), json!(10));
        assert!(validator.validate(&variables).is_ok());
    }

    #[test]
    fn test_scope_validator_with_enforcement_true() {
        let validator = ScopeValidator::with_enforcement(123, 5, true);
        assert!(validator.enforce_validation);
    }

    #[test]
    fn test_scope_validator_with_enforcement_false() {
        let validator = ScopeValidator::with_enforcement(123, 5, false);
        assert!(!validator.enforce_validation);
    }

    #[test]
    fn test_validate_with_null_values() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(null));
        assert!(validator.validate(&variables).is_err());
    }

    #[test]
    fn test_validate_with_float_ids() {
        let validator = ScopeValidator::new(123, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(123.0));
        // JSON doesn't distinguish between int and float for whole numbers
        assert!(validator.validate(&variables).is_ok());
    }

    #[test]
    fn test_validate_boundary_user_id_zero() {
        let validator = ScopeValidator::new(0, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(0));
        assert!(validator.validate(&variables).is_ok());
    }

    #[test]
    fn test_validate_boundary_large_user_id() {
        let validator = ScopeValidator::new(i64::MAX, 5);
        let mut variables = HashMap::new();
        variables.insert("user_id".to_string(), json!(i64::MAX));
        assert!(validator.validate(&variables).is_ok());
    }
}
