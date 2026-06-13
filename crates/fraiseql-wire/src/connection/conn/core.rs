//! Core `Connection` type and implementation

use super::config::ConnectionConfig;
use super::helpers::extract_entity_from_query;
use crate::auth::ScramClient;
use crate::connection::state::ConnectionState;
use crate::connection::transport::Transport;
use crate::protocol::{
    decode_message, encode_message, AuthenticationMessage, BackendMessage, FrontendMessage,
};
use crate::{Result, WireError};
use bytes::{Buf, BytesMut};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::Instrument;

// Global counter for chunk metrics sampling (1 per 10 chunks)
// Used to reduce per-chunk metric recording overhead
static CHUNK_COUNT: AtomicU64 = AtomicU64::new(0);

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
    pub const fn state(&self) -> ConnectionState {
        self.state
    }

    /// Perform startup and authentication
    ///
    /// # Errors
    ///
    /// Returns [`WireError::InvalidState`] if the connection is not in the `Initial` state.
    /// Returns [`WireError::Authentication`] if authentication is rejected by the server.
    /// Returns [`WireError`] on any I/O or protocol error during the handshake.
    pub async fn startup(&mut self, config: &ConnectionConfig) -> Result<()> {
        async {
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
                params.push(("extra_float_digits".to_string(), digits.to_string()));
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
        .instrument(tracing::info_span!(
            "startup",
            user = %config.user,
            database = %config.database
        ))
        .await
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
                        // Don't break here! Must continue reading until ReadyForQuery
                    }
                    AuthenticationMessage::CleartextPassword => {
                        auth_mechanism = crate::metrics::labels::MECHANISM_CLEARTEXT;
                        crate::metrics::counters::auth_attempted(auth_mechanism);

                        let password = config
                            .password
                            .as_ref()
                            .ok_or_else(|| WireError::Authentication("password required".into()))?;
                        // SECURITY: Convert from Zeroizing wrapper while preserving password content
                        let pwd_msg = FrontendMessage::Password(password.as_str().to_string());
                        self.send_message(&pwd_msg).await?;
                    }
                    AuthenticationMessage::Md5Password { .. } => {
                        return Err(WireError::Authentication(
                            "MD5 authentication not supported. Use SCRAM-SHA-256 or cleartext password".into(),
                        ));
                    }
                    AuthenticationMessage::Sasl { mechanisms } => {
                        auth_mechanism = crate::metrics::labels::MECHANISM_SCRAM;
                        crate::metrics::counters::auth_attempted(auth_mechanism);
                        self.handle_sasl(&mechanisms, config).await?;
                    }
                    AuthenticationMessage::SaslContinue { .. } => {
                        return Err(WireError::Protocol(
                            "unexpected SaslContinue outside of SASL flow".into(),
                        ));
                    }
                    AuthenticationMessage::SaslFinal { .. } => {
                        return Err(WireError::Protocol(
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
                BackendMessage::ReadyForQuery { status: _ } => {
                    break;
                }
                BackendMessage::ErrorResponse(err) => {
                    crate::metrics::counters::auth_failed(auth_mechanism, "server_error");
                    return Err(WireError::Authentication(err.to_string()));
                }
                _ => {
                    return Err(WireError::Protocol(format!(
                        "unexpected message during auth: {:?}",
                        msg
                    )));
                }
            }
        }

        Ok(())
    }

    /// Handle SASL authentication (SCRAM-SHA-256)
    async fn handle_sasl(
        &mut self,
        mechanisms: &[String],
        config: &ConnectionConfig,
    ) -> Result<()> {
        // Check if server supports SCRAM-SHA-256
        if !mechanisms.contains(&"SCRAM-SHA-256".to_string()) {
            return Err(WireError::Authentication(format!(
                "server does not support SCRAM-SHA-256. Available: {}",
                mechanisms.join(", ")
            )));
        }

        // Get password
        let password = config.password.as_ref().ok_or_else(|| {
            WireError::Authentication("password required for SCRAM authentication".into())
        })?;

        // Create SCRAM client
        // SECURITY: Convert from Zeroizing wrapper while preserving password content
        let mut scram = ScramClient::new(config.user.clone(), password.as_str().to_string());
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
                return Err(WireError::Authentication(format!(
                    "SASL server error: {}",
                    err
                )));
            }
            _ => {
                return Err(WireError::Protocol(
                    "expected SaslContinue message during SASL authentication".into(),
                ));
            }
        };

        let server_first = String::from_utf8(server_first_data).map_err(|e| {
            WireError::Authentication(format!("invalid UTF-8 in server first message: {}", e))
        })?;

        tracing::debug!("received SCRAM server first message");

        // Generate client final message
        let (client_final, scram_state) = scram
            .client_final(&server_first)
            .map_err(|e| WireError::Authentication(format!("SCRAM error: {}", e)))?;

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
                return Err(WireError::Authentication(format!(
                    "SASL server error: {}",
                    err
                )));
            }
            _ => {
                return Err(WireError::Protocol(
                    "expected SaslFinal message during SASL authentication".into(),
                ));
            }
        };

        let server_final = String::from_utf8(server_final_data).map_err(|e| {
            WireError::Authentication(format!("invalid UTF-8 in server final message: {}", e))
        })?;

        // Verify server signature
        scram
            .verify_server_final(&server_final, &scram_state)
            .map_err(|e| WireError::Authentication(format!("SCRAM verification failed: {}", e)))?;

        tracing::debug!("SCRAM-SHA-256 authentication successful");
        Ok(())
    }

    /// Execute a simple query (returns all backend messages)
    ///
    /// # Errors
    ///
    /// Returns [`WireError::ConnectionBusy`] if the connection is not idle.
    /// Returns [`WireError::InvalidState`] if the state machine transition fails.
    /// Returns [`WireError`] on any I/O or protocol error during execution.
    pub async fn simple_query(&mut self, query: &str) -> Result<Vec<BackendMessage>> {
        if self.state != ConnectionState::Idle {
            return Err(WireError::ConnectionBusy(format!(
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
    ///
    /// Decode errors are classified by [`std::io::ErrorKind`]: only
    /// `UnexpectedEof` means "the frame is incomplete, read more bytes". Every
    /// other kind (`InvalidData` for a malformed/unknown message, `Unsupported`
    /// for a recognized-but-unimplemented one such as the COPY family) is fatal
    /// and surfaces as [`WireError::Protocol`]. The previous `if let Ok(..)`
    /// swallowed the kind and treated *every* decode error as "need more bytes",
    /// so a malformed or unrecognized message looped forever, buffering toward
    /// the size cap (audit H42).
    async fn receive_message(&mut self) -> Result<BackendMessage> {
        loop {
            // Try to decode a message from buffer (without cloning!)
            match decode_message(&mut self.read_buf) {
                Ok((msg, consumed)) => {
                    self.read_buf.advance(consumed);
                    return Ok(msg);
                }
                // Incomplete frame: fall through and read more bytes.
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
                // Malformed / unknown / unsupported / oversized: fatal.
                Err(e) => {
                    crate::metrics::counters::protocol_error("decode");
                    return Err(WireError::Protocol(format!(
                        "failed to decode backend message: {e}"
                    )));
                }
            }

            // Bound read-buffer growth: a single backend message may not exceed
            // MAX_MESSAGE_LEN. If we have buffered more than that without
            // decoding one, the peer is sending an oversized (or
            // never-terminating) message — fail instead of buffering toward
            // ~2 GiB (audit M-wire-msg-cap). An oversized *declared* length is
            // already rejected up front by `decode_message`; this is the
            // backstop for a stream that never frames a complete message.
            if self.read_buf.len() > crate::protocol::decode::MAX_MESSAGE_LEN {
                return Err(WireError::Protocol(format!(
                    "backend message exceeds maximum length of {} bytes",
                    crate::protocol::decode::MAX_MESSAGE_LEN
                )));
            }

            // Need more data
            let n = self.transport.read_buf(&mut self.read_buf).await?;
            if n == 0 {
                return Err(WireError::ConnectionClosed);
            }
        }
    }

    /// Close the connection
    ///
    /// # Errors
    ///
    /// Returns [`WireError::InvalidState`] if the state machine transition to `Closed` fails.
    /// Returns [`WireError`] if the transport shutdown fails.
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
    ///
    /// # Errors
    ///
    /// Returns `WireError::Io` if sending the query or reading the response fails.
    /// Returns `WireError::Database` if the server returns an error response.
    /// Returns `WireError::InvalidSchema` if the row description is not a single JSON column.
    #[allow(clippy::too_many_arguments)] // Reason: streaming query requires all chunking parameters; a config struct would add allocation overhead
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
        async {
            let startup_start = std::time::Instant::now();

            use crate::json::validate_row_description;
            use crate::stream::{extract_json_bytes, parse_json, AdaptiveChunking, ChunkingStrategy, JsonStream};
            use serde_json::Value;
            use tokio::sync::mpsc;

            if self.state != ConnectionState::Idle {
                return Err(WireError::ConnectionBusy(format!(
                    "connection in state: {}",
                    self.state
                )));
            }

            self.state.transition(ConnectionState::QueryInProgress)?;

            let query_msg = FrontendMessage::Query(query.to_string());
            self.send_message(&query_msg).await?;

            self.state.transition(ConnectionState::ReadingResults)?;

            // Read RowDescription, but handle other messages that may come first
            // (e.g., ParameterStatus, BackendKeyData, ErrorResponse, NoticeResponse)
            let row_desc;
            loop {
                let msg = self.receive_message().await?;

                match msg {
                    BackendMessage::ErrorResponse(err) => {
                        // Query failed - consume ReadyForQuery and return error
                        tracing::debug!("PostgreSQL error response: {}", err);
                        loop {
                            let msg = self.receive_message().await?;
                            if matches!(msg, BackendMessage::ReadyForQuery { .. }) {
                                break;
                            }
                        }
                        return Err(WireError::Sql(err.to_string()));
                    }
                    BackendMessage::BackendKeyData { process_id, secret_key: _ } => {
                        // This provides the key needed for cancel requests - store it and continue
                        tracing::debug!("PostgreSQL backend key data received: pid={}", process_id);
                        // Note: We would store this if we need to support cancellation
                        continue;
                    }
                    BackendMessage::ParameterStatus { .. } => {
                        // Parameter status changes are informational - skip them
                        tracing::debug!("PostgreSQL parameter status change received");
                        continue;
                    }
                    BackendMessage::NoticeResponse(notice) => {
                        // Notices are non-fatal warnings - skip them
                        tracing::debug!("PostgreSQL notice: {}", notice);
                        continue;
                    }
                    BackendMessage::RowDescription(_) => {
                        row_desc = msg;
                        break;
                    }
                    BackendMessage::ReadyForQuery { .. } => {
                        // Received ReadyForQuery without RowDescription
                        // This means the query didn't produce a result set
                        return Err(WireError::Protocol(
                            "no result set received from query - \
                             check that the entity name is correct and the table/view exists"
                                .into(),
                        ));
                    }
                    _ => {
                        return Err(WireError::Protocol(format!(
                            "unexpected message type in query response: {:?}",
                            msg
                        )));
                    }
                }
            }

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

            let stream = JsonStream::new(
                result_rx,
                cancel_tx,
                entity_for_stream,
                max_memory,
                soft_limit_warn_threshold,
                soft_limit_fail_threshold,
            );

            // Shared pause/resume handles for the background reader. These are
            // allocated eagerly by `JsonStream::new`, so the reader sees the same
            // instances the caller's `pause()`/`resume()` drive (audit H43 — the
            // reader previously captured `None` clones and never paused).
            let state_lock = stream.clone_state();
            let resume_signal = stream.clone_resume_signal();

            // Clone atomic state for fast state checks in background task
            let state_atomic = stream.clone_state_atomic();

            // Live auto-resume timeout (ms, 0 = none) — read fresh on each pause so
            // `set_pause_timeout` applies after the stream is handed back.
            let pause_timeout_ms = stream.clone_pause_timeout();

            // Spawn background task to read rows
            let query_start = std::time::Instant::now();

            tokio::spawn(async move {
                let strategy = ChunkingStrategy::new(chunk_size);
                let mut chunk = strategy.new_chunk();
                let mut total_rows = 0u64;

            // Initialize adaptive chunking if enabled
            let _adaptive = if enable_adaptive_chunking {
                let mut adp = AdaptiveChunking::new();

                // Apply custom bounds if provided
                if let Some(min) = adaptive_min_chunk_size {
                    if let Some(max) = adaptive_max_chunk_size {
                        adp = adp.with_bounds(min, max);
                    }
                }

                Some(adp)
            } else {
                None
            };
            let _current_chunk_size = chunk_size;

            loop {
                // Fast path: a single relaxed atomic load gates the (rare) pause
                // handling. STATE_PAUSED == 1.
                if state_atomic.load(std::sync::atomic::Ordering::Acquire) == 1 {
                    let is_paused =
                        { *state_lock.lock().await == crate::stream::StreamState::Paused };
                    if is_paused {
                        tracing::debug!("stream paused, waiting for resume");

                        // Park until resumed, the pause timeout expires, or the
                        // stream is cancelled/dropped. Waiting on `cancel_rx` too
                        // means a drop-while-paused tears the reader down cleanly
                        // instead of leaking a task blocked forever.
                        let timeout_ms = pause_timeout_ms.load(std::sync::atomic::Ordering::Relaxed);
                        let cancelled = if timeout_ms > 0 {
                            tokio::select! {
                                () = resume_signal.notified() => {
                                    tracing::debug!("stream resumed");
                                    false
                                }
                                _ = cancel_rx.recv() => true,
                                () = tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)) => {
                                    tracing::debug!("pause timeout expired, auto-resuming");
                                    crate::metrics::counters::stream_pause_timeout_expired(&entity_for_metrics);
                                    false
                                }
                            }
                        } else {
                            tokio::select! {
                                () = resume_signal.notified() => {
                                    tracing::debug!("stream resumed");
                                    false
                                }
                                _ = cancel_rx.recv() => true,
                            }
                        };

                        if cancelled {
                            tracing::debug!("query cancelled while paused");
                            crate::metrics::counters::query_completed("cancelled", &entity_for_metrics);
                            break;
                        }

                        // Back to Running (covers both explicit resume and the
                        // timeout auto-resume).
                        *state_lock.lock().await = crate::stream::StreamState::Running;
                        state_atomic.store(0, std::sync::atomic::Ordering::Release);
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

                                                // Batch JSON parsing and sending to reduce lock contention
                                                // Send 8 values per channel send instead of 1 (8x fewer locks)
                                                const BATCH_SIZE: usize = 8;
                                                let mut batch = Vec::with_capacity(BATCH_SIZE);
                                                let mut send_error = false;

                                                for row_bytes in rows {
                                                    match parse_json(row_bytes) {
                                                        Ok(value) => {
                                                            total_rows += 1;
                                                            batch.push(Ok(value));

                                                            // Send batch when full
                                                            if batch.len() == BATCH_SIZE {
                                                                for item in batch.drain(..) {
                                                                    if result_tx.send(item).await.is_err() {
                                                                        crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                                        send_error = true;
                                                                        break;
                                                                    }
                                                                }
                                                                if send_error {
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                        Err(e) => {
                                                            crate::metrics::counters::json_parse_error(&entity_for_metrics);
                                                            let _ = result_tx.send(Err(e)).await;
                                                            crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                            send_error = true;
                                                            break;
                                                        }
                                                    }
                                                }

                                                // Send remaining batch items
                                                if !send_error {
                                                    for item in batch {
                                                        if result_tx.send(item).await.is_err() {
                                                            crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                            break;
                                                        }
                                                    }
                                                }

                                                // Record chunk metrics (sampled, not per-chunk)
                                                let chunk_duration = chunk_start.elapsed().as_millis() as u64;

                                                // Only record metrics every 10 chunks to reduce overhead
                                                let chunk_idx = CHUNK_COUNT.fetch_add(1, Ordering::Relaxed);
                                                if chunk_idx.is_multiple_of(10) {
                                                    crate::metrics::histograms::chunk_processing_duration(&entity_for_metrics, chunk_duration);
                                                    crate::metrics::histograms::chunk_size(&entity_for_metrics, chunk_size_rows);
                                                }

                                                // Adaptive chunking: disabled by default for better performance
                                                // Enable only if explicitly requested via enable_adaptive_chunking parameter
                                                // Note: adaptive adjustment adds ~0.5-1% overhead per chunk
                                                // For fixed chunk sizes (default), skip this entirely

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

                                        // Batch JSON parsing and sending to reduce lock contention
                                        const BATCH_SIZE: usize = 8;
                                        let mut batch = Vec::with_capacity(BATCH_SIZE);
                                        let mut send_error = false;

                                        for row_bytes in rows {
                                            match parse_json(row_bytes) {
                                                Ok(value) => {
                                                    total_rows += 1;
                                                    batch.push(Ok(value));

                                                    // Send batch when full
                                                    if batch.len() == BATCH_SIZE {
                                                        for item in batch.drain(..) {
                                                            if result_tx.send(item).await.is_err() {
                                                                crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                                send_error = true;
                                                                break;
                                                            }
                                                        }
                                                        if send_error {
                                                            break;
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    crate::metrics::counters::json_parse_error(&entity_for_metrics);
                                                    let _ = result_tx.send(Err(e)).await;
                                                    crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                    send_error = true;
                                                    break;
                                                }
                                            }
                                        }

                                        // Send remaining batch items
                                        if !send_error {
                                            for item in batch {
                                                if result_tx.send(item).await.is_err() {
                                                    crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                                    break;
                                                }
                                            }
                                        }

                                        // Record final chunk metrics (sampled)
                                        let chunk_duration = chunk_start.elapsed().as_millis() as u64;
                                        let chunk_idx = CHUNK_COUNT.fetch_add(1, Ordering::Relaxed);
                                        if chunk_idx.is_multiple_of(10) {
                                            crate::metrics::histograms::chunk_processing_duration(&entity_for_metrics, chunk_duration);
                                            crate::metrics::histograms::chunk_size(&entity_for_metrics, chunk_size_rows);
                                        }
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
                                    let _ = result_tx.send(Err(WireError::Sql(err.to_string()))).await;
                                    break;
                                }
                                _ => {
                                    crate::metrics::counters::query_error(&entity_for_metrics, "protocol_error");
                                    crate::metrics::counters::query_completed("error", &entity_for_metrics);
                                    let _ = result_tx.send(Err(WireError::Protocol(
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
        .instrument(tracing::debug_span!(
            "streaming_query",
            query = %query,
            chunk_size = %chunk_size
        ))
        .await
    }
}
