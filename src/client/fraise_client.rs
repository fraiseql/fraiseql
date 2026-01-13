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
