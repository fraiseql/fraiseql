//! Pure RBAC field-access classification helpers and session variable resolution.
//!
//! These are stateless functions that require no `&self` — all inputs come from
//! parameters.  They are shared by multiple runners without creating any coupling
//! to `Executor<A>`.

use crate::{
    error::{FraiseQLError, Result},
    runtime::{classify_field_access, field_filter::FieldAccessResult},
    schema::{CompiledSchema, SessionVariableSource, SessionVariablesConfig},
    security::SecurityContext,
};

/// Resolve session variable mappings against the current security context.
///
/// Returns a list of `(name, value)` pairs to inject as PostgreSQL transaction-scoped
/// session variables via `set_config()`.
///
/// Resolution rules:
/// - [`SessionVariableSource::Jwt`] — looks up the claim in `security_context.attributes`; falls
///   back to `user_id` for `"sub"` and to `tenant_id` for `"tenant_id"`.  Missing claims are
///   silently skipped.
/// - [`SessionVariableSource::Header`] — looks up the header name in `security_context.attributes`.
///   Missing headers are silently skipped.
/// - [`SessionVariableSource::Literal`] — uses the fixed value as-is.
///
/// When `config.inject_started_at` is `true`, the pair
/// `("fraiseql.started_at", <RFC 3339 now>)` is **prepended** to the returned list.
#[must_use]
pub(in super::super) fn resolve_session_variables(
    config: &SessionVariablesConfig,
    security_context: &SecurityContext,
) -> Vec<(String, String)> {
    use chrono::Utc;

    let mut vars: Vec<(String, String)> = Vec::new();

    if config.inject_started_at {
        vars.push(("fraiseql.started_at".to_string(), Utc::now().to_rfc3339()));
    }

    for mapping in &config.variables {
        let value: Option<String> = match &mapping.source {
            SessionVariableSource::Jwt { claim } => {
                // Check custom attributes first (raw JWT claims forwarded there).
                // Fall back to well-known SecurityContext fields for `sub`/`user_id`
                // and `tenant_id` so that schemas that populate only those fields
                // (not attributes) still work.
                if let Some(v) = security_context.attributes.get(claim.as_str()) {
                    Some(if let serde_json::Value::String(s) = v {
                        s.clone()
                    } else {
                        v.to_string()
                    })
                } else if claim == "sub" || claim == "user_id" {
                    Some(security_context.user_id.0.clone())
                } else if claim == "tenant_id" {
                    security_context.tenant_id.as_ref().map(|t| t.0.clone())
                } else {
                    None
                }
            },
            SessionVariableSource::Header { header } => {
                // HTTP headers are forwarded into attributes
                security_context.attributes.get(header.as_str()).map(|v| {
                    if let serde_json::Value::String(s) = v {
                        s.clone()
                    } else {
                        v.to_string()
                    }
                })
            },
            SessionVariableSource::Literal { value } => Some(value.clone()),
        };
        if let Some(v) = value {
            vars.push((mapping.name.clone(), v));
        }
    }

    vars
}

/// Classify each requested field as allowed, masked, or rejected.
///
/// Does NOT require `&self` — all data comes from parameters.
///
/// # Errors
///
/// Returns `FraiseQLError::Authorization` if any field has `on_deny = Reject`
/// and the user lacks the required scope.
pub(in super::super) fn apply_field_rbac_filtering(
    schema: &CompiledSchema,
    return_type: &str,
    projection_fields: Vec<String>,
    security_context: &SecurityContext,
) -> Result<FieldAccessResult> {
    if let Some(security_config) = schema.security.as_ref() {
        if let Some(type_def) = schema.types.iter().find(|t| t.name == return_type) {
            return classify_field_access(
                security_context,
                security_config,
                &type_def.fields,
                projection_fields,
            )
            .map_err(|rejected_field| FraiseQLError::Authorization {
                message:  format!(
                    "Access denied: field '{rejected_field}' on type '{return_type}' \
                     requires a scope you do not have"
                ),
                action:   Some("read".to_string()),
                resource: Some(format!("{return_type}.{rejected_field}")),
            });
        }
    }

    Ok(FieldAccessResult {
        allowed: projection_fields,
        masked:  Vec::new(),
    })
}
