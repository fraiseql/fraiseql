//! REST API routes for query intelligence, federation discovery, and admin operations.
//!
//! All API endpoints are under `/api/v1/` and return structured JSON responses.

use axum::{
    routing::{get, post},
    Router,
};
use fraiseql_core::db::traits::DatabaseAdapter;

pub mod types;
pub mod query;
pub mod federation;
pub mod schema;

// Re-export commonly used types
pub use types::{ApiResponse, ApiError};

/// Build API router with all v1 endpoints.
///
/// Generic over the database adapter type used by the executor.
pub fn routes<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: crate::routes::graphql::AppState<A>,
) -> Router {
    Router::new()
        // Query intelligence endpoints
        .route("/query/explain", post(query::explain_handler::<A>))
        .route("/query/validate", post(query::validate_handler::<A>))
        .route("/query/stats", get(query::stats_handler::<A>))
        // Federation endpoints
        .route("/federation/subgraphs", get(federation::subgraphs_handler::<A>))
        .route("/federation/graph", get(federation::graph_handler::<A>))
        // Schema export endpoints
        .route("/schema.graphql", get(schema::export_sdl_handler::<A>))
        .route("/schema.json", get(schema::export_json_handler::<A>))
        .with_state(state)
}
