use super::*;

#[test]
fn test_direct_database_resolver_creation() {
    let _resolver = DirectDatabaseResolver::new();
}

#[test]
fn test_connection_count_empty() {
    let resolver = DirectDatabaseResolver::new();
    assert_eq!(resolver.connection_count(), 0);
}

#[test]
fn test_close_all() {
    let resolver = DirectDatabaseResolver::new();
    resolver.close_all();
}

#[test]
fn test_close_connection() {
    let resolver = DirectDatabaseResolver::new();
    resolver.close_connection("postgresql://localhost/db");
}
