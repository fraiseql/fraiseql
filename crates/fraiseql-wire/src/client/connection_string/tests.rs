#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_parse_tcp_full() {
    let info = ConnectionInfo::parse("postgres://user:pass@localhost:5433/mydb").unwrap();
    assert_eq!(info.transport, TransportType::Tcp);
    assert_eq!(info.host, Some("localhost".to_string()));
    assert_eq!(info.port, Some(5433));
    assert_eq!(info.database.as_deref(), Some("mydb"));
    assert_eq!(info.user.as_deref(), Some("user"));
    assert_eq!(info.password.as_ref().map(|p| p.as_str()), Some("pass"));
}

#[test]
fn parse_tcp_percent_decodes_credentials() {
    // Audit L-wire-connstr: credentials are percent-encoded in the URL and must
    // be decoded. Password "p@ss:w%rd" encodes @→%40, :→%3A, %→%25; user "user"
    // encodes the 'e' as %65 to prove decoding runs on the user too.
    let info =
        ConnectionInfo::parse("postgres://us%65r:p%40ss%3Aw%25rd@localhost:5432/db").unwrap();
    assert_eq!(info.user.as_deref(), Some("user"));
    assert_eq!(
        info.password.as_ref().map(|p| p.as_str()),
        Some("p@ss:w%rd")
    );
    assert_eq!(info.host, Some("localhost".to_string()));
    assert_eq!(info.port, Some(5432));
    assert_eq!(info.database.as_deref(), Some("db"));
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
    assert_eq!(info.database.as_deref(), Some("mydb"));
}

#[test]
fn test_parse_unix() {
    let info = ConnectionInfo::parse("postgres:///mydb").unwrap();
    assert_eq!(info.transport, TransportType::Unix);
    assert_eq!(info.database.as_deref(), Some("mydb"));
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
    assert_eq!(info.database.as_deref(), Some("mydb"));
    assert_eq!(info.port, Some(5432));
    let socket_path = info.unix_socket.unwrap();
    assert_eq!(socket_path, PathBuf::from("/custom/path/.s.PGSQL.5432"));
}

#[test]
fn test_parse_unix_with_custom_port() {
    let info = ConnectionInfo::parse("postgres:///mydb?host=/tmp&port=5433").unwrap();
    assert_eq!(info.transport, TransportType::Unix);
    assert_eq!(info.database.as_deref(), Some("mydb"));
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
    assert!(
        info.database.is_none(),
        "no database in the string: OS-user default applies at to_config time"
    );
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
    assert_eq!(info.database.as_deref(), Some("db"));
    assert_eq!(info.user.as_deref(), Some("user"));
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

// ── #817: query strings on the TCP form ───────────────────────────────────────

#[test]
fn tcp_query_string_is_not_folded_into_the_database_name() {
    let info =
        ConnectionInfo::parse("postgres://u:p@h:5432/db?sslmode=require&application_name=svc")
            .unwrap();
    assert_eq!(
        info.database_or_default(),
        "db",
        "query params leaked into the database name"
    );
    assert_eq!(info.host, Some("h".to_string()));
    assert_eq!(info.port, Some(5432));
}

#[test]
fn tcp_at_sign_inside_a_query_value_does_not_resplit_the_host() {
    // The userinfo split must happen on the pre-query portion only. An '@'
    // inside a query value must not become the host delimiter — and since
    // `opt` is not a parameter this client supports, the parse must say so
    // loudly instead of silently mangling host and password.
    let err = ConnectionInfo::parse("postgres://u:p@h:5432/db?opt=a@b").unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("opt"),
        "unsupported query parameter must be named in the error, got: {msg}"
    );
}

#[test]
fn tcp_raw_at_in_password_still_parses_with_a_query_string() {
    let info = ConnectionInfo::parse("postgres://u:p@ss@h:1234/db?sslmode=disable").unwrap();
    assert_eq!(info.user_or_default(), "u");
    assert_eq!(info.password.as_ref().map(|p| p.as_str()), Some("p@ss"));
    assert_eq!(info.host, Some("h".to_string()));
    assert_eq!(info.port, Some(1234));
    assert_eq!(info.database_or_default(), "db");
}

#[test]
fn tcp_ipv6_literal_with_query_params() {
    let info = ConnectionInfo::parse("postgres://u@[::1]:5433/db?application_name=svc").unwrap();
    assert_eq!(info.host, Some("::1".to_string()));
    assert_eq!(info.port, Some(5433));
    assert_eq!(info.database_or_default(), "db");
    let config = info.to_config();
    assert_eq!(config.application_name.as_deref(), Some("svc"));
}

#[test]
fn tcp_connect_timeout_param_reaches_the_config() {
    let info = ConnectionInfo::parse("postgres://u@h/db?connect_timeout=7").unwrap();
    let config = info.to_config();
    assert_eq!(
        config.connect_timeout,
        Some(std::time::Duration::from_secs(7))
    );
}

#[test]
fn tcp_invalid_connect_timeout_is_a_loud_error() {
    let err = ConnectionInfo::parse("postgres://u@h/db?connect_timeout=soon").unwrap_err();
    assert!(format!("{err}").contains("connect_timeout"));
}

#[test]
fn tcp_percent_encoded_database_is_decoded() {
    let info = ConnectionInfo::parse("postgres://u@h/my%20db").unwrap();
    assert_eq!(info.database_or_default(), "my db");
}

#[test]
fn sslmode_require_is_recorded_for_the_entry_points_to_enforce() {
    let info = ConnectionInfo::parse("postgres://u@h/db?sslmode=require").unwrap();
    assert_eq!(info.ssl_mode, SslMode::Require);
    let info = ConnectionInfo::parse("postgres://u@h/db?sslmode=verify-full").unwrap();
    assert_eq!(info.ssl_mode, SslMode::Require);
    let info = ConnectionInfo::parse("postgres://u@h/db?sslmode=disable").unwrap();
    assert_eq!(info.ssl_mode, SslMode::Disable);
    let info = ConnectionInfo::parse("postgres://u@h/db").unwrap();
    assert_eq!(info.ssl_mode, SslMode::Unspecified);
}

#[test]
fn sslmode_prefer_is_loudly_unsupported() {
    // Opportunistic TLS (try, then silently fall back to plaintext) is exactly
    // the silent downgrade this client refuses to implement.
    let err = ConnectionInfo::parse("postgres://u@h/db?sslmode=prefer").unwrap_err();
    assert!(format!("{err}").contains("prefer"));
}

// ── #877: connect_with_config's documented merge ──────────────────────────────

#[test]
fn merge_gives_the_url_explicit_components_priority() {
    use std::time::Duration;

    use crate::connection::ConnectionConfig;

    let config = ConnectionConfig::builder("cfg_db", "cfg_user")
        .password("cfg_pass")
        .statement_timeout(Duration::from_secs(30))
        .application_name("cfg_app")
        .build();

    let info = ConnectionInfo::parse("postgres://alice:s3cret@db.example.com:5432/prod").unwrap();
    let merged = info.merge_into_config(config);

    // The string's explicit credentials win — they used to be parsed and then
    // silently discarded, sending the wrong user with no password at all.
    assert_eq!(merged.user, "alice");
    assert_eq!(merged.password.as_ref().map(|p| p.as_str()), Some("s3cret"));
    assert_eq!(merged.database, "prod");
    // Fields the string does not name keep the caller's values.
    assert_eq!(merged.statement_timeout, Some(Duration::from_secs(30)));
    assert_eq!(merged.application_name.as_deref(), Some("cfg_app"));
}

#[test]
fn merge_keeps_config_values_where_the_url_is_silent() {
    use crate::connection::ConnectionConfig;

    let config = ConnectionConfig::builder("cfg_db", "cfg_user")
        .password("cfg_pass")
        .build();

    // Host-only URL: no userinfo, no database, no params.
    let info = ConnectionInfo::parse("postgres://db.example.com:5432").unwrap();
    let merged = info.merge_into_config(config);

    assert_eq!(merged.user, "cfg_user");
    assert_eq!(
        merged.password.as_ref().map(|p| p.as_str()),
        Some("cfg_pass")
    );
    assert_eq!(merged.database, "cfg_db");
}

#[test]
fn merge_lets_url_params_override_config() {
    use std::time::Duration;

    use crate::connection::ConnectionConfig;

    let config = ConnectionConfig::builder("cfg_db", "cfg_user")
        .application_name("cfg_app")
        .connect_timeout(Duration::from_secs(3))
        .build();

    let info =
        ConnectionInfo::parse("postgres://u@h/db?application_name=url_app&connect_timeout=9")
            .unwrap();
    let merged = info.merge_into_config(config);

    assert_eq!(merged.application_name.as_deref(), Some("url_app"));
    assert_eq!(merged.connect_timeout, Some(Duration::from_secs(9)));
}

/// Property tests for #817: a query parameter must never bleed into the host,
/// the user or the database name.
///
/// `parse_tcp` used to split userinfo at `rfind('@')` and take everything after
/// the first `/` as the database, with no `?` handling at all. So every standard
/// libpq parameter (`sslmode`, `application_name`, …) was appended to the
/// database name, and an `@` inside a parameter *value* was taken as the userinfo
/// delimiter — the host was then parsed out of the tail of the query string.
///
/// Both failures are silent, and one is security-relevant: `sslmode` decides
/// whether the connection is encrypted, so folding it into the database name
/// loses the TLS requirement rather than failing the connection.
///
/// These are proptests rather than libFuzzer targets because `connection_string`
/// is a private module. Widening it to `pub` purely for a fuzz target would
/// enlarge the crate's supported API surface to buy test reach; in-crate
/// proptests reach it already and run in *every* CI leg rather than weekly.
#[cfg(test)]
mod issue_817_query_string_containment {
    use proptest::prelude::*;

    use super::*;

    /// Parameter values chosen to contain exactly the delimiters the old parser
    /// keyed on: `@` (userinfo), `/` (database), `?`/`&`/`=` (query).
    fn hostile_param_value() -> impl Strategy<Value = String> {
        prop::sample::select(vec![
            "a@b".to_string(),
            "a/b".to_string(),
            "a@b/c".to_string(),
            "x=y".to_string(),
            "p@ss/w@rd".to_string(),
            "hostile@evil.example".to_string(),
            "plain".to_string(),
        ])
    }

    fn ident() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9_]{0,7}".prop_map(|s| s)
    }

    proptest! {
        /// Whatever a parameter value contains, the authority and path components
        /// are parsed from before the `?` — so the host stays the host.
        #[test]
        fn query_parameters_never_change_the_host_or_database(
            user in ident(),
            host in ident(),
            db in ident(),
            app in hostile_param_value(),
        ) {
            let s = format!("postgres://{user}@{host}:5432/{db}?application_name={app}");
            let info = ConnectionInfo::parse(&s).unwrap();

            prop_assert_eq!(info.host.as_deref(), Some(host.as_str()));
            prop_assert_eq!(info.database.as_deref(), Some(db.as_str()));
            prop_assert_eq!(info.user.as_deref(), Some(user.as_str()));
        }

        /// The general invariant, independent of which parameter is used: no
        /// component may carry a query-string delimiter, because none of them can
        /// legally contain one.
        #[test]
        fn no_component_absorbs_the_query_string(
            host in ident(),
            db in ident(),
            value in hostile_param_value(),
            key in prop::sample::select(vec!["sslmode", "application_name", "connect_timeout"]),
        ) {
            let s = format!("postgres://{host}/{db}?{key}={value}");
            let Ok(info) = ConnectionInfo::parse(&s) else {
                return Ok(());
            };

            if let Some(h) = &info.host {
                prop_assert!(!h.contains('?'), "query string bled into host: {h:?}");
                prop_assert!(!h.contains('='), "host parsed out of query string: {h:?}");
                prop_assert!(!h.contains('/'), "host absorbed a path segment: {h:?}");
            }
            if let Some(d) = &info.database {
                prop_assert!(!d.contains('?'), "query string folded into database: {d:?}");
                prop_assert!(!d.contains('&'), "query string folded into database: {d:?}");
                prop_assert!(!d.contains('/'), "database is not one path segment: {d:?}");
            }
        }

        /// `sslmode` must still be *read* — the containment above would also be
        /// satisfied by a parser that discarded the query string entirely, and a
        /// silently dropped `sslmode=require` is how a TLS requirement is lost.
        #[test]
        fn sslmode_survives_a_hostile_neighbouring_parameter(
            host in ident(),
            db in ident(),
            app in hostile_param_value(),
        ) {
            let s = format!("postgres://{host}/{db}?application_name={app}&sslmode=require");
            let info = ConnectionInfo::parse(&s).unwrap();
            prop_assert_eq!(info.ssl_mode, SslMode::Require);
        }
    }
}
