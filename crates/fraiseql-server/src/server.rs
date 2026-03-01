//! HTTP server implementation.

use std::sync::Arc;

use axum::{
    Router, extract::DefaultBodyLimit, middleware,
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
#[cfg(any(feature = "observers", feature = "redis-rate-limiting", feature = "redis-pkce"))]
use tracing::error;
use tracing::{info, warn};
#[cfg(feature = "observers")]
use {
    crate::observers::{ObserverRuntime, ObserverRuntimeConfig},
    tokio::sync::RwLock,
};

use crate::{
    Result, ServerError,
    middleware::{
        BearerAuthState, OidcAuthState, RateLimiter, bearer_auth_middleware, cors_layer_restricted,
        metrics_middleware, oidc_auth_middleware, require_json_content_type, trace_layer,
    },
    routes::{
        AuthPkceState, PlaygroundState, SubscriptionState, api, auth_callback, auth_start,
        graphql::AppState, graphql_get_handler, graphql_handler, health_handler,
        introspection_handler, metrics_handler, metrics_json_handler, playground_handler,
        subscription_handler,
    },
    server_config::ServerConfig,
    tls::TlsSetup,
};

/// FraiseQL HTTP Server.
pub struct Server<A: DatabaseAdapter> {
    config:               ServerConfig,
    executor:             Arc<Executor<A>>,
    subscription_manager: Arc<SubscriptionManager>,
    subscription_lifecycle: Arc<dyn crate::subscriptions::SubscriptionLifecycle>,
    max_subscriptions_per_connection: Option<u32>,
    oidc_validator:       Option<Arc<OidcValidator>>,
    rate_limiter:         Option<Arc<RateLimiter>>,
    secrets_manager:      Option<Arc<crate::secrets_manager::SecretsManager>>,
    circuit_breaker:
        Option<Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>>,
    error_sanitizer:      Arc<crate::config::error_sanitization::ErrorSanitizer>,
    state_encryption:     Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
    pkce_store:           Option<Arc<crate::auth::PkceStateStore>>,
    oidc_server_client:   Option<Arc<crate::auth::OidcServerClient>>,
    api_key_authenticator: Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    revocation_manager:   Option<Arc<crate::token_revocation::TokenRevocationManager>>,
    apq_store:            Option<Arc<dyn fraiseql_core::apq::ApqStorage>>,

    #[cfg(feature = "observers")]
    observer_runtime: Option<Arc<RwLock<ObserverRuntime>>>,

    #[cfg(feature = "observers")]
    db_pool: Option<sqlx::PgPool>,

    #[cfg(feature = "arrow")]
    flight_service: Option<FraiseQLFlightService>,

    #[cfg(feature = "mcp")]
    mcp_config: Option<crate::mcp::McpConfig>,
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build a `StateEncryptionService` from `security.state_encryption` in the compiled
    /// schema, if the section is present and `enabled = true`.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` when `enabled = true` but the key environment
    /// variable is absent or invalid.  The server must not start in this state.
    fn state_encryption_from_schema(
        schema: &CompiledSchema,
    ) -> crate::Result<Option<Arc<crate::auth::state_encryption::StateEncryptionService>>> {
        match schema.security.as_ref() {
            None => Ok(None),
            Some(s) => {
                crate::auth::state_encryption::StateEncryptionService::from_compiled_schema(s)
                    .map_err(|e| ServerError::ConfigError(e.to_string()))
            },
        }
    }

    /// Build a `PkceStateStore` from the compiled schema if `security.pkce.enabled = true`.
    ///
    /// When `redis_url` is set and the `redis-pkce` feature is compiled in, initialises
    /// a Redis-backed distributed store; otherwise falls back to the in-memory backend
    /// with a warning.
    async fn pkce_store_from_schema(
        schema: &CompiledSchema,
        state_encryption: Option<&Arc<crate::auth::state_encryption::StateEncryptionService>>,
    ) -> Option<Arc<crate::auth::PkceStateStore>> {
        let security = schema.security.as_ref()?;
        let pkce_cfg = security.get("pkce")?;

        #[derive(serde::Deserialize)]
        struct PkceCfgMinimal {
            #[serde(default)]
            enabled:        bool,
            #[serde(default = "default_ttl")]
            state_ttl_secs: u64,
            #[serde(default = "default_method")]
            code_challenge_method: String,
            redis_url: Option<String>,
        }
        fn default_ttl()    -> u64    { 600 }
        fn default_method() -> String { "S256".into() }

        let cfg: PkceCfgMinimal = serde_json::from_value(pkce_cfg.clone()).ok()?;
        if !cfg.enabled {
            return None;
        }

        if state_encryption.is_none() {
            warn!(
                "pkce.enabled = true but state_encryption is disabled. \
                 PKCE state tokens are sent to the OIDC provider unencrypted. \
                 Enable [security.state_encryption] in production for full protection."
            );
        }

        if cfg.code_challenge_method.eq_ignore_ascii_case("plain") {
            warn!(
                "pkce.code_challenge_method = \"plain\" is insecure. \
                 Use \"S256\" in all production environments."
            );
        }

        let enc = state_encryption.cloned();

        // Prefer the Redis backend when redis_url is configured and the feature is compiled in.
        #[cfg(feature = "redis-pkce")]
        if let Some(ref url) = cfg.redis_url {
            match crate::auth::PkceStateStore::new_redis(url, cfg.state_ttl_secs, enc.clone())
                .await
            {
                Ok(store) => {
                    info!(redis_url = %url, "PKCE state store: Redis backend");
                    return Some(Arc::new(store));
                }
                Err(e) => {
                    error!(
                        error = %e,
                        redis_url = %url,
                        "Failed to connect to Redis PKCE store — falling back to in-memory"
                    );
                }
            }
        }

        #[cfg(not(feature = "redis-pkce"))]
        if cfg.redis_url.is_some() {
            warn!(
                "pkce.redis_url is set but the `redis-pkce` Cargo feature is not compiled in. \
                 Rebuild with `--features redis-pkce` to enable the Redis PKCE backend. \
                 Falling back to in-memory storage."
            );
        }

        warn!(
            "PKCE state store: in-memory. In a multi-replica deployment, auth flows will fail \
             if /auth/start and /auth/callback hit different replicas. \
             Set [security.pkce] redis_url to enable the Redis backend, \
             or FRAISEQL_REQUIRE_REDIS=1 to enforce it at startup."
        );

        Some(Arc::new(crate::auth::PkceStateStore::new(cfg.state_ttl_secs, enc)))
    }

    /// Validate that distributed storage is configured when `FRAISEQL_REQUIRE_REDIS` is set.
    ///
    /// When `FRAISEQL_REQUIRE_REDIS=1` is present in the environment, the server refuses
    /// to start if the PKCE state store is using in-memory storage.  This prevents silent
    /// per-replica state isolation in multi-instance deployments.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` with an operator-actionable message when the
    /// constraint is violated.
    fn check_redis_requirement(
        pkce_store: Option<&Arc<crate::auth::PkceStateStore>>,
    ) -> crate::Result<()> {
        if std::env::var("FRAISEQL_REQUIRE_REDIS").is_ok() {
            let pkce_in_memory = pkce_store.is_some_and(|s| s.is_in_memory());
            if pkce_in_memory {
                return Err(ServerError::ConfigError(concat!(
                    "FraiseQL failed to start\n\n",
                    "  FRAISEQL_REQUIRE_REDIS is set but PKCE auth state is using in-memory storage.\n",
                    "  In a multi-replica deployment, auth callbacks can fail if they hit a\n",
                    "  different replica than the one that handled /auth/start.\n\n",
                    "  To fix:\n",
                    "    [security.pkce]\n",
                    "    redis_url = \"redis://localhost:6379\"\n\n",
                    "    [security.rate_limiting]\n",
                    "    redis_url = \"redis://localhost:6379\"\n\n",
                    "  To allow in-memory (single-replica only):\n",
                    "    Unset FRAISEQL_REQUIRE_REDIS",
                )
                .into()));
            }
        }
        Ok(())
    }

    /// Build an `OidcServerClient` from the compiled schema JSON, if `[auth]` is present.
    fn oidc_server_client_from_schema(
        schema: &CompiledSchema,
    ) -> Option<Arc<crate::auth::OidcServerClient>> {
        // The full schema JSON lives in the executor's compiled schema.
        // Access it via the security Value (which contains the embedded JSON blob).
        // We expose the root schema JSON here.
        let schema_json = serde_json::to_value(schema).ok()?;
        crate::auth::OidcServerClient::from_compiled_schema(&schema_json)
    }

    /// Build a `RateLimiter` from the `security.rate_limiting` key embedded in the
    /// compiled schema, if present and `enabled = true`.
    ///
    /// When `redis_url` is set and the `redis-rate-limiting` feature is compiled in,
    /// initialises a Redis-backed distributed limiter; otherwise falls back to the
    /// in-memory backend (with a warning when `redis_url` is set but the feature is
    /// absent).
    async fn rate_limiter_from_schema(schema: &CompiledSchema) -> Option<Arc<RateLimiter>> {
        let sec: crate::middleware::RateLimitingSecurityConfig = schema
            .security
            .as_ref()
            .and_then(|s| s.get("rate_limiting"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())?;

        if !sec.enabled {
            return None;
        }

        let config = crate::middleware::RateLimitConfig::from_security_config(&sec);

        let limiter: RateLimiter = if let Some(ref redis_url) = sec.redis_url {
            #[cfg(feature = "redis-rate-limiting")]
            {
                match RateLimiter::new_redis(redis_url, config.clone()).await {
                    Ok(rl) => {
                        info!(
                            url = redis_url.as_str(),
                            rps_per_ip = config.rps_per_ip,
                            burst_size = config.burst_size,
                            "Rate limiting: using Redis distributed backend"
                        );
                        rl.with_path_rules_from_security(&sec)
                    },
                    Err(e) => {
                        error!(
                            error = %e,
                            "Failed to connect to Redis for rate limiting — \
                             falling back to in-memory backend"
                        );
                        RateLimiter::new(config).with_path_rules_from_security(&sec)
                    },
                }
            }
            #[cfg(not(feature = "redis-rate-limiting"))]
            {
                let _ = redis_url;
                warn!(
                    "rate_limiting.redis_url is set but the server was compiled without the \
                     'redis-rate-limiting' feature. Using in-memory backend."
                );
                RateLimiter::new(config).with_path_rules_from_security(&sec)
            }
        } else {
            info!(
                rps_per_ip = config.rps_per_ip,
                burst_size = config.burst_size,
                "Rate limiting: using in-memory backend"
            );
            RateLimiter::new(config).with_path_rules_from_security(&sec)
        };

        Some(Arc::new(limiter))
    }

    /// Build an `ErrorSanitizer` from the `security.error_sanitization` key in the
    /// compiled schema's security blob (if present), falling back to a disabled sanitizer.
    fn error_sanitizer_from_schema(
        schema: &CompiledSchema,
    ) -> Arc<crate::config::error_sanitization::ErrorSanitizer> {
        let sanitizer = schema
            .security
            .as_ref()
            .and_then(|s| s.get("error_sanitization"))
            .and_then(|v| {
                serde_json::from_value::<
                    crate::config::error_sanitization::ErrorSanitizationConfig,
                >(v.clone())
                .ok()
            })
            .map(crate::config::error_sanitization::ErrorSanitizer::new)
            .unwrap_or_else(crate::config::error_sanitization::ErrorSanitizer::disabled);
        Arc::new(sanitizer)
    }

    /// Create new server.
    ///
    /// Relay pagination queries will return a `Validation` error at runtime. Use
    /// [`Server::with_relay_pagination`] when the adapter implements [`RelayDatabaseAdapter`]
    /// and relay support is required.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `schema` - Compiled GraphQL schema
    /// * `adapter` - Database adapter
    /// * `db_pool` - Database connection pool (optional, required for observers)
    ///
    /// # Errors
    ///
    /// Returns error if OIDC validator initialization fails (e.g., unable to
    /// fetch discovery document or JWKS).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = ServerConfig::default();
    /// let schema = CompiledSchema::from_json(schema_json)?;
    /// let adapter = Arc::new(PostgresAdapter::new(db_url).await?);
    ///
    /// let server = Server::new(config, schema, adapter, None).await?;
    /// server.serve().await?;
    /// ```
    pub async fn new(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        #[allow(unused_variables)] db_pool: Option<sqlx::PgPool>,
    ) -> Result<Self> {
        // Read security configs from compiled schema BEFORE schema is moved.
        let circuit_breaker = schema
            .federation
            .as_ref()
            .and_then(crate::federation::circuit_breaker::FederationCircuitBreakerManager::from_schema_json);
        let error_sanitizer    = Self::error_sanitizer_from_schema(&schema);
        let state_encryption   = Self::state_encryption_from_schema(&schema)?;
        let pkce_store         = Self::pkce_store_from_schema(&schema, state_encryption.as_ref()).await;
        let oidc_server_client = Self::oidc_server_client_from_schema(&schema);
        let schema_rate_limiter = Self::rate_limiter_from_schema(&schema).await;
        let api_key_authenticator = crate::api_key::api_key_authenticator_from_schema(&schema);
        if api_key_authenticator.is_some() {
            info!("API key authentication enabled");
        }
        let revocation_manager = crate::token_revocation::revocation_manager_from_schema(&schema);
        if revocation_manager.is_some() {
            info!("Token revocation enabled");
        }

        // Warn when query-result caching is active but no RLS policies are declared.
        // Cache isolation relies on per-user WHERE clauses in the cache key.  Without RLS,
        // all users share the same (empty) WHERE clause and therefore share cache entries,
        // which can leak data between tenants in multi-tenant deployments.
        if config.cache_enabled && !schema.has_rls_configured() {
            warn!(
                "Query-result caching is enabled but no Row-Level Security policies are declared \
                 in the compiled schema. Cache isolation relies on per-user WHERE clauses in cache \
                 keys. Without RLS, users with the same query and variables will receive the same \
                 cached response. This is safe for single-tenant deployments but WILL LEAK DATA \
                 between tenants in multi-tenant deployments. Declare policies in fraiseql.toml \
                 or set cache_enabled = false if you are using PostgreSQL-native RLS without \
                 FraiseQL policy injection."
            );
        }

        // Read subscription config from compiled schema (hooks, limits).
        let subscriptions_config_json = schema.subscriptions_config.clone();

        let executor = Arc::new(Executor::new(schema.clone(), adapter));
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
            db_pool,
        )
        .await?;

        // Initialize MCP config from compiled schema when the feature is compiled in.
        #[cfg(feature = "mcp")]
        {
            if let Some(ref mcp_json) = server.executor.schema().mcp_config {
                match serde_json::from_value::<crate::mcp::McpConfig>(mcp_json.clone()) {
                    Ok(cfg) if cfg.enabled => {
                        let tool_count = crate::mcp::tools::schema_to_tools(
                            server.executor.schema(),
                            &cfg,
                        ).len();
                        info!(
                            path = %cfg.path,
                            transport = %cfg.transport,
                            tools = tool_count,
                            "MCP server configured"
                        );
                        server.mcp_config = Some(cfg);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "Invalid mcp_config in compiled schema — MCP disabled");
                    }
                }
            }
        }

        // Initialize APQ store when enabled.
        if server.config.apq_enabled {
            let apq_store: Arc<dyn fraiseql_core::apq::ApqStorage> =
                Arc::new(fraiseql_core::apq::InMemoryApqStorage::default());
            server.apq_store = Some(apq_store);
            info!("APQ (Automatic Persisted Queries) enabled — in-memory backend");
        }

        // Apply subscription lifecycle/limits from compiled schema.
        if let Some(ref subs_json) = subscriptions_config_json {
            if let Some(max) = subs_json.get("max_subscriptions_per_connection").and_then(|v| v.as_u64()) {
                #[allow(clippy::cast_possible_truncation)]
                // Reason: max_subscriptions_per_connection is a u32 config field; u64 → u32
                // truncation is acceptable for a limit that would never exceed u32::MAX.
                {
                    server.max_subscriptions_per_connection = Some(max as u32);
                }
            }
            if let Some(lifecycle) = crate::subscriptions::WebhookLifecycle::from_schema_json(subs_json) {
                server.subscription_lifecycle = Arc::new(lifecycle);
            }
        }

        Ok(server)
    }

    /// Shared initialization path used by both `new` and `with_relay_pagination`.
    ///
    /// Accepts a pre-built executor so that relay vs. non-relay constructors can supply
    /// the appropriate variant without duplicating auth/rate-limiter/observer setup.
    #[allow(clippy::too_many_arguments)]
    // Reason: internal constructor that collects all pre-built subsystems; callers pass
    // already-constructed values rather than building them here, so grouping into a
    // builder struct would not reduce call-site clarity.
    async fn from_executor(
        config: ServerConfig,
        executor: Arc<Executor<A>>,
        subscription_manager: Arc<SubscriptionManager>,
        circuit_breaker: Option<
            Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>,
        >,
        error_sanitizer:      Arc<crate::config::error_sanitization::ErrorSanitizer>,
        state_encryption:     Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
        pkce_store:           Option<Arc<crate::auth::PkceStateStore>>,
        oidc_server_client:   Option<Arc<crate::auth::OidcServerClient>>,
        schema_rate_limiter:  Option<Arc<RateLimiter>>,
        api_key_authenticator: Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
        revocation_manager:   Option<Arc<crate::token_revocation::TokenRevocationManager>>,
        #[allow(unused_variables)] db_pool: Option<sqlx::PgPool>,
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
                let limiter_config = crate::middleware::RateLimitConfig {
                    enabled:               true,
                    rps_per_ip:            rate_config.rps_per_ip,
                    rps_per_user:          rate_config.rps_per_user,
                    burst_size:            rate_config.burst_size,
                    cleanup_interval_secs: rate_config.cleanup_interval_secs,
                    trust_proxy_headers:   false,
                };
                Some(Arc::new(RateLimiter::new(limiter_config)))
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
        if pkce_store.is_some() && oidc_server_client.is_none() {
            tracing::error!(
                "pkce.enabled = true but [auth] is not configured or OIDC client init failed. \
                 Auth routes (/auth/start, /auth/callback) will NOT be mounted. \
                 Add [auth] with discovery_url, client_id, client_secret_env, and \
                 server_redirect_uri to fraiseql.toml and recompile the schema."
            );
        }

        // Refuse to start if FRAISEQL_REQUIRE_REDIS is set and PKCE store is in-memory.
        Self::check_redis_requirement(pkce_store.as_ref())?;

        // Spawn background PKCE state cleanup task (every 5 minutes).
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

        Ok(Self {
            config,
            executor,
            subscription_manager,
            subscription_lifecycle: Arc::new(crate::subscriptions::NoopLifecycle),
            max_subscriptions_per_connection: None,
            oidc_validator,
            rate_limiter,
            secrets_manager: None,
            circuit_breaker,
            error_sanitizer,
            state_encryption,
            pkce_store,
            oidc_server_client,
            api_key_authenticator,
            revocation_manager,
            apq_store: None,
            #[cfg(feature = "observers")]
            observer_runtime,
            #[cfg(feature = "observers")]
            db_pool,
            #[cfg(feature = "arrow")]
            flight_service,
            #[cfg(feature = "mcp")]
            mcp_config: None,
        })
    }

    /// Set lifecycle hooks for WebSocket subscriptions.
    #[must_use]
    pub fn with_subscription_lifecycle(
        mut self,
        lifecycle: Arc<dyn crate::subscriptions::SubscriptionLifecycle>,
    ) -> Self {
        self.subscription_lifecycle = lifecycle;
        self
    }

    /// Set maximum subscriptions allowed per WebSocket connection.
    #[must_use]
    pub fn with_max_subscriptions_per_connection(mut self, max: u32) -> Self {
        self.max_subscriptions_per_connection = Some(max);
        self
    }

    /// Set secrets manager for the server.
    ///
    /// This allows attaching a secrets manager after server creation for credential management.
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
        let mcp_cfg = self.mcp_config.ok_or_else(|| {
            ServerError::ConfigError(
                "FRAISEQL_MCP_STDIO=1 but MCP is not configured. \
                 Add [mcp] enabled = true to fraiseql.toml and recompile the schema."
                    .into(),
            )
        })?;

        let schema = Arc::new(self.executor.schema().clone());
        let executor = self.executor.clone();

        let service = crate::mcp::handler::FraiseQLMcpService::new(
            schema,
            executor,
            mcp_cfg,
        );

        info!("MCP stdio transport starting — reading from stdin, writing to stdout");

        use rmcp::ServiceExt;
        let running = service
            .serve((tokio::io::stdin(), tokio::io::stdout()))
            .await
            .map_err(|e| ServerError::ConfigError(format!("MCP stdio init failed: {e}")))?;

        running.waiting().await
            .map_err(|e| ServerError::ConfigError(format!("MCP stdio error: {e}")))?;

        Ok(())
    }
}

impl<A: DatabaseAdapter + RelayDatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Create a server with relay pagination support enabled.
    ///
    /// The adapter must implement [`RelayDatabaseAdapter`]. Currently, only
    /// `PostgresAdapter` and `CachedDatabaseAdapter<PostgresAdapter>` satisfy this bound.
    ///
    /// Relay queries issued against a server created with [`Server::new`] return a
    /// `Validation` error at runtime; those issued against a server created with this
    /// constructor succeed.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `schema` - Compiled GraphQL schema
    /// * `adapter` - Database adapter (must implement `RelayDatabaseAdapter`)
    /// * `db_pool` - Database connection pool (optional, required for observers)
    ///
    /// # Errors
    ///
    /// Returns error if OIDC validator initialization fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let adapter = Arc::new(PostgresAdapter::new(db_url).await?);
    /// let server = Server::with_relay_pagination(config, schema, adapter, None).await?;
    /// server.serve().await?;
    /// ```
    pub async fn with_relay_pagination(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        db_pool: Option<sqlx::PgPool>,
    ) -> Result<Self> {
        // Read security configs from compiled schema BEFORE schema is moved.
        let circuit_breaker = schema
            .federation
            .as_ref()
            .and_then(crate::federation::circuit_breaker::FederationCircuitBreakerManager::from_schema_json);
        let error_sanitizer    = Self::error_sanitizer_from_schema(&schema);
        let state_encryption   = Self::state_encryption_from_schema(&schema)?;
        let pkce_store         = Self::pkce_store_from_schema(&schema, state_encryption.as_ref()).await;
        let oidc_server_client = Self::oidc_server_client_from_schema(&schema);
        let schema_rate_limiter = Self::rate_limiter_from_schema(&schema).await;
        let api_key_authenticator = crate::api_key::api_key_authenticator_from_schema(&schema);
        let revocation_manager = crate::token_revocation::revocation_manager_from_schema(&schema);

        let executor = Arc::new(Executor::new_with_relay(schema.clone(), adapter));
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
            db_pool,
        )
        .await?;

        // Initialize MCP config from compiled schema when the feature is compiled in.
        #[cfg(feature = "mcp")]
        {
            if let Some(ref mcp_json) = server.executor.schema().mcp_config {
                match serde_json::from_value::<crate::mcp::McpConfig>(mcp_json.clone()) {
                    Ok(cfg) if cfg.enabled => {
                        let tool_count = crate::mcp::tools::schema_to_tools(
                            server.executor.schema(),
                            &cfg,
                        ).len();
                        info!(
                            path = %cfg.path,
                            transport = %cfg.transport,
                            tools = tool_count,
                            "MCP server configured"
                        );
                        server.mcp_config = Some(cfg);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "Invalid mcp_config in compiled schema — MCP disabled");
                    }
                }
            }
        }

        // Initialize APQ store when enabled.
        if server.config.apq_enabled {
            let apq_store: Arc<dyn fraiseql_core::apq::ApqStorage> =
                Arc::new(fraiseql_core::apq::InMemoryApqStorage::default());
            server.apq_store = Some(apq_store);
            info!("APQ (Automatic Persisted Queries) enabled — in-memory backend");
        }

        Ok(server)
    }
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Create new server with pre-configured Arrow Flight service.
    ///
    /// Use this constructor when you want to provide a Flight service with a real database adapter.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `schema` - Compiled GraphQL schema
    /// * `adapter` - Database adapter
    /// * `db_pool` - Database connection pool (optional, required for observers)
    /// * `flight_service` - Pre-configured Flight service (only available with arrow feature)
    ///
    /// # Errors
    ///
    /// Returns error if OIDC validator initialization fails.
    #[cfg(feature = "arrow")]
    pub async fn with_flight_service(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        #[allow(unused_variables)] db_pool: Option<sqlx::PgPool>,
        flight_service: Option<FraiseQLFlightService>,
    ) -> Result<Self> {
        // Read security configs from compiled schema BEFORE schema is moved.
        let circuit_breaker = schema
            .federation
            .as_ref()
            .and_then(crate::federation::circuit_breaker::FederationCircuitBreakerManager::from_schema_json);
        let error_sanitizer     = Self::error_sanitizer_from_schema(&schema);
        let state_encryption    = Self::state_encryption_from_schema(&schema)?;
        let pkce_store          = Self::pkce_store_from_schema(&schema, state_encryption.as_ref()).await;
        let oidc_server_client  = Self::oidc_server_client_from_schema(&schema);
        let schema_rate_limiter = Self::rate_limiter_from_schema(&schema).await;
        let api_key_authenticator = crate::api_key::api_key_authenticator_from_schema(&schema);
        let revocation_manager = crate::token_revocation::revocation_manager_from_schema(&schema);

        let executor = Arc::new(Executor::new(schema.clone(), adapter));
        let subscription_manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

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
                let limiter_config = crate::middleware::RateLimitConfig {
                    enabled:               true,
                    rps_per_ip:            rate_config.rps_per_ip,
                    rps_per_user:          rate_config.rps_per_user,
                    burst_size:            rate_config.burst_size,
                    cleanup_interval_secs: rate_config.cleanup_interval_secs,
                    trust_proxy_headers:   false,
                };
                Some(Arc::new(RateLimiter::new(limiter_config)))
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

        // Warn if PKCE is configured but [auth] is missing.
        if pkce_store.is_some() && oidc_server_client.is_none() {
            tracing::error!(
                "pkce.enabled = true but [auth] is not configured or OIDC client init failed. \
                 Auth routes will NOT be mounted."
            );
        }

        // Refuse to start if FRAISEQL_REQUIRE_REDIS is set and PKCE store is in-memory.
        Self::check_redis_requirement(pkce_store.as_ref())?;

        // Spawn background PKCE state cleanup task (every 5 minutes).
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

        Ok(Self {
            config,
            executor,
            subscription_manager,
            subscription_lifecycle: Arc::new(crate::subscriptions::NoopLifecycle),
            max_subscriptions_per_connection: None,
            oidc_validator,
            rate_limiter,
            secrets_manager: None,
            circuit_breaker,
            error_sanitizer,
            state_encryption,
            pkce_store,
            oidc_server_client,
            api_key_authenticator,
            revocation_manager,
            apq_store: if config.apq_enabled {
                Some(Arc::new(fraiseql_core::apq::InMemoryApqStorage::default())
                    as Arc<dyn fraiseql_core::apq::ApqStorage>)
            } else {
                None
            },
            #[cfg(feature = "observers")]
            observer_runtime,
            #[cfg(feature = "observers")]
            db_pool,
            flight_service,
        })
    }

    /// Initialize observer runtime from configuration
    #[cfg(feature = "observers")]
    async fn init_observer_runtime(
        config: &ServerConfig,
        pool: Option<&sqlx::PgPool>,
    ) -> Option<Arc<RwLock<ObserverRuntime>>> {
        // Check if enabled
        let observer_config = match &config.observers {
            Some(cfg) if cfg.enabled => cfg,
            _ => {
                info!("Observer runtime disabled");
                return None;
            },
        };

        let pool = match pool {
            Some(p) => p,
            None => {
                warn!("No database pool provided for observers");
                return None;
            },
        };

        info!("Initializing observer runtime");

        let runtime_config = ObserverRuntimeConfig::new(pool.clone())
            .with_poll_interval(observer_config.poll_interval_ms)
            .with_batch_size(observer_config.batch_size)
            .with_channel_capacity(observer_config.channel_capacity);

        let runtime = ObserverRuntime::new(runtime_config);
        Some(Arc::new(RwLock::new(runtime)))
    }

    /// Build application router.
    fn build_router(&self) -> Router {
        let mut state = AppState::new(self.executor.clone());

        // Attach secrets manager if configured
        if let Some(ref secrets_manager) = self.secrets_manager {
            state = state.with_secrets_manager(secrets_manager.clone());
            info!("SecretsManager attached to AppState");
        }

        // Attach federation circuit breaker if configured
        if let Some(ref cb) = self.circuit_breaker {
            state = state.with_circuit_breaker(cb.clone());
            info!("Federation circuit breaker attached to AppState");
        }

        // Attach error sanitizer (always present; disabled by default)
        state = state.with_error_sanitizer(self.error_sanitizer.clone());
        if self.error_sanitizer.is_enabled() {
            info!("Error sanitizer enabled — internal error details will be stripped from responses");
        }

        // Attach API key authenticator if configured
        if let Some(ref api_key_auth) = self.api_key_authenticator {
            state = state.with_api_key_authenticator(api_key_auth.clone());
            info!("API key authenticator attached to AppState");
        }

        // Attach state encryption service if configured
        match &self.state_encryption {
            Some(svc) => {
                state = state.with_state_encryption(svc.clone());
                info!("State encryption: enabled");
            },
            None => {
                info!("State encryption: disabled (no key configured)");
            },
        }

        // Build RequestValidator from compiled schema validation config
        let mut validator = crate::validation::RequestValidator::new();
        if let Some(ref vc) = self.executor.schema().validation_config {
            if let Some(depth) = vc.get("max_query_depth").and_then(serde_json::Value::as_u64) {
                validator = validator.with_max_depth(depth as usize);
                info!(max_query_depth = depth, "Custom query depth limit configured");
            }
            if let Some(complexity) = vc.get("max_query_complexity").and_then(serde_json::Value::as_u64) {
                validator = validator.with_max_complexity(complexity as usize);
                info!(max_query_complexity = complexity, "Custom query complexity limit configured");
            }
        }
        state = state.with_validator(validator);

        // Attach debug config from compiled schema
        state.debug_config.clone_from(&self.executor.schema().debug_config);

        // Attach APQ store if configured
        if let Some(ref store) = self.apq_store {
            state = state.with_apq_store(store.clone());
        }

        let metrics = state.metrics.clone();

        // Build GraphQL route (possibly with OIDC auth + Content-Type enforcement)
        // Supports both GET and POST per GraphQL over HTTP spec
        let graphql_router = if let Some(ref validator) = self.oidc_validator {
            info!(
                graphql_path = %self.config.graphql_path,
                "GraphQL endpoint protected by OIDC authentication (GET and POST)"
            );
            let auth_state = OidcAuthState::new(validator.clone());
            let router = Router::new()
                .route(
                    &self.config.graphql_path,
                    get(graphql_get_handler::<A>).post(graphql_handler::<A>),
                )
                .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware));

            if self.config.require_json_content_type {
                router
                    .route_layer(middleware::from_fn(require_json_content_type))
                    .with_state(state.clone())
            } else {
                router.with_state(state.clone())
            }
        } else {
            let router = Router::new()
                .route(
                    &self.config.graphql_path,
                    get(graphql_get_handler::<A>).post(graphql_handler::<A>),
                );

            if self.config.require_json_content_type {
                router
                    .route_layer(middleware::from_fn(require_json_content_type))
                    .with_state(state.clone())
            } else {
                router.with_state(state.clone())
            }
        };

        // Build base routes (always available without auth)
        let mut app = Router::new()
            .route(&self.config.health_path, get(health_handler::<A>))
            .with_state(state.clone())
            .merge(graphql_router);

        // Conditionally add playground route
        if self.config.playground_enabled {
            let playground_state =
                PlaygroundState::new(self.config.graphql_path.clone(), self.config.playground_tool);
            info!(
                playground_path = %self.config.playground_path,
                playground_tool = ?self.config.playground_tool,
                "GraphQL playground enabled"
            );
            let playground_router = Router::new()
                .route(&self.config.playground_path, get(playground_handler))
                .with_state(playground_state);
            app = app.merge(playground_router);
        }

        // Conditionally add subscription route (WebSocket)
        if self.config.subscriptions_enabled {
            let subscription_state = SubscriptionState::new(self.subscription_manager.clone())
                .with_lifecycle(self.subscription_lifecycle.clone())
                .with_max_subscriptions(self.max_subscriptions_per_connection);
            info!(
                subscription_path = %self.config.subscription_path,
                "GraphQL subscriptions enabled (graphql-transport-ws + graphql-ws protocols)"
            );
            let subscription_router = Router::new()
                .route(&self.config.subscription_path, get(subscription_handler))
                .with_state(subscription_state);
            app = app.merge(subscription_router);
        }

        // Conditionally add introspection endpoint (with optional auth)
        if self.config.introspection_enabled {
            if self.config.introspection_require_auth {
                if let Some(ref validator) = self.oidc_validator {
                    info!(
                        introspection_path = %self.config.introspection_path,
                        "Introspection endpoint enabled (OIDC auth required)"
                    );
                    let auth_state = OidcAuthState::new(validator.clone());
                    let introspection_router = Router::new()
                        .route(&self.config.introspection_path, get(introspection_handler::<A>))
                        .route_layer(middleware::from_fn_with_state(
                            auth_state.clone(),
                            oidc_auth_middleware,
                        ))
                        .with_state(state.clone());
                    app = app.merge(introspection_router);

                    // Schema export endpoints follow same auth as introspection
                    let schema_router = Router::new()
                        .route("/api/v1/schema.graphql", get(api::schema::export_sdl_handler::<A>))
                        .route("/api/v1/schema.json", get(api::schema::export_json_handler::<A>))
                        .route_layer(middleware::from_fn_with_state(
                            auth_state,
                            oidc_auth_middleware,
                        ))
                        .with_state(state.clone());
                    app = app.merge(schema_router);
                } else {
                    warn!(
                        "introspection_require_auth is true but no OIDC configured - introspection and schema export disabled"
                    );
                }
            } else {
                info!(
                    introspection_path = %self.config.introspection_path,
                    "Introspection endpoint enabled (no auth required - USE ONLY IN DEVELOPMENT)"
                );
                let introspection_router = Router::new()
                    .route(&self.config.introspection_path, get(introspection_handler::<A>))
                    .with_state(state.clone());
                app = app.merge(introspection_router);

                // Schema export endpoints available without auth when introspection enabled without
                // auth
                let schema_router = Router::new()
                    .route("/api/v1/schema.graphql", get(api::schema::export_sdl_handler::<A>))
                    .route("/api/v1/schema.json", get(api::schema::export_json_handler::<A>))
                    .with_state(state.clone());
                app = app.merge(schema_router);
            }
        }

        // Conditionally add metrics routes (protected by bearer token)
        if self.config.metrics_enabled {
            if let Some(ref token) = self.config.metrics_token {
                info!(
                    metrics_path = %self.config.metrics_path,
                    metrics_json_path = %self.config.metrics_json_path,
                    "Metrics endpoints enabled (bearer token required)"
                );

                let auth_state = BearerAuthState::new(token.clone());

                // Create a separate metrics router with auth middleware applied
                // The routes need relative paths since we use merge (not nest)
                let metrics_router = Router::new()
                    .route(&self.config.metrics_path, get(metrics_handler::<A>))
                    .route(&self.config.metrics_json_path, get(metrics_json_handler::<A>))
                    .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
                    .with_state(state.clone());

                app = app.merge(metrics_router);
            } else {
                warn!(
                    "metrics_enabled is true but metrics_token is not set - metrics endpoints disabled"
                );
            }
        }

        // Conditionally add admin routes (protected by bearer token)
        if self.config.admin_api_enabled {
            if let Some(ref token) = self.config.admin_token {
                info!("Admin API endpoints enabled (bearer token required)");

                let auth_state = BearerAuthState::new(token.clone());

                // Create a separate admin router with auth middleware applied
                let admin_router = Router::new()
                    .route(
                        "/api/v1/admin/reload-schema",
                        post(api::admin::reload_schema_handler::<A>),
                    )
                    .route("/api/v1/admin/cache/clear", post(api::admin::cache_clear_handler::<A>))
                    .route("/api/v1/admin/cache/stats", get(api::admin::cache_stats_handler::<A>))
                    .route("/api/v1/admin/config", get(api::admin::config_handler::<A>))
                    .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
                    .with_state(state.clone());

                app = app.merge(admin_router);
            } else {
                warn!(
                    "admin_api_enabled is true but admin_token is not set - admin endpoints disabled"
                );
            }
        }

        // Conditionally add design audit endpoints (with optional auth)
        if self.config.design_api_require_auth {
            if let Some(ref validator) = self.oidc_validator {
                info!("Design audit API endpoints enabled (OIDC auth required)");
                let auth_state = OidcAuthState::new(validator.clone());
                let design_router = Router::new()
                    .route(
                        "/design/federation-audit",
                        post(api::design::federation_audit_handler::<A>),
                    )
                    .route("/design/cost-audit", post(api::design::cost_audit_handler::<A>))
                    .route("/design/cache-audit", post(api::design::cache_audit_handler::<A>))
                    .route("/design/auth-audit", post(api::design::auth_audit_handler::<A>))
                    .route(
                        "/design/compilation-audit",
                        post(api::design::compilation_audit_handler::<A>),
                    )
                    .route("/design/audit", post(api::design::overall_design_audit_handler::<A>))
                    .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
                    .with_state(state.clone());
                app = app.nest("/api/v1", design_router);
            } else {
                warn!(
                    "design_api_require_auth is true but no OIDC configured - design endpoints unprotected"
                );
                // Add unprotected design endpoints
                let design_router = Router::new()
                    .route(
                        "/design/federation-audit",
                        post(api::design::federation_audit_handler::<A>),
                    )
                    .route("/design/cost-audit", post(api::design::cost_audit_handler::<A>))
                    .route("/design/cache-audit", post(api::design::cache_audit_handler::<A>))
                    .route("/design/auth-audit", post(api::design::auth_audit_handler::<A>))
                    .route(
                        "/design/compilation-audit",
                        post(api::design::compilation_audit_handler::<A>),
                    )
                    .route("/design/audit", post(api::design::overall_design_audit_handler::<A>))
                    .with_state(state.clone());
                app = app.nest("/api/v1", design_router);
            }
        } else {
            info!("Design audit API endpoints enabled (no auth required)");
            let design_router = Router::new()
                .route("/design/federation-audit", post(api::design::federation_audit_handler::<A>))
                .route("/design/cost-audit", post(api::design::cost_audit_handler::<A>))
                .route("/design/cache-audit", post(api::design::cache_audit_handler::<A>))
                .route("/design/auth-audit", post(api::design::auth_audit_handler::<A>))
                .route(
                    "/design/compilation-audit",
                    post(api::design::compilation_audit_handler::<A>),
                )
                .route("/design/audit", post(api::design::overall_design_audit_handler::<A>))
                .with_state(state.clone());
            app = app.nest("/api/v1", design_router);
        }

        // PKCE OAuth2 auth routes — mounted only when both pkce and [auth] are configured.
        if let (Some(store), Some(client)) = (&self.pkce_store, &self.oidc_server_client) {
            let auth_state = Arc::new(AuthPkceState {
                pkce_store:              Arc::clone(store),
                oidc_client:             Arc::clone(client),
                http_client:             Arc::new(reqwest::Client::new()),
                post_login_redirect_uri: None,
            });
            let auth_router = Router::new()
                .route("/auth/start",    get(auth_start))
                .route("/auth/callback", get(auth_callback))
                .with_state(auth_state);
            app = app.merge(auth_router);
            info!("PKCE auth routes mounted: GET /auth/start, GET /auth/callback");
        }

        // Token revocation routes — mounted only when revocation is configured.
        if let Some(ref rev_mgr) = self.revocation_manager {
            let rev_state = Arc::new(crate::routes::RevocationRouteState {
                revocation_manager: Arc::clone(rev_mgr),
            });
            let rev_router = Router::new()
                .route("/auth/revoke",     post(crate::routes::revoke_token))
                .route("/auth/revoke-all", post(crate::routes::revoke_all_tokens))
                .with_state(rev_state);
            app = app.merge(rev_router);
            info!("Token revocation routes mounted: POST /auth/revoke, POST /auth/revoke-all");
        }

        // MCP (Model Context Protocol) route — mounted when mcp feature is compiled in
        // and mcp_config is present.
        #[cfg(feature = "mcp")]
        if let Some(ref mcp_cfg) = self.mcp_config {
            if mcp_cfg.transport == "http" || mcp_cfg.transport == "both" {
                use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
                use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

                let schema = Arc::new(self.executor.schema().clone());
                let executor = self.executor.clone();
                let cfg = mcp_cfg.clone();
                let mcp_service = StreamableHttpService::new(
                    move || {
                        Ok(crate::mcp::handler::FraiseQLMcpService::new(
                            schema.clone(),
                            executor.clone(),
                            cfg.clone(),
                        ))
                    },
                    Arc::new(LocalSessionManager::default()),
                    StreamableHttpServerConfig::default(),
                );
                app = app.nest_service(&mcp_cfg.path, mcp_service);
                info!(path = %mcp_cfg.path, "MCP HTTP endpoint mounted");
            }
        }

        // Remaining API routes (query intelligence, federation)
        let api_router = api::routes(state.clone());
        app = app.nest("/api/v1", api_router);

        // RBAC Management API (if database pool available)
        #[cfg(feature = "observers")]
        if let Some(ref db_pool) = self.db_pool {
            info!("Adding RBAC Management API endpoints");
            let rbac_backend = Arc::new(
                crate::api::rbac_management::db_backend::RbacDbBackend::new(db_pool.clone()),
            );
            let rbac_state = crate::api::RbacManagementState { db: rbac_backend };
            let rbac_router = crate::api::rbac_management_router(rbac_state);
            app = app.merge(rbac_router);
        }

        // Add HTTP metrics middleware (tracks requests and response status codes)
        // This runs on ALL routes, even when metrics endpoints are disabled
        app = app.layer(middleware::from_fn_with_state(metrics, metrics_middleware));

        // Observer routes (if enabled and compiled with feature)
        #[cfg(feature = "observers")]
        {
            app = self.add_observer_routes(app);
        }

        // Add middleware
        if self.config.tracing_enabled {
            app = app.layer(trace_layer());
        }

        if self.config.cors_enabled {
            // Use restricted CORS with configured origins
            let origins = if self.config.cors_origins.is_empty() {
                // Default to localhost for development if no origins configured
                tracing::warn!(
                    "CORS enabled but no origins configured. Using localhost:3000 as default. \
                     Set cors_origins in config for production."
                );
                vec!["http://localhost:3000".to_string()]
            } else {
                self.config.cors_origins.clone()
            };
            app = app.layer(cors_layer_restricted(origins));
        }

        // Add request body size limit (default 1 MB — prevents memory exhaustion)
        if self.config.max_request_body_bytes > 0 {
            info!(
                max_bytes = self.config.max_request_body_bytes,
                "Request body size limit enabled"
            );
            app = app.layer(DefaultBodyLimit::max(self.config.max_request_body_bytes));
        }

        // Add rate limiting middleware if configured
        if let Some(ref limiter) = self.rate_limiter {
            use std::net::SocketAddr;

            use axum::extract::ConnectInfo;

            info!("Enabling rate limiting middleware");
            let limiter_clone = limiter.clone();
            app = app.layer(middleware::from_fn(move |ConnectInfo(addr): ConnectInfo<SocketAddr>, req, next: axum::middleware::Next| {
                let limiter = limiter_clone.clone();
                async move {
                    let ip = addr.ip().to_string();

                    // Check rate limit
                    let check = limiter.check_ip_limit(&ip).await;
                    if !check.allowed {
                        warn!(ip = %ip, "IP rate limit exceeded");
                        use axum::http::StatusCode;
                        use axum::response::IntoResponse;
                        let retry = check.retry_after_secs;
                        let retry_str = retry.to_string();
                        let body = format!(
                            r#"{{"errors":[{{"message":"Rate limit exceeded. Please retry after {retry} second{s}."}}]}}"#,
                            s = if retry == 1 { "" } else { "s" }
                        );
                        return (
                            StatusCode::TOO_MANY_REQUESTS,
                            [("Content-Type", "application/json"), ("Retry-After", retry_str.as_str())],
                            body,
                        ).into_response();
                    }

                    // Get remaining tokens for headers
                    let remaining = check.remaining;
                    let mut response = next.run(req).await;

                    // Add rate limit headers
                    let headers = response.headers_mut();
                    if let Ok(limit_value) = format!("{}", limiter.config().rps_per_ip).parse() {
                        headers.insert("X-RateLimit-Limit", limit_value);
                    }
                    if let Ok(remaining_value) = format!("{}", remaining as u32).parse() {
                        headers.insert("X-RateLimit-Remaining", remaining_value);
                    }

                    response
                }
            }));
        }

        app
    }

    /// Add observer-related routes to the router
    #[cfg(feature = "observers")]
    fn add_observer_routes(&self, app: Router) -> Router {
        use crate::observers::{
            ObserverRepository, ObserverState, RuntimeHealthState, observer_routes,
            observer_runtime_routes,
        };

        // Management API (always available with feature)
        let observer_state = ObserverState {
            repository: ObserverRepository::new(
                self.db_pool.clone().expect("Pool required for observers"),
            ),
        };

        let app = app.nest("/api/observers", observer_routes(observer_state));

        // Runtime health API (only if runtime present)
        if let Some(ref runtime) = self.observer_runtime {
            info!(
                path = "/api/observers",
                "Observer management and runtime health endpoints enabled"
            );

            let runtime_state = RuntimeHealthState {
                runtime: runtime.clone(),
            };

            app.merge(observer_runtime_routes(runtime_state))
        } else {
            app
        }
    }

    /// Start server and listen for requests.
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    pub async fn serve(self) -> Result<()> {
        self.serve_with_shutdown(Self::shutdown_signal()).await
    }

    /// Start server with a custom shutdown future.
    ///
    /// Enables programmatic shutdown (e.g., for `--watch` hot-reload) by accepting any
    /// future that resolves when the server should stop.
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    pub async fn serve_with_shutdown<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let app = self.build_router();

        // Initialize TLS setup
        let tls_setup = TlsSetup::new(self.config.tls.clone(), self.config.database_tls.clone())?;

        info!(
            bind_addr = %self.config.bind_addr,
            graphql_path = %self.config.graphql_path,
            tls_enabled = tls_setup.is_tls_enabled(),
            "Starting FraiseQL server"
        );

        // Start observer runtime if configured
        #[cfg(feature = "observers")]
        if let Some(ref runtime) = self.observer_runtime {
            info!("Starting observer runtime...");
            let mut guard = runtime.write().await;

            match guard.start().await {
                Ok(()) => info!("Observer runtime started"),
                Err(e) => {
                    error!("Failed to start observer runtime: {}", e);
                    warn!("Server will continue without observers");
                },
            }
            drop(guard);
        }

        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindError(e.to_string()))?;

        // Log TLS configuration
        if tls_setup.is_tls_enabled() {
            // Verify TLS setup is valid (will error if certificates are missing/invalid)
            let _ = tls_setup.create_rustls_config()?;
            info!(
                cert_path = ?tls_setup.cert_path(),
                key_path = ?tls_setup.key_path(),
                mtls_required = tls_setup.is_mtls_required(),
                "Server TLS configuration loaded (note: use reverse proxy for server-side TLS termination)"
            );
        }

        // Log database TLS configuration
        info!(
            postgres_ssl_mode = tls_setup.postgres_ssl_mode(),
            redis_ssl = tls_setup.redis_ssl_enabled(),
            clickhouse_https = tls_setup.clickhouse_https_enabled(),
            elasticsearch_https = tls_setup.elasticsearch_https_enabled(),
            "Database connection TLS configuration applied"
        );

        info!("Server listening on http://{}", self.config.bind_addr);

        // Start both HTTP and gRPC servers concurrently if Arrow Flight is enabled
        #[cfg(feature = "arrow")]
        if let Some(flight_service) = self.flight_service {
            // Flight server runs on port 50051
            let flight_addr = "0.0.0.0:50051".parse().expect("Valid Flight address");
            info!("Arrow Flight server listening on grpc://{}", flight_addr);

            // Spawn Flight server in background
            let flight_server = tokio::spawn(async move {
                tonic::transport::Server::builder()
                    .add_service(flight_service.into_server())
                    .serve(flight_addr)
                    .await
            });

            // Wrap the user-supplied shutdown future so we can also stop observer runtime
            #[cfg(feature = "observers")]
            let observer_runtime = self.observer_runtime.clone();

            let shutdown_with_cleanup = async move {
                shutdown.await;
                #[cfg(feature = "observers")]
                if let Some(ref runtime) = observer_runtime {
                    info!("Shutting down observer runtime");
                    let mut guard = runtime.write().await;
                    if let Err(e) = guard.stop().await {
                        #[cfg(feature = "observers")]
                        error!("Error stopping runtime: {}", e);
                    } else {
                        info!("Runtime stopped cleanly");
                    }
                }
            };

            // Run HTTP server with graceful shutdown
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_with_cleanup)
                .await
                .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;

            // Abort Flight server after HTTP server exits
            flight_server.abort();
        }

        // HTTP-only server (when arrow feature not enabled)
        #[cfg(not(feature = "arrow"))]
        {
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown)
                .await
                .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;
        }

        Ok(())
    }

    /// Listen for shutdown signals (Ctrl+C or SIGTERM)
    pub async fn shutdown_signal() {
        use tokio::signal;

        let ctrl_c = async {
            signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => info!("Received Ctrl+C"),
            _ = terminate => info!("Received SIGTERM"),
        }
    }
}
