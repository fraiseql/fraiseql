//! Multi-tenancy infrastructure: pool factory, executor construction, health monitoring.

pub mod audit;
pub mod pool_factory;
pub mod schema_isolation;

#[cfg(test)]
mod tests;

use std::{future::Future, pin::Pin, sync::Arc};

use fraiseql_core::runtime::Executor;
use fraiseql_error::Result;
pub use pool_factory::{
    FromPoolConfig, TenantPoolConfig, create_tenant_executor, destroy_tenant_schema,
};

/// Type-erased async factory for creating tenant executors.
///
/// Stored in `AppState` so that the management API handler (`upsert_tenant_handler`)
/// can build an `Executor<A>` without requiring `A: FromPoolConfig` as a bound on
/// the route handler or the `Server<A>` impl. The factory is set once at server
/// startup by code that knows the concrete adapter type.
pub type TenantExecutorFactory<A> = Arc<
    dyn Fn(
            String,
            String,
            TenantPoolConfig,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<Executor<A>>>> + Send>>
        + Send
        + Sync,
>;

/// Create a `TenantExecutorFactory` for an adapter that implements `FromPoolConfig`.
///
/// Captures the `FromPoolConfig` bound at construction time so that the factory
/// can be stored as a type-erased closure in `AppState`.
///
/// The first argument is the tenant key, used for schema isolation naming.
///
/// `database_tls` is the server's own `[database_tls]` setting, stamped onto every
/// tenant pool here rather than read from the registration request. Tenant pool
/// configuration arrives as an admin-API request body, so the transport security of
/// a tenant's connections must be decided by the operator who configured the server,
/// not by the payload that registers the tenant (#801). This mirrors the way
/// `search_path` is recomputed rather than trusted.
///
/// `read_replica_policy` is stamped for the same reason (#957). A tenant names its
/// own replica URLs — topology, like its connection string — but not the pin
/// window, staleness budget or probe cadence those replicas are routed under: a
/// registration that could send its own `max_lag` would be choosing how stale its
/// reads may be, against a server whose operator already decided.
#[must_use]
pub fn make_executor_factory<A: FromPoolConfig + 'static>(
    database_tls: fraiseql_core::db::postgres::PostgresTlsConfig,
    read_replica_policy: fraiseql_core::db::postgres::ReadReplicaPolicy,
) -> TenantExecutorFactory<A> {
    Arc::new(move |tenant_key, schema_json, mut pool_config| {
        pool_config.tls = database_tls.clone();
        pool_config.read_replica_policy = read_replica_policy.clone();
        Box::pin(async move {
            create_tenant_executor::<A>(&tenant_key, &schema_json, &pool_config).await
        })
    })
}
