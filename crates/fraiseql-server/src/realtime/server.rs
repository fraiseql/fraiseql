//! Realtime `WebSocket` server and configuration.
//!
//! `RealtimeServer` manages `WebSocket` connections, authenticates clients,
//! handles heartbeats and idle timeouts, enforces connection limits, and
//! processes subscription requests for entity change events.

use std::{
    collections::HashSet,
    sync::Arc,
    time::Duration,
};

use futures::future::BoxFuture;

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
    subscriptions::{EventKind, SubscriptionDetails, SubscriptionManager, parse_filter},
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
    /// Bounded channel capacity for the observer-to-pipeline event channel (default: 10,000).
    pub event_channel_capacity: usize,
    /// How often to re-validate JWT tokens (default: same as `heartbeat_interval`).
    /// Per D14: check JWT on each heartbeat.
    pub token_revalidation_interval: Duration,
    /// Consecutive per-connection delivery failures before kicking the client
    /// with close code 4002 "slow consumer" (default: 50).
    pub max_consecutive_drops: usize,
    /// Capacity of the per-connection event channel (default: 256).
    pub connection_event_capacity: usize,
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
            max_consecutive_drops: 50,
            connection_event_capacity: 256,
        }
    }
}

/// Token validator trait for authenticating `WebSocket` connections.
///
/// Implementations validate bearer tokens (JWT or otherwise) and return
/// the validated token information including user identity and expiration.
///
/// The trait is object-safe (returns `BoxFuture`) so it can be stored as
/// `Arc<dyn TokenValidator>` and mounted in `build_base_router` without
/// adding a type parameter to `Server<A>`.
///
/// # Production implementation
///
/// `JwtTokenValidator` (wired in Cycle 6) wraps the existing `OidcValidator`
/// from `fraiseql-core`, reusing the same JWT validation path as the GraphQL
/// endpoint's OIDC middleware.
pub trait TokenValidator: Send + Sync + 'static {
    /// Validate a bearer token and return the token info.
    ///
    /// # Errors
    ///
    /// Returns an error string if the token is invalid, expired, or
    /// cannot be validated.
    fn validate<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, Result<TokenInfo, String>>;
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
pub struct RealtimeState {
    /// The realtime server instance.
    pub server: Arc<RealtimeServer>,
    /// Token validator for authenticating connections.
    pub validator: Arc<dyn TokenValidator>,
}

/// The realtime `WebSocket` server.
pub struct RealtimeServer {
    /// Active connection manager.
    pub(crate) connections: Arc<ConnectionManager>,
    /// Subscription manager for entity change subscriptions.
    pub(crate) subscriptions: Arc<SubscriptionManager>,
    /// Set of entity names that accept realtime subscriptions.
    pub(crate) known_entities: HashSet<String>,
    /// Server configuration.
    pub(crate) config: RealtimeConfig,
}

impl RealtimeServer {
    /// Create a new realtime server with the given configuration.
    #[must_use]
    pub fn new(config: RealtimeConfig) -> Self {
        let max_subs = config.max_subscriptions_per_entity;
        let connections = Arc::new(ConnectionManager::new(
            config.max_consecutive_drops,
            config.connection_event_capacity,
        ));
        Self {
            connections,
            subscriptions: Arc::new(SubscriptionManager::new(max_subs)),
            known_entities: HashSet::new(),
            config,
        }
    }

    /// Create a new realtime server with known entities for subscription validation.
    #[must_use]
    pub fn with_entities(config: RealtimeConfig, entities: HashSet<String>) -> Self {
        let max_subs = config.max_subscriptions_per_entity;
        let connections = Arc::new(ConnectionManager::new(
            config.max_consecutive_drops,
            config.connection_event_capacity,
        ));
        Self {
            connections,
            subscriptions: Arc::new(SubscriptionManager::new(max_subs)),
            known_entities: entities,
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
pub async fn ws_handler(
    headers: axum::http::HeaderMap,
    Query(params): Query<WsQueryParams>,
    ws: WebSocketUpgrade,
    State(state): State<RealtimeState>,
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
#[allow(clippy::cognitive_complexity)] // Reason: WebSocket event loop with heartbeat, idle timeout, token expiry, and subscription handling
async fn handle_realtime_connection(
    socket: WebSocket,
    server: Arc<RealtimeServer>,
    token_info: TokenInfo,
) {
    let context_hash = token_info.context_hash;
    let connection_id = uuid::Uuid::new_v4().to_string();
    let config = &server.config;

    // Register connection. Returns:
    // - event_rx: receives change event JSON from the delivery pipeline
    // - control_rx: fires once if the delivery pipeline detects a slow consumer
    let conn_state = ConnectionState::new(
        connection_id.clone(),
        token_info.user_id.clone(),
        token_info.context_hash,
        token_info.expires_at,
    );
    let (mut event_rx, control_rx) = server.connections.insert(conn_state);
    // Pin the close-signal receiver so it can be polled across select! loop iterations.
    tokio::pin!(control_rx);

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

            // Events from delivery pipeline
            Some(event_json) = event_rx.recv() => {
                if sender.send(Message::Text(event_json.into())).await.is_err() {
                    break;
                }
            }

            // Slow-consumer close signal from the delivery pipeline
            signal = control_rx.as_mut() => {
                if let Ok(sig) = signal {
                    debug!(
                        connection_id = %connection_id,
                        code = sig.code,
                        reason = %sig.reason,
                        "Slow consumer: closing connection"
                    );
                    let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: sig.code,
                        reason: sig.reason.into(),
                    }))).await;
                }
                break;
            }

            // Client messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Reset idle timer on any message
                        idle_deadline = tokio::time::Instant::now() + config.idle_timeout;

                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Pong) => {
                                debug!(connection_id = %connection_id, "Received pong");
                            }
                            Ok(ClientMessage::Subscribe { entity, event, filter }) => {
                                let reply = handle_subscribe(
                                    &server,
                                    &connection_id,
                                    context_hash,
                                    &entity,
                                    &event,
                                    filter.as_deref(),
                                );
                                if let Ok(json) = reply.to_json() {
                                    if sender.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Ok(ClientMessage::Unsubscribe { entity }) => {
                                server.subscriptions.unsubscribe(&connection_id, &entity);
                                let reply = ServerMessage::Unsubscribed { entity };
                                if let Ok(json) = reply.to_json() {
                                    if sender.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(_) => {
                                // Unknown message, ignore
                            }
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

    // Cleanup: remove all subscriptions and connection state
    server.subscriptions.unsubscribe_all(&connection_id);
    server.connections.remove(&connection_id);
    info!(connection_id = %connection_id, "Realtime WebSocket disconnected");
}

/// Handle a subscribe request from a client.
fn handle_subscribe(
    server: &RealtimeServer,
    connection_id: &str,
    context_hash: u64,
    entity: &str,
    event: &str,
    filter: Option<&str>,
) -> ServerMessage {
    // Validate entity exists in schema
    if !server.known_entities.is_empty() && !server.known_entities.contains(entity) {
        return ServerMessage::Error {
            message: format!("unknown entity: {entity}"),
        };
    }

    // Parse event filter
    let event_filter = if event == "*" {
        None
    } else {
        match EventKind::parse(event) {
            Ok(kind) => Some(kind),
            Err(e) => return ServerMessage::Error { message: e },
        }
    };

    // Parse field filters
    let field_filters = if let Some(f) = filter {
        match parse_filter(f) {
            Ok(filters) => filters,
            Err(e) => return ServerMessage::Error { message: e },
        }
    } else {
        Vec::new()
    };

    let details = SubscriptionDetails {
        event_filter,
        field_filters,
        security_context_hash: context_hash,
    };

    match server.subscriptions.subscribe(connection_id, entity, details) {
        Ok(_) => ServerMessage::Subscribed {
            entity: entity.to_owned(),
        },
        Err(e) => ServerMessage::Error { message: e },
    }
}
