//! Connection manager for tracking active realtime `WebSocket` connections.
//!
//! Uses `DashMap` for lock-free concurrent access to connection state.

use dashmap::DashMap;

/// Unique identifier for a `WebSocket` connection.
pub type ConnectionId = String;

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
pub struct ConnectionManager {
    /// Active connections indexed by connection ID.
    connections: DashMap<ConnectionId, ConnectionState>,
}

impl ConnectionManager {
    /// Create a new empty connection manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Register a new connection.
    pub fn insert(&self, state: ConnectionState) {
        self.connections.insert(state.connection_id.clone(), state);
    }

    /// Remove a connection by ID.
    pub fn remove(&self, connection_id: &str) {
        self.connections.remove(connection_id);
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
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
