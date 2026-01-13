//! Core connection type

use super::state::ConnectionState;
use super::transport::Transport;
use crate::protocol::{
    decode_message, encode_message, AuthenticationMessage, BackendMessage, FrontendMessage,
};
use crate::{Error, Result};
use bytes::{Buf, BytesMut};
use std::collections::HashMap;

/// Connection configuration
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
}

impl ConnectionConfig {
    /// Create new configuration
    pub fn new(database: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            user: user.into(),
            password: None,
            params: HashMap::new(),
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
}
