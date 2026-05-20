//! GraphQL endpoint route construction with optional authentication.

use axum::{Router, middleware, routing::get};
use fraiseql_core::db::traits::DatabaseAdapter;
use tower_http::compression::{CompressionLayer, predicate::SizeAbove};
use tracing::info;

use super::super::{
    OidcAuthState, Server, graphql_get_handler, graphql_handler, oidc_auth_middleware,
    require_json_content_type,
};
use crate::{
    middleware::{Hs256AuthState, hs256_auth_middleware},
    routes::graphql::AppState,
};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build the GraphQL endpoint router with optional auth and compression.
    pub(super) fn build_graphql_router(&self, state: &AppState<A>) -> Router {
        // Build GraphQL route (possibly with auth + Content-Type enforcement).
        // Supports both GET and POST per GraphQL over HTTP spec.
        // OIDC and HS256 are mutually exclusive (enforced by ServerConfig::validate).
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
        } else if let Some(ref validator) = self.hs256_auth {
            info!(
                graphql_path = %self.config.graphql_path,
                "GraphQL endpoint protected by HS256 authentication (GET and POST)"
            );
            let realm = self
                .config
                .auth_hs256
                .as_ref()
                .and_then(|h| h.issuer.clone())
                .unwrap_or_else(|| "fraiseql".to_string());
            let auth_state = Hs256AuthState::new(validator.clone(), realm);
            let router = Router::new()
                .route(
                    &self.config.graphql_path,
                    get(graphql_get_handler::<A>).post(graphql_handler::<A>),
                )
                .route_layer(middleware::from_fn_with_state(auth_state, hs256_auth_middleware));

            if self.config.require_json_content_type {
                router
                    .route_layer(middleware::from_fn(require_json_content_type))
                    .with_state(state.clone())
            } else {
                router.with_state(state.clone())
            }
        } else {
            let router = Router::new().route(
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
