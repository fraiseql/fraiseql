//! Connection string parsing
//!
//! Supports formats:
//! * postgres://[user[:password]@][host][:port][/database]
//! * postgres:///database (Unix socket, local)

use crate::connection::ConnectionConfig;
use crate::{Error, Result};
use std::path::PathBuf;

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
    /// Password
    pub password: Option<String>,
}

/// Transport type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// TCP socket
    Tcp,
    /// Unix domain socket
    Unix,
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
        // Format: postgres:///database or postgres:////path/to/socket/database
        let path = rest.trim_start_matches('/');

        let database = if path.is_empty() {
            whoami::username()
        } else {
            path.to_string()
        };

        Ok(Self {
            transport: TransportType::Unix,
            host: None,
            port: None,
            unix_socket: Some(PathBuf::from("/var/run/postgresql")),
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
                (user.to_string(), Some(pass[1..].to_string()))
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
            config = config.password(password);
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
        assert_eq!(info.password, Some("pass".to_string()));
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
    }
}
