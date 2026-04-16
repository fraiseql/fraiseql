//! Multi-tenancy infrastructure: pool factory, executor construction, health monitoring.

pub mod pool_factory;

use std::{future::Future, pin::Pin, sync::Arc};

use fraiseql_core::runtime::Executor;
use fraiseql_error::Result;
pub use pool_factory::{FromPoolConfig, TenantPoolConfig, create_tenant_executor};

/// Type-erased async factory for creating tenant executors.
///
/// Stored in `AppState` so that the management API handler (`upsert_tenant_handler`)
/// can build an `Executor<A>` without requiring `A: FromPoolConfig` as a bound on
/// the route handler or the `Server<A>` impl. The factory is set once at server
/// startup by code that knows the concrete adapter type.
pub type TenantExecutorFactory<A> = Arc<
    dyn Fn(
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
pub fn make_executor_factory<A: FromPoolConfig + 'static>() -> TenantExecutorFactory<A> {
    Arc::new(|schema_json, pool_config| {
        Box::pin(async move { create_tenant_executor::<A>(&schema_json, &pool_config).await })
    })
}
