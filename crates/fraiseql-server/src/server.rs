//! HTTP server implementation.

use axum::{
    routing::{get, post},
    Router,
};
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor, schema::CompiledSchema};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

use crate::{
    config::ServerConfig,
    middleware::{cors_layer, trace_layer},
    routes::{graphql::AppState, graphql_handler, health_handler, introspection_handler},
    Result, ServerError,
};

/// FraiseQL HTTP Server.
pub struct Server<A: DatabaseAdapter> {
    config: ServerConfig,
    executor: Arc<Executor<A>>,
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
    /// # Example
    ///
    /// ```rust,ignore
    /// let config = ServerConfig::default();
    /// let schema = CompiledSchema::from_json(schema_json)?;
    /// let adapter = Arc::new(PostgresAdapter::new(db_url).await?);
    ///
    /// let server = Server::new(config, schema, adapter);
    /// server.serve().await?;
    /// ```
    #[must_use]
    pub fn new(config: ServerConfig, schema: CompiledSchema, adapter: Arc<A>) -> Self {
        let executor = Arc::new(Executor::new(schema, adapter));

        Self { config, executor }
    }

    /// Build application router.
    fn build_router(&self) -> Router {
        let state = AppState::new(self.executor.clone());

        let mut app = Router::new()
            .route(&self.config.graphql_path, post(graphql_handler::<A>))
            .route(&self.config.health_path, get(health_handler::<A>))
            .route(
                &self.config.introspection_path,
                get(introspection_handler::<A>),
            )
            .with_state(state);

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

        info!(
            "Server listening on http://{}",
            self.config.bind_addr
        );

        axum::serve(listener, app)
            .await
            .map_err(|e| ServerError::IoError(std::io::Error::other(e)))?;

        Ok(())
    }
}

// TODO: Add server tests
// Note: Unit tests deferred due to async_trait lifetime issues with MockAdapter.
// Will add integration tests in future commits using real database adapters.
