//! FraiseClient implementation

use super::connection_string::{ConnectionInfo, TransportType};
use super::query_builder::QueryBuilder;
use crate::connection::{Connection, Transport};
use crate::stream::JsonStream;
use crate::Result;

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

    /// Connect to Postgres with TLS encryption (future feature)
    ///
    /// # Status
    /// This API is planned for v0.1.1. For now, use the standard `connect()` method
    /// and implement TLS via a reverse proxy (e.g., pgbouncer, HAProxy) in production.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use fraiseql_wire::{FraiseClient, connection::TlsConfig};
    ///
    /// let tls = TlsConfig::builder()
    ///     .verify_hostname(true)
    ///     .build()?;
    ///
    /// let client = FraiseClient::connect_tls("postgres://secure.db.example.com/mydb", tls).await?;
    /// ```
    pub async fn connect_tls(
        _connection_string: &str,
        _tls_config: &crate::connection::TlsConfig,
    ) -> Result<Self> {
        Err(crate::Error::Config(
            "TLS support is planned for v0.1.1. Use a reverse proxy for TLS in production.".into(),
        ))
    }

    /// Start building a query for an entity (consumes self for streaming)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(client: fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
    /// let stream = client
    ///     .query("user")
    ///     .where_sql("data->>'type' = 'customer'")  // SQL predicate
    ///     .where_rust(|json| {
    ///         // Rust predicate (applied client-side)
    ///         json["estimated_value"].as_f64().unwrap_or(0.0) > 1000.0
    ///     })
    ///     .order_by("data->>'name' ASC")
    ///     .execute()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn query(self, entity: impl Into<String>) -> QueryBuilder {
        QueryBuilder::new(self, entity)
    }

    /// Execute a raw SQL query (must match fraiseql-wire constraints)
    pub(crate) async fn execute_query(self, sql: &str, chunk_size: usize) -> Result<JsonStream> {
        self.conn.streaming_query(sql, chunk_size).await
    }
}
