//! Admission: who a request is, which tenant it runs as, and what that tenant lets it
//! spend, decided by one set of steps every transport runs.
//!
//! The steps run in this order, and each lives only here:
//!
//! 1. [`identify`]: the credential the auth layer does not resolve (a static API key), the
//!    request's trace context, and identity enrichment.
//! 2. [`Identified::tenant`]: the tenant key, from the principal and the request's hints.
//! 3. [`Tenant::dispatch`]: the tenant's executor. An unregistered or suspended tenant is refused,
//!    and the tenant's concurrency and per-second quotas are charged.
//! 4. [`Tenant::charge`]: the tenant's cost budget, from the operation's estimated cost.
//!
//! A step takes what the one before it produced, so a transport cannot reach the tenant
//! without having identified the request. It can run its own steps between them: `/graphql`
//! resolves and validates its document, and replays an idempotent mutation, before it
//! dispatches. A transport only maps a [`Refusal`] to its wire. Where transports differ, the
//! difference is a field of their [`AdmissionPolicy`], not a copy of a step.

use axum::http::HeaderMap;
use fraiseql_core::security::SecurityContext;
use fraiseql_error::FraiseQLError;

use crate::{
    routes::graphql::{
        app_state::AppState,
        tenant_dispatch::{self, TenantDispatch},
    },
    tracing_utils,
};

/// What a transport's admission runs beyond the steps every transport runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionPolicy {
    /// Authenticate a static API key when the auth layer resolved no principal.
    pub api_key:     bool,
    /// Stamp the request's W3C trace context onto the principal.
    pub stamp_trace: bool,
    /// Resolve the subject's identity before the request goes further.
    pub enrich:      bool,
}

impl AdmissionPolicy {
    /// A submission to `/operations/v1/{operation}`.
    pub const ASYNC_OPERATIONS: Self = Self {
        api_key:     false,
        stamp_trace: false,
        enrich:      true,
    };
    /// A read or a cancellation of a submitted operation, which looks it up for its
    /// submitter and executes nothing.
    pub const ASYNC_OPERATION_LOOKUP: Self = Self {
        api_key:     false,
        stamp_trace: false,
        enrich:      false,
    };
    /// `/graphql` over `POST`, `GET` and `QUERY`, and its SSE stream.
    pub const GRAPHQL: Self = Self {
        api_key:     true,
        stamp_trace: true,
        enrich:      true,
    };
}

/// Why admission refused a request. A transport maps it to its own wire; the refusal
/// itself is the same whichever door the request came through.
#[derive(Debug)]
pub enum Refusal {
    /// A static API key was presented, and it is not valid.
    InvalidApiKey,
    /// Identity enrichment denied the subject.
    IdentityDenied,
    /// Identity enrichment could not reach its source.
    IdentityUnavailable,
    /// The tenant could not be resolved: a malformed hint, or one that disagrees with
    /// the principal's tenant.
    Tenant(FraiseQLError),
    /// The tenant registry refused the request: an unregistered or suspended tenant, or
    /// an exhausted quota.
    Dispatch(FraiseQLError),
    /// The operation's estimated cost exceeds the tenant's budget.
    Cost(FraiseQLError),
}

/// A request whose principal is known: authenticated, stamped and enriched.
#[derive(Debug)]
pub struct Identified {
    security_context: Option<SecurityContext>,
}

impl Identified {
    /// The principal, or `None` for an anonymous request.
    #[must_use]
    pub const fn security_context(&self) -> Option<&SecurityContext> {
        self.security_context.as_ref()
    }

    /// The principal, for the execution that runs as it.
    #[must_use]
    pub fn into_security_context(self) -> Option<SecurityContext> {
        self.security_context
    }

    /// Resolve the tenant the request runs as: the principal's for an authenticated
    /// caller, the request's hints otherwise. Strict cross-source validation is on exactly
    /// when the compiled schema configures RLS.
    ///
    /// # Errors
    ///
    /// [`Refusal::Tenant`] for a malformed hint, or hints that disagree under strict
    /// validation.
    pub fn tenant(&self, state: &AppState, headers: &HeaderMap) -> Result<Tenant, Refusal> {
        tenant_dispatch::resolve_tenant_key(state, self.security_context(), headers)
            .map(|key| Tenant { key })
            .map_err(Refusal::Tenant)
    }
}

/// The tenant a request runs as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tenant {
    key: Option<String>,
}

impl Tenant {
    /// The tenant's key, or `None` for the default executor.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// The tenant's executor, holding its concurrency permit for as long as the returned
    /// value lives.
    ///
    /// # Errors
    ///
    /// [`Refusal::Dispatch`] for an unregistered key, a suspended tenant or an exhausted
    /// quota.
    pub fn dispatch(&self, state: &AppState) -> Result<TenantDispatch, Refusal> {
        tenant_dispatch::dispatch_to_tenant(state, self.key()).map_err(Refusal::Dispatch)
    }

    /// Charge the tenant's cost budget with the operation's estimated `cost`, keyed on the
    /// principal's actor class. `None` is a document that does not parse, which the
    /// executor refuses.
    ///
    /// # Errors
    ///
    /// [`Refusal::Cost`] when `cost` exceeds the tenant's per-request budget or exhausts
    /// its window.
    pub fn charge(
        &self,
        state: &AppState,
        security_context: Option<&SecurityContext>,
        cost: Option<u64>,
    ) -> Result<(), Refusal> {
        tenant_dispatch::charge_cost_budget(state, self.key(), security_context, cost)
            .map_err(Refusal::Cost)
    }
}

/// Identify a request from the principal the auth layer resolved: authenticate a static
/// API key when it resolved none, stamp the trace context, and enrich the identity, each
/// when `policy` says to.
///
/// The auth layer has already authenticated a JWT or a service account, and refused a
/// bearer presented beside a service-account secret as ambiguous. A service account's
/// `run_as` ceiling takes precedence over a static API key on the same header (ADR-0018),
/// so the key is tried only for a request the layer resolved to no one.
///
/// # Errors
///
/// [`Refusal::InvalidApiKey`], [`Refusal::IdentityDenied`] or
/// [`Refusal::IdentityUnavailable`].
pub async fn identify(
    state: &AppState,
    headers: &HeaderMap,
    security_context: Option<SecurityContext>,
    policy: &AdmissionPolicy,
) -> Result<Identified, Refusal> {
    let mut security_context = security_context;
    if policy.api_key && security_context.is_none() {
        security_context = authenticate_api_key(state, headers).await?;
    }
    if policy.stamp_trace {
        security_context = stamp_trace_context(headers, security_context);
    }
    #[cfg(feature = "auth")]
    if policy.enrich {
        enrich_identity(state, security_context.as_mut()).await?;
    }
    Ok(Identified { security_context })
}

/// The principal a static API key on `headers` names, or `None` when none is presented.
async fn authenticate_api_key(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<SecurityContext>, Refusal> {
    let Some(ref authenticator) = state.api_key_authenticator else {
        return Ok(None);
    };
    match authenticator.authenticate(headers).await {
        crate::api_key::ApiKeyResult::Authenticated(ctx) => {
            tracing::debug!("Authenticated via API key");
            Ok(Some(*ctx))
        },
        crate::api_key::ApiKeyResult::Invalid => Err(Refusal::InvalidApiKey),
        crate::api_key::ApiKeyResult::NotPresent => Ok(None),
    }
}

/// Stamp the request's W3C trace context onto the principal.
///
/// The change-log write records both the scalar `trace_id` and the full parsed
/// `trace_context` (#375), which is what makes a row traceable to the request that wrote
/// it. An anonymous request has no principal to stamp, so its change-log rows keep
/// `trace_id` NULL, as they keep `tenant_id`.
fn stamp_trace_context(
    headers: &HeaderMap,
    security_context: Option<SecurityContext>,
) -> Option<SecurityContext> {
    let mut security_context = security_context;
    if let Some(trace_id) = tracing_utils::extract_trace_id(headers) {
        security_context = security_context.map(|ctx| ctx.with_trace_id(trace_id));
    }
    if let Some(trace_context) = tracing_utils::extract_trace_context_json(headers) {
        security_context = security_context.map(|ctx| ctx.with_trace_context(trace_context));
    }
    security_context
}

/// Resolve the subject's database identity and merge it under the forge-proof
/// `fraiseql.enriched.*` namespace (#539), before the request is dispatched, so RLS, views
/// and injected parameters scope on a database-derived identity. An anonymous request has
/// no subject and proceeds unchanged.
#[cfg(feature = "auth")]
async fn enrich_identity(
    state: &AppState,
    security_context: Option<&mut SecurityContext>,
) -> Result<(), Refusal> {
    match crate::identity::resolve_request_identity(&state.identity, security_context).await {
        crate::identity::EnrichmentOutcome::Proceed => Ok(()),
        crate::identity::EnrichmentOutcome::Denied => Err(Refusal::IdentityDenied),
        crate::identity::EnrichmentOutcome::Unavailable => Err(Refusal::IdentityUnavailable),
    }
}

#[cfg(test)]
mod tests;
