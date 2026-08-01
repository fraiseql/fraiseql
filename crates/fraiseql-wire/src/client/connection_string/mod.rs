//! Connection string parsing
//!
//! Supports formats:
//! * postgres://[user[:password]@][host][:port][/database][?params]
//! * <postgres:///database> (Unix socket, local)
//! * <postgres:///database?host=/path/to/socket> (Unix socket, custom directory)
//!
//! Query parameters are parsed strictly: the TCP form accepts `sslmode`
//! (`disable`, `require`, `verify-ca`, `verify-full` — the opportunistic
//! `prefer`/`allow` modes are refused rather than silently downgraded),
//! `application_name` and `connect_timeout` (seconds); the Unix form accepts
//! `host` (socket directory) and `port`. Any other parameter is a loud
//! [`WireError::Config`] naming the parameter — never silently folded into the
//! database name or dropped (#817).

use crate::connection::ConnectionConfig;
use crate::{Result, WireError};
use std::path::{Component, Path, PathBuf};
use zeroize::Zeroizing;

/// Split a `host_port` string into `(host, port)`, handling RFC 3986 §3.2.2
/// bracket notation for IPv6 literals (`[::1]:5432`).
///
/// Accepted formats:
/// - `[host]:port`  — IPv6 literal with explicit port
/// - `[host]`       — IPv6 literal, default port 5432
/// - `host:port`    — hostname or IPv4 with explicit port
/// - `host`         — hostname or IPv4, default port 5432
///
/// # Errors
///
/// Returns `WireError::Config` if a port string is present but not a valid `u16`.
fn split_host_port(host_port: &str) -> Result<(String, u16)> {
    if host_port.starts_with('[') {
        // IPv6 bracket notation
        let close = host_port
            .find(']')
            .ok_or_else(|| WireError::Config("unclosed '[' in IPv6 address".into()))?;
        let host = host_port[1..close].to_string();
        let rest = &host_port[close + 1..];
        let port = if let Some(port_str) = rest.strip_prefix(':') {
            port_str
                .parse()
                .map_err(|_| WireError::Config("invalid port in IPv6 address".into()))?
        } else {
            5432
        };
        Ok((host, port))
    } else if let Some(pos) = host_port.find(':') {
        let (host, port_str) = host_port.split_at(pos);
        let port = port_str[1..]
            .parse()
            .map_err(|_| WireError::Config("invalid port".into()))?;
        Ok((host.to_string(), port))
    } else {
        Ok((host_port.to_string(), 5432))
    }
}

/// Decode one hex digit (`0-9`, `a-f`, `A-F`) into its 0–15 value.
const fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Percent-decode a URL credential component (RFC 3986 §2.1).
///
/// Decodes `%XX` escapes to bytes and interprets the result as UTF-8. `+` is left
/// literal — it denotes a space only in `application/x-www-form-urlencoded` bodies,
/// not in a URI userinfo component.
///
/// # Errors
///
/// Returns [`WireError::Config`] if a `%` is not followed by two hex digits, or if
/// the decoded bytes are not valid UTF-8.
fn percent_decode(s: &str) -> Result<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            match (
                bytes.get(i + 1).copied().and_then(hex_val),
                bytes.get(i + 2).copied().and_then(hex_val),
            ) {
                (Some(hi), Some(lo)) => {
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                _ => {
                    return Err(WireError::Config(
                        "invalid percent-encoding in connection-string credential (expected %XX)"
                            .into(),
                    ));
                }
            }
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|e| {
        WireError::Config(format!(
            "percent-encoded connection-string credential is not valid UTF-8: {e}"
        ))
    })
}

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

/// TLS requirement expressed by the connection string's `sslmode` parameter.
///
/// The entry points enforce it: a plaintext `connect` refuses
/// [`SslMode::Require`], and a TLS connect refuses [`SslMode::Disable`] —
/// `sslmode` is never silently ignored (#817).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SslMode {
    /// No `sslmode` parameter: the entry point's transport stands.
    #[default]
    Unspecified,
    /// `sslmode=disable`: plaintext demanded.
    Disable,
    /// `sslmode=require` / `verify-ca` / `verify-full`: TLS demanded. The
    /// client's TLS stack always validates the chain and hostname, so the
    /// verify-* variants collapse to this.
    Require,
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
    /// Database name, when the string names one (`None` ⇒ OS-user default)
    pub database: Option<String>,
    /// Username, when the string names one (`None` ⇒ OS-user default)
    pub user: Option<String>,
    /// Password (zeroed on drop for security)
    pub password: Option<Zeroizing<String>>,
    /// TLS requirement from `sslmode`, for the entry points to enforce
    pub ssl_mode: SslMode,
    /// `application_name` query parameter
    pub application_name: Option<String>,
    /// `connect_timeout` query parameter (seconds)
    pub connect_timeout: Option<std::time::Duration>,
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

        // Strict parameter handling (#817): only `host` and `port` mean
        // anything on the Unix form; anything else must not be silently
        // dropped.
        for key in query_param_keys(query_string) {
            if key != "host" && key != "port" {
                return Err(WireError::Config(format!(
                    "unsupported query parameter {key:?} in Unix-socket connection string \
                     (supported: host, port)"
                )));
            }
        }

        let path = path.trim_start_matches('/');

        let database = if path.is_empty() {
            None
        } else {
            Some(percent_decode(path)?)
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
            user: None,
            password: None,
            ssl_mode: SslMode::Unspecified,
            application_name: None,
            connect_timeout: None,
        })
    }

    fn parse_tcp(rest: &str) -> Result<Self> {
        // Format: [user[:password]@]host[:port][/database][?params]
        //
        // The `?query` component is split off FIRST: an '@' or '/' inside a
        // query value must never influence the host or database parse (#817 —
        // the query string used to be folded into the database name, and a
        // later '@' re-split the host).
        let (rest, query_string) = if let Some(q_pos) = rest.find('?') {
            let (r, q) = rest.split_at(q_pos);
            (r, q)
        } else {
            (rest, "")
        };
        let params = parse_tcp_params(query_string)?;

        // Split userinfo from host at the LAST '@' of the pre-query portion: a
        // percent-encoded '@' in the password decodes to a literal later, but
        // the host component itself can never contain '@', so the final '@' is
        // always the delimiter (audit L-wire-connstr).
        let (auth, rest) = if let Some(pos) = rest.rfind('@') {
            let (auth, rest) = rest.split_at(pos);
            (Some(auth), &rest[1..])
        } else {
            (None, rest)
        };

        // Credentials are percent-encoded in the URL (a password may legitimately
        // contain '@', ':', '%', …); decode them before use.
        let (user, password) = if let Some(auth) = auth {
            if let Some(pos) = auth.find(':') {
                let (user, pass) = auth.split_at(pos);
                (
                    Some(percent_decode(user)?),
                    Some(Zeroizing::new(percent_decode(&pass[1..])?)),
                )
            } else {
                (Some(percent_decode(auth)?), None)
            }
        } else {
            (None, None)
        };

        let (host_port, database) = if let Some(pos) = rest.find('/') {
            let (hp, db) = rest.split_at(pos);
            let db = &db[1..];
            let database = if db.is_empty() {
                None
            } else {
                Some(percent_decode(db)?)
            };
            (hp, database)
        } else {
            (rest, None)
        };

        let (host, port) = split_host_port(host_port)?;

        Ok(Self {
            transport: TransportType::Tcp,
            host: Some(host),
            port: Some(port),
            unix_socket: None,
            database,
            user,
            password,
            ssl_mode: params.ssl_mode,
            application_name: params.application_name,
            connect_timeout: params.connect_timeout,
        })
    }

    /// The database to connect to: the string's, or the OS user name (the
    /// Postgres convention) when the string names none.
    #[must_use]
    pub fn database_or_default(&self) -> String {
        self.database.clone().unwrap_or_else(whoami::username)
    }

    /// The user to connect as: the string's, or the OS user name when the
    /// string names none.
    #[must_use]
    pub fn user_or_default(&self) -> String {
        self.user.clone().unwrap_or_else(whoami::username)
    }

    /// Convert to `ConnectionConfig`
    pub fn to_config(&self) -> ConnectionConfig {
        let mut config = ConnectionConfig::new(self.database_or_default(), self.user_or_default());
        if let Some(ref password) = self.password {
            // SECURITY: Extract password string from Zeroizing wrapper
            config = config.password(password.as_str());
        }
        config.application_name.clone_from(&self.application_name);
        config.connect_timeout = self.connect_timeout;
        config
    }

    /// Merge this parsed string into a caller-supplied config: the string's
    /// explicit components (user, password, database, `application_name`,
    /// `connect_timeout`) win; everything the string does not name keeps the
    /// caller's value. This is the documented `connect_with_config` contract —
    /// previously the string's credentials were silently discarded (#877).
    #[must_use]
    pub fn merge_into_config(&self, mut config: ConnectionConfig) -> ConnectionConfig {
        if let Some(database) = &self.database {
            config.database.clone_from(database);
        }
        if let Some(user) = &self.user {
            config.user.clone_from(user);
        }
        if let Some(password) = &self.password {
            config = config.password(password.as_str());
        }
        if let Some(app) = &self.application_name {
            config.application_name = Some(app.clone());
        }
        if let Some(timeout) = self.connect_timeout {
            config.connect_timeout = Some(timeout);
        }
        config
    }
}

/// The TCP query parameters this client supports.
struct TcpParams {
    ssl_mode: SslMode,
    application_name: Option<String>,
    connect_timeout: Option<std::time::Duration>,
}

/// Iterate the keys of a query string (leading `?` tolerated).
fn query_param_keys(query_string: &str) -> impl Iterator<Item = &str> {
    query_string
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').map_or(pair, |(k, _)| k))
}

/// Strictly parse the TCP query string: `sslmode`, `application_name` and
/// `connect_timeout` are honoured; anything else — including libpq parameters
/// this client does not implement — is a loud [`WireError::Config`] naming the
/// parameter, never a silent drop (#817).
fn parse_tcp_params(query_string: &str) -> Result<TcpParams> {
    let mut params = TcpParams {
        ssl_mode: SslMode::Unspecified,
        application_name: None,
        connect_timeout: None,
    };
    let query = query_string.trim_start_matches('?');
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "sslmode" => {
                params.ssl_mode = match value {
                    "disable" => SslMode::Disable,
                    "require" | "verify-ca" | "verify-full" => SslMode::Require,
                    "prefer" | "allow" => {
                        return Err(WireError::Config(format!(
                            "sslmode={value} (opportunistic TLS) is not supported: it \
                             silently falls back to plaintext. Use sslmode=require with \
                             connect_tls, or sslmode=disable."
                        )));
                    }
                    other => {
                        return Err(WireError::Config(format!(
                            "invalid sslmode {other:?} (supported: disable, require, \
                             verify-ca, verify-full)"
                        )));
                    }
                };
            }
            "application_name" => {
                params.application_name = Some(percent_decode(value)?);
            }
            "connect_timeout" => {
                let secs: u64 = value.parse().map_err(|_| {
                    WireError::Config(format!(
                        "invalid connect_timeout {value:?}: expected whole seconds"
                    ))
                })?;
                params.connect_timeout = Some(std::time::Duration::from_secs(secs));
            }
            other => {
                return Err(WireError::Config(format!(
                    "unsupported query parameter {other:?} in connection string \
                     (supported: sslmode, application_name, connect_timeout)"
                )));
            }
        }
    }
    Ok(params)
}

#[cfg(test)]
mod tests;
