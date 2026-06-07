//! Dynamic, decision-returning operation-level authorization.
//!
//! Where [`requires_role`](crate::schema::QueryDefinition) answers the *static*
//! question "does this principal hold role X?", an [`Authorizer`] answers the
//! *dynamic* question "may **this** principal run **this** operation, given its
//! input?". It is the operation-level analogue of the
//! [`FieldAuthorizer`](crate::security::FieldAuthorizer) and the counterpart of the
//! [`RLSPolicy`](crate::security::RLSPolicy) plugin: a Policy Enforcement Point
//! where the engine *enforces* but the *decision* is delegated to an app-supplied
//! trait object (in-process rules, a DB query, or an external service).
//!
//! # Semantics
//!
//! - **Fail-closed**: any `Err` returned by [`Authorizer::authorize`] is treated as a hard deny —
//!   the request fails with [`FraiseQLError::Authorization`] (HTTP 403 / `FORBIDDEN`). The
//!   underlying error is *not* surfaced to the client (no information leak).
//! - **Anonymous requests**: [`AuthzRequest::principal`] is `None` on the unauthenticated entry
//!   path. The authorizer is still consulted, so an app may explicitly allow public operations or
//!   deny everything anonymous — the decision is the app's, not the engine's.
//! - **AND-composition**: the decision composes with the static `requires_role` gate as a logical
//!   AND — an operation runs only if *both* the static gate and the authorizer allow it. The
//!   `requires_role` gate keeps its enumeration-hiding "not found in schema" response; the
//!   authorizer denies with an explicit 403.
//!
//! # Wiring
//!
//! Register an implementation on [`RuntimeConfig`](crate::runtime::RuntimeConfig) via
//! [`with_authorizer`](crate::runtime::RuntimeConfig::with_authorizer), exactly parallel to
//! [`with_field_authorizer`](crate::runtime::RuntimeConfig::with_field_authorizer) and
//! [`with_rls_policy`](crate::runtime::RuntimeConfig::with_rls_policy).

use crate::{
    error::{FraiseQLError, Result},
    security::SecurityContext,
};

/// The kind of GraphQL operation being authorized.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    /// A read operation (regular query, aggregate, window, node lookup, federation
    /// entity resolution, or introspection).
    Query,
    /// A write operation (GraphQL mutation, or a REST write mapped to one).
    Mutation,
    /// A subscription operation (authorized once at establishment).
    Subscription,
}

impl OperationKind {
    /// A lowercase, stable string label for this kind (used in the deny error's `action`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            OperationKind::Query => "query",
            OperationKind::Mutation => "mutation",
            OperationKind::Subscription => "subscription",
        }
    }
}

/// An operation-level authorization request handed to an [`Authorizer`].
///
/// Carries the principal (or `None` for an anonymous request), the operation kind
/// and root field name, and the request input — the inputs a static role check lacks.
#[non_exhaustive]
pub struct AuthzRequest<'a> {
    /// The authenticated principal, or `None` for an unauthenticated (anonymous) request.
    pub principal: Option<&'a SecurityContext>,
    /// The kind of operation (query / mutation / subscription).
    pub operation: OperationKind,
    /// The root operation field name (e.g. `"users"`, `"createUser"`, `"_entities"`,
    /// `"__schema"`).
    pub name:      &'a str,
    /// The request input — GraphQL variables or REST arguments — when present.
    pub input:     Option<&'a serde_json::Value>,
}

/// The decision an [`Authorizer`] returns for a single operation.
#[non_exhaustive]
pub enum AuthzDecision {
    /// Allow the operation to execute.
    Allow,
    /// Deny the operation. The `reason` is folded into the
    /// [`FraiseQLError::Authorization`] message (HTTP 403 / `FORBIDDEN`).
    Deny {
        /// A domain-specific, client-facing denial reason (e.g. `"insufficient tier"`).
        reason: String,
    },
}

/// A pluggable, decision-returning operation-level authorizer.
///
/// Implementations decide, per principal / per operation / per input, whether an
/// operation may execute. The engine enforces the decision; this trait supplies it.
/// Implementations must be `Send + Sync` to be shared across the async execution path.
///
/// # Example
///
/// ```
/// use fraiseql_core::security::{Authorizer, AuthzRequest, AuthzDecision, OperationKind};
/// use fraiseql_core::error::Result;
///
/// /// Allow reads for everyone; require an authenticated principal for writes.
/// struct WritesNeedAuth;
///
/// impl Authorizer for WritesNeedAuth {
///     fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
///         match req.operation {
///             OperationKind::Query => Ok(AuthzDecision::Allow),
///             OperationKind::Mutation | OperationKind::Subscription => {
///                 if req.principal.is_some() {
///                     Ok(AuthzDecision::Allow)
///                 } else {
///                     Ok(AuthzDecision::Deny { reason: "authentication required".to_string() })
///                 }
///             }
///         }
///     }
/// }
/// ```
pub trait Authorizer: Send + Sync {
    /// Decide whether the principal may run the requested operation.
    ///
    /// # Errors
    ///
    /// Any `Err` is treated as a **hard deny** (fail-closed): the request fails with
    /// [`FraiseQLError::Authorization`] (HTTP 403 / `FORBIDDEN`) and the underlying
    /// error is not surfaced to the client. Return [`AuthzDecision::Deny`] for an
    /// ordinary, expected denial; reserve `Err` for policy-evaluation failures (e.g.
    /// an unreachable policy backend).
    fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision>;
}

/// The fail-closed deny error: a generic 403 that never echoes the underlying policy
/// error (avoids leaking why, beyond the app-supplied `reason`).
fn authz_deny_error(op: OperationKind, name: &str, reason: &str) -> FraiseQLError {
    FraiseQLError::Authorization {
        message:  format!("Operation '{name}' denied: {reason}"),
        action:   Some(op.as_str().to_string()),
        resource: Some(name.to_string()),
    }
}

/// Run the configured [`Authorizer`] over one or more root operations, fail-closed.
///
/// A multi-root query yields one call per root. Any [`AuthzDecision::Deny`] or any
/// `Err` returns [`FraiseQLError::Authorization`] (403) and the operation never
/// executes. A `Deny`'s `reason` is folded into the message; a policy `Err` is not
/// surfaced (no information leak).
///
/// This is the canonical enforcement entry point. It is `pub` so transports that do
/// not route through the core executor (e.g. the `WebSocket` subscription handler in
/// `fraiseql-server`) can enforce the same fail-closed contract without reconstructing
/// the (`#[non_exhaustive]`) [`AuthzRequest`] themselves.
///
/// # Errors
///
/// Returns [`FraiseQLError::Authorization`] on the first `Deny` decision or policy error.
pub fn enforce_authz(
    authorizer: &dyn Authorizer,
    principal: Option<&SecurityContext>,
    operations: &[(OperationKind, String)],
    input: Option<&serde_json::Value>,
) -> Result<()> {
    for (op, name) in operations {
        let req = AuthzRequest {
            principal,
            operation: *op,
            name,
            input,
        };
        match authorizer.authorize(&req) {
            Ok(AuthzDecision::Allow) => {},
            Ok(AuthzDecision::Deny { reason }) => return Err(authz_deny_error(*op, name, &reason)),
            Err(_) => {
                // Fail-closed: any policy error is a hard deny. The underlying error is
                // not surfaced to the client (no information leak).
                return Err(authz_deny_error(*op, name, "authorization failed"));
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
