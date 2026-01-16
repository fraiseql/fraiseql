//! Introspection Enforcer (Phase 6.4)
//!
//! This module provides control over GraphQL introspection queries.
//! It enforces policies that control whether clients can query the schema.
//!
//! # Architecture
//!
//! The Introspection Enforcer acts as the fourth layer in the security middleware:
//! ```text
//! GraphQL Query
//!     ↓
//! IntrospectionEnforcer::validate_query()
//!     ├─ Check 1: Detect introspection patterns (__schema, __type)
//!     ├─ Check 2: Check user authentication (if required)
//!     └─ Check 3: Apply policy enforcement
//!     ↓
//! Result<()> (query allowed or blocked)
//! ```
//!
//! # Examples
//!
//! ```no_run
//! use fraiseql_core::security::{IntrospectionEnforcer, IntrospectionPolicy};
//!
//! // Create enforcer that disables introspection
//! let enforcer = IntrospectionEnforcer::new(IntrospectionPolicy::Disabled);
//!
//! // Check if a query is introspection
//! let introspection_query = "{ __schema { types { name } } }";
//! match enforcer.validate_query(introspection_query, None) {
//!     Err(e) => println!("Introspection not allowed: {}", e),
//!     Ok(_) => println!("Query allowed"),
//! }
//! ```

use crate::security::errors::{Result, SecurityError};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Introspection policy for controlling schema access
///
/// Defines what level of introspection is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntrospectionPolicy {
    /// Introspection queries are allowed for all clients
    Allowed,

    /// Introspection queries are completely disabled
    Disabled,

    /// Introspection queries are allowed only for authenticated users
    InternalOnly,
}

impl fmt::Display for IntrospectionPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allowed => write!(f, "Allowed"),
            Self::Disabled => write!(f, "Disabled"),
            Self::InternalOnly => write!(f, "InternalOnly"),
        }
    }
}

/// Introspection detection configuration
///
/// Contains patterns and configuration for detecting introspection queries.
#[derive(Debug, Clone)]
pub struct IntrospectionConfig {
    /// Whether to detect __schema queries
    pub detect_schema: bool,

    /// Whether to detect __type queries
    pub detect_type: bool,

    /// Whether to detect __typename queries
    pub detect_typename: bool,

    /// Whether to detect __directive queries
    pub detect_directive: bool,
}

impl IntrospectionConfig {
    /// Create a configuration that detects all introspection patterns
    #[must_use]
    pub fn all() -> Self {
        Self {
            detect_schema: true,
            detect_type: true,
            detect_typename: true,
            detect_directive: true,
        }
    }

    /// Create a configuration that detects only the main introspection patterns
    #[must_use]
    pub fn strict() -> Self {
        Self {
            detect_schema: true,
            detect_type: true,
            detect_typename: false,
            detect_directive: true,
        }
    }
}

/// Introspection Enforcer
///
/// Controls and enforces policies around GraphQL introspection queries.
/// Acts as the fourth layer in the security middleware pipeline.
#[derive(Debug, Clone)]
pub struct IntrospectionEnforcer {
    policy: IntrospectionPolicy,
    config: IntrospectionConfig,
}

impl IntrospectionEnforcer {
    /// Create a new introspection enforcer with a specific policy
    #[must_use]
    pub fn new(policy: IntrospectionPolicy) -> Self {
        Self {
            policy,
            config: IntrospectionConfig::all(),
        }
    }

    /// Create enforcer with custom configuration
    #[must_use]
    pub fn with_config(policy: IntrospectionPolicy, config: IntrospectionConfig) -> Self {
        Self { policy, config }
    }

    /// Create enforcer with Allowed policy (standard)
    #[must_use]
    pub fn allowed() -> Self {
        Self::new(IntrospectionPolicy::Allowed)
    }

    /// Create enforcer with Disabled policy (strict)
    #[must_use]
    pub fn disabled() -> Self {
        Self::new(IntrospectionPolicy::Disabled)
    }

    /// Create enforcer with InternalOnly policy (regulated)
    #[must_use]
    pub fn internal_only() -> Self {
        Self::new(IntrospectionPolicy::InternalOnly)
    }

    /// Validate whether an introspection query is allowed
    ///
    /// Performs 3 validation checks:
    /// 1. Detect if query is an introspection query
    /// 2. Check user authentication (if required by policy)
    /// 3. Apply policy enforcement
    ///
    /// Returns Ok(()) if query is allowed, Err if blocked.
    ///
    /// # Arguments
    /// * `query` - The GraphQL query string to validate
    /// * `authenticated_user_id` - Optional user ID (None = anonymous, Some(id) = authenticated)
    pub fn validate_query(&self, query: &str, authenticated_user_id: Option<&str>) -> Result<()> {
        // Check 1: Detect introspection patterns
        let is_introspection = self.is_introspection_query(query);

        // If not introspection, allow regardless of policy
        if !is_introspection {
            return Ok(());
        }

        // Check 2 & 3: Apply policy
        match self.policy {
            IntrospectionPolicy::Allowed => {
                // Introspection allowed for all
                Ok(())
            }
            IntrospectionPolicy::Disabled => {
                // Introspection disabled for everyone
                Err(SecurityError::IntrospectionDisabled {
                    detail: "Introspection queries are disabled in this environment".to_string(),
                })
            }
            IntrospectionPolicy::InternalOnly => {
                // Introspection allowed only for authenticated users
                if authenticated_user_id.is_some() {
                    Ok(())
                } else {
                    Err(SecurityError::IntrospectionDisabled {
                        detail: "Introspection queries require authentication".to_string(),
                    })
                }
            }
        }
    }

    /// Check if a query is an introspection query
    ///
    /// Detects GraphQL introspection patterns:
    /// - `__schema`
    /// - `__type`
    /// - `__typename` (if enabled)
    /// - `__directive` (if enabled)
    fn is_introspection_query(&self, query: &str) -> bool {
        let query_lower = query.to_lowercase();

        // Check for __schema
        if self.config.detect_schema && query_lower.contains("__schema") {
            return true;
        }

        // Check for __type
        if self.config.detect_type && query_lower.contains("__type") {
            return true;
        }

        // Check for __typename
        if self.config.detect_typename && query_lower.contains("__typename") {
            return true;
        }

        // Check for __directive
        if self.config.detect_directive && query_lower.contains("__directive") {
            return true;
        }

        false
    }

    /// Get the current policy
    #[must_use]
    pub const fn policy(&self) -> IntrospectionPolicy {
        self.policy
    }

    /// Get the detection configuration
    #[must_use]
    pub const fn config(&self) -> &IntrospectionConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Helper Functions
    // ============================================================================

    fn introspection_schema_query() -> &'static str {
        "{ __schema { types { name } } }"
    }

    fn introspection_type_query() -> &'static str {
        "{ __type(name: \"User\") { name fields { name } } }"
    }

    fn introspection_typename_query() -> &'static str {
        "{ user { __typename } }"
    }

    fn normal_query() -> &'static str {
        "{ user { id name email } }"
    }

    // ============================================================================
    // Check 1: Introspection Detection Tests
    // ============================================================================

    #[test]
    fn test_detect_schema_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let is_introspection = enforcer.is_introspection_query(introspection_schema_query());
        assert!(is_introspection);
    }

    #[test]
    fn test_detect_type_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let is_introspection = enforcer.is_introspection_query(introspection_type_query());
        assert!(is_introspection);
    }

    #[test]
    fn test_detect_typename_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let is_introspection = enforcer.is_introspection_query(introspection_typename_query());
        assert!(is_introspection);
    }

    #[test]
    fn test_normal_query_not_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let is_introspection = enforcer.is_introspection_query(normal_query());
        assert!(!is_introspection);
    }

    #[test]
    fn test_detect_introspection_case_insensitive() {
        let enforcer = IntrospectionEnforcer::allowed();
        let uppercase_query = "{ __SCHEMA { types { name } } }";
        let is_introspection = enforcer.is_introspection_query(uppercase_query);
        assert!(is_introspection);
    }

    // ============================================================================
    // Check 2: Authentication Check Tests
    // ============================================================================

    #[test]
    fn test_internal_only_allows_authenticated_user() {
        let enforcer = IntrospectionEnforcer::internal_only();
        let result = enforcer.validate_query(introspection_schema_query(), Some("user123"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_internal_only_rejects_anonymous_user() {
        let enforcer = IntrospectionEnforcer::internal_only();
        let result = enforcer.validate_query(introspection_schema_query(), None);
        assert!(matches!(
            result,
            Err(SecurityError::IntrospectionDisabled { .. })
        ));
    }

    // ============================================================================
    // Check 3: Policy Enforcement Tests
    // ============================================================================

    #[test]
    fn test_allowed_policy_permits_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let result = enforcer.validate_query(introspection_schema_query(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_allowed_policy_permits_anonymous_introspection() {
        let enforcer = IntrospectionEnforcer::allowed();
        let result = enforcer.validate_query(introspection_schema_query(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_disabled_policy_blocks_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        let result = enforcer.validate_query(introspection_schema_query(), None);
        assert!(matches!(
            result,
            Err(SecurityError::IntrospectionDisabled { .. })
        ));
    }

    #[test]
    fn test_disabled_policy_blocks_authenticated_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        let result = enforcer.validate_query(introspection_schema_query(), Some("user123"));
        assert!(matches!(
            result,
            Err(SecurityError::IntrospectionDisabled { .. })
        ));
    }

    #[test]
    fn test_policy_allows_normal_queries_always() {
        let disabled_enforcer = IntrospectionEnforcer::disabled();
        let result = disabled_enforcer.validate_query(normal_query(), None);
        assert!(result.is_ok());
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_introspection_config_all() {
        let config = IntrospectionConfig::all();
        assert!(config.detect_schema);
        assert!(config.detect_type);
        assert!(config.detect_typename);
        assert!(config.detect_directive);
    }

    #[test]
    fn test_introspection_config_strict() {
        let config = IntrospectionConfig::strict();
        assert!(config.detect_schema);
        assert!(config.detect_type);
        assert!(!config.detect_typename);
        assert!(config.detect_directive);
    }

    #[test]
    fn test_policy_display() {
        assert_eq!(IntrospectionPolicy::Allowed.to_string(), "Allowed");
        assert_eq!(IntrospectionPolicy::Disabled.to_string(), "Disabled");
        assert_eq!(
            IntrospectionPolicy::InternalOnly.to_string(),
            "InternalOnly"
        );
    }

    #[test]
    fn test_enforcer_helpers() {
        let allowed = IntrospectionEnforcer::allowed();
        assert_eq!(allowed.policy(), IntrospectionPolicy::Allowed);

        let disabled = IntrospectionEnforcer::disabled();
        assert_eq!(disabled.policy(), IntrospectionPolicy::Disabled);

        let internal = IntrospectionEnforcer::internal_only();
        assert_eq!(internal.policy(), IntrospectionPolicy::InternalOnly);
    }

    // ============================================================================
    // Custom Configuration Tests
    // ============================================================================

    #[test]
    fn test_custom_config_with_selective_detection() {
        let config = IntrospectionConfig {
            detect_schema: true,
            detect_type: false,
            detect_typename: false,
            detect_directive: false,
        };

        let enforcer = IntrospectionEnforcer::with_config(IntrospectionPolicy::Disabled, config);

        // __schema should be detected and blocked
        let schema_query = "{ __schema { types { name } } }";
        assert!(matches!(
            enforcer.validate_query(schema_query, None),
            Err(SecurityError::IntrospectionDisabled { .. })
        ));

        // __type should NOT be detected (allowed through)
        let type_query = "{ __type(name: \"User\") { name } }";
        assert!(enforcer.validate_query(type_query, None).is_ok());
    }

    // ============================================================================
    // Edge Cases
    // ============================================================================

    #[test]
    fn test_introspection_in_string_literal_not_detected() {
        let enforcer = IntrospectionEnforcer::disabled();
        // This query has __schema as a string, not as a field
        // Note: Real implementation would need proper parsing to avoid this false positive
        // For now, we document this limitation
        let query = r#"{ user(filter: "__schema") { name } }"#;
        // This will be detected as introspection due to simple string matching
        let is_introspection = enforcer.is_introspection_query(query);
        // Our simplified detection will match this - in production, use proper parsing
        assert!(is_introspection);
    }

    #[test]
    fn test_empty_query_not_introspection() {
        let enforcer = IntrospectionEnforcer::disabled();
        let result = enforcer.validate_query("", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_whitespace_handled_correctly() {
        let enforcer = IntrospectionEnforcer::allowed();
        let query = "{\n  __schema {\n    types { name }\n  }\n}";
        let is_introspection = enforcer.is_introspection_query(query);
        assert!(is_introspection);
    }

    #[test]
    fn test_multiple_introspection_patterns() {
        let enforcer = IntrospectionEnforcer::allowed();
        let query = "{ __schema { types { name } } __type(name: \"Query\") { name } }";
        let is_introspection = enforcer.is_introspection_query(query);
        assert!(is_introspection);
    }
}
