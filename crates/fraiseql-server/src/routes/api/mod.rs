//! REST API routes for query intelligence, federation discovery, and admin operations.
//!
//! All API endpoints are under `/api/v1/` and return structured JSON responses.

use axum::{
    Router,
    routing::{get, post},
};

pub mod admin;
/// The operator SQL console (#962) — compiled in only with the `admin-sql`
/// feature, because it executes SQL the operator typed.
#[cfg(feature = "admin-sql")]
pub mod admin_sql;
pub mod design;
#[cfg(feature = "federation")]
pub mod federation;
pub mod metadata;
pub mod openapi;
pub mod query;
pub mod query_stats;
pub mod schema;
pub mod storage_policies;
pub mod tenant_admin;
pub mod types;
pub mod usage;

// Re-export commonly used types
pub use types::{ApiError, ApiResponse};

/// Build API router with all v1 endpoints.
///
/// Generic over the database adapter type used by the executor.
pub fn routes(state: crate::routes::graphql::AppState) -> Router {
    #[allow(unused_mut)]
    // Reason: mutability required when federation feature is enabled to add federation routes
    let mut router = Router::new()
        // Query intelligence endpoints
        // NOTE: /query/explain is intentionally omitted here — it is mounted
        // in server/routing.rs under the admin bearer-auth router to prevent
        // unauthenticated access to query plan details (H13).
        .route("/query/validate", post(query::validate_handler))
        .route("/query/stats", get(query::stats_handler));

    // Federation endpoints
    #[cfg(feature = "federation")]
    {
        router = router
            .route("/federation/subgraphs", get(federation::subgraphs_handler))
            .route("/federation/graph", get(federation::graph_handler))
            .route("/federation/plan", get(federation::plan_handler));
    }

    // Schema export endpoints are now conditionally added in server.rs with optional auth
    // Admin endpoints are now conditionally added in server.rs with auth middleware
    // Design audit endpoints are now conditionally added in server.rs with optional auth
    router.with_state(state)
}

#[cfg(test)]
mod tests;
