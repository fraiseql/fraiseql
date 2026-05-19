#![allow(missing_docs)]

use fraiseql_error::IntegrationError;

#[test]
fn search_error_code_and_display() {
    let err = IntegrationError::Search {
        provider: "elasticsearch".into(),
        message: "cluster red".into(),
    };
    assert_eq!(err.error_code(), "integration_search_error");
    assert_eq!(err.to_string(), "Search provider error: elasticsearch - cluster red");
}

#[test]
fn cache_error_code_and_display() {
    let err = IntegrationError::Cache {
        message: "connection refused".into(),
    };
    assert_eq!(err.error_code(), "integration_cache_error");
    assert_eq!(err.to_string(), "Cache error: connection refused");
}

#[test]
fn queue_error_code_and_display() {
    let err = IntegrationError::Queue {
        message: "queue full".into(),
    };
    assert_eq!(err.error_code(), "integration_queue_error");
    assert_eq!(err.to_string(), "Queue error: queue full");
}

#[test]
fn connection_failed_error_code_and_display() {
    let err = IntegrationError::ConnectionFailed {
        service: "redis".into(),
    };
    assert_eq!(err.error_code(), "integration_connection_failed");
    assert_eq!(err.to_string(), "Connection failed: redis");
}

#[test]
fn timeout_error_code_and_display() {
    let err = IntegrationError::Timeout {
        operation: "bulk_index".into(),
    };
    assert_eq!(err.error_code(), "integration_timeout");
    assert_eq!(err.to_string(), "Timeout: bulk_index");
}
