//! Connection string parsing
//!
//! Supports formats:
//! * postgres://[user[:password]@][host][:port][/database]
//! * <postgres:///database> (Unix socket, local)
//! * <postgres:///database?host=/path/to/socket> (Unix socket, custom directory)

use crate::connection::ConnectionConfig;
use crate::{Result, WireError};
use std::path::{Component, Path, PathBuf};
use zeroize::Zeroizing;

/// Maximum byte length for a Unix socket directory path.
///
/// Linux's `sun_path` field is 108 bytes; 4096 is the broader POSIX PATH_MAX.
/// Any path longer than this cannot be a valid socket directory.
const MAX_SOCKET_DIR_BYTES: usize = 4096;

/// Validate a Unix socket directory path supplied via the `host` query parameter.
///
/// # Errors
///
/// Returns `WireError::Config` if:
/// - `dir` is longer than `MAX_SOCKET_DIR_BYTES`
/// - `dir` is not an absolute path (does not start with `/`)
/// - `dir` contains a `..` component (path traversal)
fn validate_socket_dir(dir: &str) -> Result<()> {
    if dir.len() > MAX_SOCKET_DIR_BYTES {
        return Err(WireError::Config(format!(
            "Unix socket directory path is too long ({} bytes, max {MAX_SOCKET_DIR_BYTES})",
            dir.len()
        )));
    }

    let p = Path::new(dir);
    if !p.is_absolute() {
        return Err(WireError::Config(format!(
            "Unix socket directory must be an absolute path (got {dir:?})"
        )));
    }

    if p.components().any(|c| c == Component::ParentDir) {
        return Err(WireError::Config(format!(
            "Unix socket directory must not contain '..' components (got {dir:?})"
        )));
    }

    Ok(())
}

/// Parsed connection info
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Transport type
    pub transport: TransportType,
    /// Host (for TCP)
    pub host: Option<String>,
    /// Port (for TCP)
    pub port: Option<u16>,
    /// Unix socket path
    pub unix_socket: Option<PathBuf>,
    /// Database name
    pub database: String,
    /// Username
    pub user: String,
    /// Password (zeroed on drop for security)
    pub password: Option<Zeroizing<String>>,
}

/// Transport type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// TCP socket
    Tcp,
    /// Unix domain socket
    Unix,
}

/// Resolve the default Unix socket directory
fn resolve_default_socket_dir() -> Option<String> {
    // Try standard locations in order (Linux convention)
    for dir in &["/run/postgresql", "/var/run/postgresql", "/tmp"] {
        if Path::new(dir).is_dir() {
            return Some(dir.to_string());
        }
    }
    None
}

/// Extract a query parameter value from a query string
fn parse_query_param(query_string: &str, param: &str) -> Option<String> {
    if query_string.is_empty() {
        return None;
    }

    // Remove leading '?' if present
    let query = query_string.trim_start_matches('?');

    // Find the parameter
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == param {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Construct the full Unix socket path
fn construct_socket_path(socket_dir: &str, port: u16) -> PathBuf {
    PathBuf::from(format!("{}/.s.PGSQL.{}", socket_dir, port))
}

impl ConnectionInfo {
    /// Parse connection string
    pub fn parse(s: &str) -> Result<Self> {
        // Simple parser (production code would use url crate)
        if !s.starts_with("postgres://") && !s.starts_with("postgresql://") {
            return Err(WireError::Config(
                "connection string must start with postgres://".into(),
            ));
        }

        let rest = s
            .strip_prefix("postgres://")
            .or_else(|| s.strip_prefix("postgresql://"))
            .expect("prefix check above guarantees one of these prefixes is present");

        // Check if Unix socket (starts with / or no host)
        if rest.starts_with('/') || rest.starts_with("///") {
            return Self::parse_unix(rest);
        }

        Self::parse_tcp(rest)
    }

    fn parse_unix(rest: &str) -> Result<Self> {
        // Format: postgres:///database or postgres:///database?host=/path/to/socket&port=5432
        // Split database name from query parameters
        let (path, query_string) = if let Some(q_pos) = rest.find('?') {
            let (p, q) = rest.split_at(q_pos);
            (p, q)
        } else {
            (rest, "")
        };

        let path = path.trim_start_matches('/');

        let database = if path.is_empty() {
            whoami::username()
        } else {
            path.to_string()
        };

        // Parse port from query parameters (default: 5432)
        let port = parse_query_param(query_string, "port")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(5432);

        // Determine socket directory
        let socket_dir = if let Some(custom_dir) = parse_query_param(query_string, "host") {
            // Validate before use: must be absolute, no traversal, within length limit.
            validate_socket_dir(&custom_dir)?;
            custom_dir
        } else {
            // Use default socket directory
            resolve_default_socket_dir().ok_or_else(|| {
                WireError::Config(
                    "could not locate Unix socket directory. Set host query parameter explicitly."
                        .into(),
                )
            })?
        };

        let unix_socket = Some(construct_socket_path(&socket_dir, port));

        Ok(Self {
            transport: TransportType::Unix,
            host: None,
            port: Some(port),
            unix_socket,
            database,
            user: whoami::username(),
            password: None,
        })
    }

    fn parse_tcp(rest: &str) -> Result<Self> {
        // Format: [user[:password]@]host[:port][/database]
        let (auth, rest) = if let Some(pos) = rest.find('@') {
            let (auth, rest) = rest.split_at(pos);
            (Some(auth), &rest[1..])
        } else {
            (None, rest)
        };

        let (user, password) = if let Some(auth) = auth {
            if let Some(pos) = auth.find(':') {
                let (user, pass) = auth.split_at(pos);
                (
                    user.to_string(),
                    Some(Zeroizing::new(pass[1..].to_string())),
                )
            } else {
                (auth.to_string(), None)
            }
        } else {
            (whoami::username(), None)
        };

        let (host_port, database) = if let Some(pos) = rest.find('/') {
            let (hp, db) = rest.split_at(pos);
            (hp, db[1..].to_string())
        } else {
            (rest, whoami::username())
        };

        let (host, port) = if let Some(pos) = host_port.find(':') {
            let (host, port) = host_port.split_at(pos);
            let port = port[1..]
                .parse()
                .map_err(|_| WireError::Config("invalid port".into()))?;
            (host.to_string(), port)
        } else {
            (host_port.to_string(), 5432)
        };

        Ok(Self {
            transport: TransportType::Tcp,
            host: Some(host),
            port: Some(port),
            unix_socket: None,
            database,
            user,
            password,
        })
    }

    /// Convert to `ConnectionConfig`
    pub fn to_config(&self) -> ConnectionConfig {
        let mut config = ConnectionConfig::new(&self.database, &self.user);
        if let Some(ref password) = self.password {
            // SECURITY: Extract password string from Zeroizing wrapper
            config = config.password(password.as_str());
        }
        config
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
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
        validate_socket_dir("/tmp")
            .unwrap_or_else(|e| panic!("expected Ok for /tmp: {e}"));
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
}
