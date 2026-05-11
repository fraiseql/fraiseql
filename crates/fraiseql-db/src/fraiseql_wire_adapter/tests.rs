#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_adapter_creation() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test");
    assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);
    assert_eq!(adapter.chunk_size, 1024);
}

#[test]
fn test_adapter_with_chunk_size() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test").with_chunk_size(512);
    assert_eq!(adapter.chunk_size, 512);
}

#[test]
fn test_build_query_simple() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test");
    let sql = adapter.build_query("v_user", None, None, None).unwrap();
    assert_eq!(sql, "SELECT data FROM v_user");
}

#[test]
fn test_build_query_with_limit_offset() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test");
    let sql = adapter.build_query("v_user", None, Some(10), Some(5)).unwrap();
    assert_eq!(sql, "SELECT data FROM v_user OFFSET 5 LIMIT 10");
}

#[test]
fn test_pool_metrics() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test");
    let metrics = adapter.pool_metrics();
    assert_eq!(metrics.total_connections, 0);
    assert_eq!(metrics.idle_connections, 0);
    assert_eq!(metrics.active_connections, 0);
    assert_eq!(metrics.waiting_requests, 0);
}
