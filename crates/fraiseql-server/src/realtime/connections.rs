//! Connection manager for tracking active realtime `WebSocket` connections.
//!
//! Uses `DashMap` for lock-free concurrent access to connection state.
//! Each connection has an associated event sender channel for pushing
//! change events from the delivery pipeline.

use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};

/// Unique identifier for a `WebSocket` connection.
pub type ConnectionId = String;

/// Signal to gracefully close a connection with a specific `WebSocket` close code.
#[derive(Debug, Clone)]
pub struct CloseSignal {
    /// `WebSocket` close code (e.g., 4002 for "slow consumer").
    pub code: u16,
    /// Human-readable close reason.
    pub reason: String,
}

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
/// Each connection has an event sender for delivering change events and a
/// oneshot close-signal channel for graceful server-initiated disconnection
/// (e.g., slow consumer policy).
pub struct ConnectionManager {
    /// Active connections indexed by connection ID.
    connections: DashMap<ConnectionId, ConnectionState>,
    /// Per-connection event senders (bounded channel for change events).
    event_senders: DashMap<ConnectionId, mpsc::Sender<String>>,
    /// Per-connection oneshot senders for server-initiated close signals.
    close_senders: DashMap<ConnectionId, oneshot::Sender<CloseSignal>>,
    /// Per-connection consecutive drop counters for slow-consumer detection.
    drop_counts: DashMap<ConnectionId, AtomicUsize>,
    /// Maximum consecutive delivery failures before a connection is kicked.
    max_consecutive_drops: usize,
    /// Capacity of per-connection event channels.
    connection_event_capacity: usize,
}

impl ConnectionManager {
    /// Create a new empty connection manager.
    #[must_use]
    pub fn new(max_consecutive_drops: usize, connection_event_capacity: usize) -> Self {
        Self {
            connections: DashMap::new(),
            event_senders: DashMap::new(),
            close_senders: DashMap::new(),
            drop_counts: DashMap::new(),
            max_consecutive_drops,
            connection_event_capacity,
        }
    }

    /// Register a new connection and return a receiver for events and a close-signal receiver.
    ///
    /// The event receiver should be polled by the connection handler to forward
    /// change events to the `WebSocket`. The close-signal receiver fires when the
    /// delivery pipeline detects a slow consumer.
    pub fn insert(
        &self,
        state: ConnectionState,
    ) -> (mpsc::Receiver<String>, oneshot::Receiver<CloseSignal>) {
        let (event_tx, event_rx) = mpsc::channel(self.connection_event_capacity);
        let (close_tx, close_rx) = oneshot::channel();
        self.event_senders
            .insert(state.connection_id.clone(), event_tx);
        self.close_senders
            .insert(state.connection_id.clone(), close_tx);
        self.drop_counts
            .insert(state.connection_id.clone(), AtomicUsize::new(0));
        self.connections.insert(state.connection_id.clone(), state);
        (event_rx, close_rx)
    }

    /// Remove a connection by ID, cleaning up all associated state.
    pub fn remove(&self, connection_id: &str) {
        self.connections.remove(connection_id);
        self.event_senders.remove(connection_id);
        self.close_senders.remove(connection_id);
        self.drop_counts.remove(connection_id);
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
    /// On success, resets the per-connection drop counter.
    /// On failure (channel full), increments the drop counter. If the counter
    /// reaches `max_consecutive_drops`, a close signal with code 4002 ("slow
    /// consumer") is sent to the connection handler.
    ///
    /// Returns `true` if the event was sent, `false` otherwise.
    pub fn send_event(&self, connection_id: &str, json: String) -> bool {
        let sent = self
            .event_senders
            .get(connection_id)
            .is_some_and(|sender| sender.try_send(json).is_ok());

        if sent {
            if let Some(counter) = self.drop_counts.get(connection_id) {
                counter.store(0, Ordering::Relaxed);
            }
        } else if let Some(counter) = self.drop_counts.get(connection_id) {
            let new_count = counter.fetch_add(1, Ordering::Relaxed) + 1;
            if new_count >= self.max_consecutive_drops {
                // Slow consumer: signal the connection handler to close with 4002.
                if let Some((_, close_tx)) = self.close_senders.remove(connection_id) {
                    let _ = close_tx.send(CloseSignal {
                        code: 4002,
                        reason: "slow consumer".to_owned(),
                    });
                }
            }
        }

        sent
    }

    /// Return the current consecutive drop count for a connection (for testing).
    #[must_use]
    pub fn drop_count(&self, connection_id: &str) -> usize {
        self.drop_counts
            .get(connection_id)
            .map_or(0, |c| c.load(Ordering::Relaxed))
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new(50, 256)
    }
}
