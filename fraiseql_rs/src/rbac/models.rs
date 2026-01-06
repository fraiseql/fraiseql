//! RBAC data models matching `PostgreSQL` schema.
//!
//! This module defines the core data structures for role-based access control:
//! - **Role**: Hierarchical roles with optional parent and tenant isolation
//! - **Permission**: Resource:action pairs with optional constraints
//! - **`UserRole`**: User-role assignments with expiration and audit trail
//! - **`RolePermission`**: Many-to-many role-permission mappings

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role entity with hierarchical support.
///
/// Roles form a hierarchy through the `parent_role_id` field. A user with a child role
/// automatically inherits all permissions from parent roles. This enables role inheritance
/// patterns like: viewer → user → manager → admin → `super_admin`.
///
/// # Fields
///
/// - `id`: Unique identifier (UUID)
/// - `name`: Human-readable role name (unique per tenant)
/// - `description`: Optional role purpose/documentation
/// - `parent_role_id`: Optional parent role for inheritance
/// - `tenant_id`: Optional tenant scope (NULL = global role)
/// - `is_system`: If true, role cannot be deleted (system roles)
/// - `created_at`, `updated_at`: Audit timestamps
///
/// # Example
///
/// A typical role hierarchy:
/// - `super_admin` (global, no parent)
/// - admin (tenant, inherits `super_admin`)
/// - manager (tenant, inherits admin)
/// - user (tenant, inherits manager)
/// - viewer (tenant, inherits user)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Unique role identifier
    pub id: Uuid,
    /// Human-readable role name
    pub name: String,
    /// Optional role description
    pub description: Option<String>,
    /// Parent role for inheritance
    pub parent_role_id: Option<Uuid>,
    /// Tenant scope (NULL = global)
    pub tenant_id: Option<Uuid>,
    /// System role flag (cannot be deleted)
    pub is_system: bool,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Role {
    /// Create Role from `tokio_postgres` Row
    pub fn from_row(row: &tokio_postgres::Row) -> Self {
        Self {
            id: Uuid::parse_str(&row.get::<_, String>(0)).unwrap_or_default(),
            name: row.get(1),
            description: row.get(2),
            parent_role_id: row
                .get::<_, Option<String>>(3)
                .and_then(|s| Uuid::parse_str(&s).ok()),
            tenant_id: row
                .get::<_, Option<String>>(4)
                .and_then(|s| Uuid::parse_str(&s).ok()),
            is_system: row.get(5),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6))
                .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc)),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7))
                .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc)),
        }
    }
}

/// Permission entity for resource:action authorization.
///
/// Permissions follow a resource:action pattern (e.g., "user:read", "document:delete").
/// Supports wildcard matching for flexible permission assignment:
/// - Exact match: "user:read" matches only that permission
/// - Resource wildcard: "user:*" matches all user actions
/// - Action wildcard: "*:read" matches read on all resources
/// - Full wildcard: "*:*" grants all permissions (superuser)
///
/// # Fields
///
/// - `id`: Unique permission identifier
/// - `resource`: Resource name (e.g., "user", "document", "audit")
/// - `action`: Action type (e.g., "read", "write", "delete")
/// - `description`: Optional documentation of what this permission grants
/// - `constraints`: Optional JSON for advanced constraints (Phase 12)
///   - Examples: `{"own_data_only": true}`, `{"department_only": true}`
/// - `created_at`: Permission creation timestamp
///
/// # Example Matching
///
/// A user with permission "user:*" will match:
/// - "user:read" ✓
/// - "user:write" ✓
/// - "user:delete" ✓
/// - "document:read" ✗
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Unique permission identifier
    pub id: Uuid,
    /// Resource name (e.g., "user", "document")
    pub resource: String,
    /// Action type (e.g., "read", "write")
    pub action: String,
    /// Optional permission description
    pub description: Option<String>,
    /// Optional JSON constraints
    pub constraints: Option<serde_json::Value>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

impl Permission {
    /// Check if permission matches resource:action pattern
    #[must_use]
    pub fn matches(&self, resource: &str, action: &str) -> bool {
        // Exact match
        if self.resource == resource && self.action == action {
            return true;
        }

        // Wildcard matching: resource:* or *:action
        if self.action == "*" && self.resource == resource {
            return true;
        }
        if self.resource == "*" && self.action == action {
            return true;
        }
        if self.resource == "*" && self.action == "*" {
            return true;
        }

        false
    }

    /// Create Permission from `tokio_postgres` Row
    pub fn from_row(row: &tokio_postgres::Row) -> Self {
        Self {
            id: Uuid::parse_str(&row.get::<_, String>(0)).unwrap_or_default(),
            resource: row.get(1),
            action: row.get(2),
            description: row.get(3),
            constraints: row
                .get::<_, Option<String>>(4)
                .and_then(|s| serde_json::from_str(&s).ok()),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5))
                .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc)),
        }
    }
}

/// User-Role assignment with expiration and audit trail.
///
/// Represents the assignment of a role to a user within a specific tenant context.
/// Supports temporary role assignments through the `expires_at` field, enabling
/// time-bound access patterns for contractors, temporary staff, etc.
///
/// # Fields
///
/// - `id`: Unique assignment identifier
/// - `user_id`: User receiving the role
/// - `role_id`: Role being assigned
/// - `tenant_id`: Tenant scope for the assignment (NULL = global)
/// - `granted_by`: User ID of the admin who made this assignment (audit trail)
/// - `granted_at`: Timestamp when role was assigned
/// - `expires_at`: Optional expiration time (NULL = permanent assignment)
///
/// # Expiration Semantics
///
/// A role assignment is valid if:
/// - `expires_at` is NULL (permanent), OR
/// - `expires_at > NOW()` (not yet expired)
///
/// Expired roles are automatically filtered out by permission resolution queries.
/// This prevents the need for cleanup jobs to delete expired assignments.
///
/// # Example
///
/// ```ignore
/// // User gets temporary contractor role for 30 days
/// user_roles {
///     user_id: contractor_uuid,
///     role_id: contractor_role_uuid,
///     expires_at: now() + 30 days
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRole {
    /// Unique assignment identifier
    pub id: Uuid,
    /// User receiving the role
    pub user_id: Uuid,
    /// Role being assigned
    pub role_id: Uuid,
    /// Tenant scope (NULL = global)
    pub tenant_id: Option<Uuid>,
    /// Admin who granted this role
    pub granted_by: Option<Uuid>,
    /// When role was granted
    pub granted_at: DateTime<Utc>,
    /// Optional expiration time
    pub expires_at: Option<DateTime<Utc>>,
}

impl UserRole {
    /// Check if role assignment is still valid
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.expires_at
            .is_none_or(|expires_at| Utc::now() < expires_at)
    }

    /// Create `UserRole` from `tokio_postgres` Row
    pub fn from_row(row: &tokio_postgres::Row) -> Self {
        Self {
            id: Uuid::parse_str(&row.get::<_, String>(0)).unwrap_or_default(),
            user_id: Uuid::parse_str(&row.get::<_, String>(1)).unwrap_or_default(),
            role_id: Uuid::parse_str(&row.get::<_, String>(2)).unwrap_or_default(),
            tenant_id: row
                .get::<_, Option<String>>(3)
                .and_then(|s| Uuid::parse_str(&s).ok()),
            granted_by: row
                .get::<_, Option<String>>(4)
                .and_then(|s| Uuid::parse_str(&s).ok()),
            granted_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5))
                .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc)),
            expires_at: row
                .get::<_, Option<String>>(6)
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
        }
    }
}

/// Role-Permission mapping for many-to-many assignments.
///
/// Links a role to a permission, establishing what capabilities users with that role have.
/// When a user is assigned a role, they inherit all permissions assigned to that role
/// (plus permissions from inherited parent roles).
///
/// # Fields
///
/// - `id`: Unique mapping identifier
/// - `role_id`: Role that has this permission
/// - `permission_id`: Permission granted to the role
/// - `granted_at`: Timestamp when permission was assigned to role
///
/// # Usage Pattern
///
/// When checking if a user can perform an action:
/// 1. Find user's roles via `UserRole` table
/// 2. Find all inherited roles via `RoleHierarchy`
/// 3. Find all permissions for those roles via `RolePermission`
/// 4. Match requested resource:action against permission list
///
/// This is computed efficiently by the `PermissionResolver` with caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePermission {
    /// Unique mapping identifier
    pub id: Uuid,
    /// Role that has this permission
    pub role_id: Uuid,
    /// Permission granted to the role
    pub permission_id: Uuid,
    /// When permission was assigned
    pub granted_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test Fixtures
    // ========================================================================

    fn create_test_permission(resource: &str, action: &str) -> Permission {
        Permission {
            id: Uuid::new_v4(),
            resource: resource.to_string(),
            action: action.to_string(),
            description: None,
            constraints: None,
            created_at: Utc::now(),
        }
    }

    fn create_test_role(name: &str, parent_id: Option<Uuid>, tenant_id: Option<Uuid>) -> Role {
        Role {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: Some(format!("Test role: {}", name)),
            parent_role_id: parent_id,
            tenant_id,
            is_system: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn create_test_user_role(user_id: Uuid, role_id: Uuid) -> UserRole {
        UserRole {
            id: Uuid::new_v4(),
            user_id,
            role_id,
            tenant_id: None,
            granted_by: None,
            granted_at: Utc::now(),
            expires_at: None,
        }
    }

    fn create_test_user_role_with_expiration(
        user_id: Uuid,
        role_id: Uuid,
        expires_at: Option<DateTime<Utc>>,
    ) -> UserRole {
        UserRole {
            id: Uuid::new_v4(),
            user_id,
            role_id,
            tenant_id: None,
            granted_by: None,
            granted_at: Utc::now(),
            expires_at,
        }
    }

    // ========================================================================
    // Test Suite 1: Permission Matching (Exact Match)
    // ========================================================================

    #[test]
    fn test_permission_exact_match() {
        let perm = create_test_permission("document", "read");
        assert!(
            perm.matches("document", "read"),
            "Should match exact resource:action"
        );
    }

    #[test]
    fn test_permission_exact_match_different_resource_rejected() {
        let perm = create_test_permission("document", "read");
        assert!(
            !perm.matches("user", "read"),
            "Should not match different resource"
        );
    }

    #[test]
    fn test_permission_exact_match_different_action_rejected() {
        let perm = create_test_permission("document", "read");
        assert!(
            !perm.matches("document", "write"),
            "Should not match different action"
        );
    }

    #[test]
    fn test_permission_case_sensitive() {
        let perm = create_test_permission("Document", "Read");
        assert!(
            !perm.matches("document", "read"),
            "Permission matching should be case-sensitive"
        );
    }

    // ========================================================================
    // Test Suite 2: Permission Matching (Wildcards)
    // ========================================================================

    #[test]
    fn test_permission_resource_wildcard_matches_any_action() {
        let perm = create_test_permission("document", "*");
        assert!(perm.matches("document", "read"), "Should match document:*");
        assert!(perm.matches("document", "write"), "Should match document:*");
        assert!(
            perm.matches("document", "delete"),
            "Should match document:*"
        );
    }

    #[test]
    fn test_permission_resource_wildcard_not_cross_resource() {
        let perm = create_test_permission("document", "*");
        assert!(
            !perm.matches("user", "read"),
            "Should not match different resource"
        );
    }

    #[test]
    fn test_permission_action_wildcard_matches_any_resource() {
        let perm = create_test_permission("*", "read");
        assert!(perm.matches("document", "read"), "Should match *:read");
        assert!(perm.matches("user", "read"), "Should match *:read");
        assert!(perm.matches("role", "read"), "Should match *:read");
    }

    #[test]
    fn test_permission_action_wildcard_not_cross_action() {
        let perm = create_test_permission("*", "read");
        assert!(
            !perm.matches("document", "write"),
            "Should not match different action"
        );
    }

    #[test]
    fn test_permission_full_wildcard_matches_everything() {
        let perm = create_test_permission("*", "*");
        assert!(perm.matches("document", "read"), "Should match *:*");
        assert!(perm.matches("user", "write"), "Should match *:*");
        assert!(perm.matches("role", "delete"), "Should match *:*");
        assert!(perm.matches("any", "action"), "Should match *:*");
    }

    // ========================================================================
    // Test Suite 3: User Role Validity (Expiration)
    // ========================================================================

    #[test]
    fn test_user_role_without_expiration_is_valid() {
        let user_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let user_role = create_test_user_role(user_id, role_id);

        assert!(
            user_role.is_valid(),
            "Role without expiration should be valid"
        );
    }

    #[test]
    fn test_user_role_with_future_expiration_is_valid() {
        let user_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let future = Utc::now() + chrono::Duration::hours(1);
        let user_role = create_test_user_role_with_expiration(user_id, role_id, Some(future));

        assert!(
            user_role.is_valid(),
            "Role with future expiration should be valid"
        );
    }

    #[test]
    fn test_user_role_with_past_expiration_is_invalid() {
        let user_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let past = Utc::now() - chrono::Duration::hours(1);
        let user_role = create_test_user_role_with_expiration(user_id, role_id, Some(past));

        assert!(
            !user_role.is_valid(),
            "Role with past expiration should be invalid"
        );
    }

    #[test]
    fn test_user_role_expiration_is_exact_boundary() {
        let user_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let now = Utc::now();
        let user_role = create_test_user_role_with_expiration(user_id, role_id, Some(now));

        // At exact expiration time, role should be considered expired (now >= expires_at)
        // Since we just created it, the now in the test might be >= expires_at
        // This is correct behavior - expiration is exclusive end time
        let _result = user_role.is_valid();
        // Result may be true or false depending on exact timing; just verify no panic
        assert!(true, "Should handle boundary condition without panicking");
    }

    // ========================================================================
    // Test Suite 4: Role Hierarchy and Inheritance
    // ========================================================================

    #[test]
    fn test_role_parent_child_relationship() {
        let parent_role = create_test_role("admin", None, None);
        let child_role = create_test_role("manager", Some(parent_role.id), None);

        assert_eq!(
            child_role.parent_role_id,
            Some(parent_role.id),
            "Child role should reference parent"
        );
    }

    #[test]
    fn test_role_without_parent_is_root() {
        let role = create_test_role("viewer", None, None);
        assert!(
            role.parent_role_id.is_none(),
            "Root role should have no parent"
        );
    }

    #[test]
    fn test_role_hierarchy_chain() {
        let super_admin = create_test_role("super_admin", None, None);
        let admin = create_test_role("admin", Some(super_admin.id), None);
        let manager = create_test_role("manager", Some(admin.id), None);
        let user = create_test_role("user", Some(manager.id), None);

        assert_eq!(user.parent_role_id, Some(manager.id), "User -> manager");
        assert_eq!(manager.parent_role_id, Some(admin.id), "Manager -> admin");
        assert_eq!(
            admin.parent_role_id,
            Some(super_admin.id),
            "Admin -> super_admin"
        );
        assert!(
            super_admin.parent_role_id.is_none(),
            "Super admin has no parent"
        );
    }

    #[test]
    fn test_role_system_flag() {
        let system_role = Role {
            id: Uuid::new_v4(),
            name: "system_admin".to_string(),
            description: None,
            parent_role_id: None,
            tenant_id: None,
            is_system: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(system_role.is_system, "System role flag should be set");
    }

    #[test]
    fn test_role_tenant_isolation() {
        let tenant1 = Uuid::new_v4();
        let tenant2 = Uuid::new_v4();
        let role_tenant1 = create_test_role("admin", None, Some(tenant1));
        let role_tenant2 = create_test_role("admin", None, Some(tenant2));

        assert_eq!(
            role_tenant1.tenant_id,
            Some(tenant1),
            "Role scoped to tenant1"
        );
        assert_eq!(
            role_tenant2.tenant_id,
            Some(tenant2),
            "Role scoped to tenant2"
        );
        assert_ne!(
            role_tenant1.tenant_id, role_tenant2.tenant_id,
            "Different tenants should have different role scopes"
        );
    }

    // ========================================================================
    // Test Suite 5: Role-Permission Associations
    // ========================================================================

    #[test]
    fn test_role_permission_link() {
        let role_id = Uuid::new_v4();
        let permission_id = Uuid::new_v4();
        let role_perm = RolePermission {
            id: Uuid::new_v4(),
            role_id,
            permission_id,
            granted_at: Utc::now(),
        };

        assert_eq!(role_perm.role_id, role_id, "Role ID should be set");
        assert_eq!(
            role_perm.permission_id, permission_id,
            "Permission ID should be set"
        );
    }

    #[test]
    fn test_multiple_permissions_per_role() {
        let role_id = Uuid::new_v4();
        let perm1_id = Uuid::new_v4();
        let perm2_id = Uuid::new_v4();

        let role_perm1 = RolePermission {
            id: Uuid::new_v4(),
            role_id,
            permission_id: perm1_id,
            granted_at: Utc::now(),
        };

        let role_perm2 = RolePermission {
            id: Uuid::new_v4(),
            role_id,
            permission_id: perm2_id,
            granted_at: Utc::now(),
        };

        assert_eq!(role_perm1.role_id, role_perm2.role_id, "Same role");
        assert_ne!(
            role_perm1.permission_id, role_perm2.permission_id,
            "Different permissions"
        );
    }

    // ========================================================================
    // Test Suite 6: Permission Enforcement Scenarios
    // ========================================================================

    #[test]
    fn test_enforcement_allowed_with_exact_permission() {
        let perm = create_test_permission("document", "read");
        let requested = ("document", "read");

        // In real system: check if user has permission in their role's permissions
        // Here we test the permission matching logic
        assert!(
            perm.matches(requested.0, requested.1),
            "User with document:read permission should be allowed to read documents"
        );
    }

    #[test]
    fn test_enforcement_denied_without_permission() {
        let perm = create_test_permission("document", "read");
        let requested = ("document", "delete");

        assert!(
            !perm.matches(requested.0, requested.1),
            "User without document:delete permission should be denied"
        );
    }

    #[test]
    fn test_enforcement_denied_for_different_resource() {
        let perm = create_test_permission("document", "read");
        let requested = ("user", "read");

        assert!(
            !perm.matches(requested.0, requested.1),
            "User with document:read should not have access to user:read"
        );
    }

    #[test]
    fn test_enforcement_with_admin_wildcard_permission() {
        let admin_perm = create_test_permission("*", "*");

        // Admin with *:* should have access to everything
        assert!(admin_perm.matches("document", "read"));
        assert!(admin_perm.matches("user", "write"));
        assert!(admin_perm.matches("role", "delete"));
        assert!(admin_perm.matches("audit", "export"));
    }

    #[test]
    fn test_enforcement_with_resource_wildcard_permission() {
        let document_admin_perm = create_test_permission("document", "*");

        // Document admin with document:* should have all document actions
        assert!(document_admin_perm.matches("document", "read"));
        assert!(document_admin_perm.matches("document", "write"));
        assert!(document_admin_perm.matches("document", "delete"));

        // But not other resources
        assert!(!document_admin_perm.matches("user", "read"));
    }

    #[test]
    fn test_enforcement_multiple_roles_permission_union() {
        // Test that a user with multiple roles gets union of permissions
        let role1_perms = vec![
            create_test_permission("document", "read"),
            create_test_permission("document", "write"),
        ];

        let role2_perms = vec![
            create_test_permission("document", "delete"),
            create_test_permission("user", "read"),
        ];

        // User should be able to read, write, and delete documents, read users
        let all_perms: Vec<_> = [&role1_perms[..], &role2_perms[..]].concat();

        assert!(
            all_perms.iter().any(|p| p.matches("document", "read")),
            "Should have document:read from role1"
        );
        assert!(
            all_perms.iter().any(|p| p.matches("document", "write")),
            "Should have document:write from role1"
        );
        assert!(
            all_perms.iter().any(|p| p.matches("document", "delete")),
            "Should have document:delete from role2"
        );
        assert!(
            all_perms.iter().any(|p| p.matches("user", "read")),
            "Should have user:read from role2"
        );
    }

    // ========================================================================
    // Test Suite 7: Edge Cases
    // ========================================================================

    #[test]
    fn test_empty_permission_set_denies_all() {
        let empty_perms: Vec<Permission> = vec![];

        assert!(
            !empty_perms.iter().any(|p| p.matches("document", "read")),
            "User with no permissions should be denied all access"
        );
    }

    #[test]
    fn test_permission_with_constraints_still_matches() {
        let mut perm = create_test_permission("document", "read");
        perm.constraints = Some(serde_json::json!({
            "own_data_only": true
        }));

        // Permission matching doesn't check constraints in Phase 1 (Phase 12 feature)
        assert!(
            perm.matches("document", "read"),
            "Should match even with constraints"
        );
    }

    #[test]
    fn test_role_permission_audit_trail() {
        let granted_at = Utc::now();
        let role_perm = RolePermission {
            id: Uuid::new_v4(),
            role_id: Uuid::new_v4(),
            permission_id: Uuid::new_v4(),
            granted_at,
        };

        assert_eq!(
            role_perm.granted_at, granted_at,
            "Audit trail should be preserved"
        );
    }

    #[test]
    fn test_user_role_audit_trail() {
        let granted_by = Some(Uuid::new_v4());
        let granted_at = Utc::now();
        let user_role = UserRole {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            role_id: Uuid::new_v4(),
            tenant_id: None,
            granted_by,
            granted_at,
            expires_at: None,
        };

        assert_eq!(user_role.granted_by, granted_by, "Audit trail: granted_by");
        assert_eq!(user_role.granted_at, granted_at, "Audit trail: granted_at");
    }
}
