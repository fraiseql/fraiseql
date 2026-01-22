//! Axum route definitions for observer management.

use axum::{
    routing::{get, post},
    Router,
};

use super::handlers::{
    create_observer, delete_observer, disable_observer, enable_observer, get_observer,
    get_observer_stats, list_observer_logs, list_observers, update_observer, ObserverState,
};

/// Create the observer management router.
///
/// # Routes
///
/// - `GET    /`           - List all observers
/// - `POST   /`           - Create a new observer
/// - `GET    /stats`      - Get statistics for all observers
/// - `GET    /logs`       - List execution logs for all observers
/// - `GET    /:id`        - Get a specific observer
/// - `PATCH  /:id`        - Update a specific observer
/// - `DELETE /:id`        - Delete a specific observer (soft delete)
/// - `POST   /:id/enable` - Enable a specific observer
/// - `POST   /:id/disable`- Disable a specific observer
/// - `GET    /:id/stats`  - Get statistics for a specific observer
/// - `GET    /:id/logs`   - List execution logs for a specific observer
pub fn observer_routes(state: ObserverState) -> Router {
    Router::new()
        // Collection routes
        .route("/", get(list_observers).post(create_observer))
        .route("/stats", get(get_all_stats))
        .route("/logs", get(list_all_logs))
        // Individual observer routes
        .route("/:id", get(get_observer).patch(update_observer).delete(delete_observer))
        .route("/:id/enable", post(enable_observer))
        .route("/:id/disable", post(disable_observer))
        .route("/:id/stats", get(get_single_stats))
        .route("/:id/logs", get(list_single_logs))
        .with_state(state)
}

/// Get stats for all observers (wrapper to pass None for observer_id).
async fn get_all_stats(
    state: axum::extract::State<ObserverState>,
) -> impl axum::response::IntoResponse {
    get_observer_stats(state, None).await
}

/// Get stats for a single observer.
async fn get_single_stats(
    state: axum::extract::State<ObserverState>,
    path: axum::extract::Path<uuid::Uuid>,
) -> impl axum::response::IntoResponse {
    get_observer_stats(state, Some(path)).await
}

/// List logs for all observers.
async fn list_all_logs(
    state: axum::extract::State<ObserverState>,
    query: axum::extract::Query<super::ListObserverLogsQuery>,
) -> impl axum::response::IntoResponse {
    list_observer_logs(state, axum::extract::Path(None), query).await
}

/// List logs for a single observer.
async fn list_single_logs(
    state: axum::extract::State<ObserverState>,
    path: axum::extract::Path<uuid::Uuid>,
    query: axum::extract::Query<super::ListObserverLogsQuery>,
) -> impl axum::response::IntoResponse {
    list_observer_logs(state, axum::extract::Path(Some(path.0)), query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observers::ObserverRepository;

    // Note: Integration tests would require a test database
    // These are placeholder tests for route configuration

    #[test]
    fn test_routes_compile() {
        // This test just ensures the routes compile correctly
        // Actual testing requires a database connection
    }
}
