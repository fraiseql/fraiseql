//! Dynamic, decision-returning field-level authorization.
//!
//! Where [`FieldFilter`](crate::security::FieldFilter) /
//! [`requires_scope`](crate::schema::FieldDefinition) answer the *static* question
//! "does this principal hold scope X?", a [`FieldAuthorizer`] answers the *dynamic*
//! question "may **this** principal read **this** field of **this** row, given the
//! field's arguments?". It is the field-level analogue of an operation-level
//! authorizer and the counterpart of the `RLSPolicy` plugin: a Policy Enforcement
//! Point where the engine *enforces* but the *decision* is delegated to an
//! app-supplied trait object.
//!
//! # Semantics
//!
//! - **Fail-closed**: any `Err` returned by [`FieldAuthorizer::authorize_field`] is treated as a
//!   hard deny — the request fails with [`FraiseQLError::Authorization`] (HTTP 403 / `FORBIDDEN`).
//!   The field value is never served on a policy failure.
//! - **AND-composition**: the dynamic decision composes with the static `requires_scope` gate as a
//!   logical AND — a field is visible only if *both* the static gate and the dynamic authorizer
//!   allow it.
//! - **Deny policy**: a [`FieldAuthzDecision::Deny`] reuses the existing [`FieldDenyPolicy`]:
//!   `Reject` fails the whole query, `Mask` nulls just that field on just that row.
//!
//! Only fields marked policy-gated in the compiled schema
//! ([`FieldDefinition::authorize`](crate::schema::FieldDefinition)) are passed to the
//! authorizer, so non-gated fields incur zero per-row overhead.
//!
//! # Wiring
//!
//! Register an implementation on [`RuntimeConfig`](crate::runtime::RuntimeConfig) via
//! [`with_field_authorizer`](crate::runtime::RuntimeConfig::with_field_authorizer),
//! exactly parallel to [`with_rls_policy`](crate::runtime::RuntimeConfig::with_rls_policy).

use serde_json::Value as JsonValue;

use crate::{
    db::types::JsonbValue,
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::projection::effective_selections,
    schema::{CompiledSchema, FieldDenyPolicy, FieldType},
    security::SecurityContext,
};

/// A field-level authorization request handed to a [`FieldAuthorizer`].
///
/// Carries the principal, the field being resolved (its GraphQL type and name), the
/// full parent row it is being projected from, and the field's GraphQL arguments —
/// the exact inputs a static scope check lacks.
#[non_exhaustive]
pub struct FieldAuthzRequest<'a> {
    /// The authenticated principal making the request.
    pub principal:  &'a SecurityContext,
    /// The GraphQL type name that owns the field (e.g. `"User"`).
    pub type_name:  &'a str,
    /// The field name being authorized (e.g. `"email"`).
    pub field_name: &'a str,
    /// The full row/object the field is projected from, when available.
    ///
    /// This is the *complete* fetched row (not just the selected fields), so a
    /// policy may key on columns the client did not select (e.g. an `owner_id`
    /// used to decide ownership). `None` only on paths where no row context exists.
    pub parent:     Option<&'a serde_json::Value>,
    /// The field's GraphQL arguments, when present.
    pub arguments:  Option<&'a serde_json::Value>,
}

/// The decision a [`FieldAuthorizer`] returns for a single field on a single row.
#[non_exhaustive]
pub enum FieldAuthzDecision {
    /// Allow the field to be resolved and projected.
    Allow,
    /// Deny access to the field.
    ///
    /// `code` is a domain-specific deny code (folded into the `Authorization`
    /// error message on a `Reject`). `on_deny` reuses [`FieldDenyPolicy`]:
    /// - [`FieldDenyPolicy::Reject`] fails the whole query with 403 `FORBIDDEN`,
    /// - [`FieldDenyPolicy::Mask`] succeeds but returns `null` for this field on this row.
    Deny {
        /// Domain-specific deny code (e.g. `"not_owner"`).
        code:    String,
        /// What to do on deny — reject the query or mask the field.
        on_deny: FieldDenyPolicy,
    },
}

/// A pluggable, decision-returning field-level authorizer.
///
/// Implementations decide, per principal / per row / per field-arguments, whether a
/// policy-gated field may be read. The engine enforces the decision; this trait
/// supplies it. Implementations must be `Send + Sync` to be shared across the async
/// execution path.
///
/// # Example
///
/// ```
/// use fraiseql_core::security::{
///     FieldAuthorizer, FieldAuthzRequest, FieldAuthzDecision,
/// };
/// use fraiseql_core::schema::FieldDenyPolicy;
/// use fraiseql_core::error::Result;
///
/// /// Reveal a gated field only to the row's owner; mask it for everyone else.
/// struct OwnerOnly;
///
/// impl FieldAuthorizer for OwnerOnly {
///     fn authorize_field(&self, req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
///         let owner = req
///             .parent
///             .and_then(|p| p.get("owner_id"))
///             .and_then(|v| v.as_str());
///         if owner == Some(req.principal.user_id.as_str()) {
///             Ok(FieldAuthzDecision::Allow)
///         } else {
///             Ok(FieldAuthzDecision::Deny {
///                 code:    "not_owner".to_string(),
///                 on_deny: FieldDenyPolicy::Mask,
///             })
///         }
///     }
/// }
/// ```
pub trait FieldAuthorizer: Send + Sync {
    /// Decide whether the principal may read the requested field on this row.
    ///
    /// # Errors
    ///
    /// Any `Err` is treated as a **hard deny** (fail-closed): the request fails
    /// with [`FraiseQLError::Authorization`]
    /// (HTTP 403 / `FORBIDDEN`) and the field value is never served. Return
    /// [`FieldAuthzDecision::Deny`] for an ordinary, expected denial; reserve `Err`
    /// for policy-evaluation failures (e.g. an unreachable policy backend).
    fn authorize_field(&self, req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision>;
}

// ============================================================================
// Runtime enforcement helpers (shared by the executor's projection paths)
// ============================================================================

/// A selected, policy-gated field on an entity row, with the data the authorizer
/// needs. `name` is the GraphQL field name — also the projected output key for the
/// entity row (consistent with the static-gate masking in
/// [`null_masked_fields`](crate::runtime::executor)).
pub(crate) struct GatedField {
    /// GraphQL field name — used for the authorizer request and the static-mask check.
    pub(crate) field_name: String,
    /// Response key (alias) when the selection aliased the field. The projected key is
    /// the field name on the query path (no alias applied) or the response key on the
    /// mutation path; enforcement tries both so masking is correct on either path.
    pub(crate) alias:      Option<String>,
    /// The field's GraphQL arguments as a JSON object, when any are present.
    pub(crate) arguments:  Option<JsonValue>,
}

/// Resolve the object type a (possibly list-wrapped) field points to, if any.
fn object_type_of(field_type: &FieldType) -> Option<&str> {
    field_type
        .type_name()
        .or_else(|| field_type.inner_type().and_then(FieldType::type_name))
}

/// Build a JSON object of a selection's GraphQL arguments, or `None` if it has none.
fn field_arguments_json(sel: &FieldSelection) -> Option<JsonValue> {
    if sel.arguments.is_empty() {
        return None;
    }
    let mut map = serde_json::Map::with_capacity(sel.arguments.len());
    for arg in &sel.arguments {
        let value = serde_json::from_str::<JsonValue>(&arg.value_json)
            .unwrap_or_else(|_| JsonValue::String(arg.value_json.clone()));
        map.insert(arg.name.clone(), value);
    }
    Some(JsonValue::Object(map))
}

/// Returns `true` if any field in `fields` (selected on `type_name`, top-level **or**
/// nested) resolves to a policy-gated [`FieldDefinition`](crate::schema::FieldDefinition).
///
/// Used pre-plan to decide whether to bypass the response cache and the SQL
/// projection hint (the per-row decision is neither cacheable nor compatible with a
/// selection-stripped row).
pub(crate) fn selection_set_selects_gated_field(
    schema: &CompiledSchema,
    type_name: &str,
    fields: &[FieldSelection],
) -> bool {
    let Some(type_def) = schema.find_type(type_name) else {
        return false;
    };
    // Resolve inline `... on T` fragments so a gated field selected through a
    // fragment (interface/union member, e.g. a cascade `entity { ... on Post {
    // gated } }` or a union mutation's `... on Post { gated }`) is not invisible.
    effective_selections(fields, type_name, schema).iter().any(|sel| {
        type_def.fields.iter().any(|f| f.name.as_str() == sel.name && f.authorize)
            || selection_field_has_gated_descendant(schema, type_name, sel)
    })
}

/// Returns `true` if `sel` (a field on `parent_type`) has a policy-gated field
/// somewhere in its sub-selection (depth ≥ 1).
fn selection_field_has_gated_descendant(
    schema: &CompiledSchema,
    parent_type: &str,
    sel: &FieldSelection,
) -> bool {
    if sel.nested_fields.is_empty() {
        return false;
    }
    let Some(parent_def) = schema.find_type(parent_type) else {
        return false;
    };
    let Some(field_def) = parent_def.fields.iter().find(|f| f.name.as_str() == sel.name) else {
        return false;
    };
    let Some(child_type) = object_type_of(&field_def.field_type) else {
        return false;
    };
    let Some(child_def) = schema.find_type(child_type) else {
        return false;
    };
    effective_selections(&sel.nested_fields, child_type, schema).iter().any(|child_sel| {
        child_def
            .fields
            .iter()
            .any(|f| f.name.as_str() == child_sel.name && f.authorize)
            || selection_field_has_gated_descendant(schema, child_type, child_sel)
    })
}

/// Returns `true` if a policy-gated field is selected **below** the given fields
/// (nested inside an object sub-selection). The current enforcement covers only the
/// top-level entity row; the caller fail-closes when this is `true`.
pub(crate) fn selection_set_has_nested_gated_field(
    schema: &CompiledSchema,
    type_name: &str,
    fields: &[FieldSelection],
) -> bool {
    effective_selections(fields, type_name, schema)
        .iter()
        .any(|sel| selection_field_has_gated_descendant(schema, type_name, sel))
}

/// Collect the top-level policy-gated fields selected on `type_name`, paired with
/// their alias and GraphQL arguments.
pub(crate) fn collect_top_level_gated_fields(
    schema: &CompiledSchema,
    type_name: &str,
    fields: &[FieldSelection],
) -> Vec<GatedField> {
    let Some(type_def) = schema.find_type(type_name) else {
        return Vec::new();
    };
    effective_selections(fields, type_name, schema)
        .into_iter()
        .filter(|sel| type_def.fields.iter().any(|f| f.name.as_str() == sel.name && f.authorize))
        .map(|sel| GatedField {
            field_name: sel.name.clone(),
            alias:      sel.alias.clone(),
            arguments:  field_arguments_json(sel),
        })
        .collect()
}

/// Fail-closed guard for a projection path that does not run the field authorizer.
///
/// Returns [`FraiseQLError::Authorization`] (403) when a policy-gated field is
/// selected on `type_name` (top-level or nested). `path` names the unsupported path
/// in the error message. Used by the anonymous-query and REST projection paths in
/// the current version; full per-row enforcement on those paths is a tracked
/// follow-up.
///
/// # Errors
///
/// Returns [`FraiseQLError::Authorization`] if a gated field is selected.
pub(crate) fn deny_if_gated_field_selected(
    schema: &CompiledSchema,
    type_name: &str,
    fields: &[FieldSelection],
    path: &str,
) -> Result<()> {
    if selection_set_selects_gated_field(schema, type_name, fields) {
        return Err(FraiseQLError::Authorization {
            message:  format!(
                "Field-level authorization is not enforced on the {path} path; a policy-gated \
                 field on type '{type_name}' was selected"
            ),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        });
    }
    Ok(())
}

/// Fail-closed guard for a path whose result type is dynamic/unknown at the call site
/// (Relay `node`, federation `_entities`). Returns [`FraiseQLError::Authorization`]
/// (403) when the schema declares **any** policy-gated field, since the path cannot
/// yet enforce the dynamic decision. Conservative by design (#423).
///
/// # Errors
///
/// Returns [`FraiseQLError::Authorization`] if the schema has any gated field.
#[cfg(feature = "federation")]
pub(crate) fn deny_if_schema_has_gated_field(schema: &CompiledSchema, path: &str) -> Result<()> {
    if schema.has_any_authorize_field() {
        return Err(FraiseQLError::Authorization {
            message:  format!(
                "Field-level authorization is not enforced on the {path} path, but the schema \
                 declares policy-gated fields"
            ),
            action:   Some("read".to_string()),
            resource: None,
        });
    }
    Ok(())
}

/// The fail-closed deny error: a generic 403 that never echoes the parent row or the
/// underlying policy error (avoids leaking why access was denied).
fn field_authz_error(type_name: &str, field: &str, code: &str) -> FraiseQLError {
    FraiseQLError::Authorization {
        message:  format!("Access denied to field '{field}' on type '{type_name}' [{code}]"),
        action:   Some("read".to_string()),
        resource: Some(format!("{type_name}.{field}")),
    }
}

/// Apply the configured [`FieldAuthorizer`] to the projected result of a regular
/// query, per row. `results` are the **full** fetched rows (the `parent` context);
/// `projected` is the response value, mutated in place (a `Mask` decision nulls the
/// field on that row). `statically_masked` are fields the static `requires_scope`
/// gate already denied — skipped here (AND-composition: already denied).
///
/// Fail-closed: a `Reject` decision or any policy `Err` returns
/// [`FraiseQLError::Authorization`] (403) and the value is never served.
///
/// # Errors
///
/// Returns [`FraiseQLError::Authorization`] on any `Reject` decision or policy error.
pub(crate) fn apply_field_authorizer(
    pass: &FieldAuthzPass<'_>,
    results: &[JsonbValue],
    projected: &mut JsonValue,
    returns_list: bool,
) -> Result<()> {
    if pass.gated.is_empty() {
        return Ok(());
    }
    match projected {
        JsonValue::Array(rows) if returns_list => {
            for (i, row) in rows.iter_mut().enumerate() {
                let parent = results.get(i).map(JsonbValue::as_value);
                enforce_row(pass, parent, row)?;
            }
        },
        JsonValue::Object(_) if !returns_list => {
            let parent = results.first().map(JsonbValue::as_value);
            enforce_row(pass, parent, projected)?;
        },
        _ => {},
    }
    Ok(())
}

/// Apply the configured [`FieldAuthorizer`] to a single already-projected entity
/// object (the mutation path), using `parent` as the full entity for the decision.
/// `projected` is mutated in place (a `Mask` decision nulls the field).
///
/// Fail-closed: a `Reject` decision or any policy `Err` returns
/// [`FraiseQLError::Authorization`] (403).
///
/// # Errors
///
/// Returns [`FraiseQLError::Authorization`] on any `Reject` decision or policy error.
pub(crate) fn apply_field_authorizer_to_entity(
    pass: &FieldAuthzPass<'_>,
    parent: &JsonValue,
    projected: &mut JsonValue,
) -> Result<()> {
    if pass.gated.is_empty() {
        return Ok(());
    }
    enforce_row(pass, Some(parent), projected)
}

/// The per-query context of a field-authorization pass: who is asking, on which type,
/// which fields are gated, and which the static gate already denied. Grouped so the
/// per-row enforcement carries one context instead of many parameters.
pub(crate) struct FieldAuthzPass<'a> {
    /// The configured authorizer.
    pub(crate) authorizer:        &'a dyn FieldAuthorizer,
    /// The authenticated principal.
    pub(crate) principal:         &'a SecurityContext,
    /// The GraphQL type that owns the rows.
    pub(crate) type_name:         &'a str,
    /// The selected, policy-gated fields to enforce.
    pub(crate) gated:             &'a [GatedField],
    /// Fields the static `requires_scope` gate already denied — skipped (AND-composition).
    pub(crate) statically_masked: &'a [String],
}

/// Enforce the gated fields on a single projected row object.
fn enforce_row(
    pass: &FieldAuthzPass<'_>,
    parent: Option<&JsonValue>,
    projected_row: &mut JsonValue,
) -> Result<()> {
    let JsonValue::Object(map) = projected_row else {
        return Ok(());
    };
    let FieldAuthzPass {
        authorizer,
        principal,
        type_name,
        gated,
        statically_masked,
    } = *pass;
    for gf in gated {
        // Resolve the projected key: the field name (query path) or the alias
        // (mutation path applies response keys). Absent → not in this row, nothing to gate.
        let projected_key = if map.contains_key(&gf.field_name) {
            gf.field_name.clone()
        } else if let Some(alias) = gf.alias.as_ref().filter(|a| map.contains_key(a.as_str())) {
            alias.clone()
        } else {
            continue;
        };
        // AND-composition: the static gate already masked this field → already denied.
        if statically_masked.iter().any(|m| m == &gf.field_name) {
            continue;
        }
        let req = FieldAuthzRequest {
            principal,
            type_name,
            field_name: &gf.field_name,
            parent,
            arguments: gf.arguments.as_ref(),
        };
        match authorizer.authorize_field(&req) {
            Ok(FieldAuthzDecision::Allow) => {},
            Ok(FieldAuthzDecision::Deny {
                on_deny: FieldDenyPolicy::Mask,
                ..
            }) => {
                map.insert(projected_key, JsonValue::Null);
            },
            Ok(FieldAuthzDecision::Deny {
                code,
                on_deny: FieldDenyPolicy::Reject,
            }) => {
                return Err(field_authz_error(type_name, &gf.field_name, &code));
            },
            Err(_) => {
                // Fail-closed: any policy error is a hard deny. The underlying error is
                // not surfaced to the client (no information leak).
                return Err(field_authz_error(
                    type_name,
                    &gf.field_name,
                    "field_authorization_failed",
                ));
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
