//! HTTP server implementation.

use std::sync::Arc;

#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, SubscriptionManager},
    security::{AuthMiddleware, OidcValidator},
};
#[cfg(feature = "observers")]
use {
    crate::observers::{ObserverRuntime, ObserverRuntimeConfig},
    tokio::sync::RwLock,
};

#[cfg(feature = "auth")]
use crate::routes::{AuthMeState, AuthPkceState, auth_callback, auth_me, auth_start};
use crate::{
    Result, ServerError,
    middleware::{
        BearerAuthState, OidcAuthState, RateLimiter, bearer_auth_middleware, cors_layer_restricted,
        metrics_middleware, oidc_auth_middleware, require_json_content_type, trace_layer,
    },
    routes::{
        PlaygroundState, SubscriptionState, api, graphql::AppState, graphql_get_handler,
        graphql_handler, health_handler, introspection_handler, metrics_handler,
        metrics_json_handler, playground_handler, readiness_handler, subscription_handler,
    },
    server_config::ServerConfig,
    tls::TlsSetup,
};

mod builder;
mod extensions;
mod initialization;
mod lifecycle;
mod routing;

#[cfg(test)]
mod routing_tests;

/// FraiseQL HTTP Server.
///
/// `Server<A>` is generic over a `DatabaseAdapter` implementation, which allows
/// swapping database backends and injecting mock adapters in tests.
///
/// # Feature: `observers`
///
/// When compiled with the `observers` Cargo feature, the server mounts observer
/// management and runtime-health API endpoints under `/api/observers`. These
/// endpoints require a live **PostgreSQL** connection pool (`sqlx::PgPool`).
///
/// Pass `Some(pg_pool)` as the `db_pool` argument to [`Server::new`] when the
/// `observers` feature is enabled. Passing `None` causes the observer routes to
/// be skipped at startup (an error is logged) rather than panicking, but the
/// rest of the server continues to function normally.
///
/// The PostgreSQL pool is distinct from the generic `DatabaseAdapter`: the
/// adapter handles application queries, while the pool is used exclusively by
/// the observer subsystem to store and retrieve reactive rule metadata.
pub struct Server<A: DatabaseAdapter> {
    pub(super) config: ServerConfig,
    pub(super) executor: Arc<Executor<A>>,
    pub(super) subscription_manager: Arc<SubscriptionManager>,
    pub(super) subscription_lifecycle: Arc<dyn crate::subscriptions::SubscriptionLifecycle>,
    pub(super) max_subscriptions_per_connection: Option<u32>,
    pub(super) oidc_validator: Option<Arc<OidcValidator>>,
    /// Local HS256 JWT validator (alternative to `oidc_validator`).
    ///
    /// When set, the GraphQL endpoint is protected by shared-secret JWT
    /// validation instead of OIDC. Intended for integration testing and
    /// internal service-to-service auth.
    pub(super) hs256_auth: Option<Arc<AuthMiddleware>>,
    pub(super) rate_limiter: Option<Arc<RateLimiter>>,
    #[cfg(feature = "secrets")]
    pub(super) secrets_manager: Option<Arc<crate::secrets_manager::SecretsManager>>,
    #[cfg(feature = "federation")]
    pub(super) circuit_breaker:
        Option<Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>>,
    pub(super) error_sanitizer: Arc<crate::config::error_sanitization::ErrorSanitizer>,
    #[cfg(feature = "auth")]
    pub(super) state_encryption: Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
    #[cfg(feature = "auth")]
    pub(super) pkce_store: Option<Arc<crate::auth::PkceStateStore>>,
    #[cfg(feature = "auth")]
    pub(super) oidc_server_client: Option<Arc<crate::auth::OidcServerClient>>,
    pub(super) api_key_authenticator: Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    // Reason: only read inside #[cfg(feature = "auth")] blocks in routing.rs
    #[allow(dead_code)] // Reason: field kept for API completeness; may be used in future features
    pub(super) revocation_manager: Option<Arc<crate::token_revocation::TokenRevocationManager>>,
    pub(super) apq_store: Option<fraiseql_core::apq::ArcApqStorage>,
    pub(super) trusted_docs: Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,

    #[cfg(feature = "observers")]
    pub(super) observer_runtime: Option<Arc<RwLock<ObserverRuntime>>>,

    #[cfg(feature = "observers")]
    pub(super) db_pool: Option<sqlx::PgPool>,

    /// PostgreSQL pool for claims enrichment queries (independent of observers).
    #[cfg(feature = "auth")]
    pub(super) enrichment_pool: Option<sqlx::PgPool>,

    #[cfg(feature = "arrow")]
    pub(super) flight_service: Option<FraiseQLFlightService>,

    #[cfg(feature = "mcp")]
    pub(super) mcp_config: Option<fraiseql_core::schema::McpConfig>,

    /// Pre-built storage state for mounting storage routes.
    ///
    /// Populated during server construction when `[storage]` is configured and
    /// a PostgreSQL pool is available for metadata tracking.
    pub(super) storage_state: Option<fraiseql_storage::StorageState>,

    /// Pre-built realtime state for mounting the `WebSocket` endpoint.
    ///
    /// When `Some`, `build_base_router` merges `realtime_router(state)` at
    /// `/realtime/v1`. Set via [`Server::with_realtime`].
    pub(super) realtime_state: Option<crate::realtime::server::RealtimeState>,

    /// Pool pressure monitoring configuration (loaded from `[pool_tuning]` in `fraiseql.toml`).
    pub(super) pool_tuning_config: Option<crate::config::pool_tuning::PoolPressureMonitorConfig>,

    /// Whether the adapter-level query result cache (`CachedDatabaseAdapter`) is active.
    ///
    /// Set to `true` when `ServerConfig::cache_enabled = true` and the server was built
    /// with `Server::new` or `Server::with_relay_pagination`.
    pub(super) adapter_cache_enabled: bool,
}
