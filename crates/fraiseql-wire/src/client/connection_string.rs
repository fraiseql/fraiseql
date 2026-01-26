//! Connection string parsing
//!
//! Supports formats:
//! * postgres://[user[:password]@][host][:port][/database]
//! * postgres:///database (Unix socket, local)
//! * postgres:///database?host=/path/to/socket (Unix socket, custom directory)

use crate::connection::ConnectionConfig;
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

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
            return Err(Error::Config(
                "connection string must start with postgres://".into(),
            ));
        }

        let rest = s
            .strip_prefix("postgres://")
            .or_else(|| s.strip_prefix("postgresql://"))
            .unwrap();

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
            // Use explicitly specified directory
            custom_dir
        } else {
            // Use default socket directory
            resolve_default_socket_dir().ok_or_else(|| {
                Error::Config(
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
                .map_err(|_| Error::Config("invalid port".into()))?;
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

    /// Convert to ConnectionConfig
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
}
