//! GraphQL endpoint route construction with optional authentication.

use axum::{Router, middleware, routing::get};
use fraiseql_core::db::traits::DatabaseAdapter;
use tower_http::compression::{CompressionLayer, predicate::SizeAbove};

use super::{
    super::{Server, graphql_get_handler, graphql_handler, require_json_content_type},
    AuthPosture,
};
use crate::routes::graphql::AppState;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build the GraphQL endpoint router with optional auth and compression.
    pub(super) fn build_graphql_router(&self, state: &AppState<A>) -> Router {
        // Build GraphQL route (possibly with auth + Content-Type enforcement).
        // Supports both GET and POST per GraphQL over HTTP spec.
        // OIDC and HS256 are mutually exclusive (enforced by ServerConfig::validate).
        // Authentication is attached by the shared `attach_auth` helper rather than
        // inline here: `/graphql` and the REST transport had independent mount code, and
        // REST's omitted the layer entirely (#812). One helper, one posture decision.
        let router = self.attach_auth(
            Router::new().route(
                &self.config.graphql_path,
                get(graphql_get_handler::<A>).post(graphql_handler::<A>),
            ),
            AuthPosture::Authenticated,
            "graphql",
        );

        let graphql_router = if self.config.require_json_content_type {
            router
                .route_layer(middleware::from_fn(require_json_content_type))
                .with_state(state.clone())
        } else {
            router.with_state(state.clone())
        };

        // Apply framework-level compression if enabled.
        // Disabled by default: in production, prefer reverse-proxy compression
        // (Nginx, Caddy, cloud LB) which offloads CPU and supports brotli.
        // When enabled, skip responses under 1 KiB — gzip overhead dominates
        // on tiny payloads (e.g. short GraphQL results, health responses).
        if self.config.compression_enabled {
            graphql_router.layer(CompressionLayer::new().compress_when(SizeAbove::new(1024)))
        } else {
            graphql_router
        }
    }
}
