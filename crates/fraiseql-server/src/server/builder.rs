//! Server constructors and builder methods.

use std::sync::Arc;

#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::traits::DatabaseAdapter,
    runtime::{Executor, SubscriptionManager},
    schema::CompiledSchema,
    security::{AuthConfig, AuthMiddleware, OidcValidator},
};
use tracing::{info, warn};

use super::{RateLimiter, Result, Server, ServerConfig, ServerError};

/// Build an HS256 validator from the server config, if configured.
pub(super) fn build_hs256_auth(config: &ServerConfig) -> Result<Option<Arc<AuthMiddleware>>> {
    let Some(ref hs) = config.auth_hs256 else {
        return Ok(None);
    };
    let secret = hs
        .load_secret()
        .map_err(|e| ServerError::ConfigError(format!("Failed to initialize HS256 auth: {e}")))?;
    let mut auth_config = AuthConfig::with_hs256(&secret);
    if let Some(ref iss) = hs.issuer {
        auth_config = auth_config.with_issuer(iss);
    }
    if let Some(ref aud) = hs.audience {
        auth_config = auth_config.with_audience(aud);
    }
    info!(
        secret_env = %hs.secret_env,
        issuer = ?hs.issuer,
        audience = ?hs.audience,
        "Initializing HS256 authentication (local validation, no network)"
    );
    Ok(Some(Arc::new(AuthMiddleware::from_config(auth_config))))
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<CachedDatabaseAdapter<A>> {
    /// Create new server.
    ///
    /// Relay pagination queries will return a `Validation` error at runtime. Use
    /// [`Server::with_relay_pagination`] when the adapter implements
    /// [`RelayDatabaseAdapter`](fraiseql_core::db::traits::RelayDatabaseAdapter)
    /// and relay support is required.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `schema` - Compiled GraphQL schema
    /// * `adapter` - Database adapter
    /// * `db_pool` — forwarded to the observer runtime; `None` when observers are disabled.
    ///
    /// # Errors
    ///
    /// Returns error if OIDC validator initialization fails (e.g., unable to
    /// fetch discovery document or JWKS).
    ///
    /// # Panics
    ///
    /// Panics if the `adapter` `Arc` has been cloned before calling this constructor
    /// (refcount > 1). The builder must have exclusive ownership to unwrap the adapter
    /// for `CachedDatabaseAdapter` construction.
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: running PostgreSQL database and compiled schema file.
    /// let config = ServerConfig::default();
    /// let schema = CompiledSchema::from_json(schema_json)?;
    /// let adapter = Arc::new(PostgresAdapter::new(db_url).await?);
    ///
    /// let server = Server::new(config, schema, adapter, None).await?;
    /// server.serve().await?;
    /// ```
    #[allow(clippy::cognitive_complexity)] // Reason: server construction with subsystem initialization (auth, rate-limit, observers, etc.)
    pub async fn new(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        db_pool: Option<sqlx::PgPool>,
    ) -> Result<Self> {
        // Validate compiled schema format version before any further setup.
        // Warns for legacy schemas (no version field); rejects incompatible future versions.
        if schema.schema_format_version.is_none() {
            warn!(
                "Loaded schema has no schema_format_version (pre-v2.1 format). \
                 Re-compile with the current fraiseql-cli for version compatibility checking."
            );
        }
        schema.validate_format_version().map_err(|msg| {
            ServerError::ConfigError(format!("Incompatible compiled schema: {msg}"))
        })?;

        // Read security configs from compiled schema BEFORE schema is moved.
        #[cfg(feature = "federation")]
        let circuit_breaker = schema.federation.as_ref().and_then(
            crate::federation::circuit_breaker::FederationCircuitBreakerManager::from_config,
        );
        #[cfg(not(feature = "federation"))]
        let circuit_breaker: Option<()> = None;
        #[cfg(not(feature = "federation"))]
        let _ = &schema.federation;
        let error_sanitizer = Self::error_sanitizer_from_schema(&schema);
        #[cfg(feature = "auth")]
        let state_encryption = Self::state_encryption_from_schema(&schema)?;
        #[cfg(not(feature = "auth"))]
        let state_encryption: Option<
            std::sync::Arc<crate::auth::state_encryption::StateEncryptionService>,
        > = None;
        #[cfg(feature = "auth")]
        let pkce_store = Self::pkce_store_from_schema(&schema, state_encryption.as_ref()).await;
        #[cfg(not(feature = "auth"))]
        let pkce_store: Option<std::sync::Arc<crate::auth::PkceStateStore>> = None;
        #[cfg(feature = "auth")]
        let oidc_server_client = Self::oidc_server_client_from_schema(&schema);
        #[cfg(not(feature = "auth"))]
        let oidc_server_client: Option<std::sync::Arc<crate::auth::OidcServerClient>> = None;
        let schema_rate_limiter = Self::rate_limiter_from_schema(&schema).await;
        let api_key_authenticator = crate::api_key::api_key_authenticator_from_schema(&schema);
        if api_key_authenticator.is_some() {
            info!("API key authentication enabled");
        }
        let revocation_manager = crate::token_revocation::revocation_manager_from_schema(&schema);
        if revocation_manager.is_some() {
            info!("Token revocation enabled");
        }
        let trusted_docs = Self::trusted_docs_from_schema(&schema);

        // Validate cache + RLS safety at startup.
        // Cache isolation relies entirely on per-user WHERE clauses in the cache key.
        // Without RLS, users with the same query and variables share the same cached
        // response, which can leak data across tenants.
        if config.cache_enabled && !schema.has_rls_configured() {
            if schema.is_multi_tenant() {
                // Multi-tenant + cache + no RLS is a hard safety violation.
                return Err(ServerError::ConfigError(
                    "Cache is enabled in a multi-tenant schema but no Row-Level Security \
                     policies are declared. This would allow cross-tenant cache hits and \
                     data leakage. In fraiseql.toml, either disable caching with \
                     [cache] enabled = false, declare [security.rls] policies, or set \
                     [security] multi_tenant = false to acknowledge single-tenant mode."
                        .to_string(),
                ));
            }
            // Single-tenant with cache and no RLS: safe, but warn in case of misconfiguration.
            warn!(
                "Query-result caching is enabled but no Row-Level Security policies are \
                 declared in the compiled schema. This is safe for single-tenant deployments. \
                 For multi-tenant deployments, declare RLS policies and set \
                 `security.multi_tenant = true` in your schema."
            );
        }

        // Build cache from config.
        let cache_config = CacheConfig::from(config.cache_enabled);
        let cache = QueryResultCache::new(cache_config);

        // Log cache state before consuming config.
        if cache_config.enabled {
            tracing::info!(
                max_entries   = cache_config.max_entries,
                ttl_seconds   = cache_config.ttl_seconds,
                rls_enforcement = ?cache_config.rls_enforcement,
                "Query result cache: active"
            );
        } else {
            tracing::info!("Query result cache: disabled");
        }

        // Read subscription config from compiled schema (hooks, limits).
        let subscriptions_config = schema.subscriptions_config.clone();

        // Unwrap Arc: refcount is 1 here — adapter has not been cloned since being passed in.
        let inner = Arc::into_inner(adapter)
            .expect("CachedDatabaseAdapter wrapping requires exclusive Arc ownership at startup");
        let cached = CachedDatabaseAdapter::new(inner, cache, schema.content_hash())
            .with_ttl_overrides_from_schema(&schema);
        let executor = Arc::new(Executor::new(schema.clone(), Arc::new(cached)));
        let subscription_manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

        let mut server = Self::from_executor(
            config,
            executor,
            subscription_manager,
            circuit_breaker,
            error_sanitizer,
            state_encryption,
            pkce_store,
            oidc_server_client,
            schema_rate_limiter,
            api_key_authenticator,
            revocation_manager,
            trusted_docs,
            db_pool,
        )
        .await?;

        server.adapter_cache_enabled = cache_config.enabled;

        // Apply pool tuning config from ServerConfig (if present).
        if let Some(pt) = server.config.pool_tuning.clone() {
            if pt.enabled {
                server = server
                    .with_pool_tuning(pt)
                    .map_err(|e| ServerError::ConfigError(format!("pool_tuning: {e}")))?;
            }
        }

        // Initialize MCP config from compiled schema when the feature is compiled in.
        #[cfg(feature = "mcp")]
        if let Some(ref cfg) = server.executor.schema().mcp_config {
            if cfg.enabled {
                let tool_count =
                    crate::mcp::tools::schema_to_tools(server.executor.schema(), cfg).len();
                info!(
                    path = %cfg.path,
                    transport = %cfg.transport,
                    tools = tool_count,
                    "MCP server configured"
                );
                server.mcp_config = Some(cfg.clone());
            }
        }

        // Initialize APQ store when enabled.
        if server.config.apq_enabled {
            let apq_store: fraiseql_core::apq::ArcApqStorage =
                Arc::new(fraiseql_core::apq::InMemoryApqStorage::default());
            server.apq_store = Some(apq_store);
            info!("APQ (Automatic Persisted Queries) enabled — in-memory backend");
        }

        // Apply subscription lifecycle/limits from compiled schema.
        if let Some(ref subs) = subscriptions_config {
            if let Some(max) = subs.max_subscriptions_per_connection {
                server.max_subscriptions_per_connection = Some(max);
            }
            if let Some(lifecycle) = crate::subscriptions::WebhookLifecycle::from_config(subs) {
                server.subscription_lifecycle = Arc::new(lifecycle);
            }
        }

        Ok(server)
    }
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Shared initialization path used by both `new` and `with_relay_pagination`.
    ///
    /// Accepts a pre-built executor so that relay vs. non-relay constructors can supply
    /// the appropriate variant without duplicating auth/rate-limiter/observer setup.
    #[allow(clippy::too_many_arguments)]
    // Reason: internal constructor collects all pre-built subsystems; a builder struct would not
    // reduce call-site clarity
    #[allow(clippy::cognitive_complexity)] // Reason: internal constructor that assembles server from pre-built subsystems
    pub(super) async fn from_executor(
        config: ServerConfig,
        executor: Arc<Executor<A>>,
        subscription_manager: Arc<SubscriptionManager>,
        #[cfg(feature = "federation")] circuit_breaker: Option<
            Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>,
        >,
        #[cfg(not(feature = "federation"))] _circuit_breaker: Option<()>,
        error_sanitizer: Arc<crate::config::error_sanitization::ErrorSanitizer>,
        state_encryption: Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
        pkce_store: Option<Arc<crate::auth::PkceStateStore>>,
        oidc_server_client: Option<Arc<crate::auth::OidcServerClient>>,
        schema_rate_limiter: Option<Arc<RateLimiter>>,
        api_key_authenticator: Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
        revocation_manager: Option<Arc<crate::token_revocation::TokenRevocationManager>>,
        trusted_docs: Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,
        // `db_pool` is forwarded to the observer runtime; unused when the `observers` feature is
        // off.
        #[cfg_attr(not(feature = "observers"), allow(unused_variables))] db_pool: Option<
            sqlx::PgPool,
        >,
    ) -> Result<Self> {
        // Initialize OIDC validator if auth is configured
        let oidc_validator = if let Some(ref auth_config) = config.auth {
            info!(
                issuer = %auth_config.issuer,
                "Initializing OIDC authentication"
            );
            let validator = OidcValidator::new(auth_config.clone())
                .await
                .map_err(|e| ServerError::ConfigError(format!("Failed to initialize OIDC: {e}")))?;
            Some(Arc::new(validator))
        } else {
            None
        };

        // Initialize HS256 validator if configured (mutually exclusive with OIDC).
        let hs256_auth = build_hs256_auth(&config)?;

        // Initialize rate limiter: compiled schema config takes priority over server config.
        let rate_limiter = if let Some(rl) = schema_rate_limiter {
            Some(rl)
        } else if let Some(ref rate_config) = config.rate_limiting {
            if rate_config.enabled {
                info!(
                    rps_per_ip = rate_config.rps_per_ip,
                    rps_per_user = rate_config.rps_per_user,
                    "Initializing rate limiting from server config"
                );
                Some(Arc::new(RateLimiter::new(rate_config.clone())))
            } else {
                info!("Rate limiting disabled by configuration");
                None
            }
        } else {
            None
        };

        // Initialize observer runtime
        #[cfg(feature = "observers")]
        let observer_runtime = Self::init_observer_runtime(&config, db_pool.as_ref()).await;

        // Initialize Flight service with OIDC authentication if configured
        #[cfg(feature = "arrow")]
        let flight_service = {
            let mut service = FraiseQLFlightService::new();
            if let Some(ref validator) = oidc_validator {
                info!("Enabling OIDC authentication for Arrow Flight");
                service.set_oidc_validator(validator.clone());
            } else {
                info!("Arrow Flight initialized without authentication (dev mode)");
            }
            Some(service)
        };

        // Warn if PKCE is configured but [auth] is missing (no OidcServerClient).
        #[cfg(feature = "auth")]
        if pkce_store.is_some() && oidc_server_client.is_none() {
            tracing::error!(
                "pkce.enabled = true but [auth] is not configured or OIDC client init failed. \
                 Auth routes (/auth/start, /auth/callback) will NOT be mounted. \
                 Add [auth] with discovery_url, client_id, client_secret_env, and \
                 server_redirect_uri to fraiseql.toml and recompile the schema."
            );
        }

        // Refuse to start if FRAISEQL_REQUIRE_REDIS is set and PKCE store is in-memory.
        #[cfg(feature = "auth")]
        Self::check_redis_requirement(pkce_store.as_ref())?;

        // Spawn background PKCE state cleanup task (every 5 minutes).
        #[cfg(feature = "auth")]
        if let Some(ref store) = pkce_store {
            use std::time::Duration;

            use tokio::time::MissedTickBehavior;
            let store_clone = Arc::clone(store);
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(300));
                ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
                loop {
                    ticker.tick().await;
                    store_clone.cleanup_expired().await;
                }
            });
        }

        // Reason: state_encryption/pkce_store/oidc_server_client are only stored when
        //         feature = "auth" is enabled; without it they are legitimately unused.
        #[cfg(not(feature = "auth"))]
        let _ = (state_encryption, pkce_store, oidc_server_client);
        Ok(Self {
            config,
            executor,
            subscription_manager,
            subscription_lifecycle: Arc::new(crate::subscriptions::NoopLifecycle),
            max_subscriptions_per_connection: None,
            oidc_validator,
            hs256_auth,
            rate_limiter,
            #[cfg(feature = "secrets")]
            secrets_manager: None,
            #[cfg(feature = "federation")]
            circuit_breaker,
            error_sanitizer,
            #[cfg(feature = "auth")]
            state_encryption,
            #[cfg(feature = "auth")]
            pkce_store,
            #[cfg(feature = "auth")]
            oidc_server_client,
            api_key_authenticator,
            revocation_manager,
            apq_store: None,
            trusted_docs,
            #[cfg(feature = "observers")]
            observer_runtime,
            #[cfg(feature = "observers")]
            db_pool,
            #[cfg(feature = "arrow")]
            flight_service,
            #[cfg(feature = "mcp")]
            mcp_config: None,
            pool_tuning_config: None,
            adapter_cache_enabled: false,
            broadcast_manager: None,
        })
    }

    /// Set lifecycle hooks for `WebSocket` subscriptions.
    #[must_use]
    pub fn with_subscription_lifecycle(
        mut self,
        lifecycle: Arc<dyn crate::subscriptions::SubscriptionLifecycle>,
    ) -> Self {
        self.subscription_lifecycle = lifecycle;
        self
    }

    /// Set maximum subscriptions allowed per `WebSocket` connection.
    #[must_use]
    pub const fn with_max_subscriptions_per_connection(mut self, max: u32) -> Self {
        self.max_subscriptions_per_connection = Some(max);
        self
    }

    /// Enable ephemeral broadcast channels (`POST /realtime/v1/broadcast`).
    #[must_use]
    pub fn with_broadcast(mut self, config: crate::subscriptions::BroadcastConfig) -> Self {
        self.broadcast_manager = Some(Arc::new(crate::subscriptions::BroadcastManager::new(config)));
        self
    }

    /// Enable adaptive connection pool sizing.
    ///
    /// When `config.enabled` is `true`, the server will spawn a background
    /// polling task that samples pool metrics and recommends or applies resizes.
    ///
    /// # Errors
    ///
    /// Returns an error string if the configuration fails validation (e.g. `min >= max`).
    pub fn with_pool_tuning(
        mut self,
        config: crate::config::pool_tuning::PoolPressureMonitorConfig,
    ) -> std::result::Result<Self, String> {
        config.validate()?;
        self.pool_tuning_config = Some(config);
        Ok(self)
    }

    /// Set secrets manager for the server.
    ///
    /// This allows attaching a secrets manager after server creation for credential management.
    #[cfg(feature = "secrets")]
    pub fn set_secrets_manager(&mut self, manager: Arc<crate::secrets_manager::SecretsManager>) {
        self.secrets_manager = Some(manager);
        info!("Secrets manager attached to server");
    }

    /// Serve MCP over stdio (stdin/stdout) instead of HTTP.
    ///
    /// This is used when `FRAISEQL_MCP_STDIO=1` is set.  The server reads JSON-RPC
    /// messages from stdin and writes responses to stdout, following the MCP stdio
    /// transport specification.
    ///
    /// # Errors
    ///
    /// Returns an error if MCP is not configured or the stdio transport fails.
    #[cfg(feature = "mcp")]
    pub async fn serve_mcp_stdio(self) -> Result<()> {
        use rmcp::ServiceExt;

        let mcp_cfg = self.mcp_config.ok_or_else(|| {
            ServerError::ConfigError(
                "FRAISEQL_MCP_STDIO=1 but MCP is not configured. \
                 Add [mcp] enabled = true to fraiseql.toml and recompile the schema."
                    .into(),
            )
        })?;

        let schema = Arc::new(self.executor.schema().clone());
        let executor = self.executor.clone();

        let service = crate::mcp::handler::FraiseQLMcpService::new(schema, executor, mcp_cfg);

        info!("MCP stdio transport starting — reading from stdin, writing to stdout");

        let running = service
            .serve((tokio::io::stdin(), tokio::io::stdout()))
            .await
            .map_err(|e| ServerError::ConfigError(format!("MCP stdio init failed: {e}")))?;

        running
            .waiting()
            .await
            .map_err(|e| ServerError::ConfigError(format!("MCP stdio error: {e}")))?;

        Ok(())
    }
}
