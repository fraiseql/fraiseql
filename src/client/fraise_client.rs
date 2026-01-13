//! FraiseClient implementation

use super::connection_string::{ConnectionInfo, TransportType};
use super::query_builder::QueryBuilder;
use crate::connection::{Connection, ConnectionConfig, Transport};
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
    /// # Examples
    ///
    /// ```no_run
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
    pub async fn connect(connection_string: &str) -> Result<Self> {
        let info = ConnectionInfo::parse(connection_string)?;

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().expect("TCP requires host");
                let port = info.port.expect("TCP requires port");
                Transport::connect_tcp(host, port).await?
            }
            TransportType::Unix => {
                let path = info.unix_socket.as_ref().expect("Unix requires path");
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
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> fraiseql_wire::Result<()> {
    /// use fraiseql_wire::{FraiseClient, connection::TlsConfig};
    ///
    /// // Configure TLS with system root certificates
    /// let tls = TlsConfig::builder()
    ///     .verify_hostname(true)
    ///     .build()?;
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

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().expect("TCP requires host");
                let port = info.port.expect("TCP requires port");
                Transport::connect_tcp_tls(host, port, &tls_config).await?
            }
            TransportType::Unix => {
                return Err(crate::Error::Config(
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
    /// connection options. The connection configuration is merged with parameters from
    /// the connection string.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> fraiseql_wire::Result<()> {
    /// use fraiseql_wire::{FraiseClient, connection::ConnectionConfig};
    /// use std::time::Duration;
    ///
    /// // Build connection configuration with timeouts
    /// let config = ConnectionConfig::builder("localhost", "mydb")
    ///     .password("secret")
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

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().expect("TCP requires host");
                let port = info.port.expect("TCP requires port");
                Transport::connect_tcp(host, port).await?
            }
            TransportType::Unix => {
                let path = info.unix_socket.as_ref().expect("Unix requires path");
                Transport::connect_unix(path).await?
            }
        };

        let mut conn = Connection::new(transport);
        conn.startup(&config).await?;

        Ok(Self { conn })
    }

    /// Connect to Postgres with both custom configuration and TLS encryption
    ///
    /// This method combines connection configuration (timeouts, keepalive, etc.)
    /// with TLS encryption for secure connections with advanced options.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> fraiseql_wire::Result<()> {
    /// use fraiseql_wire::{FraiseClient, connection::{ConnectionConfig, TlsConfig}};
    /// use std::time::Duration;
    ///
    /// // Configure connection with timeouts
    /// let config = ConnectionConfig::builder("localhost", "mydb")
    ///     .password("secret")
    ///     .statement_timeout(Duration::from_secs(30))
    ///     .build();
    ///
    /// // Configure TLS
    /// let tls = TlsConfig::builder()
    ///     .verify_hostname(true)
    ///     .build()?;
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

        let transport = match info.transport {
            TransportType::Tcp => {
                let host = info.host.as_ref().expect("TCP requires host");
                let port = info.port.expect("TCP requires port");
                Transport::connect_tcp_tls(host, port, &tls_config).await?
            }
            TransportType::Unix => {
                return Err(crate::Error::Config(
                    "TLS is only supported for TCP connections".into(),
                ));
            }
        };

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
    pub(crate) async fn execute_query(
        self,
        sql: &str,
        chunk_size: usize,
        max_memory: Option<usize>,
        soft_limit_warn_threshold: Option<f32>,
        soft_limit_fail_threshold: Option<f32>,
    ) -> Result<JsonStream> {
        self.conn.streaming_query(
            sql,
            chunk_size,
            max_memory,
            soft_limit_warn_threshold,
            soft_limit_fail_threshold,
            false,  // enable_adaptive_chunking: disabled by default for backward compatibility
            None,   // adaptive_min_chunk_size
            None,   // adaptive_max_chunk_size
        ).await
    }
}
