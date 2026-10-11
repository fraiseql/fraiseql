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
//! - **Hot-reload-safe and tenant-scoped.** It resolves its executor per call from the server's
//!   [`ExecutorSource`](crate::query_bridge::ExecutorSource): the serving executor's current
//!   snapshot, or, under the tenancy runtime, the executor of the tenant the write's identity
//!   carries (#1550). A schema reload or a tenant registration is picked up by the next firing.
//! - **Per-message tenant seam.** The identity is not a frozen field. A single `execute_query`
//!   re-scopes to a per-message tenant carried in the reserved
//!   [`SOURCE_TENANT_VAR`](crate::query_bridge::SOURCE_TENANT_VAR) variable — the runtime half of
//!   the multi-tenant source path (only the connector knows which tenant a fetched record belongs
//!   to). An identity already pinned to a tenant by `run_as` ignores the override and cannot forge
//!   writes for another tenant. Only a scheduled source's bridge
//!   ([`RunAsQueryExecutor::for_source`](crate::query_bridge::RunAsQueryExecutor::for_source))
//!   honours it; an event-dispatched function's variable is stripped and ignored, so it cannot
//!   steer its writes to another tenant.

use std::{future::Future, pin::Pin, sync::Arc};

use arc_swap::ArcSwap;
use fraiseql_core::{runtime::Executor, security::SecurityContext};
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

/// Where a bridged write finds its executor (#1550): the tenant's, when the tenancy
/// runtime is on.
///
/// Built only from the server's state ([`ExecutorSource::of`]), so no dispatch path can
/// bridge to the default executor while tenants are registered. With the runtime off,
/// every write runs on the hot-reloadable serving executor; with it on, a write whose
/// identity carries a tenant runs on that tenant's executor (its pool and
/// `search_path`), and an unregistered or suspended tenant is refused exactly as
/// `/graphql` refuses it.
#[derive(Clone)]
pub struct ExecutorSource(Source);

#[derive(Clone)]
enum Source {
    /// No tenancy runtime: the one serving executor.
    Single(Arc<ArcSwap<Executor>>),
    /// The tenancy runtime: the registry, whose default is the serving executor.
    Tenants(Arc<TenantExecutorRegistry>),
}

use crate::routes::graphql::tenant_registry::TenantExecutorRegistry;

impl ExecutorSource {
    /// The source the server's request path uses.
    #[must_use]
    pub fn of(state: &crate::routes::graphql::AppState) -> Self {
        Self(match state.tenant_registry() {
            Some(registry) => Source::Tenants(Arc::clone(registry)),
            None => Source::Single(state.executor.clone()),
        })
    }

    /// The executor a write by a principal of `tenant` runs on.
    ///
    /// # Errors
    ///
    /// Under the tenancy runtime, `Authorization` for an unregistered tenant and
    /// `ServiceUnavailable` for a suspended one.
    pub fn executor_for(&self, tenant: Option<&str>) -> Result<Arc<Executor>> {
        match &self.0 {
            Source::Single(executor) => Ok(executor.load_full()),
            Source::Tenants(registry) => Ok(Arc::clone(&*registry.executor_for(tenant)?)),
        }
    }

    /// The tenant registry this source dispatches through, under the tenancy runtime.
    #[must_use]
    pub const fn registry(&self) -> Option<&Arc<TenantExecutorRegistry>> {
        match &self.0 {
            Source::Tenants(registry) => Some(registry),
            Source::Single(_) => None,
        }
    }

    /// The serving executor handle, when there is no tenancy runtime.
    #[must_use]
    pub const fn serving(&self) -> Option<&Arc<ArcSwap<Executor>>> {
        match &self.0 {
            Source::Single(executor) => Some(executor),
            Source::Tenants(_) => None,
        }
    }

    /// A source over one executor, for tests of the bridge itself.
    #[cfg(test)]
    pub(crate) const fn single(executor: Arc<ArcSwap<Executor>>) -> Self {
        Self(Source::Single(executor))
    }

    /// A handle that does not keep the executors alive, for a holder the executor itself
    /// owns (the after-mutation observer), so the two do not form a cycle.
    #[must_use]
    pub fn downgrade(&self) -> WeakExecutorSource {
        WeakExecutorSource(match &self.0 {
            Source::Single(executor) => WeakSource::Single(Arc::downgrade(executor)),
            Source::Tenants(registry) => WeakSource::Tenants(Arc::downgrade(registry)),
        })
    }
}

/// [`ExecutorSource`] without ownership; see [`ExecutorSource::downgrade`].
#[derive(Clone)]
pub struct WeakExecutorSource(WeakSource);

#[derive(Clone)]
enum WeakSource {
    Single(std::sync::Weak<ArcSwap<Executor>>),
    Tenants(std::sync::Weak<TenantExecutorRegistry>),
}

impl WeakExecutorSource {
    /// The source, while the server that owns it is alive.
    #[must_use]
    pub fn upgrade(&self) -> Option<ExecutorSource> {
        Some(ExecutorSource(match &self.0 {
            WeakSource::Single(executor) => Source::Single(executor.upgrade()?),
            WeakSource::Tenants(registry) => Source::Tenants(registry.upgrade()?),
        }))
    }
}

/// Adapts the server's [`Executor`] to the functions [`QueryExecutor`] so a background
/// dispatch's mutations run under a `run_as` identity (#573, #594).
///
/// Shared by scheduled sources and event-dispatched functions (after:mutation /
/// after:ingest / cron / after:capture) — the identity distinguishes them.
pub struct RunAsQueryExecutor {
    /// Where each write finds its executor: the hot-reloadable serving executor, or the
    /// tenant's (#1550). Resolved per call, so a schema swap or a tenant's registration
    /// is reflected on the next firing.
    source:             ExecutorSource,
    /// The base `run_as` identity (a `SystemJob` context). A per-message tenant may
    /// re-scope it, but never widen its roles/scopes.
    identity:           SecurityContext,
    /// Whether [`SOURCE_TENANT_VAR`] re-scopes a write: a scheduled source's only, where the
    /// connector knows each record's tenant. An event-dispatched function's variable is
    /// stripped and ignored, so it cannot steer its writes to another tenant (#1550 review).
    per_message_tenant: bool,
}

impl RunAsQueryExecutor {
    /// Bridge `executor` to a background job running under `identity` (its `run_as`
    /// ceiling — a source's
    /// [`SourceDefinition::identity`](fraiseql_core::schema::SourceDefinition::identity)
    /// or a function's
    /// [`FunctionDefinition::identity`](fraiseql_functions::FunctionDefinition::identity)).
    #[must_use]
    pub const fn new(source: ExecutorSource, identity: SecurityContext) -> Self {
        Self {
            source,
            identity,
            per_message_tenant: false,
        }
    }

    /// [`new`](Self::new) for a scheduled source: its connector scopes each write to a
    /// record's tenant through [`SOURCE_TENANT_VAR`].
    #[must_use]
    pub const fn for_source(source: ExecutorSource, identity: SecurityContext) -> Self {
        Self {
            source,
            identity,
            per_message_tenant: true,
        }
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

impl QueryExecutor for RunAsQueryExecutor {
    fn execute_query(
        &self,
        query: &str,
        variables: Option<&Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>> {
        // Snapshot the current executor (hot-reload-safe) and resolve the identity
        // for this call, then own everything so the future borrows nothing.
        let (variables, tenant) = split_tenant_override(variables);
        let tenant = tenant.filter(|_| self.per_message_tenant);
        let identity = resolve_identity(&self.identity, tenant.as_deref());
        let executor = self.source.executor_for(identity.tenant_id.as_ref().map(|t| t.0.as_str()));
        let query = query.to_owned();
        // Every write a bridge makes is a dispatched function's or a scheduled source's,
        // never a request's, so it runs at dispatch depth 1: the after-mutation observer
        // does not dispatch on it (M-bridge), and a function writing the entity it is
        // triggered by cannot trigger itself (#1340, #1440).
        Box::pin(fraiseql_core::runtime::dispatched_at(1, async move {
            executor?.execute_with_security(&query, variables.as_ref(), &identity).await
        }))
    }
}

#[cfg(test)]
mod tests;
