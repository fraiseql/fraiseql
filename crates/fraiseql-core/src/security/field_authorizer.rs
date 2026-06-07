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
//!   hard deny — the request fails with
//!   [`FraiseQLError::Authorization`](crate::error::FraiseQLError::Authorization) (HTTP 403 /
//!   `FORBIDDEN`). The field value is never served on a policy failure.
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

use crate::{error::Result, schema::FieldDenyPolicy, security::SecurityContext};

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
    /// with [`FraiseQLError::Authorization`](crate::error::FraiseQLError::Authorization)
    /// (HTTP 403 / `FORBIDDEN`) and the field value is never served. Return
    /// [`FieldAuthzDecision::Deny`] for an ordinary, expected denial; reserve `Err`
    /// for policy-evaluation failures (e.g. an unreachable policy backend).
    fn authorize_field(&self, req: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision>;
}

#[cfg(test)]
mod tests;
