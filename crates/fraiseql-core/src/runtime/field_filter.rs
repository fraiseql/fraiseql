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
/// - If the user lacks scope and `on_deny = Reject` → returns `Err` with the field name (caller
///   should produce a FORBIDDEN error)
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
/// 1. Have a required scope (`requires_scope` is Some)
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
/// ```no_run
/// // Requires: SecurityContext and SecurityConfig from compiled schema.
/// # use fraiseql_core::security::SecurityContext;
/// # use fraiseql_core::schema::SecurityConfig;
/// # use fraiseql_core::schema::FieldDefinition;
/// # use fraiseql_core::runtime::field_filter::filter_fields;
/// # let context: SecurityContext = panic!("example");
/// # let config: SecurityConfig = panic!("example");
/// # let all_fields: Vec<FieldDefinition> = panic!("example");
/// let accessible = filter_fields(&context, &config, &all_fields);
/// ```
#[must_use]
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
///
/// # Panics
///
/// Cannot panic in practice — the `expect` on `requires_scope` is guarded
/// by an `is_none()` early-return immediately above.
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
    let required_scope = field
        .requires_scope
        .as_ref()
        .expect("requires_scope is Some; None was returned above");
    context.can_access_scope(security_config, required_scope)
}
