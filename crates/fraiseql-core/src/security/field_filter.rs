//! Field selection filtering for GraphQL queries
//!
//! This module provides field-level access control that prevents unauthorized users
//! from querying specific fields. Unlike field masking (which redacts data in responses),
//! field filtering rejects unauthorized field access at query validation time.
//!
//! ## Scope Format
//!
//! Field access is controlled via JWT scopes using the pattern:
//! ```text
//! {action}:{Type}.{field}
//! ```
//!
//! Examples:
//! - `read:User.salary` - Can read the salary field on User type
//! - `read:User.*` - Can read all fields on User type
//! - `read:*` - Can read all fields on all types
//! - `admin` - Full access to everything (superuser scope)
//!
//! ## Usage
//!
//! ```
//! use fraiseql_core::security::{FieldFilter, FieldFilterConfig};
//!
//! // Create a filter with protected fields
//! let config = FieldFilterConfig::new()
//!     .protect_field("User", "salary")
//!     .protect_field("User", "ssn")
//!     .protect_field("Employee", "compensation");
//!
//! let filter = FieldFilter::new(config);
//!
//! // Check if user can access a field
//! let scopes = vec!["read:User.salary".to_string()];
//! assert!(filter.can_access("User", "salary", &scopes).is_ok());
//!
//! // Without scope, access is denied
//! let no_scopes: Vec<String> = vec![];
//! assert!(filter.can_access("User", "salary", &no_scopes).is_err());
//! ```
//!
//! ## Integration with AuthenticatedUser
//!
//! ```ignore
//! use fraiseql_core::security::{AuthenticatedUser, FieldFilter};
//!
//! fn check_field_access(
//!     filter: &FieldFilter,
//!     user: &AuthenticatedUser,
//!     type_name: &str,
//!     field_name: &str
//! ) -> Result<(), FieldAccessError> {
//!     filter.can_access(type_name, field_name, &user.scopes)
//! }
//! ```

use std::{
    collections::{HashMap, HashSet},
    fmt,
};

/// Error returned when field access is denied
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAccessError {
    /// The GraphQL type containing the field
    pub type_name:  String,
    /// The field that was denied
    pub field_name: String,
    /// Human-readable message
    pub message:    String,
}

impl fmt::Display for FieldAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FieldAccessError {}

impl FieldAccessError {
    /// Create a new field access error
    #[must_use]
    pub fn new(type_name: impl Into<String>, field_name: impl Into<String>) -> Self {
        let type_name = type_name.into();
        let field_name = field_name.into();
        let message = format!(
            "Access denied: you do not have permission to access field '{field_name}' on type '{type_name}'"
        );
        Self {
            type_name,
            field_name,
            message,
        }
    }

    /// Create error with custom message
    #[must_use]
    pub fn with_message(
        type_name: impl Into<String>,
        field_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            type_name:  type_name.into(),
            field_name: field_name.into(),
            message:    message.into(),
        }
    }
}

/// Configuration for field filtering
///
/// Defines which fields require authorization and optionally
/// specifies the required scopes for each protected field.
#[derive(Debug, Clone, Default)]
pub struct FieldFilterConfig {
    /// Fields that require authorization, grouped by type
    /// Key: type name, Value: set of protected field names
    protected_fields: HashMap<String, HashSet<String>>,

    /// Explicit scope requirements for specific fields
    /// Key: "Type.field", Value: required scope (if not using default pattern)
    explicit_scopes: HashMap<String, String>,

    /// Scopes that grant full access (bypass all checks)
    admin_scopes: HashSet<String>,

    /// Default action for scope pattern (default: "read")
    default_action: String,
}

impl FieldFilterConfig {
    /// Create a new empty configuration
    #[must_use]
    pub fn new() -> Self {
        Self {
            protected_fields: HashMap::new(),
            explicit_scopes:  HashMap::new(),
            admin_scopes:     HashSet::from(["admin".to_string()]),
            default_action:   "read".to_string(),
        }
    }

    /// Mark a field as protected (requires authorization)
    ///
    /// Protected fields require a scope matching `{action}:{Type}.{field}`
    /// or a wildcard scope like `{action}:{Type}.*` or `{action}:*`.
    #[must_use]
    pub fn protect_field(mut self, type_name: &str, field_name: &str) -> Self {
        self.protected_fields
            .entry(type_name.to_string())
            .or_default()
            .insert(field_name.to_string());
        self
    }

    /// Mark multiple fields on a type as protected
    #[must_use]
    pub fn protect_fields(mut self, type_name: &str, field_names: &[&str]) -> Self {
        let fields = self.protected_fields.entry(type_name.to_string()).or_default();
        for field_name in field_names {
            fields.insert((*field_name).to_string());
        }
        self
    }

    /// Protect all fields on a type (require explicit scopes for any field)
    #[must_use]
    pub fn protect_type(mut self, type_name: &str) -> Self {
        // Use special "*" marker to indicate all fields are protected
        self.protected_fields
            .entry(type_name.to_string())
            .or_default()
            .insert("*".to_string());
        self
    }

    /// Set an explicit scope requirement for a field
    ///
    /// By default, fields use the pattern `{action}:{Type}.{field}`.
    /// Use this to override with a custom scope.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::security::FieldFilterConfig;
    ///
    /// let config = FieldFilterConfig::new()
    ///     .protect_field("User", "salary")
    ///     .require_scope("User", "salary", "hr:view_compensation");
    /// ```
    #[must_use]
    pub fn require_scope(mut self, type_name: &str, field_name: &str, scope: &str) -> Self {
        let key = format!("{type_name}.{field_name}");
        self.explicit_scopes.insert(key, scope.to_string());
        // Also mark as protected
        self.protected_fields
            .entry(type_name.to_string())
            .or_default()
            .insert(field_name.to_string());
        self
    }

    /// Add an admin scope that bypasses all checks
    #[must_use]
    pub fn add_admin_scope(mut self, scope: &str) -> Self {
        self.admin_scopes.insert(scope.to_string());
        self
    }

    /// Set the default action for scope patterns (default: "read")
    #[must_use]
    pub fn with_default_action(mut self, action: &str) -> Self {
        self.default_action = action.to_string();
        self
    }

    /// Check if a field is protected
    #[must_use]
    pub fn is_protected(&self, type_name: &str, field_name: &str) -> bool {
        if let Some(fields) = self.protected_fields.get(type_name) {
            // Check if specific field or all fields (*) are protected
            fields.contains(field_name) || fields.contains("*")
        } else {
            false
        }
    }
}

/// Field filter for access control
///
/// Validates that users have the required scopes to access specific fields
/// in GraphQL queries.
#[derive(Debug, Clone)]
pub struct FieldFilter {
    config: FieldFilterConfig,
}

impl FieldFilter {
    /// Create a new field filter with the given configuration
    #[must_use]
    pub fn new(config: FieldFilterConfig) -> Self {
        Self { config }
    }

    /// Create a permissive filter that allows all access (for testing)
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            config: FieldFilterConfig::new(),
        }
    }

    /// Check if the user can access a field
    ///
    /// Returns `Ok(())` if access is allowed, or `Err(FieldAccessError)` if denied.
    ///
    /// # Arguments
    ///
    /// * `type_name` - The GraphQL type containing the field
    /// * `field_name` - The field being accessed
    /// * `scopes` - The user's scopes (from JWT token)
    pub fn can_access(
        &self,
        type_name: &str,
        field_name: &str,
        scopes: &[String],
    ) -> Result<(), FieldAccessError> {
        // If field is not protected, allow access
        if !self.config.is_protected(type_name, field_name) {
            return Ok(());
        }

        // Check for admin scopes (bypass all checks)
        for scope in scopes {
            if self.config.admin_scopes.contains(scope) {
                return Ok(());
            }
        }

        // Check for explicit scope requirement
        let key = format!("{type_name}.{field_name}");
        if let Some(required_scope) = self.config.explicit_scopes.get(&key) {
            if scopes.iter().any(|s| s == required_scope) {
                return Ok(());
            }
            // Explicit scope not found, deny access
            return Err(FieldAccessError::new(type_name, field_name));
        }

        // Check for pattern-based scopes
        let action = &self.config.default_action;

        // Check exact match: read:User.salary
        let exact_scope = format!("{action}:{type_name}.{field_name}");
        if scopes.iter().any(|s| s == &exact_scope) {
            return Ok(());
        }

        // Check type wildcard: read:User.*
        let type_wildcard = format!("{action}:{type_name}.*");
        if scopes.iter().any(|s| s == &type_wildcard) {
            return Ok(());
        }

        // Check global wildcard: read:*
        let global_wildcard = format!("{action}:*");
        if scopes.iter().any(|s| s == &global_wildcard) {
            return Ok(());
        }

        // No matching scope found
        Err(FieldAccessError::new(type_name, field_name))
    }

    /// Validate all requested fields for a type
    ///
    /// Returns a list of errors for any denied fields.
    pub fn validate_fields(
        &self,
        type_name: &str,
        field_names: &[&str],
        scopes: &[String],
    ) -> Vec<FieldAccessError> {
        field_names
            .iter()
            .filter_map(|field_name| self.can_access(type_name, field_name, scopes).err())
            .collect()
    }

    /// Get the configuration (for inspection/debugging)
    #[must_use]
    pub fn config(&self) -> &FieldFilterConfig {
        &self.config
    }
}

/// Builder for creating field filter from schema annotations
///
/// This is typically used during schema compilation to build
/// a filter based on `@requiresScope` directives.
#[derive(Debug, Default)]
pub struct FieldFilterBuilder {
    config: FieldFilterConfig,
}

impl FieldFilterBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: FieldFilterConfig::new(),
        }
    }

    /// Add a protected field from schema directive
    pub fn add_protected_field(&mut self, type_name: &str, field_name: &str) {
        self.config
            .protected_fields
            .entry(type_name.to_string())
            .or_default()
            .insert(field_name.to_string());
    }

    /// Add an explicit scope requirement
    pub fn add_scope_requirement(&mut self, type_name: &str, field_name: &str, scope: &str) {
        let key = format!("{type_name}.{field_name}");
        self.config.explicit_scopes.insert(key, scope.to_string());
        self.add_protected_field(type_name, field_name);
    }

    /// Set admin scopes
    pub fn set_admin_scopes(&mut self, scopes: Vec<String>) {
        self.config.admin_scopes = scopes.into_iter().collect();
    }

    /// Build the filter
    #[must_use]
    pub fn build(self) -> FieldFilter {
        FieldFilter::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test Suite 1: Basic Configuration
    // ========================================================================

    #[test]
    fn test_empty_config_allows_all() {
        let filter = FieldFilter::permissive();
        let scopes: Vec<String> = vec![];

        assert!(filter.can_access("User", "name", &scopes).is_ok());
        assert!(filter.can_access("User", "email", &scopes).is_ok());
        assert!(filter.can_access("User", "salary", &scopes).is_ok());
    }

    #[test]
    fn test_protect_single_field() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        // Unprotected fields are allowed
        let no_scopes: Vec<String> = vec![];
        assert!(filter.can_access("User", "name", &no_scopes).is_ok());
        assert!(filter.can_access("User", "email", &no_scopes).is_ok());

        // Protected field is denied without scope
        assert!(filter.can_access("User", "salary", &no_scopes).is_err());
    }

    #[test]
    fn test_protect_multiple_fields() {
        let config = FieldFilterConfig::new().protect_fields("User", &["salary", "ssn", "bonus"]);
        let filter = FieldFilter::new(config);

        let no_scopes: Vec<String> = vec![];
        assert!(filter.can_access("User", "name", &no_scopes).is_ok());
        assert!(filter.can_access("User", "salary", &no_scopes).is_err());
        assert!(filter.can_access("User", "ssn", &no_scopes).is_err());
        assert!(filter.can_access("User", "bonus", &no_scopes).is_err());
    }

    #[test]
    fn test_protect_entire_type() {
        let config = FieldFilterConfig::new().protect_type("Secret");
        let filter = FieldFilter::new(config);

        let no_scopes: Vec<String> = vec![];
        // All fields on Secret type require authorization
        assert!(filter.can_access("Secret", "anything", &no_scopes).is_err());
        assert!(filter.can_access("Secret", "data", &no_scopes).is_err());

        // Other types are fine
        assert!(filter.can_access("User", "name", &no_scopes).is_ok());
    }

    // ========================================================================
    // Test Suite 2: Scope Matching
    // ========================================================================

    #[test]
    fn test_exact_scope_match() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let scopes = vec!["read:User.salary".to_string()];
        assert!(filter.can_access("User", "salary", &scopes).is_ok());
    }

    #[test]
    fn test_type_wildcard_scope() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("User", "ssn");
        let filter = FieldFilter::new(config);

        let scopes = vec!["read:User.*".to_string()];
        assert!(filter.can_access("User", "salary", &scopes).is_ok());
        assert!(filter.can_access("User", "ssn", &scopes).is_ok());
    }

    #[test]
    fn test_global_wildcard_scope() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("Employee", "compensation");
        let filter = FieldFilter::new(config);

        let scopes = vec!["read:*".to_string()];
        assert!(filter.can_access("User", "salary", &scopes).is_ok());
        assert!(filter.can_access("Employee", "compensation", &scopes).is_ok());
    }

    #[test]
    fn test_admin_scope_bypasses_all() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("User", "ssn")
            .protect_type("Secret");
        let filter = FieldFilter::new(config);

        let scopes = vec!["admin".to_string()];
        assert!(filter.can_access("User", "salary", &scopes).is_ok());
        assert!(filter.can_access("User", "ssn", &scopes).is_ok());
        assert!(filter.can_access("Secret", "data", &scopes).is_ok());
    }

    #[test]
    fn test_custom_admin_scope() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .add_admin_scope("superuser");
        let filter = FieldFilter::new(config);

        let scopes = vec!["superuser".to_string()];
        assert!(filter.can_access("User", "salary", &scopes).is_ok());
    }

    #[test]
    fn test_wrong_scope_denied() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        // Wrong type
        let scopes = vec!["read:Employee.salary".to_string()];
        assert!(filter.can_access("User", "salary", &scopes).is_err());

        // Wrong field
        let scopes = vec!["read:User.name".to_string()];
        assert!(filter.can_access("User", "salary", &scopes).is_err());

        // Wrong action (write instead of read)
        let scopes = vec!["write:User.salary".to_string()];
        assert!(filter.can_access("User", "salary", &scopes).is_err());
    }

    // ========================================================================
    // Test Suite 3: Explicit Scope Requirements
    // ========================================================================

    #[test]
    fn test_explicit_scope_requirement() {
        let config = FieldFilterConfig::new().protect_field("User", "salary").require_scope(
            "User",
            "salary",
            "hr:view_compensation",
        );
        let filter = FieldFilter::new(config);

        // Default pattern doesn't work
        let wrong_scope = vec!["read:User.salary".to_string()];
        assert!(filter.can_access("User", "salary", &wrong_scope).is_err());

        // Explicit scope works
        let right_scope = vec!["hr:view_compensation".to_string()];
        assert!(filter.can_access("User", "salary", &right_scope).is_ok());
    }

    #[test]
    fn test_admin_still_bypasses_explicit() {
        let config = FieldFilterConfig::new().protect_field("User", "salary").require_scope(
            "User",
            "salary",
            "hr:view_compensation",
        );
        let filter = FieldFilter::new(config);

        let admin_scope = vec!["admin".to_string()];
        assert!(filter.can_access("User", "salary", &admin_scope).is_ok());
    }

    // ========================================================================
    // Test Suite 4: Error Messages
    // ========================================================================

    #[test]
    fn test_error_contains_field_info() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let no_scopes: Vec<String> = vec![];
        let err = filter.can_access("User", "salary", &no_scopes).unwrap_err();

        assert_eq!(err.type_name, "User");
        assert_eq!(err.field_name, "salary");
        assert!(err.message.contains("salary"));
        assert!(err.message.contains("User"));
    }

    #[test]
    fn test_error_display() {
        let err = FieldAccessError::new("User", "salary");
        let display = err.to_string();

        assert!(display.contains("Access denied"));
        assert!(display.contains("salary"));
        assert!(display.contains("User"));
    }

    // ========================================================================
    // Test Suite 5: Batch Validation
    // ========================================================================

    #[test]
    fn test_validate_multiple_fields() {
        let config = FieldFilterConfig::new()
            .protect_field("User", "salary")
            .protect_field("User", "ssn");
        let filter = FieldFilter::new(config);

        let fields = ["name", "email", "salary", "ssn"];
        let no_scopes: Vec<String> = vec![];

        let errors = filter.validate_fields("User", &fields, &no_scopes);
        assert_eq!(errors.len(), 2);

        let error_fields: Vec<&str> = errors.iter().map(|e| e.field_name.as_str()).collect();
        assert!(error_fields.contains(&"salary"));
        assert!(error_fields.contains(&"ssn"));
    }

    #[test]
    fn test_validate_all_allowed() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let fields = ["name", "email", "salary"];
        let scopes = vec!["read:User.salary".to_string()];

        let errors = filter.validate_fields("User", &fields, &scopes);
        assert!(errors.is_empty());
    }

    // ========================================================================
    // Test Suite 6: Builder Pattern
    // ========================================================================

    #[test]
    fn test_builder_basic() {
        let mut builder = FieldFilterBuilder::new();
        builder.add_protected_field("User", "salary");
        builder.add_protected_field("User", "ssn");

        let filter = builder.build();
        let no_scopes: Vec<String> = vec![];

        assert!(filter.can_access("User", "salary", &no_scopes).is_err());
        assert!(filter.can_access("User", "ssn", &no_scopes).is_err());
        assert!(filter.can_access("User", "name", &no_scopes).is_ok());
    }

    #[test]
    fn test_builder_with_explicit_scopes() {
        let mut builder = FieldFilterBuilder::new();
        builder.add_scope_requirement("User", "salary", "hr:compensation");

        let filter = builder.build();

        let wrong = vec!["read:User.salary".to_string()];
        let right = vec!["hr:compensation".to_string()];

        assert!(filter.can_access("User", "salary", &wrong).is_err());
        assert!(filter.can_access("User", "salary", &right).is_ok());
    }

    #[test]
    fn test_builder_custom_admin_scopes() {
        let mut builder = FieldFilterBuilder::new();
        builder.add_protected_field("User", "salary");
        builder.set_admin_scopes(vec!["root".to_string(), "superadmin".to_string()]);

        let filter = builder.build();

        // Default admin scope no longer works
        let admin = vec!["admin".to_string()];
        assert!(filter.can_access("User", "salary", &admin).is_err());

        // Custom admin scopes work
        let root = vec!["root".to_string()];
        assert!(filter.can_access("User", "salary", &root).is_ok());

        let superadmin = vec!["superadmin".to_string()];
        assert!(filter.can_access("User", "salary", &superadmin).is_ok());
    }

    // ========================================================================
    // Test Suite 7: Config Inspection
    // ========================================================================

    #[test]
    fn test_is_protected() {
        let config =
            FieldFilterConfig::new().protect_field("User", "salary").protect_type("Secret");

        assert!(config.is_protected("User", "salary"));
        assert!(!config.is_protected("User", "name"));
        assert!(config.is_protected("Secret", "anything"));
        assert!(!config.is_protected("Public", "data"));
    }

    #[test]
    fn test_config_default_action() {
        let config = FieldFilterConfig::new()
            .with_default_action("view")
            .protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        // "read" action doesn't work
        let read_scope = vec!["read:User.salary".to_string()];
        assert!(filter.can_access("User", "salary", &read_scope).is_err());

        // "view" action works
        let view_scope = vec!["view:User.salary".to_string()];
        assert!(filter.can_access("User", "salary", &view_scope).is_ok());
    }

    // ========================================================================
    // Test Suite 8: Edge Cases
    // ========================================================================

    #[test]
    fn test_empty_scopes() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let empty: Vec<String> = vec![];
        assert!(filter.can_access("User", "salary", &empty).is_err());
    }

    #[test]
    fn test_multiple_scopes_one_match() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        let scopes = vec![
            "read:Product.*".to_string(),
            "write:Order.status".to_string(),
            "read:User.salary".to_string(), // This one matches
            "other:scope".to_string(),
        ];
        assert!(filter.can_access("User", "salary", &scopes).is_ok());
    }

    #[test]
    fn test_case_sensitive_scopes() {
        let config = FieldFilterConfig::new().protect_field("User", "salary");
        let filter = FieldFilter::new(config);

        // Scopes are case-sensitive
        let wrong_case = vec!["READ:User.salary".to_string()];
        assert!(filter.can_access("User", "salary", &wrong_case).is_err());

        let wrong_type_case = vec!["read:user.salary".to_string()];
        assert!(filter.can_access("User", "salary", &wrong_type_case).is_err());
    }

    #[test]
    fn test_special_characters_in_names() {
        let config =
            FieldFilterConfig::new().protect_field("UserProfile", "social_security_number");
        let filter = FieldFilter::new(config);

        let scopes = vec!["read:UserProfile.social_security_number".to_string()];
        assert!(filter.can_access("UserProfile", "social_security_number", &scopes).is_ok());
    }
}
