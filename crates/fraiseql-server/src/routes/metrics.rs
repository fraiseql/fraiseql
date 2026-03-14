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

use axum::{Json, extract::State, response::IntoResponse};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::Serialize;
use tracing::debug;

use crate::{metrics_server::PrometheusMetrics, routes::graphql::AppState};

/// Metrics response containing summary statistics.
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    /// Total GraphQL queries
    pub queries_total:           u64,
    /// Successfully executed queries
    pub queries_success:         u64,
    /// Failed queries
    pub queries_error:           u64,
    /// Average query duration (ms)
    pub avg_query_duration_ms:   f64,
    /// Cache hit ratio (0-1)
    pub cache_hit_ratio:         f64,
    /// Total connections in pool
    pub pool_connections_total:  u32,
    /// Idle (available) connections in pool
    pub pool_connections_idle:   u32,
    /// Active (in-use) connections in pool
    pub pool_connections_active: u32,
    /// Requests waiting for a pool connection
    pub pool_requests_waiting:   u32,
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
    State(state): State<AppState<A>>,
) -> impl IntoResponse {
    debug!("Metrics endpoint requested");

    // Collect metrics from AppState
    let prometheus_metrics = PrometheusMetrics::from(state.metrics.as_ref());
    let mut output = prometheus_metrics.to_prometheus_format();

    // Append per-entity federation circuit breaker state gauge (Step 8 of Issue #39)
    if let Some(ref cb_manager) = state.circuit_breaker {
        let states = cb_manager.collect_states();
        if !states.is_empty() {
            output.push_str(concat!(
                "\n# HELP fraiseql_federation_circuit_breaker_state ",
                "Federation circuit breaker state per entity type ",
                "(0=closed, 1=open, 2=half_open)\n",
                "# TYPE fraiseql_federation_circuit_breaker_state gauge\n",
            ));
            for (entity, state_code) in states {
                output.push_str(&format!(
                    "fraiseql_federation_circuit_breaker_state{{entity=\"{entity}\"}} \
                     {state_code}\n"
                ));
            }
        }
    }

    // Append Redis rate-limiter error counter when the feature is compiled in.
    #[cfg(feature = "redis-rate-limiting")]
    {
        let errors = crate::middleware::rate_limit::redis_error_count_total();
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_rate_limit_redis_errors_total ",
                "Total Redis rate limiter fail-open events (Redis unreachable)\n",
                "# TYPE fraiseql_rate_limit_redis_errors_total counter\n",
                "fraiseql_rate_limit_redis_errors_total {errors}\n",
            ),
            errors = errors
        ));
    }

    // Append Redis PKCE store error counter when the feature is compiled in.
    #[cfg(feature = "redis-pkce")]
    {
        let errors = fraiseql_auth::pkce::redis_pkce_error_count_total();
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_pkce_redis_errors_total ",
                "Total Redis PKCE store errors (connection failures, etc.)\n",
                "# TYPE fraiseql_pkce_redis_errors_total counter\n",
                "fraiseql_pkce_redis_errors_total {errors}\n",
            ),
            errors = errors
        ));
    }

    // Append APQ (Automatic Persisted Queries) counters.
    {
        let apq = &state.apq_metrics;
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_apq_hits_total Total APQ cache hits\n",
                "# TYPE fraiseql_apq_hits_total counter\n",
                "fraiseql_apq_hits_total {hits}\n",
                "\n# HELP fraiseql_apq_misses_total Total APQ cache misses\n",
                "# TYPE fraiseql_apq_misses_total counter\n",
                "fraiseql_apq_misses_total {misses}\n",
                "\n# HELP fraiseql_apq_stored_total Total APQ queries stored\n",
                "# TYPE fraiseql_apq_stored_total counter\n",
                "fraiseql_apq_stored_total {stored}\n",
            ),
            hits = apq.get_hits(),
            misses = apq.get_misses(),
            stored = apq.get_stored(),
        ));
    }

    // Append Redis APQ error counter when the feature is compiled in.
    #[cfg(feature = "redis-apq")]
    {
        let errors = fraiseql_core::apq::redis_storage::redis_apq_error_count_total();
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_apq_redis_errors_total ",
                "Total Redis APQ fail-open events\n",
                "# TYPE fraiseql_apq_redis_errors_total counter\n",
                "fraiseql_apq_redis_errors_total {errors}\n",
            ),
            errors = errors
        ));
    }

    // Append MCP tool call counters when the feature is compiled in.
    #[cfg(feature = "mcp")]
    {
        let calls = crate::mcp::handler::mcp_tool_calls_total();
        let errors = crate::mcp::handler::mcp_tool_errors_total();
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_mcp_tool_calls_total Total MCP tool calls\n",
                "# TYPE fraiseql_mcp_tool_calls_total counter\n",
                "fraiseql_mcp_tool_calls_total {calls}\n",
                "\n# HELP fraiseql_mcp_tool_errors_total Total MCP tool call errors\n",
                "# TYPE fraiseql_mcp_tool_errors_total counter\n",
                "fraiseql_mcp_tool_errors_total {errors}\n",
            ),
            calls = calls,
            errors = errors,
        ));
    }

    // Append trusted document counters.
    if state.trusted_docs.is_some() {
        let hits = crate::trusted_documents::hits_total();
        let misses = crate::trusted_documents::misses_total();
        let rejected = crate::trusted_documents::rejected_total();
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_trusted_documents_hits_total Trusted document lookups resolved from manifest\n",
                "# TYPE fraiseql_trusted_documents_hits_total counter\n",
                "fraiseql_trusted_documents_hits_total {hits}\n",
                "\n# HELP fraiseql_trusted_documents_misses_total Unknown document ID lookups\n",
                "# TYPE fraiseql_trusted_documents_misses_total counter\n",
                "fraiseql_trusted_documents_misses_total {misses}\n",
                "\n# HELP fraiseql_trusted_documents_rejected_total Raw queries rejected (strict mode)\n",
                "# TYPE fraiseql_trusted_documents_rejected_total counter\n",
                "fraiseql_trusted_documents_rejected_total {rejected}\n",
            ),
            hits = hits,
            misses = misses,
            rejected = rejected,
        ));
    }

    // Pool health metrics (sampled live from adapter on each request)
    {
        let pool = state.executor.pool_metrics();
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_db_pool_connections_total Total connections in pool\n",
                "# TYPE fraiseql_db_pool_connections_total gauge\n",
                "fraiseql_db_pool_connections_total {total}\n",
                "\n# HELP fraiseql_db_pool_connections_idle Idle (available) connections\n",
                "# TYPE fraiseql_db_pool_connections_idle gauge\n",
                "fraiseql_db_pool_connections_idle {idle}\n",
                "\n# HELP fraiseql_db_pool_connections_active Active (in-use) connections\n",
                "# TYPE fraiseql_db_pool_connections_active gauge\n",
                "fraiseql_db_pool_connections_active {active}\n",
                "\n# HELP fraiseql_db_pool_requests_waiting Requests waiting for a connection\n",
                "# TYPE fraiseql_db_pool_requests_waiting gauge\n",
                "fraiseql_db_pool_requests_waiting {waiting}\n",
            ),
            total = pool.total_connections,
            idle = pool.idle_connections,
            active = pool.active_connections,
            waiting = pool.waiting_requests,
        ));
    }

    // Pool auto-tuner metrics (when enabled)
    if let Some(ref tuner) = state.pool_tuner {
        let adjustments = tuner.adjustments_total();
        let recommended = tuner.recommended_size();
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_pool_tuning_adjustments_total ",
                "Total pool resize operations applied or recommended\n",
                "# TYPE fraiseql_pool_tuning_adjustments_total counter\n",
                "fraiseql_pool_tuning_adjustments_total {adjustments}\n",
                "\n# HELP fraiseql_pool_recommended_size ",
                "Current recommended connection pool size\n",
                "# TYPE fraiseql_pool_recommended_size gauge\n",
                "fraiseql_pool_recommended_size {recommended}\n",
            ),
            adjustments = adjustments,
            recommended = recommended,
        ));
    }

    // Append per-operation histogram metrics
    output.push_str(&state.metrics.operation_metrics.to_prometheus_format());

    // Multi-root parallel query counter.
    {
        let multi_root = fraiseql_core::runtime::multi_root_queries_total();
        output.push_str(&format!(
            concat!(
                "\n# HELP fraiseql_multi_root_queries_total ",
                "Total multi-root GraphQL queries dispatched via parallel execution\n",
                "# TYPE fraiseql_multi_root_queries_total counter\n",
                "fraiseql_multi_root_queries_total {multi_root}\n",
            ),
            multi_root = multi_root,
        ));
    }

    // Append subscription counters.
    let subs = crate::routes::subscription_metrics();
    output.push_str(&format!(
        concat!(
            "\n# HELP fraiseql_ws_connections_total Total WebSocket subscription connections\n",
            "# TYPE fraiseql_ws_connections_total counter\n",
            "fraiseql_ws_connections_total{{result=\"accepted\"}} {accepted}\n",
            "fraiseql_ws_connections_total{{result=\"rejected\"}} {rejected}\n",
            "\n# HELP fraiseql_ws_subscriptions_total Total subscription registrations\n",
            "# TYPE fraiseql_ws_subscriptions_total counter\n",
            "fraiseql_ws_subscriptions_total{{result=\"accepted\"}} {sub_accepted}\n",
            "fraiseql_ws_subscriptions_total{{result=\"rejected\"}} {sub_rejected}\n",
        ),
        accepted = subs.connections_accepted,
        rejected = subs.connections_rejected,
        sub_accepted = subs.subscriptions_accepted,
        sub_rejected = subs.subscriptions_rejected,
    ));

    (
        axum::http::StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4")],
        output,
    )
}

/// JSON metrics handler - returns metrics in JSON format.
///
/// Useful for dashboards and monitoring systems that consume JSON.
pub async fn metrics_json_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
) -> impl IntoResponse {
    debug!("JSON metrics endpoint requested");

    // Collect metrics from AppState
    let prometheus_metrics = PrometheusMetrics::from(state.metrics.as_ref());
    let pool = state.executor.pool_metrics();

    let response = MetricsResponse {
        queries_total:           prometheus_metrics.queries_total,
        queries_success:         prometheus_metrics.queries_success,
        queries_error:           prometheus_metrics.queries_error,
        avg_query_duration_ms:   prometheus_metrics.queries_avg_duration_ms,
        cache_hit_ratio:         prometheus_metrics.cache_hit_ratio,
        pool_connections_total:  pool.total_connections,
        pool_connections_idle:   pool.idle_connections,
        pool_connections_active: pool.active_connections,
        pool_requests_waiting:   pool.waiting_requests,
    };

    Json(response)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::*;

    #[test]
    fn test_metrics_response_structure() {
        let response = MetricsResponse {
            queries_total:           1000,
            queries_success:         950,
            queries_error:           50,
            avg_query_duration_ms:   12.5,
            cache_hit_ratio:         0.75,
            pool_connections_total:  20,
            pool_connections_idle:   15,
            pool_connections_active: 5,
            pool_requests_waiting:   0,
        };

        assert_eq!(response.queries_total, 1000);
        assert_eq!(response.queries_success, 950);
        assert_eq!(response.queries_error, 50);
        assert!((response.avg_query_duration_ms - 12.5).abs() < 0.001);
        assert!((response.cache_hit_ratio - 0.75).abs() < 0.001);
        assert_eq!(response.pool_connections_total, 20);
        assert_eq!(response.pool_connections_idle, 15);
        assert_eq!(response.pool_connections_active, 5);
        assert_eq!(response.pool_requests_waiting, 0);
    }

    #[test]
    fn test_metrics_response_serialization() {
        let response = MetricsResponse {
            queries_total:           100,
            queries_success:         95,
            queries_error:           5,
            avg_query_duration_ms:   5.0,
            cache_hit_ratio:         0.85,
            pool_connections_total:  10,
            pool_connections_idle:   8,
            pool_connections_active: 2,
            pool_requests_waiting:   0,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("queries_total"));
        assert!(json.contains("100"));
        assert!(json.contains("queries_success"));
        assert!(json.contains("pool_connections_total"));
        assert!(json.contains("pool_connections_idle"));
        assert!(json.contains("pool_connections_active"));
        assert!(json.contains("pool_requests_waiting"));
    }
}
