//! HTTP server implementation.

use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};
#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    db::traits::{DatabaseAdapter, RelayDatabaseAdapter},
    runtime::{Executor, SubscriptionManager},
    schema::CompiledSchema,
    security::OidcValidator,
};
use tokio::net::TcpListener;
#[cfg(any(
    feature = "observers",
    feature = "redis-rate-limiting",
    feature = "redis-pkce",
    feature = "mcp"
))]
use tracing::error;
use tracing::{info, warn};
#[cfg(feature = "observers")]
use {
    crate::observers::{ObserverRuntime, ObserverRuntimeConfig},
    tokio::sync::RwLock,
};

#[cfg(feature = "auth")]
use crate::routes::{AuthPkceState, auth_callback, auth_start};
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
    pub(super) rate_limiter: Option<Arc<RateLimiter>>,
    #[cfg(feature = "secrets")]
    pub(super) secrets_manager: Option<Arc<crate::secrets_manager::SecretsManager>>,
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
    #[allow(dead_code)] // Reason: used only when token-revocation feature is active
    pub(super) revocation_manager: Option<Arc<crate::token_revocation::TokenRevocationManager>>,
    pub(super) apq_store: Option<Arc<dyn fraiseql_core::apq::ApqStorage>>,
    pub(super) trusted_docs: Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,

    #[cfg(feature = "observers")]
    pub(super) observer_runtime: Option<Arc<RwLock<ObserverRuntime>>>,

    #[cfg(feature = "observers")]
    pub(super) db_pool: Option<sqlx::PgPool>,

    #[cfg(feature = "arrow")]
    pub(super) flight_service: Option<FraiseQLFlightService>,

    #[cfg(feature = "mcp")]
    pub(super) mcp_config: Option<fraiseql_core::schema::McpConfig>,

    /// Pool auto-tuning configuration (loaded from `[pool_tuning]` in `fraiseql.toml`).
    pub(super) pool_tuning_config: Option<crate::config::pool_tuning::PoolTuningConfig>,
}
