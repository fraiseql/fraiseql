//! Field-level RBAC filtering for runtime field projection.
//!
//! Filters fields based on user roles and scope requirements.
//! Supports two deny policies:
//! - `Reject`: query fails with FORBIDDEN if user lacks scope
//! - `Mask`: query succeeds, field value is replaced with `null`

use crate::{
    schema::{FieldDefinition, FieldDenyPolicy, SecurityConfig},
    security::SecurityContext,
};

/// Result of classifying requested fields against RBAC policies.
#[derive(Debug)]
pub struct FieldAccessResult {
    /// Fields the user can access (returned as-is).
    pub allowed: Vec<String>,
    /// Fields the user cannot access but `on_deny = Mask` (nulled out).
    pub masked: Vec<String>,
}

/// Classify requested projection fields into allowed, masked, or rejected.
///
/// For each requested field:
/// - If the user can access it (public or has scope) → `allowed`
/// - If the user lacks scope and `on_deny = Mask` → `masked`
/// - If the user lacks scope and `on_deny = Reject` → returns `Err` with
///   the field name (caller should produce a FORBIDDEN error)
///
/// # Errors
///
/// Returns `Err(field_name)` if any requested field has `on_deny = Reject`
/// and the user lacks the required scope.
pub fn classify_field_access(
    context: &SecurityContext,
    security_config: &SecurityConfig,
    fields: &[FieldDefinition],
    requested: Vec<String>,
) -> std::result::Result<FieldAccessResult, String> {
    let mut allowed = Vec::new();
    let mut masked = Vec::new();

    for name in requested {
        let field_def = fields.iter().find(|f| f.name == name);

        let Some(field) = field_def else {
            // Field not in type definition — pass through (may be a built-in like __typename)
            allowed.push(name);
            continue;
        };

        if can_access_field(context, security_config, field) {
            allowed.push(name);
        } else {
            match field.on_deny {
                FieldDenyPolicy::Mask => masked.push(name),
                FieldDenyPolicy::Reject => return Err(name),
            }
        }
    }

    Ok(FieldAccessResult { allowed, masked })
}

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
    use crate::schema::{FieldDenyPolicy, FieldType, RoleDefinition};

    fn create_test_field(name: &str, requires_scope: Option<&str>) -> FieldDefinition {
        FieldDefinition {
            name:           name.into(),
            field_type:     FieldType::String,
            nullable:       false,
            default_value:  None,
            description:    None,
            vector_config:  None,
            alias:          None,
            deprecation:    None,
            requires_scope: requires_scope.map(|s| s.to_string()),
            on_deny: FieldDenyPolicy::default(),
            encryption:     None,
        }
    }

    fn create_test_context(roles: Vec<&str>) -> SecurityContext {
        SecurityContext {
            user_id:          "test-user".to_string(),
            roles:            roles.iter().map(|&r| r.to_string()).collect(),
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

    // =========================================================================
    // classify_field_access tests
    // =========================================================================

    fn create_field_with_deny(
        name: &str,
        requires_scope: Option<&str>,
        on_deny: FieldDenyPolicy,
    ) -> FieldDefinition {
        FieldDefinition {
            name:           name.into(),
            field_type:     FieldType::String,
            nullable:       false,
            default_value:  None,
            description:    None,
            vector_config:  None,
            alias:          None,
            deprecation:    None,
            requires_scope: requires_scope.map(|s| s.to_string()),
            on_deny,
            encryption:     None,
        }
    }

    #[test]
    fn test_classify_all_public_fields() {
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("name", None, FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(vec![]);
        let config = SecurityConfig::new();

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "name".to_string()],
        );

        let access = result.expect("should succeed");
        assert_eq!(access.allowed, vec!["id", "name"]);
        assert!(access.masked.is_empty());
    }

    #[test]
    fn test_classify_mask_field_unauthorized() {
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("email", Some("read:email"), FieldDenyPolicy::Mask),
        ];
        let ctx = create_test_context(vec!["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:name".to_string()]));

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "email".to_string()],
        );

        let access = result.expect("should succeed (mask, not reject)");
        assert_eq!(access.allowed, vec!["id"]);
        assert_eq!(access.masked, vec!["email"]);
    }

    #[test]
    fn test_classify_reject_field_unauthorized() {
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("salary", Some("admin:*"), FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(vec!["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:*".to_string()]));

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "salary".to_string()],
        );

        assert_eq!(result.unwrap_err(), "salary");
    }

    #[test]
    fn test_classify_authorized_user_gets_all_fields() {
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("email", Some("read:email"), FieldDenyPolicy::Mask),
            create_field_with_deny("salary", Some("admin:*"), FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(vec!["admin"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["*".to_string()]));

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "email".to_string(), "salary".to_string()],
        );

        let access = result.expect("admin has all scopes");
        assert_eq!(access.allowed, vec!["id", "email", "salary"]);
        assert!(access.masked.is_empty());
    }

    #[test]
    fn test_classify_mixed_mask_and_reject_rejects() {
        // If a query requests both mask and reject fields the user lacks,
        // the reject field causes failure (reject wins).
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("email", Some("read:email"), FieldDenyPolicy::Mask),
            create_field_with_deny("salary", Some("hr:*"), FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(vec!["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:name".to_string()]));

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "email".to_string(), "salary".to_string()],
        );

        // salary is reject → error
        assert_eq!(result.unwrap_err(), "salary");
    }

    #[test]
    fn test_classify_unrequested_protected_field_no_error() {
        // If a protected field exists but isn't requested, no error.
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("salary", Some("admin:*"), FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(vec!["viewer"]);
        let config = SecurityConfig::new();

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string()], // salary not requested
        );

        let access = result.expect("should succeed — salary not requested");
        assert_eq!(access.allowed, vec!["id"]);
        assert!(access.masked.is_empty());
    }
}
