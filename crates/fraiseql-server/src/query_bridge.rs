//! The shared `fraiseql_query` bridge for background dispatch paths (#573, #594).
//!
//! A dispatched function's Deno/WASM guest issues mutations via `fraiseql_query`,
//! which reaches the host as
//! [`HostContext::query`](fraiseql_functions::HostContext::query) and delegates to a
//! [`QueryExecutor`](fraiseql_functions::host::live::QueryExecutor). Production
//! dispatch originally wired one only for scheduled sources; every other dispatch
//! path (after:mutation, after:ingest, cron, after:capture) failed with "query
//! executor not configured".
//!
//! [`RunAsQueryExecutor`](crate::query_bridge::RunAsQueryExecutor) is that bridge,
//! shared across all of them: it wraps the server's
//! [`Executor`](fraiseql_core::runtime::Executor) and runs each query/mutation under a
//! **`run_as` identity** (a `SystemJob`
//! [`SecurityContext`](fraiseql_core::security::SecurityContext) built from the source's or
//! function's `run_as` ceiling — fail-closed when absent). It was extracted from the
//! sources-only `SourceQueryExecutor` (#573) so after:mutation / after:ingest
//! dispatch reuses the exact same authority + hot-reload seam (#594): one authority
//! model, every dispatch path.
//!
//! Two properties matter:
//!
//! - **Hot-reload-safe.** It holds the same `Arc<ArcSwap<Executor<A>>>` the request path holds and
//!   `load`s a fresh snapshot per call, so a schema reload is picked up by the next firing rather
//!   than pinned at construction.
//! - **Per-message tenant seam.** The identity is not a frozen field. A single `execute_query`
//!   re-scopes to a per-message tenant carried in the reserved
//!   [`SOURCE_TENANT_VAR`](crate::query_bridge::SOURCE_TENANT_VAR) variable — the runtime half of
//!   the multi-tenant source path (only the connector knows which tenant a fetched record belongs
//!   to). An identity already pinned to a tenant by `run_as` ignores the override and cannot forge
//!   writes for another tenant. (Event-dispatched functions never set the variable, so the override
//!   is inert for them — their identity is exactly their `run_as`.)

use std::{future::Future, pin::Pin, sync::Arc};

use arc_swap::ArcSwap;
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor, security::SecurityContext};
use fraiseql_error::Result;
use fraiseql_functions::host::live::QueryExecutor;
use serde_json::Value;

/// Reserved GraphQL variable a multi-tenant source sets to scope one write to a
/// tenant (#573).
///
/// It is stripped from the variables before the query runs, so it never reaches the
/// mutation itself. The SDK surfaces it ergonomically (e.g.
/// `ctx.query(mutation, vars, { tenant })`); this constant is the wire contract both
/// sides agree on. Event-dispatched functions do not set it.
pub const SOURCE_TENANT_VAR: &str = "__source_tenant";

/// Adapts the server's [`Executor`] to the functions [`QueryExecutor`] so a background
/// dispatch's mutations run under a `run_as` identity (#573, #594).
///
/// Shared by scheduled sources and event-dispatched functions (after:mutation /
/// after:ingest / cron / after:capture) — the identity distinguishes them.
pub struct RunAsQueryExecutor<A: DatabaseAdapter> {
    /// The hot-reloadable executor — the exact handle the request path uses, so a
    /// schema swap is reflected on the next firing (loaded per call).
    executor: Arc<ArcSwap<Executor<A>>>,
    /// The base `run_as` identity (a `SystemJob` context). A per-message tenant may
    /// re-scope it, but never widen its roles/scopes.
    identity: SecurityContext,
}

impl<A: DatabaseAdapter> RunAsQueryExecutor<A> {
    /// Bridge `executor` to a background job running under `identity` (its `run_as`
    /// ceiling — a source's
    /// [`SourceDefinition::identity`](fraiseql_core::schema::SourceDefinition::identity)
    /// or a function's
    /// [`FunctionDefinition::identity`](fraiseql_functions::FunctionDefinition::identity)).
    #[must_use]
    pub const fn new(executor: Arc<ArcSwap<Executor<A>>>, identity: SecurityContext) -> Self {
        Self { executor, identity }
    }
}

/// The effective identity for one query: the base `run_as` ceiling, re-scoped to
/// `tenant` **only** when the base is not already pinned to a tenant.
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

impl<A: DatabaseAdapter + 'static> QueryExecutor for RunAsQueryExecutor<A> {
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
