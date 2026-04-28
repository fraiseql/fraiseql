//! Realtime `WebSocket` server and configuration.
//!
//! `RealtimeServer` manages `WebSocket` connections, authenticates clients,
//! handles heartbeats and idle timeouts, and enforces connection limits.

use std::{sync::Arc, time::Duration};

use axum::{
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tracing::{debug, info, warn};

use super::{
    connections::{ConnectionManager, ConnectionState},
    protocol::{ClientMessage, ServerMessage},
};

/// Configuration for the realtime `WebSocket` server.
#[derive(Debug, Clone)]
pub struct RealtimeConfig {
    /// Maximum concurrent connections per security context hash (default: 10).
    pub max_connections_per_context: usize,
    /// Interval between server heartbeat pings (default: 30s).
    pub heartbeat_interval: Duration,
    /// Disconnect after this duration of inactivity (default: 60s).
    pub idle_timeout: Duration,
    /// Maximum subscriptions per entity across all connections (default: 10,000).
    pub max_subscriptions_per_entity: usize,
    /// Bounded channel capacity for event delivery (default: 10,000).
    pub event_channel_capacity: usize,
    /// How often to re-validate JWT tokens (default: same as `heartbeat_interval`).
    /// Per D14: check JWT on each heartbeat.
    pub token_revalidation_interval: Duration,
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self {
            max_connections_per_context: 10,
            heartbeat_interval: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            max_subscriptions_per_entity: 10_000,
            event_channel_capacity: 10_000,
            token_revalidation_interval: Duration::from_secs(30),
        }
    }
}

/// Token validator trait for authenticating `WebSocket` connections.
///
/// Implementations validate bearer tokens (JWT or otherwise) and return
/// the validated token information including user identity and expiration.
pub trait TokenValidator: Send + Sync + 'static {
    /// Validate a bearer token and return the token info.
    ///
    /// # Errors
    ///
    /// Returns an error string if the token is invalid, expired, or
    /// cannot be validated.
    fn validate(
        &self,
        token: &str,
    ) -> impl std::future::Future<Output = Result<TokenInfo, String>> + Send;
}

/// Information extracted from a validated token.
#[derive(Debug, Clone)]
pub struct TokenInfo {
    /// User identifier (from JWT `sub` claim).
    pub user_id: String,
    /// Security context hash for connection grouping.
    pub context_hash: u64,
    /// When the token expires (Unix timestamp in seconds).
    pub expires_at: i64,
}

/// Shared state for the realtime `WebSocket` handler.
#[derive(Clone)]
pub struct RealtimeState<V: TokenValidator> {
    /// The realtime server instance.
    pub server: Arc<RealtimeServer>,
    /// Token validator for authenticating connections.
    pub validator: Arc<V>,
}

/// The realtime `WebSocket` server.
pub struct RealtimeServer {
    /// Active connection manager.
    pub(crate) connections: Arc<ConnectionManager>,
    /// Server configuration.
    pub(crate) config: RealtimeConfig,
}

impl RealtimeServer {
    /// Create a new realtime server with the given configuration.
    #[must_use]
    pub fn new(config: RealtimeConfig) -> Self {
        Self {
            connections: Arc::new(ConnectionManager::new()),
            config,
        }
    }

    /// Returns the number of active connections.
    #[must_use]
    pub fn active_connections(&self) -> usize {
        self.connections.count()
    }
}

/// Query parameters for the `WebSocket` upgrade request.
#[derive(Debug, Deserialize)]
pub struct WsQueryParams {
    /// Bearer token passed as query parameter.
    pub token: Option<String>,
}

/// `WebSocket` upgrade handler for `/realtime/v1`.
///
/// Authenticates via `?token=` query parameter or `Authorization: Bearer` header
/// before upgrading the connection.
///
/// # Errors
///
/// Returns HTTP 401 if authentication fails (missing, invalid, or expired token).
/// Returns HTTP 429 if the connection limit for this security context is reached.
pub async fn ws_handler<V: TokenValidator>(
    headers: axum::http::HeaderMap,
    Query(params): Query<WsQueryParams>,
    ws: WebSocketUpgrade,
    State(state): State<RealtimeState<V>>,
) -> impl IntoResponse {
    // Extract token from query param or Authorization header
    let token = params.token.or_else(|| {
        headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_owned)
    });

    let Some(token) = token else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    // Validate token before upgrade
    let token_info = match state.validator.validate(&token).await {
        Ok(info) => info,
        Err(reason) => {
            warn!(reason = %reason, "Realtime WebSocket auth failed");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    // Check connection limit for this security context
    let context_hash = token_info.context_hash;
    let current = state.server.connections.count_by_context(context_hash);
    if current >= state.server.config.max_connections_per_context {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    let server = state.server.clone();
    ws.on_upgrade(move |socket| handle_realtime_connection(socket, server, token_info))
        .into_response()
}

/// Handle an authenticated realtime `WebSocket` connection.
#[allow(clippy::cognitive_complexity)] // Reason: WebSocket event loop with heartbeat, idle timeout, and token expiry checks
async fn handle_realtime_connection(
    socket: WebSocket,
    server: Arc<RealtimeServer>,
    token_info: TokenInfo,
) {
    let connection_id = uuid::Uuid::new_v4().to_string();
    let config = &server.config;

    // Register connection
    let conn_state = ConnectionState::new(
        connection_id.clone(),
        token_info.user_id.clone(),
        token_info.context_hash,
        token_info.expires_at,
    );
    server.connections.insert(conn_state);

    info!(
        connection_id = %connection_id,
        user_id = %token_info.user_id,
        "Realtime WebSocket connected"
    );

    let (mut sender, mut receiver) = socket.split();

    // Send connected message
    let connected_msg = ServerMessage::Connected {
        connection_id: connection_id.clone(),
    };
    if let Ok(json) = connected_msg.to_json() {
        if sender.send(Message::Text(json.into())).await.is_err() {
            server.connections.remove(&connection_id);
            return;
        }
    }

    let mut heartbeat_interval = tokio::time::interval(config.heartbeat_interval);
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Skip the immediate first tick
    heartbeat_interval.tick().await;

    let mut idle_deadline = tokio::time::Instant::now() + config.idle_timeout;

    loop {
        tokio::select! {
            // Heartbeat tick
            _ = heartbeat_interval.tick() => {
                // D14: Check token expiry on each heartbeat
                let now_ts = chrono::Utc::now().timestamp();
                if now_ts >= token_info.expires_at {
                    debug!(connection_id = %connection_id, "Token expired, closing connection");
                    if let Ok(json) = ServerMessage::TokenExpired.to_json() {
                        let _ = sender.send(Message::Text(json.into())).await;
                    }
                    let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: 4401,
                        reason: "token expired".into(),
                    }))).await;
                    break;
                }

                // Send ping
                if let Ok(json) = ServerMessage::Ping.to_json() {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }

            // Idle timeout
            () = tokio::time::sleep_until(idle_deadline) => {
                debug!(connection_id = %connection_id, "Idle timeout, closing connection");
                let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 1000,
                    reason: "idle timeout".into(),
                }))).await;
                break;
            }

            // Client messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Reset idle timer on any message
                        idle_deadline = tokio::time::Instant::now() + config.idle_timeout;

                        if matches!(serde_json::from_str(&text), Ok(ClientMessage::Pong)) {
                            debug!(connection_id = %connection_id, "Received pong");
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        debug!(connection_id = %connection_id, "Client sent close");
                        break;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // WebSocket-level pong, reset idle timer
                        idle_deadline = tokio::time::Instant::now() + config.idle_timeout;
                    }
                    Some(Err(e)) => {
                        warn!(connection_id = %connection_id, error = %e, "WebSocket error");
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
        }
    }

    // Cleanup
    server.connections.remove(&connection_id);
    info!(connection_id = %connection_id, "Realtime WebSocket disconnected");
}
