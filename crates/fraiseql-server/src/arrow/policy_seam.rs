//! #954 — the policy seam the Flight GraphQL path executes through.
//!
//! The Flight `do_get` GraphQL path and the `do_exchange` `Query` arm both run
//! through a [`QueryExecutor`]. Attaching a **bare** executor — which
//! [`ExecutorQueryAdapter`](super::ExecutorQueryAdapter) is — would make Flight the
//! one transport that skips tenant resolution, the suspended-tenant gate, per-tenant
//! concurrency/RPS/cost quotas and trusted documents. Those are not executor-level
//! concerns: they live in `AppState`, which the executor does not have.
//!
//! [`PolicyGatedExecutor`] is what the server mounts instead. It runs the same
//! chokepoint sequence the HTTP handler runs, in the same order, from the same
//! functions — trusted documents → tenant resolution → dispatch (suspension +
//! quotas) → cost charge → execute — so a policy that changes for HTTP changes for
//! Flight, rather than being reimplemented here and drifting.
//!
//! The executor-level floors (#379 GATE-1 depth/complexity from `[validation]`, and
//! `[security.cost_budget] per_request_max`) still apply beneath this; they are the
//! floor, not the policy.

use std::sync::Arc;

use async_trait::async_trait;
use fraiseql_arrow::QueryExecutor;
use fraiseql_core::{db::traits::DatabaseAdapter, security::SecurityContext};
use http::HeaderMap;

use crate::routes::graphql::{app_state::AppState, tenant_dispatch};

/// A [`QueryExecutor`] that enforces the transport-independent policy before it
/// executes anything.
pub struct PolicyGatedExecutor<A: DatabaseAdapter> {
    /// The full request-handling state: tenant registry, trusted-document store,
    /// per-tenant executors and quotas.
    state: AppState<A>,
}

impl<A: DatabaseAdapter> PolicyGatedExecutor<A> {
    /// Wrap `state` as the Flight service's executor.
    #[must_use]
    pub const fn new(state: AppState<A>) -> Self {
        Self { state }
    }
}

// Reason: QueryExecutor is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> QueryExecutor for PolicyGatedExecutor<A> {
    /// # Errors
    ///
    /// Returns the refusal as a `String` — the Flight protocol's error channel — for
    /// a document the trusted-document store forbids, a tenant that cannot be
    /// resolved, one that is suspended or unregistered, an exhausted quota, or an
    /// over-budget request. Only a request that passes all of them is executed.
    async fn execute_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<serde_json::Value, String> {
        // 1. Trusted documents / `persisted_queries_only`. Flight carries no `documentId`, so every
        //    Flight GraphQL request is ad-hoc text by construction — which is exactly what a store
        //    in strict mode must refuse. Passing `None` as the document ID is not a shortcut: it is
        //    the honest statement that this transport has no persisted-document channel.
        if let Some(ref store) = self.state.trusted_docs {
            store.resolve(None, Some(query)).map_err(|e| match e {
                crate::trusted_documents::TrustedDocumentError::ForbiddenRawQuery => {
                    crate::trusted_documents::record_rejected();
                    "Ad-hoc GraphQL documents are forbidden (persisted queries only). The Arrow \
                     Flight transport cannot supply a persisted document ID."
                        .to_string()
                },
                other => format!("Trusted document rejection: {other}"),
            })?;
        }

        // 2. Tenant resolution, through the seam MCP and HTTP use (#858). Flight has no HTTP
        //    headers at this point, so resolution rests on the session's `SecurityContext` alone;
        //    when the schema declares RLS the resolver is strict and an unresolvable tenant is
        //    refused rather than defaulted.
        let tenant_key = tenant_dispatch::resolve_tenant_key(
            &self.state,
            Some(security_context),
            &HeaderMap::new(),
        )
        .map_err(|e| format!("Tenant resolution failed: {e}"))?;

        // 3. Dispatch: the suspended-tenant gate and per-tenant concurrency/RPS quotas. `dispatch`
        //    holds the concurrency permit for the rest of this call and releases it on drop. An
        //    unregistered key errors here — never a silent fallback to the default executor, which
        //    would serve another tenant's data.
        let dispatch = tenant_dispatch::dispatch_to_tenant(&self.state, tenant_key.as_deref())
            .map_err(|e| format!("Tenant dispatch refused: {e}"))?;
        let executor = &dispatch.executor;

        // 4. Per-tenant cost budget, charged at the same chokepoint as the other quotas and from
        //    the same estimate.
        let estimated_cost = tenant_dispatch::estimate_request_cost(query, variables, executor);
        tenant_dispatch::charge_cost_budget(&self.state, tenant_key.as_deref(), estimated_cost)
            .map_err(|e| format!("Cost budget refused: {e}"))?;

        // 5. Execute on the *tenant's* executor, not the default one.
        executor
            .execute_with_security(query, variables, security_context)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Build the Flight service's policy-gated executor from a server's state.
///
/// This is the wiring #954 exists for: the shipped binary attaches **this**, and
/// never a bare `ExecutorQueryAdapter`.
pub(crate) fn policy_gated_executor<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: AppState<A>,
) -> Arc<dyn QueryExecutor> {
    Arc::new(PolicyGatedExecutor::new(state))
}
