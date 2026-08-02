//! Per-tenant executor dispatch, shared by every transport that executes GraphQL.
//!
//! Resolving which tenant a request belongs to, refusing an unregistered key,
//! refusing a suspended tenant and charging the tenant's quotas is one policy. It
//! used to be written out inline in the `/graphql` handler and nowhere else, so
//! the MCP transport — mounted from the same [`AppState`] — captured the default
//! executor at session construction and never consulted the registry at all: an
//! authenticated MCP caller read the boot database rather than their own tenant's,
//! and a suspended tenant kept working over MCP while `/graphql` correctly
//! answered 503 (#858).
//!
//! Both steps live here so a control added to one transport is a control on both.
//! They are two functions rather than one because the caller distinguishes their
//! failures: a malformed `X-Tenant-ID` is the client's mistake (400) while an
//! unregistered or suspended tenant is a dispatch decision (403 / 503 / 429).

use std::sync::Arc;

use axum::http::HeaderMap;
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor, security::SecurityContext};

use super::{AppState, TenantKeyResolver};

/// The executor a request must run on, plus the quota permits it holds.
pub struct TenantDispatch<A: DatabaseAdapter> {
    /// The tenant's executor, or the default one when no key was resolved.
    pub executor: arc_swap::Guard<Arc<Executor<A>>>,

    /// The per-tenant concurrency permit, released when this value is dropped.
    ///
    /// Bound for the remainder of the request: dropping it early would let a
    /// tenant exceed its configured in-flight quota.
    _concurrency_permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

/// Resolve the tenant key for a request from its security context and headers.
///
/// Strict cross-source validation is enabled exactly when the compiled schema
/// configures RLS, so a multi-tenant deployment cannot be addressed with
/// conflicting tenant hints.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` when the `X-Tenant-ID` header is malformed,
/// or when strict validation is on and the available sources disagree.
pub fn resolve_tenant_key<A: DatabaseAdapter>(
    state: &AppState<A>,
    security_context: Option<&SecurityContext>,
    headers: &HeaderMap,
) -> fraiseql_error::Result<Option<String>> {
    let strict = state.executor().schema().has_rls_configured();
    TenantKeyResolver::resolve(security_context, headers, Some(state.domain_registry()), strict)
}

/// Dispatch to the resolved tenant's executor and charge its quotas.
///
/// - `None` key → the default executor, unlimited.
/// - registered + active → the tenant's own executor, holding a concurrency permit and having
///   consumed one request from the per-second window.
/// - registered + suspended → `ServiceUnavailable`.
/// - not registered → `Authorization`. Never a silent fallback to the default executor, which would
///   serve another tenant's data.
///
/// # Errors
///
/// Propagates the registry's decision: `Authorization` for an unregistered key,
/// `ServiceUnavailable` for a suspended tenant, `RateLimited` when a quota is
/// exhausted.
pub fn dispatch_to_tenant<A: DatabaseAdapter>(
    state: &AppState<A>,
    tenant_key: Option<&str>,
) -> fraiseql_error::Result<TenantDispatch<A>> {
    let executor = state.executor_for_tenant(tenant_key)?;

    // M-quotas: enforce the per-tenant concurrency limit. Only an explicit,
    // registered tenant key carries a limit — the default (`None`) executor is
    // unlimited, and the registry errors on an unregistered key. The acquired
    // permit is bound for the remainder of the request and released on drop, so a
    // tenant can never exceed its configured in-flight quota. An exhausted limit
    // surfaces as `RateLimited` → HTTP 429.
    let concurrency_permit = match (tenant_key, state.tenant_registry()) {
        (Some(key), Some(registry)) => registry.try_acquire_concurrency(key)?,
        _ => None,
    };

    // M-quotas (RPS): enforce the per-tenant per-second request-rate limit at the
    // same chokepoint. Like the concurrency permit, this is meaningful only for an
    // explicitly-keyed, registered tenant.
    #[cfg(feature = "auth")]
    if let (Some(key), Some(registry)) = (tenant_key, state.tenant_registry()) {
        registry.try_acquire_rps(key)?;
    }

    Ok(TenantDispatch {
        executor,
        _concurrency_permit: concurrency_permit,
    })
}

/// Charge the tenant's per-operation cost budget for `document` (#379).
///
/// Only an explicitly-keyed, registered tenant carries a budget, and the re-parse
/// plus estimate are skipped entirely unless one is configured. A document that
/// fails to parse here is left for the executor to reject.
///
/// Lives beside the other two because it is the fourth per-tenant quota and
/// belongs to the same chokepoint; it is separate only because it needs the
/// document.
///
/// **Deliberately not called by the MCP transport.** `estimate_query_cost` scores
/// the *shape* of the document — root fields against
/// `operation_cost_weights`, and the selection set — none of which an MCP caller
/// can vary: the document is built from the schema, carries exactly one root
/// field and a fixed scalar projection, and argument values travel as variables
/// (#808). The score for a given tool is therefore constant, so the check would
/// either always pass or permanently disable that tool for a budgeted tenant,
/// rather than metering anything. Volume over MCP is bounded by the concurrency
/// permit and the per-second limiter in [`dispatch_to_tenant`], which do apply.
/// If a future MCP surface lets the caller shape the document, this is the call
/// to add.
///
/// # Errors
///
/// Returns `FraiseQLError::RateLimited` when the estimated cost exceeds the
/// tenant's budget.
pub fn charge_cost_budget<A: DatabaseAdapter>(
    state: &AppState<A>,
    tenant_key: Option<&str>,
    document: &str,
    variables: Option<&serde_json::Value>,
    executor: &Executor<A>,
) -> fraiseql_error::Result<()> {
    let (Some(key), Some(registry)) = (tenant_key, state.tenant_registry()) else {
        return Ok(());
    };
    if !registry.has_cost_budget(key) {
        return Ok(());
    }
    if let Ok(doc) = fraiseql_core::graphql::parse_graphql_document(document) {
        let cost = fraiseql_core::graphql::estimate_query_cost(
            &doc,
            &executor.schema().operation_cost_weights,
            variables,
        );
        registry.check_cost_budget(key, cost)?;
    }
    Ok(())
}
