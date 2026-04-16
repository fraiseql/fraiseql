//! REST API routes for query intelligence, federation discovery, and admin operations.
//!
//! All API endpoints are under `/api/v1/` and return structured JSON responses.

use axum::{
    Router,
    routing::{get, post},
};
use fraiseql_core::db::traits::DatabaseAdapter;

pub mod admin;
pub mod design;
#[cfg(feature = "federation")]
pub mod federation;
pub mod openapi;
pub mod query;
pub mod schema;
pub mod tenant_admin;
pub mod types;

// Re-export commonly used types
pub use types::{ApiError, ApiResponse};

/// Build API router with all v1 endpoints.
///
/// Generic over the database adapter type used by the executor.
pub fn routes<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: crate::routes::graphql::AppState<A>,
) -> Router {
    #[allow(unused_mut)]
    // Reason: mutability required when federation feature is enabled to add federation routes
    let mut router = Router::new()
        // Query intelligence endpoints
        // NOTE: /query/explain is intentionally omitted here — it is mounted
        // in server/routing.rs under the admin bearer-auth router to prevent
        // unauthenticated access to query plan details (H13).
        .route("/query/validate", post(query::validate_handler::<A>))
        .route("/query/stats", get(query::stats_handler::<A>));

    // Federation endpoints
    #[cfg(feature = "federation")]
    {
        router = router
            .route("/federation/subgraphs", get(federation::subgraphs_handler::<A>))
            .route("/federation/graph", get(federation::graph_handler::<A>));
    }

    // Schema export endpoints are now conditionally added in server.rs with optional auth
    // Admin endpoints are now conditionally added in server.rs with auth middleware
    // Design audit endpoints are now conditionally added in server.rs with optional auth
    router.with_state(state)
}
