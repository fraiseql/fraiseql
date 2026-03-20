//! Server constructors and builder methods.

use super::*;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
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
    /// * `db_pool` — forwarded to the observer runtime; `None` when observers are disabled.
    ///
    /// # Errors
    ///
    /// Returns error if OIDC validator initialization fails (e.g., unable to
    /// fetch discovery document or JWKS).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Requires a running PostgreSQL database and a compiled schema file on disk.
    /// use std::sync::Arc;
    /// use fraiseql_server::{Server, ServerConfig};
    /// use fraiseql_core::schema::CompiledSchema;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = ServerConfig::default();
    /// let schema = CompiledSchema::from_json(schema_json)?;
    /// let adapter = Arc::new(PostgresAdapter::new(db_url).await?);
    ///
    /// let server = Server::new(config, schema, adapter, None).await?;
    /// server.serve_mut().await?; // or server.serve() for read-only mode
    /// # Ok(())
    /// # }
    /// ```
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
        let circuit_breaker = schema.federation.as_ref().and_then(
            crate::federation::circuit_breaker::FederationCircuitBreakerManager::from_config,
        );
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
                     data leakage. Either disable caching, declare RLS policies, or set \
                     `security.multi_tenant = false` to acknowledge single-tenant mode."
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

        // Read subscription config from compiled schema (hooks, limits).
        let subscriptions_config = schema.subscriptions_config.clone();

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
            trusted_docs,
            db_pool,
        )
        .await?;

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

        // Initialize gRPC service from compiled schema when the feature is compiled in.
        #[cfg(feature = "grpc")]
        {
            match crate::routes::grpc::build_grpc_service(
                Arc::new(server.executor.schema().clone()),
                server.executor.adapter().clone(),
                server.oidc_validator.clone(),
                server.rate_limiter.clone(),
            ) {
                Ok(Some(grpc_services)) => {
                    info!(service = %grpc_services.service_name, "gRPC transport service initialized");
                    server.grpc_service = Some(grpc_services.service);
                    server.grpc_reflection_bytes = grpc_services.reflection_descriptor_bytes;
                },
                Ok(None) => {
                    // gRPC not configured or disabled — no-op.
                },
                Err(e) => {
                    warn!("gRPC transport initialization failed: {e}");
                },
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

    /// Shared initialization path used by both `new` and `with_relay_pagination`.
    ///
    /// Accepts a pre-built executor so that relay vs. non-relay constructors can supply
    /// the appropriate variant without duplicating auth/rate-limiter/observer setup.
    #[allow(clippy::too_many_arguments)] // Reason: internal constructor that collects all pre-built subsystems; callers pass
    // already-constructed values rather than building them here, so grouping into a
    // builder struct would not reduce call-site clarity.
    pub(super) async fn from_executor(
        config: ServerConfig,
        executor: Arc<Executor<A>>,
        subscription_manager: Arc<SubscriptionManager>,
        circuit_breaker: Option<
            Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>,
        >,
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

        Ok(Self {
            config,
            executor,
            subscription_manager,
            subscription_lifecycle: Arc::new(crate::subscriptions::NoopLifecycle),
            max_subscriptions_per_connection: None,
            oidc_validator,
            rate_limiter,
            #[cfg(feature = "secrets")]
            secrets_manager: None,
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
            #[cfg(feature = "grpc")]
            grpc_service: None,
            #[cfg(feature = "grpc")]
            grpc_reflection_bytes: None,
            #[cfg(feature = "mcp")]
            mcp_config: None,
            pool_tuning_config: None,
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
    pub const fn with_max_subscriptions_per_connection(mut self, max: u32) -> Self {
        self.max_subscriptions_per_connection = Some(max);
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

        use rmcp::ServiceExt;
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
