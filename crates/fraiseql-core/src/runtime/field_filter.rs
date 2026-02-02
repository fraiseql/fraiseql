//! Field-level RBAC filtering for runtime field projection.
//!
//! Filters fields based on user roles and scope requirements,
//! removing fields the user cannot access.

use crate::{
    schema::{FieldDefinition, SecurityConfig},
    security::SecurityContext,
};

/// Filter fields based on user's roles and scope requirements.
///
/// Removes fields that:
/// 1. Have a required scope (requires_scope is Some)
/// 2. User's roles don't grant access to that scope
///
/// # Arguments
///
/// * `context` - Security context with user's roles
/// * `security_config` - Compiled security config with role definitions
/// * `fields` - All available fields
///
/// # Returns
///
/// Vector of accessible fields
///
/// # Example
///
/// ```ignore
/// let accessible = filter_fields(&context, &config, &all_fields);
/// ```
pub fn filter_fields<'a>(
    context: &SecurityContext,
    security_config: &SecurityConfig,
    fields: &'a [FieldDefinition],
) -> Vec<&'a FieldDefinition> {
    fields
        .iter()
        .filter(|field| can_access_field(context, security_config, field))
        .collect()
}

/// Check if user can access a specific field.
///
/// Returns true if:
/// 1. Field has no scope requirement (public), OR
/// 2. User's roles grant the required scope
///
/// # Arguments
///
/// * `context` - Security context with user's roles
/// * `security_config` - Compiled security config with role definitions
/// * `field` - Field definition to check
///
/// # Returns
///
/// `true` if user can access the field, `false` otherwise.
#[must_use]
pub fn can_access_field(
    context: &SecurityContext,
    security_config: &SecurityConfig,
    field: &FieldDefinition,
) -> bool {
    // If field has no scope requirement, it's public and always accessible
    if field.requires_scope.is_none() {
        return true;
    }

    // Field has a scope requirement - check if user's roles grant it
    let required_scope = field.requires_scope.as_ref().unwrap();
    context.can_access_scope(security_config, required_scope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{FieldType, RoleDefinition};

    fn create_test_field(name: &str, requires_scope: Option<&str>) -> FieldDefinition {
        FieldDefinition {
            name:           name.to_string(),
            field_type:     FieldType::String,
            nullable:       false,
            default_value:  None,
            description:    None,
            vector_config:  None,
            alias:          None,
            deprecation:    None,
            requires_scope: requires_scope.map(|s| s.to_string()),
        }
    }

    fn create_test_context(roles: Vec<&str>) -> SecurityContext {
        SecurityContext {
            user_id:          "test-user".to_string(),
            roles:            roles.iter().map(|r| r.to_string()).collect(),
            tenant_id:        None,
            scopes:           vec![],
            attributes:       std::collections::HashMap::new(),
            request_id:       "test-req".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        }
    }

    #[test]
    fn test_can_access_public_field() {
        let field = create_test_field("email", None);
        let context = create_test_context(vec![]);
        let config = SecurityConfig::new();

        assert!(
            can_access_field(&context, &config, &field),
            "Public field should be accessible to any user"
        );
    }

    #[test]
    fn test_cannot_access_scoped_field_without_role() {
        let field = create_test_field("password", Some("admin:*"));
        let context = create_test_context(vec!["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:*".to_string()]));
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]));

        assert!(
            !can_access_field(&context, &config, &field),
            "User without admin role cannot access admin field"
        );
    }

    #[test]
    fn test_can_access_scoped_field_with_role() {
        let field = create_test_field("password", Some("admin:*"));
        let context = create_test_context(vec!["admin"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]));

        assert!(
            can_access_field(&context, &config, &field),
            "User with admin role can access admin field"
        );
    }

    #[test]
    fn test_filter_fields_removes_inaccessible() {
        let fields = vec![
            create_test_field("id", None),                       // public
            create_test_field("name", None),                     // public
            create_test_field("email", Some("read:User.email")), // scoped
            create_test_field("password", Some("admin:*")),      // admin only
        ];

        let context = create_test_context(vec!["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:User.*".to_string()]));
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]));

        let accessible = filter_fields(&context, &config, &fields);

        // Should have: id, name, email (viewer has read:User.*)
        // Should not have: password (requires admin:*)
        assert_eq!(accessible.len(), 3, "Should have 3 accessible fields");
        assert_eq!(accessible[0].name, "id");
        assert_eq!(accessible[1].name, "name");
        assert_eq!(accessible[2].name, "email");
    }

    #[test]
    fn test_filter_fields_all_accessible() {
        let fields = vec![
            create_test_field("id", None),
            create_test_field("name", None),
            create_test_field("email", Some("read:User.email")),
            create_test_field("password", Some("admin:*")),
        ];

        let context = create_test_context(vec!["admin"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["*".to_string()]));

        let accessible = filter_fields(&context, &config, &fields);

        // Admin has global wildcard (*) which matches all scopes
        assert_eq!(accessible.len(), 4, "Admin with global wildcard should access all fields");
    }
}
