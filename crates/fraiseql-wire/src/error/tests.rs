use super::*;

#[test]
fn test_error_helpers() {
    let conn_err = WireError::connection("failed to connect");
    assert!(matches!(conn_err, WireError::Connection(_)));

    let proto_err = WireError::protocol("unexpected message");
    assert!(matches!(proto_err, WireError::Protocol(_)));

    let sql_err = WireError::sql("syntax error");
    assert!(matches!(sql_err, WireError::Sql(_)));

    let schema_err = WireError::invalid_schema("expected single column");
    assert!(matches!(schema_err, WireError::InvalidSchema(_)));
}

#[test]
fn test_error_connection_refused() {
    let err = WireError::connection_refused("localhost", 5432);
    let msg = err.to_string();
    assert!(msg.contains("connection refused"));
    assert!(msg.contains("Is Postgres running?"));
    assert!(msg.contains("localhost"));
    assert!(msg.contains("5432"));
}

#[test]
fn test_error_invalid_schema_columns() {
    let err = WireError::invalid_schema_columns(2);
    let msg = err.to_string();
    assert!(msg.contains("2 columns"));
    assert!(msg.contains("instead of 1"));
    assert!(msg.contains("SELECT data FROM"));
}

#[test]
fn test_error_auth_failed() {
    let err = WireError::auth_failed("postgres", "invalid password");
    let msg = err.to_string();
    assert!(msg.contains("postgres"));
    assert!(msg.contains("invalid password"));
    assert!(msg.contains("psql"));
}

#[test]
fn test_error_config_invalid() {
    let err = WireError::config_invalid("missing database name");
    let msg = err.to_string();
    assert!(msg.contains("invalid configuration"));
    assert!(msg.contains("postgres://"));
    assert!(msg.contains("missing database name"));
}

#[test]
fn test_error_category() {
    assert_eq!(WireError::connection("test").category(), "connection");
    assert_eq!(WireError::sql("test").category(), "sql");
    assert_eq!(WireError::Cancelled.category(), "cancelled");
    assert_eq!(WireError::ConnectionClosed.category(), "connection_closed");
}

#[test]
fn test_error_message_clarity() {
    // Verify error messages are clear and actionable
    let err = WireError::connection_refused("example.com", 5432);
    let msg = err.to_string();

    // Should suggest a diagnostic command
    assert!(msg.contains("pg_isready"));

    // Should include the connection details
    assert!(msg.contains("example.com"));
}

#[test]
fn test_is_retriable() {
    assert!(WireError::ConnectionClosed.is_retriable());
    assert!(WireError::Io(io::Error::new(io::ErrorKind::TimedOut, "timeout")).is_retriable());

    assert!(!WireError::connection("test").is_retriable());
    assert!(!WireError::sql("test").is_retriable());
    assert!(!WireError::invalid_schema("test").is_retriable());
}

#[test]
fn test_retriable_classification() {
    // Transient errors should be retriable
    assert!(WireError::ConnectionClosed.is_retriable());
    assert!(WireError::Io(io::Error::new(io::ErrorKind::ConnectionReset, "reset")).is_retriable());

    // Permanent errors should not be retriable
    assert!(!WireError::auth_failed("user", "invalid password").is_retriable());
    assert!(!WireError::sql("syntax error").is_retriable());
    assert!(!WireError::invalid_schema_columns(3).is_retriable());
}

#[test]
fn test_deserialization_error() {
    let err = WireError::Deserialization {
        type_name: "Project".to_string(),
        details: "missing field `id`".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("Project"));
    assert!(msg.contains("missing field"));
    assert_eq!(err.category(), "deserialization");
}

#[test]
fn test_deserialization_error_not_retriable() {
    let err = WireError::Deserialization {
        type_name: "User".to_string(),
        details: "invalid type".to_string(),
    };
    assert!(!err.is_retriable());
}

#[test]
fn test_memory_limit_exceeded_error() {
    let err = WireError::MemoryLimitExceeded {
        limit: 1_000_000,
        estimated_memory: 1_500_000,
    };
    let msg = err.to_string();
    assert!(msg.contains("1500000"));
    assert!(msg.contains("1000000"));
    assert!(msg.contains("memory limit exceeded"));
    assert_eq!(err.category(), "memory_limit_exceeded");
}

#[test]
fn test_memory_limit_exceeded_not_retriable() {
    let err = WireError::MemoryLimitExceeded {
        limit: 100_000,
        estimated_memory: 150_000,
    };
    assert!(!err.is_retriable());
}
