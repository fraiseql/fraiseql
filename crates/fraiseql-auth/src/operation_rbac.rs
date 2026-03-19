//! Operation-level Role-Based Access Control (RBAC).
//!
//! Defines the [`OperationPermission`] enum, the [`Role`] type that bundles a set
//! of permissions, and the [`RBACPolicy`] engine that evaluates authorization
//! decisions for authenticated users.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{AuthError, error::Result, middleware::AuthenticatedUser};

/// A discrete permission that can be granted to a [`Role`].
///
/// Each variant maps to one or more GraphQL mutations or system operations.
/// The string representation returned by [`OperationPermission::as_str`] is used
/// when storing role-permission mappings in configuration or databases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OperationPermission {
    /// Create a new observer rule.
    CreateRule,
    /// Modify an existing observer rule.
    UpdateRule,
    /// Remove an observer rule.
    DeleteRule,
    /// Trigger immediate execution of an observer rule.
    ExecuteRule,

    /// Create a new action definition.
    CreateAction,
    /// Modify an existing action definition.
    UpdateAction,
    /// Remove an action definition.
    DeleteAction,
    /// Trigger execution of an action.
    ExecuteAction,

    /// Create, update, or remove webhook subscriptions.
    ManageWebhooks,
    /// Read or rotate application secrets.
    ManageSecrets,
    /// Create, modify, or disable user accounts.
    ManageUsers,
    /// Define and assign roles within the system.
    ManageRoles,
    /// Create or modify multi-tenant isolation boundaries.
    ManageTenants,

    /// Export data records from the system.
    ExportData,
    /// Import data records into the system.
    ImportData,
    /// Permanently delete data records.
    DeleteData,

    /// Read the security audit trail.
    ViewAuditLogs,
    /// Modify system-wide configuration settings.
    ManageConfiguration,
    /// Configure third-party integrations and connectors.
    ManageIntegrations,
}

impl OperationPermission {
    /// Human-readable name for the permission
    pub const fn name(&self) -> &'static str {
        match self {
            Self::CreateRule => "Create Observer Rule",
            Self::UpdateRule => "Update Observer Rule",
            Self::DeleteRule => "Delete Observer Rule",
            Self::ExecuteRule => "Execute Observer Rule",
            Self::CreateAction => "Create Action",
            Self::UpdateAction => "Update Action",
            Self::DeleteAction => "Delete Action",
            Self::ExecuteAction => "Execute Action",
            Self::ManageWebhooks => "Manage Webhooks",
            Self::ManageSecrets => "Manage Secrets",
            Self::ManageUsers => "Manage Users",
            Self::ManageRoles => "Manage Roles",
            Self::ManageTenants => "Manage Tenants",
            Self::ExportData => "Export Data",
            Self::ImportData => "Import Data",
            Self::DeleteData => "Delete Data",
            Self::ViewAuditLogs => "View Audit Logs",
            Self::ManageConfiguration => "Manage Configuration",
            Self::ManageIntegrations => "Manage Integrations",
        }
    }

    /// Convert to string for policy storage
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::CreateRule => "create_rule",
            Self::UpdateRule => "update_rule",
            Self::DeleteRule => "delete_rule",
            Self::ExecuteRule => "execute_rule",
            Self::CreateAction => "create_action",
            Self::UpdateAction => "update_action",
            Self::DeleteAction => "delete_action",
            Self::ExecuteAction => "execute_action",
            Self::ManageWebhooks => "manage_webhooks",
            Self::ManageSecrets => "manage_secrets",
            Self::ManageUsers => "manage_users",
            Self::ManageRoles => "manage_roles",
            Self::ManageTenants => "manage_tenants",
            Self::ExportData => "export_data",
            Self::ImportData => "import_data",
            Self::DeleteData => "delete_data",
            Self::ViewAuditLogs => "view_audit_logs",
            Self::ManageConfiguration => "manage_configuration",
            Self::ManageIntegrations => "manage_integrations",
        }
    }
}

/// Predefined roles with their associated permissions
#[derive(Debug, Clone)]
pub struct Role {
    /// Role name (e.g., `"admin"`, `"viewer"`)
    pub name:        String,
    /// Set of operations this role is allowed to perform
    pub permissions: Vec<OperationPermission>,
}

impl Role {
    /// Create a new role with specified permissions
    pub const fn new(name: String, permissions: Vec<OperationPermission>) -> Self {
        Self { name, permissions }
    }

    /// Check if role has a specific permission
    pub fn has_permission(&self, permission: OperationPermission) -> bool {
        self.permissions.contains(&permission)
    }

    /// Get all permissions for this role
    pub fn get_permissions(&self) -> &[OperationPermission] {
        &self.permissions
    }
}

/// RBAC policy engine
#[derive(Debug, Clone)]
pub struct RBACPolicy {
    roles: HashMap<String, Role>,
}

impl Default for RBACPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl RBACPolicy {
    /// Create a new RBAC policy with default roles
    pub fn new() -> Self {
        let mut roles = HashMap::new();

        // Admin role - full permissions
        roles.insert(
            "admin".to_string(),
            Role::new(
                "admin".to_string(),
                vec![
                    OperationPermission::CreateRule,
                    OperationPermission::UpdateRule,
                    OperationPermission::DeleteRule,
                    OperationPermission::ExecuteRule,
                    OperationPermission::CreateAction,
                    OperationPermission::UpdateAction,
                    OperationPermission::DeleteAction,
                    OperationPermission::ExecuteAction,
                    OperationPermission::ManageWebhooks,
                    OperationPermission::ManageSecrets,
                    OperationPermission::ManageUsers,
                    OperationPermission::ManageRoles,
                    OperationPermission::ManageTenants,
                    OperationPermission::ExportData,
                    OperationPermission::ImportData,
                    OperationPermission::DeleteData,
                    OperationPermission::ViewAuditLogs,
                    OperationPermission::ManageConfiguration,
                    OperationPermission::ManageIntegrations,
                ],
            ),
        );

        // Operator role - can modify rules and actions, view logs
        roles.insert(
            "operator".to_string(),
            Role::new(
                "operator".to_string(),
                vec![
                    OperationPermission::CreateRule,
                    OperationPermission::UpdateRule,
                    OperationPermission::DeleteRule,
                    OperationPermission::ExecuteRule,
                    OperationPermission::CreateAction,
                    OperationPermission::UpdateAction,
                    OperationPermission::DeleteAction,
                    OperationPermission::ExecuteAction,
                    OperationPermission::ManageWebhooks,
                    OperationPermission::ExportData,
                    OperationPermission::ViewAuditLogs,
                ],
            ),
        );

        // Viewer role - read-only access
        roles.insert(
            "viewer".to_string(),
            Role::new(
                "viewer".to_string(),
                vec![
                    OperationPermission::ExportData,
                    OperationPermission::ViewAuditLogs,
                ],
            ),
        );

        Self { roles }
    }

    /// Register a custom role
    pub fn register_role(&mut self, role: Role) {
        self.roles.insert(role.name.clone(), role);
    }

    /// Check if a user has permission to perform an operation
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Forbidden` if the user lacks the required permission.
    pub fn authorize(
        &self,
        user: &AuthenticatedUser,
        permission: OperationPermission,
    ) -> Result<()> {
        // Get user's roles (can be single role or array of roles)
        let user_roles = self.extract_user_roles(user);

        // Check if any of user's roles has the permission
        for role_name in user_roles {
            if let Some(role) = self.roles.get(&role_name) {
                if role.has_permission(permission) {
                    return Ok(());
                }
            }
        }

        Err(AuthError::Forbidden {
            message: format!(
                "User {} does not have permission to: {}",
                user.user_id,
                permission.name()
            ),
        })
    }

    /// Check multiple permissions at once
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Forbidden` if the user lacks all of the given permissions.
    pub fn authorize_any(
        &self,
        user: &AuthenticatedUser,
        permissions: &[OperationPermission],
    ) -> Result<()> {
        for permission in permissions {
            if self.authorize(user, *permission).is_ok() {
                return Ok(());
            }
        }

        Err(AuthError::Forbidden {
            message: format!("User {} does not have any of the required permissions", user.user_id),
        })
    }

    /// Check that user has all permissions
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Forbidden` if the user lacks any of the required permissions.
    pub fn authorize_all(
        &self,
        user: &AuthenticatedUser,
        permissions: &[OperationPermission],
    ) -> Result<()> {
        for permission in permissions {
            self.authorize(user, *permission)?;
        }
        Ok(())
    }

    /// Get all permissions for a user
    pub fn get_user_permissions(&self, user: &AuthenticatedUser) -> Vec<OperationPermission> {
        let user_roles = self.extract_user_roles(user);
        let mut permissions = Vec::new();

        for role_name in user_roles {
            if let Some(role) = self.roles.get(&role_name) {
                permissions.extend(role.get_permissions());
            }
        }

        // Remove duplicates
        permissions.sort_by_key(|p| *p as u32);
        permissions.dedup();

        permissions
    }

    /// Extract user's roles from their claims
    fn extract_user_roles(&self, user: &AuthenticatedUser) -> Vec<String> {
        let mut roles = Vec::new();

        // Check for single role claim
        if let Some(serde_json::Value::String(role)) = user.get_custom_claim("role") {
            roles.push(role.clone());
        }

        // Check for roles array
        if let Some(serde_json::Value::Array(role_array)) = user.get_custom_claim("roles") {
            for role_val in role_array {
                if let serde_json::Value::String(role_name) = role_val {
                    roles.push(role_name.clone());
                }
            }
        }

        // Check for standard claim name variations
        if let Some(serde_json::Value::Array(role_array)) = user.get_custom_claim("fraiseql_roles")
        {
            for role_val in role_array {
                if let serde_json::Value::String(role_name) = role_val {
                    roles.push(role_name.clone());
                }
            }
        }

        // Remove duplicates
        roles.sort();
        roles.dedup();

        roles
    }
}

#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]  // Reason: test module wildcard import; brings all items into test scope
    // Reason: test modules use wildcard imports for conciseness
    use super::*;
    use crate::jwt::Claims;

    fn create_test_user(role: &str) -> AuthenticatedUser {
        let mut extra = std::collections::HashMap::new();
        extra.insert("role".to_string(), serde_json::json!(role));

        AuthenticatedUser {
            user_id: "test-user".to_string(),
            claims:  Claims {
                sub: "test-user".to_string(),
                iat: 1_000_000,
                exp: 2_000_000,
                iss: "test-issuer".to_string(),
                aud: vec!["fraiseql".to_string()],
                extra,
            },
        }
    }

    fn create_test_user_with_roles(roles: Vec<&str>) -> AuthenticatedUser {
        let mut extra = std::collections::HashMap::new();
        extra.insert("roles".to_string(), serde_json::json!(roles));

        AuthenticatedUser {
            user_id: "test-user".to_string(),
            claims:  Claims {
                sub: "test-user".to_string(),
                iat: 1_000_000,
                exp: 2_000_000,
                iss: "test-issuer".to_string(),
                aud: vec!["fraiseql".to_string()],
                extra,
            },
        }
    }

    #[test]
    fn test_admin_has_all_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("admin");

        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(r.is_ok(), "admin should have CreateRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::DeleteRule);
        assert!(r.is_ok(), "admin should have DeleteRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageUsers);
        assert!(r.is_ok(), "admin should have ManageUsers: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageTenants);
        assert!(r.is_ok(), "admin should have ManageTenants: {r:?}");
    }

    #[test]
    fn test_operator_has_limited_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(r.is_ok(), "operator should have CreateRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageWebhooks);
        assert!(r.is_ok(), "operator should have ManageWebhooks: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageUsers);
        assert!(matches!(r, Err(AuthError::Forbidden { .. })), "operator should not have ManageUsers: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageTenants);
        assert!(matches!(r, Err(AuthError::Forbidden { .. })), "operator should not have ManageTenants: {r:?}");
    }

    #[test]
    fn test_viewer_has_minimal_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("viewer");

        let r = policy.authorize(&user, OperationPermission::ExportData);
        assert!(r.is_ok(), "viewer should have ExportData: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ViewAuditLogs);
        assert!(r.is_ok(), "viewer should have ViewAuditLogs: {r:?}");
        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(matches!(r, Err(AuthError::Forbidden { .. })), "viewer should not have CreateRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ManageWebhooks);
        assert!(matches!(r, Err(AuthError::Forbidden { .. })), "viewer should not have ManageWebhooks: {r:?}");
    }

    #[test]
    fn test_multiple_roles() {
        let policy = RBACPolicy::new();
        let user = create_test_user_with_roles(vec!["viewer", "operator"]);

        // Should have operator's permissions
        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(r.is_ok(), "viewer+operator should have CreateRule: {r:?}");
        let r = policy.authorize(&user, OperationPermission::ExportData);
        assert!(r.is_ok(), "viewer+operator should have ExportData: {r:?}");

        // Should not have admin permissions
        let r = policy.authorize(&user, OperationPermission::ManageTenants);
        assert!(matches!(r, Err(AuthError::Forbidden { .. })), "viewer+operator should not have ManageTenants: {r:?}");
    }

    #[test]
    fn test_authorize_any() {
        let policy = RBACPolicy::new();
        let user = create_test_user("viewer");

        let permissions = vec![
            OperationPermission::ManageTenants,
            OperationPermission::ExportData,
        ];

        let r = policy.authorize_any(&user, &permissions);
        assert!(r.is_ok(), "viewer should have at least one of the permissions: {r:?}");
    }

    #[test]
    fn test_authorize_all() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        let permissions = vec![
            OperationPermission::CreateRule,
            OperationPermission::UpdateRule,
        ];

        let r = policy.authorize_all(&user, &permissions);
        assert!(r.is_ok(), "operator should have all rule permissions: {r:?}");
    }

    #[test]
    fn test_authorize_all_fails_if_missing_one() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        let permissions = vec![
            OperationPermission::CreateRule,
            OperationPermission::ManageTenants, // operator doesn't have this
        ];

        let r = policy.authorize_all(&user, &permissions);
        assert!(matches!(r, Err(AuthError::Forbidden { .. })), "operator missing ManageTenants should fail authorize_all: {r:?}");
    }

    #[test]
    fn test_get_user_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("viewer");

        let permissions = policy.get_user_permissions(&user);
        assert_eq!(permissions.len(), 2);
        assert!(permissions.contains(&OperationPermission::ExportData));
        assert!(permissions.contains(&OperationPermission::ViewAuditLogs));
    }

    #[test]
    fn test_custom_role() {
        let mut policy = RBACPolicy::new();

        let custom_role = Role::new(
            "auditor".to_string(),
            vec![
                OperationPermission::ViewAuditLogs,
                OperationPermission::ExportData,
            ],
        );

        policy.register_role(custom_role);
        let user = create_test_user("auditor");

        let r = policy.authorize(&user, OperationPermission::ViewAuditLogs);
        assert!(r.is_ok(), "auditor should have ViewAuditLogs: {r:?}");
        let r = policy.authorize(&user, OperationPermission::CreateRule);
        assert!(matches!(r, Err(AuthError::Forbidden { .. })), "auditor should not have CreateRule: {r:?}");
    }

    #[test]
    fn test_permission_string_format() {
        assert_eq!(OperationPermission::CreateRule.as_str(), "create_rule");
        assert_eq!(OperationPermission::ManageSecrets.as_str(), "manage_secrets");
        assert_eq!(OperationPermission::ViewAuditLogs.as_str(), "view_audit_logs");
    }
}
