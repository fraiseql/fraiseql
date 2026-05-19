use super::*;

#[test]
fn test_database_type_as_str() {
    assert_eq!(DatabaseType::PostgreSQL.as_str(), "postgresql");
    assert_eq!(DatabaseType::MySQL.as_str(), "mysql");
    assert_eq!(DatabaseType::SQLite.as_str(), "sqlite");
    assert_eq!(DatabaseType::SQLServer.as_str(), "sqlserver");
}

#[test]
fn test_database_type_display() {
    assert_eq!(DatabaseType::PostgreSQL.to_string(), "postgresql");
}

#[test]
fn test_jsonb_value() {
    let value = serde_json::json!({"id": "123", "name": "test"});
    let jsonb = JsonbValue::new(value.clone());

    assert_eq!(jsonb.as_value(), &value);
    assert_eq!(jsonb.into_value(), value);
}

#[test]
fn test_pool_metrics_utilization() {
    let metrics = PoolMetrics {
        total_connections: 10,
        idle_connections: 5,
        active_connections: 5,
        waiting_requests: 0,
    };

    assert!((metrics.utilization() - 0.5).abs() < f64::EPSILON);
    assert!(!metrics.is_exhausted());
}

#[test]
fn test_pool_metrics_exhausted() {
    let metrics = PoolMetrics {
        total_connections: 10,
        idle_connections: 0,
        active_connections: 10,
        waiting_requests: 5,
    };

    assert!((metrics.utilization() - 1.0).abs() < f64::EPSILON);
    assert!(metrics.is_exhausted());
}
