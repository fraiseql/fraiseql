//! Server extensions: relay pagination, Arrow Flight service, and observer runtime
//! initialization.

use super::*;

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
    /// let server = Server::with_relay_pagination(config, schema, adapter, None).await?;
    /// server.serve_mut().await?; // or server.serve() for read-only mode
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_relay_pagination(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        db_pool: Option<sqlx::PgPool>,
    ) -> Result<Self> {
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
        let revocation_manager = crate::token_revocation::revocation_manager_from_schema(&schema);
        let trusted_docs = Self::trusted_docs_from_schema(&schema);

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
            trusted_docs,
            db_pool,
        )
        .await?;

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
        #[allow(unused_variables)] // Reason: db_pool is only used when the "observers" or "arrow" features are enabled
        db_pool: Option<sqlx::PgPool>,
        flight_service: Option<FraiseQLFlightService>,
    ) -> Result<Self> {
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
        let revocation_manager = crate::token_revocation::revocation_manager_from_schema(&schema);
        let trusted_docs = Self::trusted_docs_from_schema(&schema);

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

        // Warn if PKCE is configured but [auth] is missing.
        #[cfg(feature = "auth")]
        if pkce_store.is_some() && oidc_server_client.is_none() {
            tracing::error!(
                "pkce.enabled = true but [auth] is not configured or OIDC client init failed. \
                 Auth routes will NOT be mounted."
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

        let apq_enabled = config.apq_enabled;

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
            apq_store: if apq_enabled {
                Some(Arc::new(fraiseql_core::apq::InMemoryApqStorage::default())
                    as fraiseql_core::apq::ArcApqStorage)
            } else {
                None
            },
            trusted_docs,
            #[cfg(feature = "mcp")]
            mcp_config: None,
            pool_tuning_config: None,
            #[cfg(feature = "observers")]
            observer_runtime,
            #[cfg(feature = "observers")]
            db_pool,
            flight_service,
            #[cfg(feature = "grpc")]
            grpc_service: None,
            #[cfg(feature = "grpc")]
            grpc_reflection_bytes: None,
        })
    }

    /// Initialize observer runtime from configuration
    #[cfg(feature = "observers")]
    pub(super) async fn init_observer_runtime(
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
}
