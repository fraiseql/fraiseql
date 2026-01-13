//! Core connection type

use super::state::ConnectionState;
use super::transport::Transport;
use crate::protocol::{
    decode_message, encode_message, AuthenticationMessage, BackendMessage, FrontendMessage,
};
use crate::{Error, Result};
use bytes::{Buf, BytesMut};
use std::collections::HashMap;
use std::time::Duration;

/// Connection configuration
///
/// Stores connection parameters including database, credentials, and optional timeouts.
/// Use `ConnectionConfig::builder()` for advanced configuration with timeouts and keepalive.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Database name
    pub database: String,
    /// Username
    pub user: String,
    /// Password (optional)
    pub password: Option<String>,
    /// Additional connection parameters
    pub params: HashMap<String, String>,
    /// TCP connection timeout (default: 10 seconds)
    pub connect_timeout: Option<Duration>,
    /// Query statement timeout
    pub statement_timeout: Option<Duration>,
    /// TCP keepalive idle interval (default: 5 minutes)
    pub keepalive_idle: Option<Duration>,
    /// Application name for Postgres logs (default: "fraiseql-wire")
    pub application_name: Option<String>,
    /// Postgres extra_float_digits setting
    pub extra_float_digits: Option<i32>,
}

impl ConnectionConfig {
    /// Create new configuration with defaults
    ///
    /// # Arguments
    ///
    /// * `database` - Database name
    /// * `user` - Username
    ///
    /// # Defaults
    ///
    /// - `connect_timeout`: None
    /// - `statement_timeout`: None
    /// - `keepalive_idle`: None
    /// - `application_name`: None
    /// - `extra_float_digits`: None
    ///
    /// For configured timeouts and keepalive, use `builder()` instead.
    pub fn new(database: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            user: user.into(),
            password: None,
            params: HashMap::new(),
            connect_timeout: None,
            statement_timeout: None,
            keepalive_idle: None,
            application_name: None,
            extra_float_digits: None,
        }
    }

    /// Create a builder for advanced configuration
    ///
    /// Use this to configure timeouts, keepalive, and application name.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let config = ConnectionConfig::builder("mydb", "user")
    ///     .connect_timeout(Duration::from_secs(10))
    ///     .statement_timeout(Duration::from_secs(30))
    ///     .build();
    /// ```
    pub fn builder(database: impl Into<String>, user: impl Into<String>) -> ConnectionConfigBuilder {
        ConnectionConfigBuilder {
            database: database.into(),
            user: user.into(),
            password: None,
            params: HashMap::new(),
            connect_timeout: None,
            statement_timeout: None,
            keepalive_idle: None,
            application_name: None,
            extra_float_digits: None,
        }
    }

    /// Set password
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Add connection parameter
    pub fn param(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

/// Builder for creating `ConnectionConfig` with advanced options
///
/// Provides a fluent API for configuring timeouts, keepalive, and application name.
///
/// # Examples
///
/// ```ignore
/// let config = ConnectionConfig::builder("mydb", "user")
///     .password("secret")
///     .connect_timeout(Duration::from_secs(10))
///     .statement_timeout(Duration::from_secs(30))
///     .keepalive_idle(Duration::from_secs(300))
///     .application_name("my_app")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ConnectionConfigBuilder {
    database: String,
    user: String,
    password: Option<String>,
    params: HashMap<String, String>,
    connect_timeout: Option<Duration>,
    statement_timeout: Option<Duration>,
    keepalive_idle: Option<Duration>,
    application_name: Option<String>,
    extra_float_digits: Option<i32>,
}

impl ConnectionConfigBuilder {
    /// Set the password
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Add a connection parameter
    pub fn param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// Set TCP connection timeout
    ///
    /// Default: None (no timeout)
    ///
    /// # Arguments
    ///
    /// * `duration` - Timeout duration for establishing TCP connection
    pub fn connect_timeout(mut self, duration: Duration) -> Self {
        self.connect_timeout = Some(duration);
        self
    }

    /// Set statement (query) timeout
    ///
    /// Default: None (unlimited)
    ///
    /// # Arguments
    ///
    /// * `duration` - Timeout duration for query execution
    pub fn statement_timeout(mut self, duration: Duration) -> Self {
        self.statement_timeout = Some(duration);
        self
    }

    /// Set TCP keepalive idle interval
    ///
    /// Default: None (OS default)
    ///
    /// # Arguments
    ///
    /// * `duration` - Idle duration before sending keepalive probes
    pub fn keepalive_idle(mut self, duration: Duration) -> Self {
        self.keepalive_idle = Some(duration);
        self
    }

    /// Set application name for Postgres logs
    ///
    /// Default: None (Postgres will not set application_name)
    ///
    /// # Arguments
    ///
    /// * `name` - Application name to identify in Postgres logs
    pub fn application_name(mut self, name: impl Into<String>) -> Self {
        self.application_name = Some(name.into());
        self
    }

    /// Set extra_float_digits for float precision
    ///
    /// Default: None (use Postgres default)
    ///
    /// # Arguments
    ///
    /// * `digits` - Number of extra digits (typically 0-2)
    pub fn extra_float_digits(mut self, digits: i32) -> Self {
        self.extra_float_digits = Some(digits);
        self
    }

    /// Build the configuration
    pub fn build(self) -> ConnectionConfig {
        ConnectionConfig {
            database: self.database,
            user: self.user,
            password: self.password,
            params: self.params,
            connect_timeout: self.connect_timeout,
            statement_timeout: self.statement_timeout,
            keepalive_idle: self.keepalive_idle,
            application_name: self.application_name,
            extra_float_digits: self.extra_float_digits,
        }
    }
}

/// Postgres connection
pub struct Connection {
    transport: Transport,
    state: ConnectionState,
    read_buf: BytesMut,
    process_id: Option<i32>,
    secret_key: Option<i32>,
}

impl Connection {
    /// Create connection from transport
    pub fn new(transport: Transport) -> Self {
        Self {
            transport,
            state: ConnectionState::Initial,
            read_buf: BytesMut::with_capacity(8192),
            process_id: None,
            secret_key: None,
        }
    }

    /// Get current connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Perform startup and authentication
    pub async fn startup(&mut self, config: &ConnectionConfig) -> Result<()> {
        let _span = tracing::info_span!(
            "startup",
            user = %config.user,
            database = %config.database
        )
        .entered();

        self.state.transition(ConnectionState::AwaitingAuth)?;

        // Build startup parameters
        let mut params = vec![
            ("user".to_string(), config.user.clone()),
            ("database".to_string(), config.database.clone()),
        ];
        for (k, v) in &config.params {
            params.push((k.clone(), v.clone()));
        }

        // Send startup message
        let startup = FrontendMessage::Startup {
            version: crate::protocol::constants::PROTOCOL_VERSION,
            params,
        };
        self.send_message(&startup).await?;

        // Authentication loop
        self.state.transition(ConnectionState::Authenticating)?;
        self.authenticate(config).await?;

        self.state.transition(ConnectionState::Idle)?;
        tracing::info!("startup complete");
        Ok(())
    }

    /// Handle authentication
    async fn authenticate(&mut self, config: &ConnectionConfig) -> Result<()> {
        loop {
            let msg = self.receive_message().await?;

            match msg {
                BackendMessage::Authentication(auth) => match auth {
                    AuthenticationMessage::Ok => {
                        tracing::debug!("authentication successful");
                        break;
                    }
                    AuthenticationMessage::CleartextPassword => {
                        let password = config
                            .password
                            .as_ref()
                            .ok_or_else(|| {
                                Error::Authentication("password required".into())
                            })?;
                        let pwd_msg = FrontendMessage::Password(password.clone());
                        self.send_message(&pwd_msg).await?;
                    }
                    AuthenticationMessage::Md5Password { .. } => {
                        return Err(Error::Authentication(
                            "MD5 authentication not yet implemented".into(),
                        ));
                    }
                },
                BackendMessage::BackendKeyData {
                    process_id,
                    secret_key,
                } => {
                    self.process_id = Some(process_id);
                    self.secret_key = Some(secret_key);
                }
                BackendMessage::ParameterStatus { name, value } => {
                    tracing::debug!("parameter status: {} = {}", name, value);
                }
                BackendMessage::ReadyForQuery { .. } => {
                    break;
                }
                BackendMessage::ErrorResponse(err) => {
                    return Err(Error::Authentication(err.to_string()));
                }
                _ => {
                    return Err(Error::Protocol(format!(
                        "unexpected message during auth: {:?}",
                        msg
                    )));
                }
            }
        }

        Ok(())
    }

    /// Execute a simple query (returns all backend messages)
    pub async fn simple_query(&mut self, query: &str) -> Result<Vec<BackendMessage>> {
        if self.state != ConnectionState::Idle {
            return Err(Error::ConnectionBusy(format!(
                "connection in state: {}",
                self.state
            )));
        }

        self.state.transition(ConnectionState::QueryInProgress)?;

        let query_msg = FrontendMessage::Query(query.to_string());
        self.send_message(&query_msg).await?;

        self.state.transition(ConnectionState::ReadingResults)?;

        let mut messages = Vec::new();

        loop {
            let msg = self.receive_message().await?;
            let is_ready = matches!(msg, BackendMessage::ReadyForQuery { .. });
            messages.push(msg);

            if is_ready {
                break;
            }
        }

        self.state.transition(ConnectionState::Idle)?;
        Ok(messages)
    }

    /// Send a frontend message
    async fn send_message(&mut self, msg: &FrontendMessage) -> Result<()> {
        let buf = encode_message(msg)?;
        self.transport.write_all(&buf).await?;
        self.transport.flush().await?;
        Ok(())
    }

    /// Receive a backend message
    async fn receive_message(&mut self) -> Result<BackendMessage> {
        loop {
            // Try to decode a message from buffer
            if let Ok((msg, remaining)) = decode_message(self.read_buf.clone().freeze()) {
                let consumed = self.read_buf.len() - remaining.len();
                self.read_buf.advance(consumed);
                return Ok(msg);
            }

            // Need more data
            let n = self.transport.read_buf(&mut self.read_buf).await?;
            if n == 0 {
                return Err(Error::ConnectionClosed);
            }
        }
    }

    /// Close the connection
    pub async fn close(mut self) -> Result<()> {
        self.state.transition(ConnectionState::Closed)?;
        let _ = self.send_message(&FrontendMessage::Terminate).await;
        self.transport.shutdown().await?;
        Ok(())
    }

    /// Execute a streaming query
    ///
    /// Note: This method consumes the connection. The stream maintains the connection
    /// internally. Once the stream is exhausted or dropped, the connection is closed.
    pub async fn streaming_query(
        mut self,
        query: &str,
        chunk_size: usize,
    ) -> Result<crate::stream::JsonStream> {
        let _span = tracing::debug_span!(
            "streaming_query",
            query = %query,
            chunk_size = %chunk_size
        )
        .entered();

        use crate::json::validate_row_description;
        use crate::stream::{extract_json_bytes, parse_json, ChunkingStrategy, JsonStream};
        use serde_json::Value;
        use tokio::sync::mpsc;

        if self.state != ConnectionState::Idle {
            return Err(Error::ConnectionBusy(format!(
                "connection in state: {}",
                self.state
            )));
        }

        self.state.transition(ConnectionState::QueryInProgress)?;

        let query_msg = FrontendMessage::Query(query.to_string());
        self.send_message(&query_msg).await?;

        self.state.transition(ConnectionState::ReadingResults)?;

        // Read RowDescription first
        let row_desc = self.receive_message().await?;
        validate_row_description(&row_desc)?;

        // Create channels
        let (result_tx, result_rx) = mpsc::channel::<Result<Value>>(chunk_size);
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);

        // Spawn background task to read rows
        tokio::spawn(async move {
            let strategy = ChunkingStrategy::new(chunk_size);
            let mut chunk = strategy.new_chunk();

            loop {
                tokio::select! {
                    // Check for cancellation
                    _ = cancel_rx.recv() => {
                        tracing::debug!("query cancelled");
                        break;
                    }

                    // Read next message
                    msg_result = self.receive_message() => {
                        match msg_result {
                            Ok(msg) => match msg {
                                BackendMessage::DataRow(_) => {
                                    match extract_json_bytes(&msg) {
                                        Ok(json_bytes) => {
                                            chunk.push(json_bytes);

                                            if strategy.is_full(&chunk) {
                                                let rows = chunk.into_rows();
                                                for row_bytes in rows {
                                                    match parse_json(row_bytes) {
                                                        Ok(value) => {
                                                            if result_tx.send(Ok(value)).await.is_err() {
                                                                break;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            let _ = result_tx.send(Err(e)).await;
                                                            break;
                                                        }
                                                    }
                                                }
                                                chunk = strategy.new_chunk();
                                            }
                                        }
                                        Err(e) => {
                                            let _ = result_tx.send(Err(e)).await;
                                            break;
                                        }
                                    }
                                }
                                BackendMessage::CommandComplete(_) => {
                                    // Send remaining chunk
                                    if !chunk.is_empty() {
                                        let rows = chunk.into_rows();
                                        for row_bytes in rows {
                                            match parse_json(row_bytes) {
                                                Ok(value) => {
                                                    if result_tx.send(Ok(value)).await.is_err() {
                                                        break;
                                                    }
                                                }
                                                Err(e) => {
                                                    let _ = result_tx.send(Err(e)).await;
                                                    break;
                                                }
                                            }
                                        }
                                        chunk = strategy.new_chunk();
                                    }
                                }
                                BackendMessage::ReadyForQuery { .. } => {
                                    break;
                                }
                                BackendMessage::ErrorResponse(err) => {
                                    let _ = result_tx.send(Err(Error::Sql(err.to_string()))).await;
                                    break;
                                }
                                _ => {
                                    let _ = result_tx.send(Err(Error::Protocol(
                                        format!("unexpected message: {:?}", msg)
                                    ))).await;
                                    break;
                                }
                            },
                            Err(e) => {
                                let _ = result_tx.send(Err(e)).await;
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(JsonStream::new(result_rx, cancel_tx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_config() {
        let config = ConnectionConfig::new("testdb", "testuser")
            .password("testpass")
            .param("application_name", "fraiseql-wire");

        assert_eq!(config.database, "testdb");
        assert_eq!(config.user, "testuser");
        assert_eq!(config.password, Some("testpass".to_string()));
        assert_eq!(
            config.params.get("application_name"),
            Some(&"fraiseql-wire".to_string())
        );
    }

    #[test]
    fn test_connection_config_builder_basic() {
        let config = ConnectionConfig::builder("mydb", "myuser")
            .password("mypass")
            .build();

        assert_eq!(config.database, "mydb");
        assert_eq!(config.user, "myuser");
        assert_eq!(config.password, Some("mypass".to_string()));
        assert_eq!(config.connect_timeout, None);
        assert_eq!(config.statement_timeout, None);
        assert_eq!(config.keepalive_idle, None);
        assert_eq!(config.application_name, None);
    }

    #[test]
    fn test_connection_config_builder_with_timeouts() {
        let connect_timeout = Duration::from_secs(10);
        let statement_timeout = Duration::from_secs(30);
        let keepalive_idle = Duration::from_secs(300);

        let config = ConnectionConfig::builder("mydb", "myuser")
            .password("mypass")
            .connect_timeout(connect_timeout)
            .statement_timeout(statement_timeout)
            .keepalive_idle(keepalive_idle)
            .build();

        assert_eq!(config.connect_timeout, Some(connect_timeout));
        assert_eq!(config.statement_timeout, Some(statement_timeout));
        assert_eq!(config.keepalive_idle, Some(keepalive_idle));
    }

    #[test]
    fn test_connection_config_builder_with_application_name() {
        let config = ConnectionConfig::builder("mydb", "myuser")
            .application_name("my_app")
            .extra_float_digits(2)
            .build();

        assert_eq!(config.application_name, Some("my_app".to_string()));
        assert_eq!(config.extra_float_digits, Some(2));
    }

    #[test]
    fn test_connection_config_builder_fluent() {
        let config = ConnectionConfig::builder("mydb", "myuser")
            .password("secret")
            .param("key1", "value1")
            .connect_timeout(Duration::from_secs(5))
            .statement_timeout(Duration::from_secs(60))
            .application_name("test_app")
            .build();

        assert_eq!(config.database, "mydb");
        assert_eq!(config.user, "myuser");
        assert_eq!(config.password, Some("secret".to_string()));
        assert_eq!(config.params.get("key1"), Some(&"value1".to_string()));
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(5)));
        assert_eq!(config.statement_timeout, Some(Duration::from_secs(60)));
        assert_eq!(config.application_name, Some("test_app".to_string()));
    }

    #[test]
    fn test_connection_config_defaults() {
        let config = ConnectionConfig::new("db", "user");

        assert!(config.connect_timeout.is_none());
        assert!(config.statement_timeout.is_none());
        assert!(config.keepalive_idle.is_none());
        assert!(config.application_name.is_none());
        assert!(config.extra_float_digits.is_none());
    }
}
