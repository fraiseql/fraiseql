//! @require_permission directive handler for field-level RBAC
//!
//! Implements field-level authorization via GraphQL directives.
//! Users must have explicit permission to access protected fields.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::graphql::directive_evaluator::{
    DirectiveError, DirectiveHandler, DirectiveResult, EvaluationContext,
};

/// Handles @require_permission directives for field-level access control.
///
/// # Directive Syntax
///
/// ```graphql
/// type Query {
///   users: [User!]! @require_permission(permission: "query:users:read")
///   adminPanel: String! @require_permission(permission: "admin:*")
/// }
/// ```
///
/// # Permission Format
///
/// Permissions use a colon-separated format: `resource:action:scope`
///
/// - `*:*` - Full wildcard (admin access)
/// - `query:*` - All query operations
/// - `query:users:read` - Specific query permission
/// - `admin:*` - Admin operations
///
/// # Field Masking
///
/// Optional `maskValue` argument masks sensitive field values:
///
/// ```graphql
/// email: String! @require_permission(
///   permission: "read:User.email",
///   maskValue: "[REDACTED]"
/// )
/// ```
pub struct RequirePermissionDirective;

impl RequirePermissionDirective {
    /// Create a new require_permission directive handler.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Check if a user's permission matches a required permission.
    ///
    /// Supports wildcard matching:
    /// - `*:*` matches everything
    /// - `query:*` matches any query permission
    /// - `query:users:*` matches any user query permission
    /// - `query:users:read` matches exact permission
    fn permission_matches(user_permission: &str, required_permission: &str) -> bool {
        // Exact match
        if user_permission == required_permission {
            return true;
        }

        // Wildcard: user has full admin access
        if user_permission == "*:*" {
            return true;
        }

        // Wildcard matching (e.g., "query:*" matches "query:users:read")
        let user_parts: Vec<&str> = user_permission.split(':').collect();
        let required_parts: Vec<&str> = required_permission.split(':').collect();

        // If last part is a wildcard, check if prefix matches
        if let Some(&last_part) = user_parts.last() {
            if last_part == "*" {
                let user_prefix_len = user_parts.len() - 1;
                if user_prefix_len <= required_parts.len() {
                    return user_parts[..user_prefix_len] == required_parts[..user_prefix_len];
                }
            }
        }

        false
    }

    /// Extract user permissions from the evaluation context.
    ///
    /// Permissions are expected to be stored in the "permissions" key as a JSON array of strings.
    fn get_user_permissions(context: &EvaluationContext) -> Vec<String> {
        context
            .get_user_context("permissions")
            .and_then(|v| v.as_array())
            .map(|perms| perms.iter().filter_map(|p| p.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }

    /// Check if user has required permission.
    ///
    /// Returns true if any user permission matches the required permission.
    fn user_has_permission(required_permission: &str, user_permissions: &[String]) -> bool {
        user_permissions
            .iter()
            .any(|perm| Self::permission_matches(perm, required_permission))
    }
}

impl Default for RequirePermissionDirective {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectiveHandler for RequirePermissionDirective {
    fn name(&self) -> &'static str {
        "require_permission"
    }

    fn evaluate(
        &self,
        args: &HashMap<String, JsonValue>,
        context: &EvaluationContext,
    ) -> Result<DirectiveResult, DirectiveError> {
        // Get required permission from arguments
        let required_permission = args
            .get("permission")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DirectiveError::MissingDirectiveArgument("permission".to_string()))?;

        // Get user permissions from context
        let user_permissions = Self::get_user_permissions(context);

        // Check if user has required permission
        if Self::user_has_permission(required_permission, &user_permissions) {
            // User is authorized - include field
            return Ok(DirectiveResult::Include);
        }

        // User lacks permission - check if masking is requested
        if let Some(mask_value) = args.get("maskValue") {
            return Ok(DirectiveResult::Transform(mask_value.clone()));
        }

        // Deny access
        Ok(DirectiveResult::Error(format!(
            "User lacks required permission: {}",
            required_permission
        )))
    }

    fn validate_args(&self, args: &HashMap<String, JsonValue>) -> Result<(), DirectiveError> {
        // Check that permission argument is present and is a string
        if !args.contains_key("permission") {
            return Err(DirectiveError::MissingDirectiveArgument("permission".to_string()));
        }

        let permission = args
            .get("permission")
            .and_then(|v| v.as_str())
            .ok_or(DirectiveError::InvalidDirectiveArgument)?;

        // Validate permission format (basic check)
        if permission.is_empty() {
            return Err(DirectiveError::InvalidDirectiveArgument);
        }

        // Validate maskValue if present
        if let Some(mask) = args.get("maskValue") {
            if !mask.is_string() && !mask.is_number() && !mask.is_null() {
                return Err(DirectiveError::InvalidDirectiveArgument);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_matches_exact() {
        assert!(RequirePermissionDirective::permission_matches(
            "query:users:read",
            "query:users:read"
        ));
        assert!(!RequirePermissionDirective::permission_matches(
            "query:users:read",
            "query:users:write"
        ));
    }

    #[test]
    fn test_permission_matches_wildcard() {
        assert!(RequirePermissionDirective::permission_matches("*:*", "query:users:read"));
        assert!(RequirePermissionDirective::permission_matches("query:*", "query:users:read"));
        assert!(!RequirePermissionDirective::permission_matches(
            "mutation:*",
            "query:users:read"
        ));
    }
}
