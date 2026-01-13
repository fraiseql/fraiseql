//! Core connection type

use super::state::ConnectionState;
use super::transport::Transport;
use crate::auth::ScramClient;
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

        // Add configured application name if specified
        if let Some(app_name) = &config.application_name {
            params.push(("application_name".to_string(), app_name.clone()));
        }

        // Add statement timeout if specified (in milliseconds)
        if let Some(timeout) = config.statement_timeout {
            params.push((
                "statement_timeout".to_string(),
                timeout.as_millis().to_string(),
            ));
        }

        // Add extra_float_digits if specified
        if let Some(digits) = config.extra_float_digits {
            params.push((
                "extra_float_digits".to_string(),
                digits.to_string(),
            ));
        }

        // Add user-provided parameters
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
        let auth_start = std::time::Instant::now();
        let mut auth_mechanism = "unknown";

        loop {
            let msg = self.receive_message().await?;

            match msg {
                BackendMessage::Authentication(auth) => match auth {
                    AuthenticationMessage::Ok => {
                        tracing::debug!("authentication successful");
                        crate::metrics::counters::auth_successful(auth_mechanism);
                        crate::metrics::histograms::auth_duration(
                            auth_mechanism,
                            auth_start.elapsed().as_millis() as u64,
                        );
                        break;
                    }
                    AuthenticationMessage::CleartextPassword => {
                        auth_mechanism = crate::metrics::labels::MECHANISM_CLEARTEXT;
                        crate::metrics::counters::auth_attempted(auth_mechanism);

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
                            "MD5 authentication not supported. Use SCRAM-SHA-256 or cleartext password".into(),
                        ));
                    }
                    AuthenticationMessage::Sasl { mechanisms } => {
                        auth_mechanism = crate::metrics::labels::MECHANISM_SCRAM;
                        crate::metrics::counters::auth_attempted(auth_mechanism);
                        self.handle_sasl(&mechanisms, config).await?;
                    }
                    AuthenticationMessage::SaslContinue { .. } => {
                        return Err(Error::Protocol(
                            "unexpected SaslContinue outside of SASL flow".into(),
                        ));
                    }
                    AuthenticationMessage::SaslFinal { .. } => {
                        return Err(Error::Protocol(
                            "unexpected SaslFinal outside of SASL flow".into(),
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
                    crate::metrics::counters::auth_failed(auth_mechanism, "server_error");
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

    /// Handle SASL authentication (SCRAM-SHA-256)
    async fn handle_sasl(&mut self, mechanisms: &[String], config: &ConnectionConfig) -> Result<()> {
        // Check if server supports SCRAM-SHA-256
        if !mechanisms.contains(&"SCRAM-SHA-256".to_string()) {
            return Err(Error::Authentication(
                format!(
                    "server does not support SCRAM-SHA-256. Available: {}",
                    mechanisms.join(", ")
                )
            ));
        }

        // Get password
        let password = config.password.as_ref().ok_or_else(|| {
            Error::Authentication("password required for SCRAM authentication".into())
        })?;

        // Create SCRAM client
        let mut scram = ScramClient::new(config.user.clone(), password.clone());
        tracing::debug!("initiating SCRAM-SHA-256 authentication");

        // Send SaslInitialResponse with client first message
        let client_first = scram.client_first();
        let msg = FrontendMessage::SaslInitialResponse {
            mechanism: "SCRAM-SHA-256".to_string(),
            data: client_first.into_bytes(),
        };
        self.send_message(&msg).await?;

        // Receive SaslContinue with server first message
        let server_first_msg = self.receive_message().await?;
        let server_first_data = match server_first_msg {
            BackendMessage::Authentication(AuthenticationMessage::SaslContinue { data }) => data,
            BackendMessage::ErrorResponse(err) => {
                return Err(Error::Authentication(format!(
                    "SASL server error: {}",
                    err
                )));
            }
            _ => {
                return Err(Error::Protocol(
                    "expected SaslContinue message during SASL authentication".into(),
                ));
            }
        };

        let server_first = String::from_utf8(server_first_data).map_err(|e| {
            Error::Authentication(format!("invalid UTF-8 in server first message: {}", e))
        })?;

        tracing::debug!("received SCRAM server first message");

        // Generate client final message
        let (client_final, scram_state) =
            scram
                .client_final(&server_first)
                .map_err(|e| Error::Authentication(format!("SCRAM error: {}", e)))?;

        // Send SaslResponse with client final message
        let msg = FrontendMessage::SaslResponse {
            data: client_final.into_bytes(),
        };
        self.send_message(&msg).await?;

        // Receive SaslFinal with server verification
        let server_final_msg = self.receive_message().await?;
        let server_final_data = match server_final_msg {
            BackendMessage::Authentication(AuthenticationMessage::SaslFinal { data }) => data,
            BackendMessage::ErrorResponse(err) => {
                return Err(Error::Authentication(format!(
                    "SASL server error: {}",
                    err
                )));
            }
            _ => {
                return Err(Error::Protocol(
                    "expected SaslFinal message during SASL authentication".into(),
                ));
            }
        };

        let server_final = String::from_utf8(server_final_data).map_err(|e| {
            Error::Authentication(format!("invalid UTF-8 in server final message: {}", e))
        })?;

        // Verify server signature
        scram
            .verify_server_final(&server_final, &scram_state)
            .map_err(|e| Error::Authentication(format!("SCRAM verification failed: {}", e)))?;

        tracing::debug!("SCRAM-SHA-256 authentication successful");
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
        max_memory: Option<usize>,
        soft_limit_warn_threshold: Option<f32>,
        soft_limit_fail_threshold: Option<f32>,
        enable_adaptive_chunking: bool,
        adaptive_min_chunk_size: Option<usize>,
        adaptive_max_chunk_size: Option<usize>,
    ) -> Result<crate::stream::JsonStream> {
        let startup_start = std::time::Instant::now();

        let _span = tracing::debug_span!(
            "streaming_query",
            query = %query,
            chunk_size = %chunk_size
        )
        .entered();

        use crate::json::validate_row_description;
        use crate::stream::{extract_json_bytes, parse_json, AdaptiveChunking, ChunkingStrategy, JsonStream};
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

        // Record startup timing
        let startup_duration = startup_start.elapsed().as_millis() as u64;
        let entity = extract_entity_from_query(query).unwrap_or_else(|| "unknown".to_string());
        crate::metrics::histograms::query_startup_duration(&entity, startup_duration);

        // Create channels
        let (result_tx, result_rx) = mpsc::channel::<Result<Value>>(chunk_size);
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);

        // Create stream instance first so we can clone its pause/resume signals
        let entity_for_metrics = extract_entity_from_query(query).unwrap_or_else(|| "unknown".to_string());
        let entity_for_stream = entity_for_metrics.clone();  // Clone for stream

        let mut stream = JsonStream::new(
            result_rx,
            cancel_tx,
            entity_for_stream,
            max_memory,
            soft_limit_warn_threshold,
            soft_limit_fail_threshold,
        );

        // Clone pause/resume signals for background task
        let state_lock = stream.clone_state();
        let pause_signal = stream.clone_pause_signal();
        let resume_signal = stream.clone_resume_signal();

        // Spawn background task to read rows
        let query_start = std::time::Instant::now();

        tokio::spawn(async move {
            let mut strategy = ChunkingStrategy::new(chunk_size);
            let mut chunk = strategy.new_chunk();
            let mut total_rows = 0u64;

            // Initialize adaptive chunking if enabled
            let mut adaptive = if enable_adaptive_chunking {
                let adp = AdaptiveChunking::new();
                // Note: AdaptiveChunking::new() uses hardcoded defaults (16-1024 bounds)
                // Custom min/max would require setter methods on AdaptiveChunking
                // For now, we accept the default bounds and store the parameters for future use
                let _ = (adaptive_min_chunk_size, adaptive_max_chunk_size);
                Some(adp)
            } else {
                None
            };
            let mut current_chunk_size = chunk_size;

            loop {
                // Check pause/resume state machine
                {
                    let current_state = state_lock.lock().await;
                    if *current_state == crate::stream::StreamState::Paused {
                        tracing::debug!("stream paused, waiting for resume");
                        drop(current_state); // Release lock before waiting
                        // Block until resume signal is sent
                        resume_signal.notified().await;
                        tracing::debug!("stream resumed");
                        // Update state back to Running
                        let mut state = state_lock.lock().await;
                        *state = crate::stream::StreamState::Running;
                    }
                }

                tokio::select! {
                    // Check for cancellation
                    _ = cancel_rx.recv() => {
                        tracing::debug!("query cancelled");
                        crate::metrics::counters::query_completed("cancelled", &entity_for_metrics);
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
                                                let chunk_start = std::time::Instant::now();
                                                let rows = chunk.into_rows();
                                                let chunk_size_rows = rows.len() as u64;

                                                for row_bytes in rows {
                                                    match parse_json(row_bytes) {
                                                        Ok(value) => {
                                                            total_rows += 1;
                                                            if result_tx.send(Ok(value)).await.is_err() {
                                                                crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                                break;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            crate::metrics::counters::json_parse_error(&entity_for_metrics);
                                                            let _ = result_tx.send(Err(e)).await;
                                                            crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                            break;
                                                        }
                                                    }
                                                }

                                                // Record chunk metrics
                                                let chunk_duration = chunk_start.elapsed().as_millis() as u64;
                                                crate::metrics::histograms::chunk_processing_duration(&entity_for_metrics, chunk_duration);
                                                crate::metrics::histograms::chunk_size(&entity_for_metrics, chunk_size_rows);

                                                // Adaptive chunking: observe occupancy and adjust if needed
                                                // We estimate occupancy based on the number of rows just sent.
                                                // In steady state, if we're sending full chunks, the channel is working normally.
                                                // If chunk_size_rows < current_chunk_size, we're reaching end of stream.
                                                if let Some(ref mut adaptive) = adaptive {
                                                    // Estimate current channel occupancy:
                                                    // - If we just sent a full chunk: likely high occupancy
                                                    // - If we're near end: likely low occupancy
                                                    // Use the rows we just sent as the occupancy estimate
                                                    let occupancy = chunk_size_rows as usize;
                                                    if let Some(new_size) = adaptive.observe(occupancy, current_chunk_size) {
                                                        let old_size = current_chunk_size;
                                                        current_chunk_size = new_size;

                                                        // Update strategy for next chunk
                                                        strategy = ChunkingStrategy::new(current_chunk_size);

                                                        // Record metric
                                                        crate::metrics::counters::adaptive_chunk_adjusted(
                                                            &entity_for_metrics,
                                                            old_size,
                                                            new_size,
                                                        );

                                                        tracing::debug!(
                                                            entity = &entity_for_metrics,
                                                            old_size = old_size,
                                                            new_size = new_size,
                                                            "adaptive chunk size adjusted"
                                                        );
                                                    }
                                                }

                                                chunk = strategy.new_chunk();
                                            }
                                        }
                                        Err(e) => {
                                            crate::metrics::counters::json_parse_error(&entity_for_metrics);
                                            let _ = result_tx.send(Err(e)).await;
                                            crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                            break;
                                        }
                                    }
                                }
                                BackendMessage::CommandComplete(_) => {
                                    // Send remaining chunk
                                    if !chunk.is_empty() {
                                        let chunk_start = std::time::Instant::now();
                                        let rows = chunk.into_rows();
                                        let chunk_size_rows = rows.len() as u64;

                                        for row_bytes in rows {
                                            match parse_json(row_bytes) {
                                                Ok(value) => {
                                                    total_rows += 1;
                                                    if result_tx.send(Ok(value)).await.is_err() {
                                                        crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                        break;
                                                    }
                                                }
                                                Err(e) => {
                                                    crate::metrics::counters::json_parse_error(&entity_for_metrics);
                                                    let _ = result_tx.send(Err(e)).await;
                                                    crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                    break;
                                                }
                                            }
                                        }

                                        // Record final chunk metrics
                                        let chunk_duration = chunk_start.elapsed().as_millis() as u64;
                                        crate::metrics::histograms::chunk_processing_duration(&entity_for_metrics, chunk_duration);
                                        crate::metrics::histograms::chunk_size(&entity_for_metrics, chunk_size_rows);
                                        chunk = strategy.new_chunk();
                                    }

                                    // Record query completion metrics
                                    let query_duration = query_start.elapsed().as_millis() as u64;
                                    crate::metrics::counters::rows_processed(&entity_for_metrics, total_rows, "ok");
                                    crate::metrics::histograms::query_total_duration(&entity_for_metrics, query_duration);
                                    crate::metrics::counters::query_completed("success", &entity_for_metrics);
                                }
                                BackendMessage::ReadyForQuery { .. } => {
                                    break;
                                }
                                BackendMessage::ErrorResponse(err) => {
                                    crate::metrics::counters::query_error(&entity_for_metrics, "server_error");
                                    crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                    let _ = result_tx.send(Err(Error::Sql(err.to_string()))).await;
                                    break;
                                }
                                _ => {
                                    crate::metrics::counters::query_error(&entity_for_metrics, "protocol_error");
                                    crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                    let _ = result_tx.send(Err(Error::Protocol(
                                        format!("unexpected message: {:?}", msg)
                                    ))).await;
                                    break;
                                }
                            },
                            Err(e) => {
                                crate::metrics::counters::query_error(&entity_for_metrics, "connection_error");
                                crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                let _ = result_tx.send(Err(e)).await;
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(stream)
    }
}

/// Extract entity name from query for metrics
/// Query format: SELECT data FROM v_{entity} ...
fn extract_entity_from_query(query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();
    if let Some(from_pos) = query_lower.find("from") {
        let after_from = &query_lower[from_pos + 4..].trim_start();
        if let Some(entity_start) = after_from.find('v').or_else(|| after_from.find('t')) {
            let potential_table = &after_from[entity_start..];
            // Extract table name: "v_entity" or "tv_entity"
            let end_pos = potential_table
                .find(' ')
                .or_else(|| potential_table.find(';'))
                .unwrap_or(potential_table.len());
            let table_name = &potential_table[..end_pos];
            // Extract entity from table name
            if let Some(entity_pos) = table_name.rfind('_') {
                return Some(table_name[entity_pos + 1..].to_string());
            }
        }
    }
    None
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
