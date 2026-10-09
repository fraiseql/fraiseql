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
#[derive(Debug, Clone)]
pub struct FieldAccessResult {
    /// Every field to project, **in the order the client requested them**.
    ///
    /// Masked fields are included: the response still carries the key, valued
    /// `null`, and GraphQL requires it to appear where the query put it. This
    /// used to be an `allowed` list that callers extended with `masked`, which
    /// silently moved every masked field to the end of the response object.
    pub projected: Vec<String>,
    /// The subset of `projected` the caller must null out after projecting.
    pub masked:    Vec<String>,
}

impl FieldAccessResult {
    /// Fields the user can access, in request order.
    ///
    /// Derived rather than stored so it cannot drift out of step with
    /// `projected` — the two used to be built side by side and only their
    /// concatenation was ever used.
    #[must_use]
    pub fn allowed(&self) -> Vec<String> {
        self.projected.iter().filter(|f| !self.masked.contains(f)).cloned().collect()
    }
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
    let mut projected = Vec::with_capacity(requested.len());
    let mut masked = Vec::new();

    for name in requested {
        // A translations sibling is gated by the field it lists (#1523).
        let field_def = crate::schema::gated_field(fields, &name);

        let Some(field) = field_def else {
            // Field not in type definition — pass through (may be a built-in like __typename)
            projected.push(name);
            continue;
        };

        if !can_access_field(context, security_config, field) {
            match field.on_deny {
                FieldDenyPolicy::Mask => masked.push(name.clone()),
                FieldDenyPolicy::Reject => return Err(name),
            }
        }
        // Allowed and masked fields alike keep their requested position.
        projected.push(name);
    }

    Ok(FieldAccessResult { projected, masked })
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

/// Whether a request may *reference* `field`: filter, order, rank or search by it.
///
/// A reference answers a question about the value by another route than reading it (ruling
/// AA 3): which rows come back, and in what order. So it needs what reading needs, and a
/// masked value is no better than a refused one — a filter probes it all the same:
///
/// * an `authorize` field: never. Its decision is per row, taken after the read; no reference to it
///   can be judged before one;
/// * a `requires_scope` field: only when the caller holds the scope, whatever its `on_deny`. A
///   request with no principal holds none (#743), and a schema with no `security` section grants
///   none (ruling Y 7).
///
/// The one rule every read path asks — the engine's filter and order checks and REST's
/// `?search=` planning — so the paths cannot disagree about what may be referenced.
#[must_use]
pub fn can_reference_field(
    security_config: Option<&SecurityConfig>,
    field: &FieldDefinition,
    context: Option<&SecurityContext>,
) -> bool {
    if field.authorize {
        return false;
    }
    let Some(scope) = field.requires_scope.as_deref() else {
        return true;
    };
    let no_roles = SecurityConfig::default();
    context.is_some_and(|ctx| ctx.can_access_scope(security_config.unwrap_or(&no_roles), scope))
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
