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
//! - **Every level, by default**: a read that reaches a nested type — a GraphQL selection `users {
//!   orders { … } }`, a REST embed `?select=orders(…)` — puts that level to the authorizer too,
//!   once per request per path (never per row), with [`AuthzRequest::nesting`] set. The rule the
//!   developer wrote for reading `Order` holds wherever `Order` is read. An authorizer that means
//!   to gate operations only allows every request whose `nesting` is `Some`.
//!
//! # Matching a type or a name
//!
//! [`AuthzRequest::target_type`] is the type a request reads (or writes), on every call:
//! root and nested, GraphQL and REST. Match on it to hold every read of `Order`, whatever
//! the transport or the entry point. [`AuthzRequest::name`] is the root field at the
//! root. At a nested level it is the type's **canonical list query**: its first declared
//! SQL-backed list query, the spelling REST has always passed, or the type name when it
//! has none. A rule on `name` therefore catches a nested read only through that one query.
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

/// Where a nested level sits: the type it is reached through, and the path to it.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzNesting {
    /// The type whose field reaches this level (`"User"` for `users { orders }`).
    pub parent_type: String,
    /// The field names from the root field to this level, dot-separated: `"orders"` for
    /// `users { orders }`, `"orders.items"` one level further. REST names relationships.
    pub path:        String,
}

impl AuthzNesting {
    /// A nested level reached through `parent_type` at `path`.
    #[must_use]
    pub fn new(parent_type: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            parent_type: parent_type.into(),
            path:        path.into(),
        }
    }
}

/// One operation or level to put to the [`Authorizer`] — what [`enforce_authz`] turns
/// into an [`AuthzRequest`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzOperation {
    /// [`AuthzRequest::operation`].
    pub kind:        OperationKind,
    /// [`AuthzRequest::name`].
    pub name:        String,
    /// [`AuthzRequest::target_type`].
    pub target_type: Option<String>,
    /// [`AuthzRequest::nesting`].
    pub nesting:     Option<AuthzNesting>,
}

impl AuthzOperation {
    /// A root operation: `name` is the root field, `target_type` the type it returns
    /// (`None` where it names no single type — introspection, `node`, `_entities`).
    #[must_use]
    pub fn root(kind: OperationKind, name: impl Into<String>, target_type: Option<&str>) -> Self {
        Self {
            kind,
            name: name.into(),
            target_type: target_type.map(str::to_string),
            nesting: None,
        }
    }

    /// A nested read of `target_type`, named by its canonical list query (or the type).
    #[must_use]
    pub fn nested(name: impl Into<String>, target_type: &str, nesting: AuthzNesting) -> Self {
        Self {
            kind:        OperationKind::Query,
            name:        name.into(),
            target_type: Some(target_type.to_string()),
            nesting:     Some(nesting),
        }
    }
}

/// An operation-level authorization request handed to an [`Authorizer`].
///
/// Carries the principal (or `None` for an anonymous request), the operation kind
/// and root field name, the type read, where a nested level sits, and the request
/// input — the inputs a static role check lacks.
#[non_exhaustive]
pub struct AuthzRequest<'a> {
    /// The authenticated principal, or `None` for an unauthenticated (anonymous) request.
    pub principal:   Option<&'a SecurityContext>,
    /// The kind of operation (query / mutation / subscription).
    pub operation:   OperationKind,
    /// The root operation field name (e.g. `"users"`, `"createUser"`, `"_entities"`,
    /// `"__schema"`).
    pub name:        &'a str,
    /// The request input — GraphQL variables or REST arguments — when present.
    pub input:       Option<&'a serde_json::Value>,
    /// The type this request reads or writes: the root field's return type at the root, the
    /// level's type at a nested level. `None` only where the root names no single type
    /// (introspection, `node` and `_entities` before their type is known — each is asked
    /// again with it once it is).
    pub target_type: Option<&'a str>,
    /// `None` at the root; where a nested level sits otherwise.
    pub nesting:     Option<&'a AuthzNesting>,
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
///         // Reads are public; writes (and any future operation kind) need a principal.
///         // `OperationKind` is `#[non_exhaustive]`, so avoid an exhaustive match here.
///         if matches!(req.operation, OperationKind::Query) || req.principal.is_some() {
///             Ok(AuthzDecision::Allow)
///         } else {
///             Ok(AuthzDecision::Deny { reason: "authentication required".to_string() })
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
fn authz_deny_error(op: &AuthzOperation, reason: &str) -> FraiseQLError {
    let message = match (&op.nesting, &op.target_type) {
        (Some(nesting), Some(target)) => format!(
            "Read of '{target}' at '{}.{}' denied: {reason}",
            nesting.parent_type, nesting.path
        ),
        _ => format!("Operation '{}' denied: {reason}", op.name),
    };
    FraiseQLError::Authorization {
        message,
        action: Some(op.kind.as_str().to_string()),
        resource: Some(op.name.clone()),
    }
}

/// The answer when a policy backend fails to decide: fail closed, and say so honestly.
///
/// A policy `Err` is not a refusal: nobody adjudicated the caller, the backend could not
/// be reached. Reporting it as 403 told operators an outage was a permissions problem
/// and told clients not to retry exactly when they should (#1374). It is a 503 with no
/// detail — the policy's own error is never surfaced (no information leak) — and the
/// operation never executes, exactly as for a deny. Shared by the operation and the
/// field authorizer, so the transports cannot disagree.
#[must_use]
pub fn policy_unavailable() -> FraiseQLError {
    FraiseQLError::ServiceUnavailable {
        message:     "authorization policy unavailable".to_string(),
        retry_after: None,
    }
}

/// Run the configured [`Authorizer`] over one or more operations or levels, fail-closed.
///
/// A multi-root query yields one call per root; a nested level, one call of its own. Any
/// [`AuthzDecision::Deny`] returns [`FraiseQLError::Authorization`] (403), with the `Deny`'s
/// `reason` folded into the message. A policy `Err` returns [`policy_unavailable`] (503,
/// the error itself not surfaced). Either way the operation never executes.
///
/// This is the canonical enforcement entry point. It is `pub` so transports that do
/// not route through the core executor (e.g. the `WebSocket` subscription handler in
/// `fraiseql-server`) can enforce the same fail-closed contract without reconstructing
/// the (`#[non_exhaustive]`) [`AuthzRequest`] themselves.
///
/// # Errors
///
/// Returns [`FraiseQLError::Authorization`] on the first `Deny` decision, and
/// [`FraiseQLError::ServiceUnavailable`] on the first policy error.
pub fn enforce_authz(
    authorizer: &dyn Authorizer,
    principal: Option<&SecurityContext>,
    operations: &[AuthzOperation],
    input: Option<&serde_json::Value>,
) -> Result<()> {
    for op in operations {
        let req = AuthzRequest {
            principal,
            operation: op.kind,
            name: &op.name,
            input,
            target_type: op.target_type.as_deref(),
            nesting: op.nesting.as_ref(),
        };
        match authorizer.authorize(&req) {
            Ok(AuthzDecision::Allow) => {},
            Ok(AuthzDecision::Deny { reason }) => return Err(authz_deny_error(op, &reason)),
            Err(_) => return Err(policy_unavailable()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
