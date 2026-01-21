//! HTTP server implementation.

use std::sync::Arc;

use axum::{Router, middleware, routing::get};
use fraiseql_core::{
    db::traits::DatabaseAdapter, runtime::{Executor, SubscriptionManager}, schema::CompiledSchema,
    security::OidcValidator,
};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::{
    Result, ServerError,
    server_config::ServerConfig,
    middleware::{
        BearerAuthState, OidcAuthState, bearer_auth_middleware, cors_layer, metrics_middleware,
        oidc_auth_middleware, trace_layer,
    },
    routes::{
        PlaygroundState, SubscriptionState, graphql::AppState, graphql_get_handler, graphql_handler,
        health_handler, introspection_handler, metrics_handler, metrics_json_handler, playground_handler,
        subscription_handler,
    },
};

/// FraiseQL HTTP Server.
pub struct Server<A: DatabaseAdapter> {
    config:                ServerConfig,
    executor:              Arc<Executor<A>>,
    subscription_manager:  Arc<SubscriptionManager>,
    oidc_validator:        Option<Arc<OidcValidator>>,
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Create new server.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `schema` - Compiled GraphQL schema
    /// * `adapter` - Database adapter
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
    /// let server = Server::new(config, schema, adapter).await?;
    /// server.serve().await?;
    /// ```
    pub async fn new(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
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

        Ok(Self {
            config,
            executor,
            subscription_manager,
            oidc_validator,
        })
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
                    .with_state(state);

                app = app.merge(metrics_router);
            } else {
                warn!(
                    "metrics_enabled is true but metrics_token is not set - metrics endpoints disabled"
                );
            }
        }

        // Add HTTP metrics middleware (tracks requests and response status codes)
        // This runs on ALL routes, even when metrics endpoints are disabled
        app = app.layer(middleware::from_fn_with_state(metrics, metrics_middleware));

        // Add middleware
        if self.config.tracing_enabled {
            app = app.layer(trace_layer());
        }

        if self.config.cors_enabled {
            app = app.layer(cors_layer());
        }

        app
    }

    /// Start server and listen for requests.
    ///
    /// # Errors
    ///
    /// Returns error if server fails to bind or encounters runtime errors.
    pub async fn serve(self) -> Result<()> {
        let app = self.build_router();

        info!(
            bind_addr = %self.config.bind_addr,
            graphql_path = %self.config.graphql_path,
            "Starting FraiseQL server"
        );

        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| ServerError::BindError(e.to_string()))?;

        info!("Server listening on http://{}", self.config.bind_addr);

        axum::serve(listener, app)
            .await
            .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;

        Ok(())
    }
}

// TODO: Add server tests
// Note: Unit tests deferred due to async_trait lifetime issues with MockAdapter.
// Will add integration tests in future commits using real database adapters.
