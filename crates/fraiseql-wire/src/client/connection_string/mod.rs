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
pub fn validate_socket_dir(dir: &str) -> Result<()> {
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
#[non_exhaustive]
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
            return Some((*dir).to_string());
        }
    }
    None
}

/// Extract a query parameter value from a query string
pub fn parse_query_param(query_string: &str, param: &str) -> Option<String> {
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
pub fn construct_socket_path(socket_dir: &str, port: u16) -> PathBuf {
    PathBuf::from(format!("{}/.s.PGSQL.{}", socket_dir, port))
}

impl ConnectionInfo {
    /// Parse connection string
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Config`] if the string does not start with `postgres://` or
    /// `postgresql://`, or if the host/port/database fields cannot be parsed.
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
mod tests;
