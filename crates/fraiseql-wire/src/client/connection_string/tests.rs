#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_parse_tcp_full() {
    let info = ConnectionInfo::parse("postgres://user:pass@localhost:5433/mydb").unwrap();
    assert_eq!(info.transport, TransportType::Tcp);
    assert_eq!(info.host, Some("localhost".to_string()));
    assert_eq!(info.port, Some(5433));
    assert_eq!(info.database, "mydb");
    assert_eq!(info.user, "user");
    assert_eq!(info.password.as_ref().map(|p| p.as_str()), Some("pass"));
}

#[test]
fn parse_tcp_percent_decodes_credentials() {
    // Audit L-wire-connstr: credentials are percent-encoded in the URL and must
    // be decoded. Password "p@ss:w%rd" encodes @→%40, :→%3A, %→%25; user "user"
    // encodes the 'e' as %65 to prove decoding runs on the user too.
    let info =
        ConnectionInfo::parse("postgres://us%65r:p%40ss%3Aw%25rd@localhost:5432/db").unwrap();
    assert_eq!(info.user, "user");
    assert_eq!(
        info.password.as_ref().map(|p| p.as_str()),
        Some("p@ss:w%rd")
    );
    assert_eq!(info.host, Some("localhost".to_string()));
    assert_eq!(info.port, Some(5432));
    assert_eq!(info.database, "db");
}

#[test]
fn parse_tcp_splits_userinfo_at_last_at() {
    // A '@' inside the (encoded) password must not be mistaken for the
    // userinfo/host delimiter — the last '@' delimits host.
    let info = ConnectionInfo::parse("postgres://user:p%40ss@host/db").unwrap();
    assert_eq!(info.password.as_ref().map(|p| p.as_str()), Some("p@ss"));
    assert_eq!(info.host, Some("host".to_string()));
}

#[test]
fn parse_tcp_rejects_invalid_percent_encoding() {
    let result = ConnectionInfo::parse("postgres://user:p%ZZss@host/db");
    assert!(
        result.is_err(),
        "invalid percent-encoding in credentials must be rejected"
    );
}

#[test]
fn test_parse_tcp_minimal() {
    let info = ConnectionInfo::parse("postgres://localhost/mydb").unwrap();
    assert_eq!(info.transport, TransportType::Tcp);
    assert_eq!(info.host, Some("localhost".to_string()));
    assert_eq!(info.port, Some(5432));
    assert_eq!(info.database, "mydb");
}

#[test]
fn test_parse_unix() {
    let info = ConnectionInfo::parse("postgres:///mydb").unwrap();
    assert_eq!(info.transport, TransportType::Unix);
    assert_eq!(info.database, "mydb");
    assert_eq!(info.port, Some(5432)); // Default port
                                       // Socket path should contain the database name and port
    assert!(info.unix_socket.is_some());
    let path = info.unix_socket.unwrap();
    assert!(path.to_string_lossy().contains(".s.PGSQL.5432"));
}

#[test]
fn test_parse_unix_socket_path_construction() {
    let info = ConnectionInfo::parse("postgres:///mydb").unwrap();
    let socket_path = info.unix_socket.unwrap();
    // Socket path should end with .s.PGSQL.5432
    assert!(socket_path.to_string_lossy().ends_with(".s.PGSQL.5432"));
}

#[test]
fn test_parse_unix_with_custom_directory() {
    let info = ConnectionInfo::parse("postgres:///mydb?host=/custom/path").unwrap();
    assert_eq!(info.transport, TransportType::Unix);
    assert_eq!(info.database, "mydb");
    assert_eq!(info.port, Some(5432));
    let socket_path = info.unix_socket.unwrap();
    assert_eq!(socket_path, PathBuf::from("/custom/path/.s.PGSQL.5432"));
}

#[test]
fn test_parse_unix_with_custom_port() {
    let info = ConnectionInfo::parse("postgres:///mydb?host=/tmp&port=5433").unwrap();
    assert_eq!(info.transport, TransportType::Unix);
    assert_eq!(info.database, "mydb");
    assert_eq!(info.port, Some(5433));
    let socket_path = info.unix_socket.unwrap();
    assert_eq!(socket_path, PathBuf::from("/tmp/.s.PGSQL.5433"));
}

#[test]
fn test_construct_socket_path() {
    let path = construct_socket_path("/run/postgresql", 5432);
    assert_eq!(path, PathBuf::from("/run/postgresql/.s.PGSQL.5432"));

    let path = construct_socket_path("/var/run/postgresql", 5433);
    assert_eq!(path, PathBuf::from("/var/run/postgresql/.s.PGSQL.5433"));
}

#[test]
fn test_parse_query_param() {
    let host = parse_query_param("?host=/tmp", "host");
    assert_eq!(host, Some("/tmp".to_string()));

    let port = parse_query_param("?host=/tmp&port=5433", "port");
    assert_eq!(port, Some("5433".to_string()));

    let missing = parse_query_param("?host=/tmp", "port");
    assert_eq!(missing, None);

    let empty = parse_query_param("", "host");
    assert_eq!(empty, None);
}

#[test]
fn test_parse_unix_default_database() {
    // When no database specified, should use username
    let info = ConnectionInfo::parse("postgres:///").unwrap();
    assert_eq!(info.transport, TransportType::Unix);
    // Database should be the username (from whoami)
    assert!(!info.database.is_empty());
}

#[test]
fn test_password_field_present() {
    // Verify password field exists and is properly handled (and zeroed on drop)
    let info = ConnectionInfo::parse("postgres://user:secret@localhost/db").unwrap();
    assert_eq!(info.password.as_ref().map(|p| p.as_str()), Some("secret"));
}

// ── Socket-dir validation tests ────────────────────────────────────────────

#[test]
fn test_valid_socket_dir_accepted() {
    validate_socket_dir("/run/postgresql")
        .unwrap_or_else(|e| panic!("expected Ok for /run/postgresql: {e}"));
    validate_socket_dir("/tmp").unwrap_or_else(|e| panic!("expected Ok for /tmp: {e}"));
    validate_socket_dir("/var/run/postgresql")
        .unwrap_or_else(|e| panic!("expected Ok for /var/run/postgresql: {e}"));
}

#[test]
fn test_relative_socket_dir_rejected() {
    let err = validate_socket_dir("run/postgresql").unwrap_err();
    assert!(matches!(err, WireError::Config(_)));
    let msg = err.to_string();
    assert!(msg.contains("absolute"), "error must say 'absolute': {msg}");
}

#[test]
fn test_dot_dot_in_socket_dir_rejected() {
    let err = validate_socket_dir("/run/../etc").unwrap_err();
    assert!(matches!(err, WireError::Config(_)));
    let msg = err.to_string();
    assert!(msg.contains(".."), "error must mention '..': {msg}");
}

#[test]
fn test_socket_dir_too_long_rejected() {
    // 4097-byte path must be rejected by the length guard.
    let long = format!("/{}", "a".repeat(4096));
    let err = validate_socket_dir(&long).unwrap_err();
    assert!(matches!(err, WireError::Config(_)));
    let msg = err.to_string();
    assert!(msg.contains("4096"), "error must mention the limit: {msg}");
}

#[test]
fn test_connection_string_rejects_traversal_in_host_param() {
    let result = ConnectionInfo::parse("postgres:///mydb?host=/run/../etc");
    assert!(result.is_err(), "path traversal in host must be rejected");
}

#[test]
fn test_connection_string_rejects_relative_host_param() {
    let result = ConnectionInfo::parse("postgres:///mydb?host=relative/path");
    assert!(result.is_err(), "relative host param must be rejected");
}

// ── IPv6 literal tests (RFC 3986 §3.2.2) ──────────────────────────────────

#[test]
fn test_parse_ipv6_with_port() {
    let info = ConnectionInfo::parse("postgres://user@[::1]:5432/db").unwrap();
    assert_eq!(info.host, Some("::1".to_string()));
    assert_eq!(info.port, Some(5432));
    assert_eq!(info.database, "db");
    assert_eq!(info.user, "user");
}

#[test]
fn test_parse_ipv6_default_port() {
    let info = ConnectionInfo::parse("postgres://user@[::1]/db").unwrap();
    assert_eq!(info.host, Some("::1".to_string()));
    assert_eq!(info.port, Some(5432));
}

#[test]
fn test_parse_ipv6_non_default_port() {
    let info = ConnectionInfo::parse("postgres://user@[::1]:5433/db").unwrap();
    assert_eq!(info.host, Some("::1".to_string()));
    assert_eq!(info.port, Some(5433));
}

#[test]
fn test_parse_ipv6_zone_id() {
    // Zone ID encoded as %25 per RFC 6874
    let info = ConnectionInfo::parse("postgres://user@[fe80::1%25eth0]:5432/db").unwrap();
    assert_eq!(info.host, Some("fe80::1%25eth0".to_string()));
    assert_eq!(info.port, Some(5432));
}
