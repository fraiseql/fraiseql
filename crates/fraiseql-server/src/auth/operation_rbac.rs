// Operation-level Role-Based Access Control (RBAC)
// Defines permissions for mutations on observer rules, actions, and system operations

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::auth::{AuthError, error::Result, middleware::AuthenticatedUser};

/// Permission for a specific GraphQL operation/mutation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationPermission {
    // Observer rules
    CreateRule,
    UpdateRule,
    DeleteRule,
    ExecuteRule,

    // Actions
    CreateAction,
    UpdateAction,
    DeleteAction,
    ExecuteAction,

    // System operations
    ManageWebhooks,
    ManageSecrets,
    ManageUsers,
    ManageRoles,
    ManageTenants,

    // Data operations
    ExportData,
    ImportData,
    DeleteData,

    // Administrative
    ViewAuditLogs,
    ManageConfiguration,
    ManageIntegrations,
}

impl OperationPermission {
    /// Human-readable name for the permission
    pub fn name(&self) -> &'static str {
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
    pub fn as_str(&self) -> &'static str {
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
    pub name:        String,
    pub permissions: Vec<OperationPermission>,
}

impl Role {
    /// Create a new role with specified permissions
    pub fn new(name: String, permissions: Vec<OperationPermission>) -> Self {
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
    use super::*;
    use crate::auth::jwt::Claims;

    fn create_test_user(role: &str) -> AuthenticatedUser {
        let mut extra = std::collections::HashMap::new();
        extra.insert("role".to_string(), serde_json::json!(role));

        AuthenticatedUser {
            user_id: "test-user".to_string(),
            claims:  Claims {
                sub: "test-user".to_string(),
                iat: 1000000,
                exp: 2000000,
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
                iat: 1000000,
                exp: 2000000,
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

        assert!(policy.authorize(&user, OperationPermission::CreateRule).is_ok());
        assert!(policy.authorize(&user, OperationPermission::DeleteRule).is_ok());
        assert!(policy.authorize(&user, OperationPermission::ManageUsers).is_ok());
        assert!(policy.authorize(&user, OperationPermission::ManageTenants).is_ok());
    }

    #[test]
    fn test_operator_has_limited_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        assert!(policy.authorize(&user, OperationPermission::CreateRule).is_ok());
        assert!(policy.authorize(&user, OperationPermission::ManageWebhooks).is_ok());
        assert!(policy.authorize(&user, OperationPermission::ManageUsers).is_err());
        assert!(policy.authorize(&user, OperationPermission::ManageTenants).is_err());
    }

    #[test]
    fn test_viewer_has_minimal_permissions() {
        let policy = RBACPolicy::new();
        let user = create_test_user("viewer");

        assert!(policy.authorize(&user, OperationPermission::ExportData).is_ok());
        assert!(policy.authorize(&user, OperationPermission::ViewAuditLogs).is_ok());
        assert!(policy.authorize(&user, OperationPermission::CreateRule).is_err());
        assert!(policy.authorize(&user, OperationPermission::ManageWebhooks).is_err());
    }

    #[test]
    fn test_multiple_roles() {
        let policy = RBACPolicy::new();
        let user = create_test_user_with_roles(vec!["viewer", "operator"]);

        // Should have operator's permissions
        assert!(policy.authorize(&user, OperationPermission::CreateRule).is_ok());
        assert!(policy.authorize(&user, OperationPermission::ExportData).is_ok());

        // Should not have admin permissions
        assert!(policy.authorize(&user, OperationPermission::ManageTenants).is_err());
    }

    #[test]
    fn test_authorize_any() {
        let policy = RBACPolicy::new();
        let user = create_test_user("viewer");

        let permissions = vec![
            OperationPermission::ManageTenants,
            OperationPermission::ExportData,
        ];

        assert!(policy.authorize_any(&user, &permissions).is_ok());
    }

    #[test]
    fn test_authorize_all() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        let permissions = vec![
            OperationPermission::CreateRule,
            OperationPermission::UpdateRule,
        ];

        assert!(policy.authorize_all(&user, &permissions).is_ok());
    }

    #[test]
    fn test_authorize_all_fails_if_missing_one() {
        let policy = RBACPolicy::new();
        let user = create_test_user("operator");

        let permissions = vec![
            OperationPermission::CreateRule,
            OperationPermission::ManageTenants, // operator doesn't have this
        ];

        assert!(policy.authorize_all(&user, &permissions).is_err());
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

        assert!(policy.authorize(&user, OperationPermission::ViewAuditLogs).is_ok());
        assert!(policy.authorize(&user, OperationPermission::CreateRule).is_err());
    }

    #[test]
    fn test_permission_string_format() {
        assert_eq!(OperationPermission::CreateRule.as_str(), "create_rule");
        assert_eq!(OperationPermission::ManageSecrets.as_str(), "manage_secrets");
        assert_eq!(OperationPermission::ViewAuditLogs.as_str(), "view_audit_logs");
    }
}
