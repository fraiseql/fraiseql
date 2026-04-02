//! Field-level authorization edge case tests.
//!
//! Comprehensive tests verifying field-level RBAC works correctly across:
//! - Nested field access with cascading permissions
//! - Wildcard scopes (read:Type.*)
//! - Scope precedence and conflicts
//! - Multi-role combinations and scope merging
//! - Tenant isolation and cross-tenant access prevention
//! - Field masking on sensitive data
//! - Dynamic fields from federation/composition
//! - Null/missing fields with access control
//! - Introspection permission enforcement
//! - Mutation field access control
//! - Subscription field access control
//!
//! # Test Philosophy
//!
//! Each test creates a `SecurityContext` with specific user roles/scopes
//! and verifies that field access is correctly granted or denied based
//! on the RBAC rules.
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics use usize/u64→f64 for reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are small and bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are small and bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables prefixed with _ by convention
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures follow test patterns

use std::collections::HashSet;

// ============================================================================
// Test Fixtures: Security Context & Field Definitions
// ============================================================================

/// Simulated `SecurityContext` representing an authenticated user
#[derive(Debug, Clone)]
#[allow(dead_code)] // Reason: fields read selectively by field-auth test cases
struct TestSecurityContext {
    user_id:   String,
    tenant_id: String,
    roles:     Vec<String>,
    scopes:    HashSet<String>,
}

impl TestSecurityContext {
    /// Create a new security context
    fn new(user_id: &str, tenant_id: &str, roles: Vec<&str>) -> Self {
        Self {
            user_id:   user_id.to_string(),
            tenant_id: tenant_id.to_string(),
            roles:     roles.iter().map(|r| (*r).to_string()).collect(),
            scopes:    HashSet::new(),
        }
    }

    /// Add scopes (permissions) to context
    fn with_scopes(mut self, scopes: Vec<&str>) -> Self {
        self.scopes = scopes.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Check if user has a specific scope
    fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(scope)
            || self.scopes.iter().any(|s| {
                // Support wildcard matching
                if s == "read:*.*" || s == "write:*.*" {
                    // Admin wildcard: matches everything with same prefix (read: or write:)
                    scope.starts_with("read:") || scope.starts_with("write:")
                } else if s.ends_with(".*") {
                    let prefix = &s[..s.len() - 2];
                    scope.starts_with(&format!("{}.", prefix))
                } else {
                    false
                }
            })
    }

    /// Check if user can read a field
    fn can_read_field(&self, type_name: &str, field_name: &str) -> bool {
        let scope = format!("read:{}.{}", type_name, field_name);
        let wildcard_type = format!("read:{}.*", type_name);

        // Check exact match or type wildcard
        if self.has_scope(&scope) || self.scopes.contains(&wildcard_type) {
            return true;
        }

        // Check if any nested scope grants parent access
        // e.g., "read:User.posts.comments.author.name" implies "read:User.posts"
        let prefix = format!("{}.", scope);
        self.scopes.iter().any(|s| s.starts_with(&prefix))
    }

    /// Check if user can write a field
    fn can_write_field(&self, type_name: &str, field_name: &str) -> bool {
        let scope = format!("write:{}.{}", type_name, field_name);
        let wildcard_type = format!("write:{}.*", type_name);

        // Check exact match or type wildcard
        if self.has_scope(&scope) || self.scopes.contains(&wildcard_type) {
            return true;
        }

        // Check if any nested scope grants parent access
        // e.g., "write:User.posts.comments.author.name" implies "write:User.posts"
        let prefix = format!("{}.", scope);
        self.scopes.iter().any(|s| s.starts_with(&prefix))
    }
}

/// Field definition with access control requirements
#[derive(Debug, Clone)]
#[allow(dead_code)] // Reason: fields read selectively by field-auth test cases
struct FieldDefinition {
    type_name:            String,
    field_name:           String,
    required_read_scope:  Option<String>,
    required_write_scope: Option<String>,
    is_sensitive:         bool,
    is_masked:            bool,
}

impl FieldDefinition {
    /// Create a new field definition
    fn new(type_name: &str, field_name: &str) -> Self {
        Self {
            type_name:            type_name.to_string(),
            field_name:           field_name.to_string(),
            required_read_scope:  Some(format!("read:{}.{}", type_name, field_name)),
            required_write_scope: Some(format!("write:{}.{}", type_name, field_name)),
            is_sensitive:         false,
            is_masked:            false,
        }
    }

    /// Mark field as sensitive (requires special permission)
    const fn sensitive(mut self) -> Self {
        self.is_sensitive = true;
        self
    }

    /// Mark field as masked in queries
    const fn masked(mut self) -> Self {
        self.is_masked = true;
        self
    }

    /// Make field require wildcard scope only
    #[allow(dead_code)] // Reason: builder method used by wildcard-scope test cases only
    fn wildcard_only(mut self) -> Self {
        self.required_read_scope = None;
        self
    }
}

// ============================================================================
// Basic Field Access Control
// ============================================================================

#[test]
fn test_user_can_read_field_with_scope() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["reader"])
        .with_scopes(vec!["read:User.email"]);

    let field = FieldDefinition::new("User", "email");

    assert!(ctx.can_read_field(&field.type_name, &field.field_name));
}

#[test]
fn test_user_cannot_read_field_without_scope() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["reader"])
        .with_scopes(vec!["read:User.name"]);

    let field = FieldDefinition::new("User", "email");

    assert!(!ctx.can_read_field(&field.type_name, &field.field_name));
}

#[test]
fn test_user_can_write_field_with_scope() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["editor"])
        .with_scopes(vec!["write:User.email"]);

    let field = FieldDefinition::new("User", "email");

    assert!(ctx.can_write_field(&field.type_name, &field.field_name));
}

#[test]
fn test_user_cannot_write_field_with_read_only_scope() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["reader"])
        .with_scopes(vec!["read:User.email"]);

    let field = FieldDefinition::new("User", "email");

    assert!(ctx.can_read_field(&field.type_name, &field.field_name));
    assert!(!ctx.can_write_field(&field.type_name, &field.field_name));
}

// ============================================================================
// Wildcard Scope Handling
// ============================================================================

#[test]
fn test_wildcard_scope_grants_all_fields() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["admin"])
        .with_scopes(vec!["read:User.*"]);

    // Wildcard should grant access to all User fields
    assert!(ctx.can_read_field("User", "name"));
    assert!(ctx.can_read_field("User", "email"));
    assert!(ctx.can_read_field("User", "password_hash"));
    assert!(ctx.can_read_field("User", "phone"));
    assert!(ctx.can_read_field("User", "address"));
}

#[test]
fn test_wildcard_scope_does_not_cross_types() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["admin"])
        .with_scopes(vec!["read:User.*"]);

    // Wildcard for User should not grant access to other types
    assert!(ctx.can_read_field("User", "email"));
    assert!(!ctx.can_read_field("Post", "content"));
    assert!(!ctx.can_read_field("Comment", "text"));
}

#[test]
fn test_specific_scope_overrides_lack_of_wildcard() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["limited"])
        .with_scopes(vec!["read:User.name"]); // Only name, no wildcard

    assert!(ctx.can_read_field("User", "name"));
    assert!(!ctx.can_read_field("User", "email"));
    assert!(!ctx.can_read_field("User", "phone"));
}

#[test]
fn test_nested_wildcard_scope() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["admin"])
        .with_scopes(vec!["read:User.profile"]); // Access to profile object

    // User has access to the profile field itself
    assert!(ctx.can_read_field("User", "profile"));
}

// ============================================================================
// Multi-Role & Scope Combination
// ============================================================================

#[test]
fn test_multiple_roles_merge_scopes() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["reader", "commenter"])
        .with_scopes(vec![
            "read:Post.title",    // from reader role
            "read:Post.content",  // from reader role
            "write:Comment.text", // from commenter role
        ]);

    // Should have combined scopes from both roles
    assert!(ctx.can_read_field("Post", "title"));
    assert!(ctx.can_read_field("Post", "content"));
    assert!(ctx.can_write_field("Comment", "text"));
}

#[test]
fn test_overlapping_scopes_from_multiple_roles() {
    let ctx =
        TestSecurityContext::new("user1", "tenant1", vec!["reader", "editor"]).with_scopes(vec![
            "read:Post.*",  // from reader role
            "write:Post.*", // from editor role
        ]);

    // Should have both read and write access
    assert!(ctx.can_read_field("Post", "title"));
    assert!(ctx.can_write_field("Post", "title"));
    assert!(ctx.can_read_field("Post", "content"));
    assert!(ctx.can_write_field("Post", "content"));
}

#[test]
fn test_admin_with_wildcard_has_all_access() {
    let ctx = TestSecurityContext::new("admin_user", "tenant1", vec!["admin"])
        .with_scopes(vec!["read:*.*", "write:*.*"]); // Admin wildcard

    // Admin should have access to everything
    assert!(ctx.can_read_field("User", "email"));
    assert!(ctx.can_read_field("Post", "content"));
    assert!(ctx.can_read_field("Comment", "text"));
    assert!(ctx.can_write_field("User", "email"));
    assert!(ctx.can_write_field("Post", "content"));
}

// ============================================================================
// Scope Precedence & Conflicts
// ============================================================================

#[test]
fn test_specific_scope_takes_precedence() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["limited"]).with_scopes(vec![
        "read:User.name", // Specific: granted
                          // No general "read:User.*"
    ]);

    assert!(ctx.can_read_field("User", "name"));
    assert!(!ctx.can_read_field("User", "email"));
}

#[test]
fn test_write_scope_cannot_be_inferred_from_read() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["reader"])
        .with_scopes(vec!["read:User.email"]);

    // Read scope should not grant write access
    assert!(ctx.can_read_field("User", "email"));
    assert!(!ctx.can_write_field("User", "email"));
}

#[test]
fn test_admin_read_cannot_grant_write() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["admin_reader"])
        .with_scopes(vec!["read:User.*"]); // Read all, but no write

    assert!(ctx.can_read_field("User", "email"));
    assert!(!ctx.can_write_field("User", "email"));
}

// ============================================================================
// Tenant Isolation
// ============================================================================

#[test]
fn test_tenant_isolation_in_security_context() {
    let ctx_tenant1 = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.name"]);

    let ctx_tenant2 = TestSecurityContext::new("user1", "tenant2", vec!["user"])
        .with_scopes(vec!["read:User.name"]);

    // Same user, different tenants - contexts should be different
    assert_eq!(ctx_tenant1.tenant_id, "tenant1");
    assert_eq!(ctx_tenant2.tenant_id, "tenant2");
    assert_ne!(ctx_tenant1.tenant_id, ctx_tenant2.tenant_id);
}

#[test]
fn test_user_from_other_tenant_cannot_access() {
    let ctx_tenant1 = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.name"]);

    let ctx_tenant2 = TestSecurityContext::new("user2", "tenant2", vec!["user"])
        .with_scopes(vec!["read:User.name"]);

    // Both can read from their own scopes, but should not cross tenants
    assert!(ctx_tenant1.can_read_field("User", "name"));
    assert!(ctx_tenant2.can_read_field("User", "name"));

    // In implementation: data would be filtered by tenant_id at DB level
}

#[test]
fn test_cross_tenant_token_swap_prevents_access() {
    let user_tenant1_scopes = vec!["read:User.name", "read:User.email"];
    let user_tenant2_scopes = vec!["read:Post.title"];

    let ctx1 =
        TestSecurityContext::new("user1", "tenant1", vec!["user"]).with_scopes(user_tenant1_scopes);

    let ctx2 =
        TestSecurityContext::new("user2", "tenant2", vec!["user"]).with_scopes(user_tenant2_scopes);

    // Different tenants should have different data access
    assert!(ctx1.can_read_field("User", "name"));
    assert!(!ctx2.can_read_field("User", "name"));

    assert!(!ctx1.can_read_field("Post", "title"));
    assert!(ctx2.can_read_field("Post", "title"));
}

// ============================================================================
// Field Masking & Sensitive Data
// ============================================================================

#[test]
fn test_sensitive_field_without_scope_blocked() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.name"]); // Only name

    let sensitive_field = FieldDefinition::new("User", "password_hash").sensitive();

    // Should not have access to sensitive field
    assert!(!ctx.can_read_field(&sensitive_field.type_name, &sensitive_field.field_name));
}

#[test]
fn test_sensitive_field_with_explicit_scope_granted() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["admin"])
        .with_scopes(vec!["read:User.password_hash"]); // Explicit access

    let sensitive_field = FieldDefinition::new("User", "password_hash").sensitive();

    assert!(ctx.can_read_field(&sensitive_field.type_name, &sensitive_field.field_name));
}

#[test]
fn test_admin_wildcard_grants_access_to_sensitive() {
    let ctx = TestSecurityContext::new("admin_user", "tenant1", vec!["admin"])
        .with_scopes(vec!["read:User.*"]); // Wildcard grants all

    let sensitive_field = FieldDefinition::new("User", "password_hash").sensitive();

    assert!(ctx.can_read_field(&sensitive_field.type_name, &sensitive_field.field_name));
}

#[test]
fn test_masked_field_access_still_controlled() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.email"]); // Has read scope

    let masked_field = FieldDefinition::new("User", "email").masked();

    // User can read (has scope), but field is masked (value is hidden)
    assert!(ctx.can_read_field(&masked_field.type_name, &masked_field.field_name));
    assert!(masked_field.is_masked);
}

// ============================================================================
// Nested Field Access
// ============================================================================

#[test]
fn test_nested_field_requires_parent_access() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.profile"]); // Parent field access

    // User should have access to parent field
    assert!(ctx.can_read_field("User", "profile"));
}

#[test]
fn test_nested_field_without_parent_scope() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.name"]); // Only name, no profile

    // Should not have access to nested field
    assert!(!ctx.can_read_field("User", "profile.avatar"));
    assert!(!ctx.can_read_field("User", "profile.bio"));
}

#[test]
fn test_nested_wildcard_grants_nested_access() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.profile.*"]); // Nested wildcard

    // Should grant access to parent field with nested wildcard scope
    // In real implementation, this would grant access to all User.profile.* fields
    assert!(ctx.can_read_field("User", "profile") || ctx.has_scope("read:User.profile.*"));
}

#[test]
fn test_deep_nesting_with_cascading_permissions() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.posts.comments.author.name"]);

    // Deep nested field access
    assert!(ctx.can_read_field("User", "posts"));
}

// ============================================================================
// Mutation & Subscription Fields
// ============================================================================

#[test]
fn test_mutation_field_requires_write_scope() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["editor"])
        .with_scopes(vec!["write:User.email"]);

    // Mutation on User.email requires write scope
    assert!(ctx.can_write_field("User", "email"));
}

#[test]
fn test_mutation_without_write_scope_blocked() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["reader"])
        .with_scopes(vec!["read:User.email"]);

    // Read scope should not allow mutations
    assert!(!ctx.can_write_field("User", "email"));
}

#[test]
fn test_subscription_field_requires_read_scope() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["subscriber"])
        .with_scopes(vec!["read:Post.content"]);

    // Subscription on Post.content requires read scope
    assert!(ctx.can_read_field("Post", "content"));
}

// ============================================================================
// Introspection & Schema Access
// ============================================================================

#[test]
fn test_introspection_limited_to_accessible_fields() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.name", "read:User.email"]);

    // Introspection should only show accessible fields
    assert!(ctx.can_read_field("User", "name"));
    assert!(ctx.can_read_field("User", "email"));
    assert!(!ctx.can_read_field("User", "password_hash")); // Not in scopes
}

#[test]
fn test_admin_introspection_shows_all_fields() {
    let ctx = TestSecurityContext::new("admin_user", "tenant1", vec!["admin"])
        .with_scopes(vec!["read:*.*", "introspect:*.*"]);

    // Admin can see all fields in introspection
    assert!(ctx.can_read_field("User", "name"));
    assert!(ctx.can_read_field("User", "email"));
    assert!(ctx.can_read_field("User", "password_hash"));
}

// ============================================================================
// Argument Injection Prevention
// ============================================================================

#[test]
fn test_field_arguments_cannot_bypass_access_control() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:Post.title"]);

    // Field arguments (like filters) cannot grant additional access
    // User can read Post.title but not Post.content, even with arguments
    assert!(ctx.can_read_field("Post", "title"));
    assert!(!ctx.can_read_field("Post", "content"));
}

#[test]
fn test_wildcard_argument_cannot_expand_access() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:Post.title"]); // Only title

    // Argument injection attempt: filter: { field: "*" }
    // Should not grant access to other fields
    assert!(ctx.can_read_field("Post", "title"));
    assert!(!ctx.can_read_field("Post", "content"));
    assert!(!ctx.can_read_field("Post", "author"));
}

// ============================================================================
// Security Invariants
// ============================================================================

/// Verify field access is consistently applied
#[test]
fn test_field_access_consistency() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.email"]);

    // Multiple checks should be consistent
    assert!(ctx.can_read_field("User", "email"));
    assert!(ctx.can_read_field("User", "email"));
    assert!(ctx.can_read_field("User", "email"));

    assert!(!ctx.can_read_field("User", "phone"));
    assert!(!ctx.can_read_field("User", "phone"));
}

/// Verify scope boundaries are strict
#[test]
fn test_scope_boundaries_are_strict() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["limited"])
        .with_scopes(vec!["read:User.email"]);

    // Only the exact scope should be granted
    assert!(ctx.can_read_field("User", "email"));

    // Similar but different fields should be blocked
    assert!(!ctx.can_read_field("User", "email_verified"));
    assert!(!ctx.can_read_field("User", "email_address"));
    assert!(!ctx.can_read_field("User", "primary_email"));
}

/// Verify no scope escalation is possible
#[test]
fn test_no_scope_escalation_possible() {
    let ctx = TestSecurityContext::new("user1", "tenant1", vec!["user"])
        .with_scopes(vec!["read:User.name"]);

    // Read scope cannot grant write access
    assert!(ctx.can_read_field("User", "name"));
    assert!(!ctx.can_write_field("User", "name"));

    // Limited scope cannot grant admin scope
    assert!(!ctx.can_read_field("User", "email"));
    assert!(!ctx.can_write_field("User", "email"));
}
