//! `FraiseClient` implementation

use super::connection_string::{ConnectionInfo, TransportType};
use super::query_builder::QueryBuilder;
use crate::connection::{Connection, ConnectionConfig, Transport};
#[allow(unused_imports)] // Reason: used only in doc links for `# Errors` sections
use crate::error::WireError;
use crate::stream::JsonStream;
use crate::Result;
use serde::de::DeserializeOwned;

/// FraiseQL wire protocol client
pub struct FraiseClient {
    conn: Connection,
}

impl FraiseClient {
    /// Connect to Postgres using connection string
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Config`] if the connection string is invalid or missing required
    /// fields. Returns [`WireError`] if the TCP or Unix socket connection fails, or if
    /// startup/authentication is rejected by the server.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires: live Postgres server.
    /// # async fn example() -> fraiseql_wire::Result<()> {
    /// use fraiseql_wire::FraiseClient;
    ///
    /// // TCP connection
    /// let client = FraiseClient::connect("postgres://localhost/mydb").await?;
    ///
    /// // Unix socket
    /// let client = FraiseClient::connect("postgres:///mydb").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Config`] if the connection string is malformed or missing
    /// required fields (host/port for TCP, path for Unix sockets).
    /// Returns [`WireError::Io`] if the underlying TCP or Unix socket connection fails.
    pub async fn connect(connection_string: &str) -> Result<Self> {
        let info = ConnectionInfo::parse(connection_string)?;
        require_plaintext_allowed(&info)?;

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().ok_or_else(|| {
                    crate::WireError::Config("TCP transport requires a host".into())
                })?;
                let port = info.port.ok_or_else(|| {
                    crate::WireError::Config("TCP transport requires a port".into())
                })?;
                Transport::connect_tcp(host, port).await?
            }
            TransportType::Unix => {
                let path = info.unix_socket.as_ref().ok_or_else(|| {
                    crate::WireError::Config("Unix transport requires a socket path".into())
                })?;
                Transport::connect_unix(path).await?
            }
        };

        let mut conn = Connection::new(transport);
        let config = info.to_config();
        conn.startup(&config).await?;

        Ok(Self { conn })
    }

    /// Connect to Postgres with TLS encryption
    ///
    /// TLS is configured independently from the connection string. The connection string
    /// should contain the hostname and credentials (user/password), while TLS configuration
    /// is provided separately via `TlsConfig`.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Config`] if the connection string is invalid, TLS is requested
    /// over a Unix socket, or required fields are missing. Returns [`WireError::Io`] if the
    /// TLS handshake or TCP connection fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires: live Postgres server with TLS.
    /// # async fn example() -> fraiseql_wire::Result<()> {
    /// use fraiseql_wire::{FraiseClient, connection::TlsConfig};
    ///
    /// // Configure TLS with system root certificates
    /// let tls = TlsConfig::builder().build()?;
    ///
    /// // Connect with TLS
    /// let client = FraiseClient::connect_tls("postgres://secure.db.example.com/mydb", tls).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect_tls(
        connection_string: &str,
        tls_config: crate::connection::TlsConfig,
    ) -> Result<Self> {
        let info = ConnectionInfo::parse(connection_string)?;
        require_tls_allowed(&info)?;

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().ok_or_else(|| {
                    crate::WireError::Config("TCP transport requires a host".into())
                })?;
                let port = info.port.ok_or_else(|| {
                    crate::WireError::Config("TCP transport requires a port".into())
                })?;
                Transport::connect_tcp_tls(host, port, &tls_config).await?
            }
            TransportType::Unix => {
                return Err(crate::WireError::Config(
                    "TLS is only supported for TCP connections".into(),
                ));
            }
        };

        let mut conn = Connection::new(transport);
        let config = info.to_config();
        conn.startup(&config).await?;

        Ok(Self { conn })
    }

    /// Connect to Postgres with custom connection configuration
    ///
    /// This method allows you to configure timeouts, keepalive intervals, and other
    /// connection options. The connection string's explicit components (user,
    /// password, database, `application_name`, `connect_timeout`) override the
    /// passed configuration; every field the string does not name keeps the
    /// caller's value.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Config`] if the connection string is invalid or missing required
    /// fields. Returns [`WireError::Io`] if the TCP or Unix socket connection fails, or if
    /// startup/authentication is rejected by the server.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires: live Postgres server.
    /// # async fn example() -> fraiseql_wire::Result<()> {
    /// use fraiseql_wire::{FraiseClient, connection::ConnectionConfig};
    /// use std::time::Duration;
    ///
    /// // Build connection configuration with timeouts (builder takes
    /// // (database, user); the URL's components override them when present)
    /// let config = ConnectionConfig::builder("mydb", "app_user")
    ///     .statement_timeout(Duration::from_secs(30))
    ///     .keepalive_idle(Duration::from_secs(300))
    ///     .application_name("my_app")
    ///     .build();
    ///
    /// // Connect with configuration
    /// let client = FraiseClient::connect_with_config("postgres://localhost:5432/mydb", config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect_with_config(
        connection_string: &str,
        config: ConnectionConfig,
    ) -> Result<Self> {
        let info = ConnectionInfo::parse(connection_string)?;
        require_plaintext_allowed(&info)?;
        // The documented merge (#877): the string's explicit credentials and
        // parameters win; they were previously parsed and then discarded.
        let config = info.merge_into_config(config);

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().ok_or_else(|| {
                    crate::WireError::Config("TCP transport requires a host".into())
                })?;
                let port = info.port.ok_or_else(|| {
                    crate::WireError::Config("TCP transport requires a port".into())
                })?;
                with_connect_timeout(config.connect_timeout, Transport::connect_tcp(host, port))
                    .await?
            }
            TransportType::Unix => {
                let path = info.unix_socket.as_ref().ok_or_else(|| {
                    crate::WireError::Config("Unix transport requires a socket path".into())
                })?;
                with_connect_timeout(config.connect_timeout, Transport::connect_unix(path)).await?
            }
        };

        // Apply TCP keepalive when configured.
        if let Some(idle) = config.keepalive_idle {
            if let Err(e) = transport.apply_keepalive(idle) {
                tracing::warn!("Failed to apply TCP keepalive (idle={idle:?}): {e}");
            }
        }

        let mut conn = Connection::new(transport);
        conn.startup(&config).await?;

        Ok(Self { conn })
    }

    /// Connect to Postgres with both custom configuration and TLS encryption
    ///
    /// This method combines connection configuration (timeouts, keepalive, etc.)
    /// with TLS encryption for secure connections with advanced options. The
    /// connection string's explicit components (user, password, database,
    /// `application_name`, `connect_timeout`) override the passed
    /// configuration, exactly as in [`FraiseClient::connect_with_config`].
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Config`] if the connection string is invalid, TLS is requested
    /// over a Unix socket, or required fields are missing. Returns [`WireError::Io`] if the
    /// TLS handshake or TCP connection fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires: live Postgres server with TLS.
    /// # async fn example() -> fraiseql_wire::Result<()> {
    /// use fraiseql_wire::{FraiseClient, connection::{ConnectionConfig, TlsConfig}};
    /// use std::time::Duration;
    ///
    /// // Configure connection with timeouts (builder takes (database, user))
    /// let config = ConnectionConfig::builder("mydb", "app_user")
    ///     .statement_timeout(Duration::from_secs(30))
    ///     .build();
    ///
    /// // Configure TLS
    /// let tls = TlsConfig::builder().build()?;
    ///
    /// // Connect with both configuration and TLS
    /// let client = FraiseClient::connect_with_config_and_tls(
    ///     "postgres://secure.db.example.com/mydb",
    ///     config,
    ///     tls
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect_with_config_and_tls(
        connection_string: &str,
        config: ConnectionConfig,
        tls_config: crate::connection::TlsConfig,
    ) -> Result<Self> {
        let info = ConnectionInfo::parse(connection_string)?;
        require_tls_allowed(&info)?;
        let config = info.merge_into_config(config);

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().ok_or_else(|| {
                    crate::WireError::Config("TCP transport requires a host".into())
                })?;
                let port = info.port.ok_or_else(|| {
                    crate::WireError::Config("TCP transport requires a port".into())
                })?;
                with_connect_timeout(
                    config.connect_timeout,
                    Transport::connect_tcp_tls(host, port, &tls_config),
                )
                .await?
            }
            TransportType::Unix => {
                return Err(crate::WireError::Config(
                    "TLS is only supported for TCP connections".into(),
                ));
            }
        };

        // Apply TCP keepalive when configured.
        if let Some(idle) = config.keepalive_idle {
            if let Err(e) = transport.apply_keepalive(idle) {
                tracing::warn!("Failed to apply TCP keepalive (idle={idle:?}): {e}");
            }
        }

        let mut conn = Connection::new(transport);
        conn.startup(&config).await?;

        Ok(Self { conn })
    }

    /// Start building a query for an entity with automatic deserialization
    ///
    /// The type parameter T controls consumer-side deserialization only.
    /// Type T does NOT affect SQL generation, filtering, ordering, or wire protocol.
    ///
    /// # Examples
    ///
    /// Type-safe query (recommended):
    /// ```no_run
    /// // Requires: live Postgres server.
    /// # async fn example(client: fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
    /// use serde::Deserialize;
    /// use futures::stream::StreamExt;
    ///
    /// #[derive(Deserialize)]
    /// struct User {
    ///     id: String,
    ///     name: String,
    /// }
    ///
    /// let mut stream = client
    ///     .query::<User>("user")
    ///     .where_sql("data->>'type' = 'customer'")  // SQL predicate
    ///     .where_rust(|json| {
    ///         // Rust predicate (applied client-side, on JSON)
    ///         json["estimated_value"].as_f64().unwrap_or(0.0) > 1000.0
    ///     })
    ///     .order_by("data->>'name' ASC")
    ///     .execute()
    ///     .await?;
    ///
    /// while let Some(result) = stream.next().await {
    ///     let user: User = result?;
    ///     println!("User: {}", user.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Raw JSON query (debugging, forward compatibility):
    /// ```no_run
    /// // Requires: live Postgres server.
    /// # async fn example(client: fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
    /// use futures::stream::StreamExt;
    ///
    /// let mut stream = client
    ///     .query::<serde_json::Value>("user")  // Escape hatch
    ///     .execute()
    ///     .await?;
    ///
    /// while let Some(result) = stream.next().await {
    ///     let json = result?;
    ///     println!("JSON: {:?}", json);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn query<T: DeserializeOwned + std::marker::Unpin + 'static>(
        self,
        entity: impl Into<String>,
    ) -> QueryBuilder<T> {
        QueryBuilder::new(self, entity)
    }

    /// Execute a raw SQL query (must match fraiseql-wire constraints)
    ///
    /// The adaptive-chunking options are threaded through from the query builder
    /// instead of being hardcoded off, so `adaptive_chunking`/`adaptive_min_size`/
    /// `adaptive_max_size` actually take effect (audit L-wire-builder).
    #[allow(clippy::too_many_arguments)] // Reason: mirrors streaming_query's chunking parameters; a struct would add allocation in the hot path
    pub(crate) async fn execute_query(
        self,
        sql: &str,
        entity: &str,
        chunk_size: usize,
        max_memory: Option<usize>,
        soft_limit_warn_threshold: Option<f32>,
        soft_limit_fail_threshold: Option<f32>,
        enable_adaptive_chunking: bool,
        adaptive_min_chunk_size: Option<usize>,
        adaptive_max_chunk_size: Option<usize>,
    ) -> Result<JsonStream> {
        self.conn
            .streaming_query(
                sql,
                entity,
                chunk_size,
                max_memory,
                soft_limit_warn_threshold,
                soft_limit_fail_threshold,
                enable_adaptive_chunking,
                adaptive_min_chunk_size,
                adaptive_max_chunk_size,
            )
            .await
    }
}

/// Refuse a plaintext connect when the connection string demanded TLS.
///
/// `?sslmode=require` (or `verify-ca`/`verify-full`) on a plaintext entry point
/// used to be silently folded into the database name and ignored (#817); a user
/// who believes it enforces TLS must get an error, not a cleartext connection.
fn require_plaintext_allowed(info: &ConnectionInfo) -> Result<()> {
    if info.ssl_mode == crate::client::connection_string::SslMode::Require {
        return Err(crate::WireError::Config(
            "the connection string demands TLS (sslmode=require/verify-*) but this is a \
             plaintext connect: use connect_tls or connect_with_config_and_tls"
                .into(),
        ));
    }
    Ok(())
}

/// Refuse a TLS connect when the connection string demanded plaintext.
fn require_tls_allowed(info: &ConnectionInfo) -> Result<()> {
    if info.ssl_mode == crate::client::connection_string::SslMode::Disable {
        return Err(crate::WireError::Config(
            "the connection string demands plaintext (sslmode=disable) but this is a TLS \
             connect: drop the parameter or use a plaintext connect method"
                .into(),
        ));
    }
    Ok(())
}

/// Apply an optional connect timeout to a transport-connect future.
///
/// When `timeout` is `Some`, the future is bounded by [`tokio::time::timeout`] and a
/// lapse surfaces as [`crate::WireError::Connection`]; when `None` the future runs
/// to completion unbounded. The `connect_timeout` config field was parsed but never
/// applied to the connect path (audit L-wire-timeout); the `connect_with_config*`
/// methods now route their transport setup through this helper.
async fn with_connect_timeout<F, T>(timeout: Option<std::time::Duration>, fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    match timeout {
        Some(d) => match tokio::time::timeout(d, fut).await {
            Ok(result) => result,
            Err(_) => Err(crate::WireError::Connection(format!(
                "connection timed out after {d:?}"
            ))),
        },
        None => fut.await,
    }
}

#[cfg(test)]
mod tests;
