//! Prometheus metrics endpoint.
//!
//! Exposes server metrics in Prometheus text format.
//!
//! # Metrics Exposed
//!
//! - GraphQL queries (total, success, error)
//! - Query execution time (average)
//! - Database queries (total, duration)
//! - Error rates (validation, parse, execution)
//! - HTTP responses (2xx, 4xx, 5xx)
//! - Cache hit ratio

use axum::{extract::State, response::IntoResponse, Json};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::Serialize;
use tracing::debug;

use crate::metrics::PrometheusMetrics;
use crate::routes::graphql::AppState;

/// Metrics response containing summary statistics.
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    /// Total GraphQL queries
    pub queries_total: u64,
    /// Successfully executed queries
    pub queries_success: u64,
    /// Failed queries
    pub queries_error: u64,
    /// Average query duration (ms)
    pub avg_query_duration_ms: f64,
    /// Cache hit ratio (0-1)
    pub cache_hit_ratio: f64,
}

/// Metrics handler - returns Prometheus format metrics.
///
/// # Response
///
/// Returns metrics in Prometheus text-based format (text/plain).
/// Can be scraped by Prometheus or viewed in browser.
///
/// # Example Response
///
/// ```text
/// # HELP fraiseql_graphql_queries_total Total GraphQL queries executed
/// # TYPE fraiseql_graphql_queries_total counter
/// fraiseql_graphql_queries_total 1250
///
/// # HELP fraiseql_graphql_query_duration_ms Average query execution time in milliseconds
/// # TYPE fraiseql_graphql_query_duration_ms gauge
/// fraiseql_graphql_query_duration_ms 12.5
/// ```
pub async fn metrics_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(_state): State<AppState<A>>,
) -> impl IntoResponse {
    debug!("Metrics endpoint requested");

    // In production, metrics would be collected from the AppState
    // For now, return zero metrics as placeholder
    let prometheus_metrics = PrometheusMetrics {
        queries_total: 0,
        queries_success: 0,
        queries_error: 0,
        queries_avg_duration_ms: 0.0,
        db_queries_total: 0,
        db_queries_avg_duration_ms: 0.0,
        validation_errors_total: 0,
        parse_errors_total: 0,
        execution_errors_total: 0,
        http_requests_total: 0,
        http_responses_2xx: 0,
        http_responses_4xx: 0,
        http_responses_5xx: 0,
        cache_hits: 0,
        cache_misses: 0,
        cache_hit_ratio: 0.0,
    };

    (
        axum::http::StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4")],
        prometheus_metrics.to_prometheus_format(),
    )
}

/// JSON metrics handler - returns metrics in JSON format.
///
/// Useful for dashboards and monitoring systems that consume JSON.
pub async fn metrics_json_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(_state): State<AppState<A>>,
) -> impl IntoResponse {
    debug!("JSON metrics endpoint requested");

    let response = MetricsResponse {
        queries_total: 0,
        queries_success: 0,
        queries_error: 0,
        avg_query_duration_ms: 0.0,
        cache_hit_ratio: 0.0,
    };

    Json(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_response_structure() {
        let response = MetricsResponse {
            queries_total: 1000,
            queries_success: 950,
            queries_error: 50,
            avg_query_duration_ms: 12.5,
            cache_hit_ratio: 0.75,
        };

        assert_eq!(response.queries_total, 1000);
        assert_eq!(response.queries_success, 950);
        assert_eq!(response.queries_error, 50);
        assert!((response.avg_query_duration_ms - 12.5).abs() < 0.001);
        assert!((response.cache_hit_ratio - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_metrics_response_serialization() {
        let response = MetricsResponse {
            queries_total: 100,
            queries_success: 95,
            queries_error: 5,
            avg_query_duration_ms: 5.0,
            cache_hit_ratio: 0.85,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("queries_total"));
        assert!(json.contains("100"));
        assert!(json.contains("queries_success"));
    }
}
