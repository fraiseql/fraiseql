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
        BearerAuthState, OidcAuthState, RateLimiter, admin_auth_middleware, bearer_auth_middleware,
        metrics_middleware, oidc_auth_middleware, require_json_content_type,
        required_auth_middleware, trace_layer,
    },
    routes::{
        PlaygroundState, SubscriptionState, api, graphql_get_handler, graphql_handler,
        health_handler, introspection_handler, metrics_handler, metrics_json_handler,
        playground_handler, readiness_handler, subscription_handler,
    },
    server_config::ServerConfig,
    tls::TlsSetup,
};

mod builder;
mod extensions;
#[cfg(feature = "functions-runtime")]
mod functions_setup;
pub(crate) mod initialization;
mod lifecycle;
mod routing;

#[cfg(test)]
mod parity_tests;

#[cfg(test)]
mod routing_tests;

#[cfg(test)]
mod tests;

/// Builds the REST router for a server whose adapter supports mutations.
///
/// The arguments mirror [`rest_router`](crate::routes::rest::rest_router):
/// `(app state, compression enabled, auth layer attached)`.
///
/// This exists as a stored closure rather than a direct call because
/// `SupportsMutations` is not — and must not become — a bound on `Server<A>`'s
/// lifecycle: adding it would lock read-only adapters such as `SqliteAdapter` and
/// `FraiseWireAdapter` out of every deployment. The closure is installed from the one
/// place where the concrete adapter is known to support mutations (the binary's boot
/// path), which is the same idiom as
/// [`tenant_executor_factory`](Server::with_tenant_executor_factory).
#[cfg(feature = "rest")]
pub(super) type RestRouterBuilder<A> = Arc<
    dyn Fn(
            &crate::routes::graphql::AppState<A>,
            &crate::routes::rest::RestMountConfig,
        ) -> Option<axum::Router>
        + Send
        + Sync,
>;

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
    /// #571: drain signal for live `WebSocket` subscription connections. Flipped to
    /// `true` when graceful shutdown begins; every connection then sends a
    /// `Complete` frame per active operation and closes with 1001 (Going Away),
    /// so a rolling deploy ends streams cleanly instead of aborting them.
    pub(super) subscription_drain: Arc<tokio::sync::watch::Sender<bool>>,
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
    /// Social-login state (#368) — the trust-gated `multi_provider` flow.
    ///
    /// When `Some`, the server mounts `GET /auth/v1/providers`,
    /// `GET /auth/v1/authorize` and `GET /auth/v1/callback`. Built from the
    /// compiled `[auth.social]` block in `from_executor`; library embedders
    /// may override via [`Server::with_social_login`].
    #[cfg(feature = "auth")]
    pub(super) social_login: Option<Arc<crate::auth::MultiProviderAuthState>>,
    /// Anonymous session signup state.
    ///
    /// When `Some`, mounts `POST /auth/v1/signup`.  Set via [`Server::with_anon_signup`].
    #[cfg(feature = "auth")]
    pub(super) anon_signup_state: Option<Arc<crate::auth::AnonSignupState>>,
    /// `TOTP` `MFA` route state (`[auth.local] mfa = true`, #367).
    ///
    /// When `Some`, the server mounts the four `MFA` endpoints under
    /// `/auth/v1/mfa/`, backed by the Postgres enrollment store. Also settable
    /// via [`Server::with_mfa`] for library embedders.
    #[cfg(feature = "auth")]
    pub(super) mfa_state: Option<Arc<crate::auth::MfaRouteState>>,
    /// Email `OTP` / magic-link route state (`[auth.local] otp = true`, #367).
    ///
    /// When `Some`, mounts `POST /auth/v1/otp` and `POST /auth/v1/verify`.
    #[cfg(feature = "auth")]
    pub(super) otp_state: Option<Arc<crate::auth::OtpRouteState>>,
    /// Local email+password route state (`[auth.local] password = true`, #367).
    ///
    /// When `Some`, mounts `/auth/v1/password/{signup,login,reset,reset/confirm}`.
    #[cfg(feature = "auth")]
    pub(super) local_password_state: Option<Arc<crate::auth::LocalPasswordRouteState>>,
    /// Session-state subsystem (#389) — present when `[session_state]` is
    /// configured. The lifecycle spawns its periodic eviction sweep; the MCP
    /// session-continuity binding is its per-request consumer.
    #[cfg(feature = "auth")]
    pub(super) session_state: Option<Arc<fraiseql_auth::session_state::SessionState>>,
    pub(super) api_key_authenticator: Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    /// SAML SP state (#381) — present when `[saml]` is configured on an
    /// `auth-saml` build; `mount_auth_routes` mounts the login/ACS routes.
    #[cfg(feature = "auth-saml")]
    pub(super) saml_state: Option<fraiseql_auth::saml::SamlAuthState>,
    pub(super) service_account_authenticator:
        Option<Arc<crate::service_account::ServiceAccountAuthenticator>>,
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
    #[allow(dead_code)] // Reason: read by enrichment routing code (ported in sub-phase 4e)
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

    /// Before-mutation function-dispatch hooks, prepared at serve time from the
    /// compiled schema's functions config (modules loaded, runtimes registered,
    /// `send_email` wiring attached). When `Some`, `build_app_state` attaches them
    /// so after:mutation functions fire. `None` when no functions are declared or
    /// the `functions-runtime` feature is off.
    #[cfg(feature = "functions-runtime")]
    pub(super) functions_hooks: Option<Arc<crate::subsystems::BeforeMutationHooks>>,

    /// Factory for building per-tenant executors at registration time.
    ///
    /// Set by the binary's PostgreSQL boot path (where the concrete adapter
    /// implements [`FromPoolConfig`](crate::tenancy::FromPoolConfig)) via
    /// [`Server::with_tenant_executor_factory`]. When the multi-tenant runtime is
    /// enabled, `build_app_state` installs it into `AppState` so
    /// `PUT /api/v1/admin/tenants/{key}` can provision tenants. `None` leaves
    /// runtime provisioning unavailable (dispatch to pre-registered tenants still
    /// works).
    pub(super) tenant_executor_factory: Option<crate::tenancy::TenantExecutorFactory<A>>,

    /// Builds the REST router *including its write half* (POST/PUT/PATCH/DELETE and
    /// the collection-level bulk routes).
    ///
    /// Set by a boot path whose concrete adapter implements
    /// [`SupportsMutations`](fraiseql_core::db::traits::SupportsMutations), via
    /// [`Server::with_rest_write_surface`]. `None` — the default — mounts only the
    /// read-only `rest_query_router`, which is the correct posture for adapters that
    /// cannot execute mutations at all.
    ///
    /// `mount_extensions` chooses between this and `rest_query_router` at the single
    /// REST mount site, so both routers pass through the same
    /// [`attach_auth`](Server::attach_auth) call. Mounting writes on a router that
    /// bypassed that call would put the whole write surface behind no authentication
    /// (#812).
    #[cfg(feature = "rest")]
    pub(super) rest_router_builder: Option<RestRouterBuilder<A>>,

    /// Pool pressure monitoring configuration (loaded from `[pool_tuning]` in `fraiseql.toml`).
    pub(super) pool_tuning_config: Option<crate::config::pool_tuning::PoolPressureMonitorConfig>,

    /// Whether the adapter-level query result cache (`CachedDatabaseAdapter`) is active.
    ///
    /// Set to `true` when `ServerConfig::cache_enabled = true` and the server was built
    /// with `Server::new` or `Server::with_relay_pagination`.
    pub(super) adapter_cache_enabled: bool,

    /// Object storage backend for the `/storage/v1/` routes.
    ///
    /// Maximum allowed upload size for the storage backend (bytes).
    ///
    /// Defaults to 100 `MiB`. Applied as a per-request body limit on upload routes.
    pub(super) storage_max_upload_bytes: usize,

    /// Function deployment store for the `/functions/v1/` routes.
    ///
    /// Set via [`Server::with_functions`]. When `None`, function routes are not mounted.
    #[cfg(feature = "functions")]
    pub(super) function_store: Option<Arc<dyn fraiseql_functions::FunctionStore>>,

    /// Function execution runtime for the `/functions/v1/` routes.
    ///
    /// Set via [`Server::with_functions`]. When `None`, function routes are not mounted.
    #[cfg(feature = "functions")]
    pub(super) function_runtime: Option<Arc<dyn fraiseql_functions::runtime::SendFunctionRuntime>>,

    /// Shared usage aggregator — written by [`MutationAuditLayer`] and read by
    /// the `GET /api/v1/admin/usage` endpoint via [`AppState::usage`].
    ///
    /// [`MutationAuditLayer`]: crate::usage::layer::MutationAuditLayer
    /// [`AppState::usage`]: crate::routes::graphql::AppState::usage
    pub(super) usage: Arc<crate::usage::aggregator::UsageAggregator>,

    /// How this constructor built its executor, so a hot-reload can rebuild the
    /// same kind rather than silently downgrading it.
    ///
    /// `Executor::with_config` by default; `with_relay_pagination` replaces it
    /// with `Executor::with_config_and_relay`, which is the only place the
    /// `RelayDatabaseAdapter` bound is in scope. Threaded into `AppState` by
    /// `build_app_state` (#750).
    pub(super) executor_rebuilder: crate::routes::graphql::app_state::ExecutorRebuilder<A>,

    /// Background lifecycle tasks owned by the server.
    ///
    /// Long-running tasks spawned during server construction or `serve_with_shutdown`
    /// (e.g. SIGUSR1 schema reload, PKCE state cleanup, trusted-documents manifest
    /// reload, usage persistence flush, Arrow Flight gRPC server) are tracked on
    /// this [`tokio::task::JoinSet`]. On graceful shutdown the server aborts and
    /// awaits the set so per-process state is not abandoned mid-flight.
    pub(super) tasks: tokio::task::JoinSet<()>,
}
