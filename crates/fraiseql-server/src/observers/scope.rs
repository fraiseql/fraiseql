//! Which observers an admin caller may see and change.
//!
//! The observer admin API sits behind the `fraiseql:admin` scope. Within it, a token that names
//! a tenant is an organisation's administrator and a token that names none is the operator.
//!
//! An organisation manages its own observers (`fk_customer_org` = its id) and sees their
//! executions. It also *sees* global observers (`fk_customer_org IS NULL`) as definitions, but
//! does not change them and does not see their executions: a global observer fires on every
//! tenant's events, so its logs and statistics mix tenants, and changing it changes every
//! tenant's delivery.
//!
//! The change log, the dead-letter queue and the runtime controls span every tenant, so they
//! are operator surfaces: [`OperatorOnly`] refuses any token that names a tenant.

use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use fraiseql_core::security::SecurityContext;

use crate::extractors::OptionalSecurityContext;

/// The observers an admin caller acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverScope {
    /// A token that names no tenant: every organisation's observers.
    Operator,
    /// A token scoped to one organisation, by its `fk_customer_org` id.
    Organisation(i64),
}

/// Why a caller has no [`ObserverScope`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeRefusal {
    /// No authenticated principal reached the handler.
    Unauthenticated,
    /// The token names a tenant the observer tables cannot scope by: `fk_customer_org` is a
    /// `BIGINT`, so a tenant that is not one names no organisation they can express. Reading it
    /// as "no tenant" would make the caller the operator.
    TenantIsNotAnOrganisation,
    /// The token names a tenant, and the endpoint is an operator surface.
    OperatorOnly,
}

impl ObserverScope {
    /// The scope a security context grants.
    ///
    /// # Errors
    ///
    /// Returns [`ScopeRefusal::Unauthenticated`] without a context, and
    /// [`ScopeRefusal::TenantIsNotAnOrganisation`] for a tenant that is not a decimal `i64`.
    pub fn of(context: Option<&SecurityContext>) -> Result<Self, ScopeRefusal> {
        let context = context.ok_or(ScopeRefusal::Unauthenticated)?;
        match context.tenant_id.as_ref() {
            None => Ok(Self::Operator),
            Some(tenant) => tenant
                .as_str()
                .parse::<i64>()
                .map(Self::Organisation)
                .map_err(|_| ScopeRefusal::TenantIsNotAnOrganisation),
        }
    }

    /// The organisation a new observer belongs to: `None` makes it global.
    #[must_use]
    pub const fn organisation(self) -> Option<i64> {
        match self {
            Self::Operator => None,
            Self::Organisation(org) => Some(org),
        }
    }

    /// Restrict `query` to observers this scope can see: its own and the global ones.
    pub(crate) fn push_visible(self, query: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>) {
        if let Self::Organisation(org) = self {
            query
                .push(" AND (fk_customer_org IS NULL OR fk_customer_org = ")
                .push_bind(org)
                .push(")");
        }
    }

    /// Restrict `query` to observers this scope owns, and so may change and see the executions
    /// of. `observer_key` is the column holding the observer's `pk_observer`.
    pub(crate) fn push_owned(
        self,
        query: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
        observer_key: &str,
    ) {
        if let Self::Organisation(org) = self {
            query
                .push(" AND ")
                .push(observer_key)
                .push(" IN (SELECT pk_observer FROM tb_observer WHERE fk_customer_org = ")
                .push_bind(org)
                .push(")");
        }
    }
}

impl IntoResponse for ScopeRefusal {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthenticated => (StatusCode::UNAUTHORIZED, "Authentication required"),
            Self::TenantIsNotAnOrganisation => (
                StatusCode::FORBIDDEN,
                "The observer admin API scopes by a numeric organisation id, and this token's \
                 tenant is not one",
            ),
            Self::OperatorOnly => (
                StatusCode::FORBIDDEN,
                "This endpoint spans every tenant and refuses a token scoped to one",
            ),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

impl<S> FromRequestParts<S> for ObserverScope
where
    S: Send + Sync + 'static,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let OptionalSecurityContext(context) =
            OptionalSecurityContext::from_request_parts(parts, state)
                .await
                .map_err(IntoResponse::into_response)?;
        Self::of(context.as_ref()).map_err(IntoResponse::into_response)
    }
}

/// Admits only the operator: a token that names no tenant.
#[derive(Debug, Clone, Copy)]
pub struct OperatorOnly;

impl<S> FromRequestParts<S> for OperatorOnly
where
    S: Send + Sync + 'static,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let OptionalSecurityContext(context) =
            OptionalSecurityContext::from_request_parts(parts, state)
                .await
                .map_err(IntoResponse::into_response)?;
        let context = context.ok_or_else(|| ScopeRefusal::Unauthenticated.into_response())?;
        if context.tenant_id.is_some() {
            return Err(ScopeRefusal::OperatorOnly.into_response());
        }
        Ok(Self)
    }
}

#[cfg(test)]
mod tests;
