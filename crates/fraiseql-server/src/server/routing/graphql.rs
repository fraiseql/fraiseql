//! GraphQL endpoint route construction with optional authentication.

use axum::{Router, middleware, routing::get};
use fraiseql_core::db::traits::DatabaseAdapter;
use tower_http::compression::{
    CompressionLayer,
    predicate::{NotForContentType, Predicate as _, SizeAbove},
};

use super::{
    super::{Server, graphql_get_handler, graphql_handler, require_json_content_type},
    AuthPosture,
};
use crate::routes::graphql::{AppState, handler::graphql_query_method_handler};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build the GraphQL endpoint router with optional auth and compression.
    pub(super) fn build_graphql_router(&self, state: &AppState<A>) -> Router {
        // Build GraphQL route (possibly with auth + Content-Type enforcement).
        // Supports both GET and POST per GraphQL over HTTP spec.
        // OIDC and HS256 are mutually exclusive (enforced by ServerConfig::validate).
        // Authentication is attached by the shared `attach_auth` helper rather than
        // inline here: `/graphql` and the REST transport had independent mount code, and
        // REST's omitted the layer entirely (#812). One helper, one posture decision.
        // GET and POST are mounted exactly as before. The HTTP QUERY method
        // (RFC 10008, #508) is added as a `MethodRouter` **fallback** rather than a
        // typed arm, because axum 0.8 has no `MethodFilter::QUERY` yet — a fallback
        // keeps GET/POST dispatch byte-for-byte unchanged, where an `any()` +
        // manual-match rewrite would put every method through new code.
        //
        // The fallback also catches PUT/DELETE/…; the handler answers 405 for
        // anything that is not QUERY, which is what the router did before.
        let mut methods = get(graphql_get_handler::<A>).post(graphql_handler::<A>);
        if self.config.enable_http_query {
            methods = methods.fallback(graphql_query_method_handler::<A>);
        }

        let router = self.attach_auth(
            Router::new().route(&self.config.graphql_path, methods),
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
        //
        // `compress_when` REPLACES tower-http's default predicate, which is what
        // normally exempts `text/event-stream`; without re-composing that
        // exemption here, a large SSE response (#387) would be buffered by the
        // encoder and its events would stop flushing incrementally.
        //
        // `multipart/mixed` is the same delivery under a different framing (#958)
        // and needs the same exemption — an incremental transport that arrives all at
        // once is not one. `NotForContentType::SSE` is a `text/event-stream`-only
        // constant, so the multipart exemption is spelled out beside it rather than
        // assumed.
        if self.config.compression_enabled {
            graphql_router.layer(
                CompressionLayer::new().compress_when(
                    SizeAbove::new(1024)
                        .and(NotForContentType::SSE)
                        .and(NotForContentType::const_new("multipart/mixed")),
                ),
            )
        } else {
            graphql_router
        }
    }
}
