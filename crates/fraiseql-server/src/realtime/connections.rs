//! Connection manager for tracking active realtime `WebSocket` connections.
//!
//! Uses `DashMap` for lock-free concurrent access to connection state.
//! Each connection has an associated event sender channel for pushing
//! change events from the delivery pipeline.

use dashmap::DashMap;
use tokio::sync::mpsc;

/// Unique identifier for a `WebSocket` connection.
pub type ConnectionId = String;

/// Capacity of the per-connection event channel.
const CONNECTION_EVENT_CHANNEL_CAPACITY: usize = 256;

/// State for a single active `WebSocket` connection.
#[derive(Debug, Clone)]
pub struct ConnectionState {
    /// Unique connection identifier (UUID v4).
    pub connection_id: ConnectionId,
    /// User identifier (from JWT `sub` claim).
    pub user_id: String,
    /// Security context hash for grouping connections with identical RLS context.
    pub context_hash: u64,
    /// Token expiration (Unix timestamp in seconds).
    pub expires_at: i64,
}

impl ConnectionState {
    /// Create a new connection state.
    #[must_use]
    pub const fn new(
        connection_id: ConnectionId,
        user_id: String,
        context_hash: u64,
        expires_at: i64,
    ) -> Self {
        Self {
            connection_id,
            user_id,
            context_hash,
            expires_at,
        }
    }
}

/// Thread-safe manager for active `WebSocket` connections.
///
/// Uses `DashMap` for lock-free concurrent reads and writes.
/// Each connection has an event sender for delivering change events.
pub struct ConnectionManager {
    /// Active connections indexed by connection ID.
    connections: DashMap<ConnectionId, ConnectionState>,
    /// Event senders indexed by connection ID (for pushing events to connections).
    event_senders: DashMap<ConnectionId, mpsc::Sender<String>>,
}

impl ConnectionManager {
    /// Create a new empty connection manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            event_senders: DashMap::new(),
        }
    }

    /// Register a new connection and return a receiver for events.
    ///
    /// The returned receiver should be polled by the connection handler
    /// to forward events to the `WebSocket`.
    pub fn insert(&self, state: ConnectionState) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(CONNECTION_EVENT_CHANNEL_CAPACITY);
        self.event_senders
            .insert(state.connection_id.clone(), tx);
        self.connections.insert(state.connection_id.clone(), state);
        rx
    }

    /// Remove a connection by ID.
    pub fn remove(&self, connection_id: &str) {
        self.connections.remove(connection_id);
        self.event_senders.remove(connection_id);
    }

    /// Total number of active connections.
    #[must_use]
    pub fn count(&self) -> usize {
        self.connections.len()
    }

    /// Number of connections for a specific security context hash.
    #[must_use]
    pub fn count_by_context(&self, context_hash: u64) -> usize {
        self.connections
            .iter()
            .filter(|entry| entry.value().context_hash == context_hash)
            .count()
    }

    /// Send a serialized event to a connection's event channel.
    ///
    /// Returns `true` if the event was sent, `false` if the channel is full
    /// or the connection no longer exists.
    pub fn send_event(&self, connection_id: &str, json: String) -> bool {
        self.event_senders
            .get(connection_id)
            .is_some_and(|sender| sender.try_send(json).is_ok())
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
