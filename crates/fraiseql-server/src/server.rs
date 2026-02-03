//! HTTP server implementation.

use std::sync::Arc;

use axum::{Router, middleware, routing::get};
#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, SubscriptionManager},
    schema::CompiledSchema,
    security::OidcValidator,
};
use tokio::net::TcpListener;
#[cfg(feature = "observers")]
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
        BearerAuthState, OidcAuthState, bearer_auth_middleware, cors_layer_restricted,
        metrics_middleware, oidc_auth_middleware, trace_layer,
    },
    routes::{
        PlaygroundState, SubscriptionState, graphql::AppState, graphql_get_handler,
        graphql_handler, health_handler, introspection_handler, metrics_handler,
        metrics_json_handler, playground_handler, subscription_handler, api,
    },
    server_config::ServerConfig,
    tls::TlsSetup,
};

/// FraiseQL HTTP Server.
pub struct Server<A: DatabaseAdapter> {
    config:               ServerConfig,
    executor:             Arc<Executor<A>>,
    subscription_manager: Arc<SubscriptionManager>,
    oidc_validator:       Option<Arc<OidcValidator>>,

    #[cfg(feature = "observers")]
    observer_runtime: Option<Arc<RwLock<ObserverRuntime>>>,

    #[cfg(feature = "observers")]
    db_pool: Option<sqlx::PgPool>,

    #[cfg(feature = "arrow")]
    flight_service: Option<FraiseQLFlightService>,
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Create new server.
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

        // Initialize observer runtime
        #[cfg(feature = "observers")]
        let observer_runtime = Self::init_observer_runtime(&config, db_pool.as_ref()).await;

        // Initialize Flight service (with placeholder data by default)
        #[cfg(feature = "arrow")]
        let flight_service = Some(FraiseQLFlightService::new());

        Ok(Self {
            config,
            executor,
            subscription_manager,
            oidc_validator,
            #[cfg(feature = "observers")]
            observer_runtime,
            #[cfg(feature = "observers")]
            db_pool,
            #[cfg(feature = "arrow")]
            flight_service,
        })
    }

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

        // Initialize observer runtime
        #[cfg(feature = "observers")]
        let observer_runtime = Self::init_observer_runtime(&config, db_pool.as_ref()).await;

        Ok(Self {
            config,
            executor,
            subscription_manager,
            oidc_validator,
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
        let state = AppState::new(self.executor.clone());
        let metrics = state.metrics.clone();

        // Build GraphQL route (possibly with OIDC auth)
        // Supports both GET and POST per GraphQL over HTTP spec
        let graphql_router = if let Some(ref validator) = self.oidc_validator {
            info!(
                graphql_path = %self.config.graphql_path,
                "GraphQL endpoint protected by OIDC authentication (GET and POST)"
            );
            let auth_state = OidcAuthState::new(validator.clone());
            Router::new()
                .route(
                    &self.config.graphql_path,
                    get(graphql_get_handler::<A>).post(graphql_handler::<A>),
                )
                .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
                .with_state(state.clone())
        } else {
            Router::new()
                .route(
                    &self.config.graphql_path,
                    get(graphql_get_handler::<A>).post(graphql_handler::<A>),
                )
                .with_state(state.clone())
        };

        // Build base routes (always available without auth)
        let mut app = Router::new()
            .route(&self.config.health_path, get(health_handler::<A>))
            .route(&self.config.introspection_path, get(introspection_handler::<A>))
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
            let subscription_state = SubscriptionState::new(self.subscription_manager.clone());
            info!(
                subscription_path = %self.config.subscription_path,
                "GraphQL subscriptions enabled (graphql-ws protocol)"
            );
            let subscription_router = Router::new()
                .route(&self.config.subscription_path, get(subscription_handler))
                .with_state(subscription_state);
            app = app.merge(subscription_router);
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

        // Design quality audit API routes
        info!("Design audit API endpoints available at /api/v1/design/*");
        let api_router = api::routes(state.clone());
        app = app.nest("/api/v1", api_router);

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

            // Run HTTP server with graceful shutdown
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    Self::shutdown_signal().await;

                    // Stop observer runtime
                    #[cfg(feature = "observers")]
                    if let Some(ref runtime) = self.observer_runtime {
                        info!("Shutting down observer runtime");
                        let mut guard = runtime.write().await;
                        if let Err(e) = guard.stop().await {
                            error!("Error stopping runtime: {}", e);
                        } else {
                            info!("Runtime stopped cleanly");
                        }
                    }
                })
                .await
                .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;

            // Wait for Flight server to shut down
            flight_server.abort();
        }

        // HTTP-only server (when arrow feature not enabled)
        #[cfg(not(feature = "arrow"))]
        {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    Self::shutdown_signal().await;

                    // Stop observer runtime
                    #[cfg(feature = "observers")]
                    if let Some(ref runtime) = self.observer_runtime {
                        info!("Shutting down observer runtime");
                        let mut guard = runtime.write().await;
                        if let Err(e) = guard.stop().await {
                            error!("Error stopping runtime: {}", e);
                        } else {
                            info!("Runtime stopped cleanly");
                        }
                    }
                })
                .await
                .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;
        }

        Ok(())
    }

    /// Listen for shutdown signals (Ctrl+C or SIGTERM)
    async fn shutdown_signal() {
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
