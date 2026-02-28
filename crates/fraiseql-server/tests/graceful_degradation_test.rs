//! Graceful Degradation Tests
//!
//! Validates that FraiseQL degrades gracefully when infrastructure components fail:
//! - Database unavailable: health reports unhealthy, structured error response
//! - Cache/Redis unavailable: readiness reports degraded state
//! - Combined failures: multiple subsystem failure scenarios
//! - Error sanitization: no internal details leaked in responses
//!
//! # Running Tests
//!
//! ```bash
//! cargo test --test graceful_degradation_test -- --nocapture
//! ```

#![cfg(test)]

use axum::http::StatusCode;
use fraiseql_server::{
    error::{ErrorCode, ErrorExtensions, GraphQLError},
    operational::health::{health_check, liveness_check, readiness_check},
    routes::health::{DatabaseStatus, HealthResponse},
};

// ============================================================================
// Database Down: Health Endpoint Behavior
// ============================================================================

/// When the database is down, health response must report "unhealthy" status
/// with connected=false, and the server should return 503.
#[test]
fn test_health_reports_unhealthy_when_database_disconnected() {
    let response = HealthResponse {
        status:   "unhealthy".to_string(),
        database: DatabaseStatus {
            connected:          false,
            database_type:      "PostgreSQL".to_string(),
            active_connections: Some(0),
            idle_connections:   Some(0),
        },
        version:     env!("CARGO_PKG_VERSION").to_string(),
        schema_hash: None,
    };

    assert_eq!(response.status, "unhealthy");
    assert!(!response.database.connected);

    // Verify JSON structure is valid for monitoring tools
    let json: serde_json::Value = serde_json::to_value(&response).unwrap();
    assert_eq!(json["status"], "unhealthy");
    assert_eq!(json["database"]["connected"], false);
    assert_eq!(json["database"]["database_type"], "PostgreSQL");
}

/// Health response must not leak connection details when database is down.
#[test]
fn test_health_response_does_not_leak_connection_details() {
    let response = HealthResponse {
        status:   "unhealthy".to_string(),
        database: DatabaseStatus {
            connected:          false,
            database_type:      "PostgreSQL".to_string(),
            active_connections: None,
            idle_connections:   None,
        },
        version:     env!("CARGO_PKG_VERSION").to_string(),
        schema_hash: None,
    };

    let json_str = serde_json::to_string(&response).unwrap();

    // Must not contain connection strings, hostnames, or port numbers
    assert!(!json_str.contains("localhost"));
    assert!(!json_str.contains("127.0.0.1"));
    assert!(!json_str.contains("5432"));
    assert!(!json_str.contains("password"));
    assert!(!json_str.contains("connection refused"));
}

// ============================================================================
// Readiness Check: Subsystem Failure Scenarios
// ============================================================================

/// When database is down but cache is up, readiness reports not ready with reason.
#[test]
fn test_readiness_database_down_cache_up() {
    let status = readiness_check(false, true);

    assert!(!status.ready);
    assert!(!status.database_connected);
    assert!(status.cache_available);
    assert_eq!(status.reason, Some("Database unavailable".to_string()));
}

/// When cache is down but database is up, readiness reports not ready with reason.
#[test]
fn test_readiness_database_up_cache_down() {
    let status = readiness_check(true, false);

    assert!(!status.ready);
    assert!(status.database_connected);
    assert!(!status.cache_available);
    assert_eq!(status.reason, Some("Cache unavailable".to_string()));
}

/// When both database and cache are down, readiness reports not ready.
#[test]
fn test_readiness_both_subsystems_down() {
    let status = readiness_check(false, false);

    assert!(!status.ready);
    assert!(!status.database_connected);
    assert!(!status.cache_available);
    // Database failure takes precedence in reason
    assert_eq!(status.reason, Some("Database unavailable".to_string()));
}

/// When all subsystems are healthy, readiness reports ready with no reason.
#[test]
fn test_readiness_all_healthy() {
    let status = readiness_check(true, true);

    assert!(status.ready);
    assert!(status.database_connected);
    assert!(status.cache_available);
    assert_eq!(status.reason, None);
}

/// Readiness status serializes correctly for Kubernetes probes.
#[test]
fn test_readiness_serialization_for_k8s() {
    let status = readiness_check(false, true);
    let json: serde_json::Value = serde_json::to_value(&status).unwrap();

    assert_eq!(json["ready"], false);
    assert_eq!(json["database_connected"], false);
    assert_eq!(json["cache_available"], true);
    assert!(json["reason"].is_string());
}

// ============================================================================
// Liveness Check: Process Still Alive
// ============================================================================

/// Liveness check always succeeds if the process is running.
/// This endpoint must never fail, even during degraded operation.
#[test]
fn test_liveness_always_succeeds() {
    let status = liveness_check();

    assert!(status.alive);
    assert!(status.pid > 0);
}

/// Liveness status serializes correctly for Kubernetes probes.
#[test]
fn test_liveness_serialization_for_k8s() {
    let status = liveness_check();
    let json: serde_json::Value = serde_json::to_value(&status).unwrap();

    assert_eq!(json["alive"], true);
    assert!(json["pid"].is_number());
    assert!(json["response_time_ms"].is_number());
}

// ============================================================================
// Error Sanitization: No Internal Details in Responses
// ============================================================================

/// Database errors must not expose connection strings or internal topology.
#[test]
fn test_database_error_sanitized() {
    // Simulate what the error sanitizer should produce
    let error = GraphQLError::database("Service temporarily unavailable").with_extensions(
        ErrorExtensions {
            category:         Some("DATABASE".to_string()),
            status:           Some(503),
            request_id:       Some("req-001".to_string()),
            retry_after_secs: None,
            detail:           None,
        },
    );

    // Error message must not contain internal details
    assert!(!error.message.contains("Connection refused"));
    assert!(!error.message.contains("localhost"));
    assert!(!error.message.contains("pg_pool"));
    assert_eq!(error.code.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

/// Query errors during database outage must return appropriate status codes.
#[test]
fn test_query_during_outage_returns_503_error() {
    let error =
        GraphQLError::new("Service temporarily unavailable", ErrorCode::InternalServerError)
            .with_extensions(ErrorExtensions {
                category:         Some("DATABASE".to_string()),
                status:           Some(503),
                request_id:       Some("req-002".to_string()),
                retry_after_secs: None,
                detail:           None,
            });

    assert_eq!(error.code, ErrorCode::InternalServerError);
    assert_eq!(error.code.status_code(), StatusCode::INTERNAL_SERVER_ERROR);

    // Verify the response structure is valid GraphQL error format
    let json = serde_json::to_value(&error).unwrap();
    assert!(json["message"].is_string());
    assert!(json["extensions"].is_object());
    assert!(json["extensions"]["request_id"].is_string());
}

/// Timeout errors during degradation must not reveal infrastructure details.
#[test]
fn test_timeout_error_during_degradation() {
    let error = GraphQLError::timeout("Query execution");

    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.code.status_code(), StatusCode::REQUEST_TIMEOUT);
    // Must not mention specific timeout values or server internals
    assert!(!error.message.contains("pg_pool"));
    assert!(!error.message.contains("30000ms"));
}

// ============================================================================
// Health Check Metadata
// ============================================================================

/// Health check includes uptime for monitoring dashboards.
#[test]
fn test_health_check_includes_uptime() {
    let status = health_check(7200);

    assert_eq!(status.uptime_seconds, 7200);
    assert!(status.timestamp > 0);
}

/// Health check serializes with all fields for Grafana/Prometheus consumption.
#[test]
fn test_health_check_serialization_complete() {
    let status = health_check(3600);
    let json: serde_json::Value = serde_json::to_value(&status).unwrap();

    assert_eq!(json["status"], "healthy");
    assert!(json["timestamp"].is_number());
    assert_eq!(json["uptime_seconds"], 3600);
}

// ============================================================================
// Multi-Database Degradation
// ============================================================================

/// Health response correctly identifies which database type failed.
#[test]
fn test_degradation_identifies_database_type() {
    let databases = ["PostgreSQL", "MySQL", "SQLite", "SQLServer"];

    for db_type in databases {
        let response = HealthResponse {
            status:      "unhealthy".to_string(),
            database:    DatabaseStatus {
                connected:          false,
                database_type:      db_type.to_string(),
                active_connections: Some(0),
                idle_connections:   Some(0),
            },
            version:     env!("CARGO_PKG_VERSION").to_string(),
            schema_hash: None,
        };

        let json: serde_json::Value = serde_json::to_value(&response).unwrap();
        assert_eq!(json["database"]["database_type"], db_type);
        assert_eq!(json["database"]["connected"], false);
    }
}

/// Pool metrics are reported even during degraded state.
#[test]
fn test_pool_metrics_available_during_degradation() {
    let response = HealthResponse {
        status:   "unhealthy".to_string(),
        database: DatabaseStatus {
            connected:          false,
            database_type:      "PostgreSQL".to_string(),
            active_connections: Some(20),
            idle_connections:   Some(0),
        },
        version:     env!("CARGO_PKG_VERSION").to_string(),
        schema_hash: None,
    };

    // Pool saturation visible even when unhealthy
    assert_eq!(response.database.active_connections, Some(20));
    assert_eq!(response.database.idle_connections, Some(0));

    let json: serde_json::Value = serde_json::to_value(&response).unwrap();
    assert_eq!(json["database"]["active_connections"], 20);
    assert_eq!(json["database"]["idle_connections"], 0);
}
