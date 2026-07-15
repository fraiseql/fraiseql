//! The query-executor bridge for scheduled sources (#573 D6, Phase 06 Step 2).
//!
//! A Model B source's Deno connector issues mutations via `fraiseql_query`, which
//! reaches the host as [`HostContext::query`](fraiseql_functions::HostContext::query)
//! and delegates to a [`QueryExecutor`](fraiseql_functions::host::live::QueryExecutor).
//! Production dispatch never wired one, so a connector's `host.query()` failed with
//! "query executor not configured". [`SourceQueryExecutor`] is that missing bridge:
//! it wraps the server's [`Executor`] and runs each query/mutation under the
//! source's **`run_as` identity** (a `SystemJob` [`SecurityContext`], D6).
//!
//! Two properties matter:
//!
//! - **Hot-reload-safe.** It holds the same `Arc<ArcSwap<Executor<A>>>` the request path holds and
//!   `load`s a fresh snapshot per call, so a schema reload is picked up by the next source firing
//!   rather than pinned at construction.
//! - **Per-message tenant seam.** The identity is not a frozen field. A single `execute_query`
//!   re-scopes to a per-message tenant carried in the reserved [`SOURCE_TENANT_VAR`] variable — the
//!   runtime half of the multi-tenant path (only the connector knows which tenant a fetched record
//!   belongs to). A source already pinned to a tenant by `run_as` ignores the override and cannot
//!   forge writes for another tenant.

use std::{future::Future, pin::Pin, sync::Arc};

use arc_swap::ArcSwap;
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor, security::SecurityContext};
use fraiseql_error::Result;
use fraiseql_functions::host::live::QueryExecutor;
use serde_json::Value;

/// Reserved GraphQL variable a multi-tenant source sets to scope one write to a
/// tenant (#573 D6). It is stripped from the variables before the query runs, so it
/// never reaches the mutation itself. The Phase 07 SDK surfaces it ergonomically
/// (e.g. `ctx.query(mutation, vars, { tenant })`); this constant is the wire
/// contract both sides agree on.
pub const SOURCE_TENANT_VAR: &str = "__source_tenant";

/// Adapts the server's [`Executor`] to the functions
/// [`QueryExecutor`](fraiseql_functions::host::live::QueryExecutor) so a scheduled
/// source's mutations run under its `run_as` identity (#573 D6).
pub struct SourceQueryExecutor<A: DatabaseAdapter> {
    /// The hot-reloadable executor — the exact handle the request path uses, so a
    /// schema swap is reflected on the next firing (loaded per call).
    executor: Arc<ArcSwap<Executor<A>>>,
    /// The source's base `run_as` identity (a `SystemJob` context). A per-message
    /// tenant may re-scope it, but never widen its roles/scopes.
    identity: SecurityContext,
}

impl<A: DatabaseAdapter> SourceQueryExecutor<A> {
    /// Bridge `executor` to a source running under `identity` (its `run_as`
    /// ceiling, built via
    /// [`SourceDefinition::identity`](fraiseql_core::schema::SourceDefinition::identity)).
    #[must_use]
    pub const fn new(executor: Arc<ArcSwap<Executor<A>>>, identity: SecurityContext) -> Self {
        Self { executor, identity }
    }
}

/// The effective identity for one source query: the base `run_as` ceiling,
/// re-scoped to `tenant` **only** when the base is not already pinned to a tenant.
///
/// A multi-tenant source (base tenant unset) scopes each write to the message's
/// tenant; a single-tenant/global source (base tenant set, or no override) runs
/// under its base identity — a pinned source cannot forge writes for a tenant it
/// was not granted. Either way the roles/scopes ceiling is untouched.
fn resolve_identity(base: &SecurityContext, tenant: Option<&str>) -> SecurityContext {
    match (base.tenant_id.is_none(), tenant) {
        (true, Some(tenant)) => base.clone().with_tenant(tenant),
        _ => base.clone(),
    }
}

/// Split the reserved [`SOURCE_TENANT_VAR`] out of a connector's query variables.
///
/// Returns the variables with the reserved key **always removed** (so it never
/// reaches the mutation) plus the tenant when the key held a non-blank string. A
/// blank or non-string value is stripped but yields no tenant.
fn split_tenant_override(variables: Option<&Value>) -> (Option<Value>, Option<String>) {
    let Some(Value::Object(map)) = variables else {
        return (variables.cloned(), None);
    };
    if !map.contains_key(SOURCE_TENANT_VAR) {
        return (Some(Value::Object(map.clone())), None);
    }
    let mut map = map.clone();
    let tenant = match map.remove(SOURCE_TENANT_VAR) {
        Some(Value::String(tenant)) if !tenant.trim().is_empty() => Some(tenant),
        _ => None,
    };
    (Some(Value::Object(map)), tenant)
}

impl<A: DatabaseAdapter + 'static> QueryExecutor for SourceQueryExecutor<A> {
    fn execute_query(
        &self,
        query: &str,
        variables: Option<&Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>> {
        // Snapshot the current executor (hot-reload-safe) and resolve the identity
        // for this call, then own everything so the future borrows nothing.
        let executor = self.executor.load_full();
        let (variables, tenant) = split_tenant_override(variables);
        let identity = resolve_identity(&self.identity, tenant.as_deref());
        let query = query.to_owned();
        Box::pin(async move {
            executor.execute_with_security(&query, variables.as_ref(), &identity).await
        })
    }
}

#[cfg(test)]
mod tests;
