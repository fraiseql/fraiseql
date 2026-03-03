//! Security configuration types for compiled schemas.
//!
//! Contains role definitions, scope types, and injected parameter sources
//! that are compiled from `fraiseql.toml` into `schema.compiled.json`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::domain_types::{RoleName, Scope};

/// Source from which an injected SQL parameter is resolved at runtime.
///
/// Injected parameters are not exposed in the GraphQL schema. They are
/// silently added to SQL queries and function calls, resolved from the
/// authenticated request context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "claim", rename_all = "snake_case")]
pub enum InjectedParamSource {
    /// Extract a value from the JWT claims.
    ///
    /// Special aliases resolved before attribute lookup:
    /// - `"sub"` → `SecurityContext.user_id`
    /// - `"tenant_id"` / `"org_id"` → `SecurityContext.tenant_id`
    /// - any other name → `SecurityContext.attributes.get(name)`
    Jwt(String),
}

/// Role definition for field-level RBAC.
///
/// Defines which GraphQL scopes a role grants access to.
/// Used by the runtime to determine which fields a user can access
/// based on their assigned roles.
///
/// # Example
///
/// ```json
/// {
///   "name": "admin",
///   "description": "Administrator with all scopes",
///   "scopes": ["admin:*"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleDefinition {
    /// Role name (e.g., "admin", "user", "viewer").
    pub name: RoleName,

    /// Optional role description for documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// List of scopes this role grants access to.
    /// Scopes follow the format: `action:resource` (e.g., "read:User.email", "admin:*")
    pub scopes: Vec<Scope>,
}

impl RoleDefinition {
    /// Create a new role definition.
    #[must_use]
    pub fn new(name: impl Into<String>, scopes: Vec<String>) -> Self {
        Self {
            name: RoleName::new(name),
            description: None,
            scopes: scopes.into_iter().map(Scope::new).collect(),
        }
    }

    /// Add a description to the role.
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Check if this role has a specific scope.
    ///
    /// Supports exact matching and wildcard patterns:
    /// - `read:User.email` matches exactly
    /// - `read:*` matches any scope starting with "read:"
    /// - `read:User.*` matches "read:User.email", "read:User.name", etc.
    /// - `admin:*` matches any admin scope
    #[must_use]
    pub fn has_scope(&self, required_scope: &str) -> bool {
        self.scopes.iter().any(|scope| {
            let scope = scope.as_str();
            if scope == "*" {
                return true; // Wildcard matches everything
            }

            if scope == required_scope {
                return true; // Exact match
            }

            // Handle wildcard patterns like "read:*" or "admin:*"
            if let Some(prefix) = scope.strip_suffix(":*") {
                return required_scope.starts_with(prefix) && required_scope.contains(':');
            }

            // Handle Type.* wildcard patterns like "read:User.*"
            if let Some(prefix) = scope.strip_suffix('*') {
                return required_scope.starts_with(prefix);
            }

            false
        })
    }
}

/// Security configuration from fraiseql.toml.
///
/// Contains role definitions and other security-related settings
/// that are compiled into schema.compiled.json.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Role definitions mapping role names to their granted scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_definitions: Vec<RoleDefinition>,

    /// Default role when none is specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_role: Option<String>,

    /// Additional security settings (rate limiting, audit logging, etc.)
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

impl SecurityConfig {
    /// Create a new empty security configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a role definition.
    pub fn add_role(&mut self, role: RoleDefinition) {
        self.role_definitions.push(role);
    }

    /// Find a role definition by name.
    #[must_use]
    pub fn find_role(&self, name: &str) -> Option<&RoleDefinition> {
        self.role_definitions.iter().find(|r| r.name == name)
    }

    /// Get all scopes granted to a role.
    #[must_use]
    pub fn get_role_scopes(&self, role_name: &str) -> Vec<String> {
        self.find_role(role_name)
            .map(|role| role.scopes.iter().map(|s| s.to_string()).collect::<Vec<String>>())
            .unwrap_or_default()
    }

    /// Check if a role has a specific scope.
    #[must_use]
    pub fn role_has_scope(&self, role_name: &str, scope: &str) -> bool {
        self.find_role(role_name).map(|role| role.has_scope(scope)).unwrap_or(false)
    }
}
