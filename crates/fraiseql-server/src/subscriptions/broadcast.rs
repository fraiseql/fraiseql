//! Ephemeral broadcast channels for realtime pub/sub.
//!
//! Provides in-memory named channels that clients can publish to via REST
//! (`POST /realtime/v1/broadcast`) and subscribe to via `WebSocket`.
//! No database persistence — messages are lost on server restart.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tokio::sync::{RwLock, broadcast};
use tracing::debug;

/// Configuration for the broadcast subsystem.
#[derive(Debug, Clone)]
pub struct BroadcastConfig {
    /// Per-channel buffer capacity (number of messages retained for slow subscribers).
    pub channel_capacity: usize,

    /// Maximum number of named channels that can exist simultaneously.
    pub max_channels: usize,

    /// Maximum message payload size in bytes.
    pub max_message_bytes: usize,
}

impl BroadcastConfig {
    /// Create config with production defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            channel_capacity: 128,
            max_channels: 1_000,
            max_message_bytes: 65_536,
        }
    }
}

impl Default for BroadcastConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for the broadcast subsystem.
#[derive(Debug, Clone)]
pub struct BroadcastStats {
    /// Total messages published across all channels.
    pub messages_published: u64,

    /// Number of active channels.
    pub active_channels: usize,

    /// Total active receivers across all channels.
    pub active_receivers: usize,
}

/// A single named broadcast channel.
#[derive(Debug)]
struct BroadcastChannel {
    sender: broadcast::Sender<BroadcastMessage>,
    created_at: Instant,
}

/// A message published to a broadcast channel.
#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    /// The channel this message was published to.
    pub channel: String,

    /// The event name (e.g., `cursor_move`, `typing`).
    pub event: String,

    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
}

/// Manages named broadcast channels.
///
/// Thread-safe via interior mutability (`RwLock` for channel map, atomics for counters).
#[derive(Debug)]
pub struct BroadcastManager {
    channels: RwLock<HashMap<String, BroadcastChannel>>,
    config: BroadcastConfig,
    messages_published: AtomicU64,
}

impl BroadcastManager {
    /// Create a new broadcast manager.
    #[must_use]
    pub fn new(config: BroadcastConfig) -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
            config,
            messages_published: AtomicU64::new(0),
        }
    }

    /// Publish a message to a named channel.
    ///
    /// Creates the channel if it doesn't exist. Returns the number of receivers
    /// that were notified (0 if nobody is listening).
    ///
    /// # Errors
    ///
    /// Returns error if the channel limit is exceeded or the payload is too large.
    pub async fn publish(
        &self,
        channel: &str,
        event: String,
        payload: serde_json::Value,
    ) -> Result<usize, BroadcastError> {
        // Validate payload size
        let payload_str = serde_json::to_string(&payload)
            .map_err(|e| BroadcastError::InvalidPayload(e.to_string()))?;
        if payload_str.len() > self.config.max_message_bytes {
            return Err(BroadcastError::PayloadTooLarge {
                size: payload_str.len(),
                max: self.config.max_message_bytes,
            });
        }

        let message = BroadcastMessage {
            channel: channel.to_string(),
            event,
            payload,
        };

        // Try to send on existing channel first (read lock — fast path)
        {
            let channels = self.channels.read().await;
            if let Some(ch) = channels.get(channel) {
                let receivers = ch.sender.send(message).unwrap_or(0);
                self.messages_published.fetch_add(1, Ordering::Relaxed);
                debug!(channel, receivers, "broadcast message sent (existing channel)");
                return Ok(receivers);
            }
        }

        // Channel doesn't exist — create it (write lock)
        let mut channels = self.channels.write().await;

        // Double-check after acquiring write lock
        if let Some(ch) = channels.get(channel) {
            let receivers = ch.sender.send(message).unwrap_or(0);
            self.messages_published.fetch_add(1, Ordering::Relaxed);
            return Ok(receivers);
        }

        // Check channel limit
        if channels.len() >= self.config.max_channels {
            return Err(BroadcastError::TooManyChannels {
                max: self.config.max_channels,
            });
        }

        let (sender, _) = broadcast::channel(self.config.channel_capacity);
        let receivers = sender.send(message).unwrap_or(0);
        channels.insert(
            channel.to_string(),
            BroadcastChannel {
                sender,
                created_at: Instant::now(),
            },
        );
        self.messages_published.fetch_add(1, Ordering::Relaxed);
        debug!(channel, "broadcast channel created");

        Ok(receivers)
    }

    /// Subscribe to a named channel. Creates the channel if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns error if the channel limit is exceeded.
    pub async fn subscribe(
        &self,
        channel: &str,
    ) -> Result<broadcast::Receiver<BroadcastMessage>, BroadcastError> {
        // Fast path: read lock
        {
            let channels = self.channels.read().await;
            if let Some(ch) = channels.get(channel) {
                return Ok(ch.sender.subscribe());
            }
        }

        // Create channel
        let mut channels = self.channels.write().await;

        // Double-check
        if let Some(ch) = channels.get(channel) {
            return Ok(ch.sender.subscribe());
        }

        if channels.len() >= self.config.max_channels {
            return Err(BroadcastError::TooManyChannels {
                max: self.config.max_channels,
            });
        }

        let (sender, receiver) = broadcast::channel(self.config.channel_capacity);
        channels.insert(
            channel.to_string(),
            BroadcastChannel {
                sender,
                created_at: Instant::now(),
            },
        );
        debug!(channel, "broadcast channel created for subscriber");

        Ok(receiver)
    }

    /// Remove channels with no active subscribers to prevent memory leaks.
    pub async fn gc_empty_channels(&self) -> usize {
        let mut channels = self.channels.write().await;
        let before = channels.len();
        channels.retain(|name, ch| {
            let has_receivers = ch.sender.receiver_count() > 0;
            if !has_receivers {
                debug!(channel = %name, age_secs = ch.created_at.elapsed().as_secs(), "gc: removing empty broadcast channel");
            }
            has_receivers
        });
        before - channels.len()
    }

    /// Get current broadcast statistics.
    pub async fn stats(&self) -> BroadcastStats {
        let channels = self.channels.read().await;
        let active_receivers: usize = channels.values().map(|ch| ch.sender.receiver_count()).sum();
        BroadcastStats {
            messages_published: self.messages_published.load(Ordering::Relaxed),
            active_channels: channels.len(),
            active_receivers,
        }
    }

    /// Get the number of active channels.
    pub async fn channel_count(&self) -> usize {
        self.channels.read().await.len()
    }
}

/// Errors from broadcast operations.
#[derive(Debug, thiserror::Error)]
pub enum BroadcastError {
    /// Payload exceeds maximum allowed size.
    #[error("payload too large: {size} bytes exceeds max {max}")]
    PayloadTooLarge {
        /// Actual payload size.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Too many named channels exist.
    #[error("channel limit exceeded: max {max} channels")]
    TooManyChannels {
        /// Maximum allowed channels.
        max: usize,
    },

    /// Invalid payload data.
    #[error("invalid payload: {0}")]
    InvalidPayload(String),
}

impl BroadcastError {
    /// HTTP status code for this error.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::PayloadTooLarge { .. } => 413,
            Self::TooManyChannels { .. } => 503,
            Self::InvalidPayload(_) => 400,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn test_publish_creates_channel_on_demand() {
        let manager = BroadcastManager::new(BroadcastConfig::new());

        let receivers = manager
            .publish("chat:room1", "message".into(), serde_json::json!({"text": "hello"}))
            .await
            .unwrap();

        // No subscribers yet, so 0 receivers
        assert_eq!(receivers, 0);
        assert_eq!(manager.channel_count().await, 1);
    }

    #[tokio::test]
    async fn test_subscribe_then_publish() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

        let mut rx = manager.subscribe("chat:room1").await.unwrap();

        let receivers = manager
            .publish(
                "chat:room1",
                "message".into(),
                serde_json::json!({"text": "hello"}),
            )
            .await
            .unwrap();

        assert_eq!(receivers, 1);

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.channel, "chat:room1");
        assert_eq!(msg.event, "message");
        assert_eq!(msg.payload["text"], "hello");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

        let mut rx1 = manager.subscribe("events").await.unwrap();
        let mut rx2 = manager.subscribe("events").await.unwrap();

        let receivers = manager
            .publish("events", "update".into(), serde_json::json!({"v": 1}))
            .await
            .unwrap();

        assert_eq!(receivers, 2);

        let msg1 = rx1.recv().await.unwrap();
        let msg2 = rx2.recv().await.unwrap();
        assert_eq!(msg1.payload, msg2.payload);
    }

    #[tokio::test]
    async fn test_payload_too_large() {
        let config = BroadcastConfig {
            max_message_bytes: 10,
            ..BroadcastConfig::new()
        };
        let manager = BroadcastManager::new(config);

        let result = manager
            .publish("ch", "e".into(), serde_json::json!({"big": "data that is too large"}))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), 413);
    }

    #[tokio::test]
    async fn test_too_many_channels() {
        let config = BroadcastConfig {
            max_channels: 2,
            ..BroadcastConfig::new()
        };
        let manager = BroadcastManager::new(config);

        manager.publish("ch1", "e".into(), serde_json::json!({})).await.unwrap();
        manager.publish("ch2", "e".into(), serde_json::json!({})).await.unwrap();
        let result = manager.publish("ch3", "e".into(), serde_json::json!({})).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), 503);
    }

    #[tokio::test]
    async fn test_gc_empty_channels() {
        let manager = BroadcastManager::new(BroadcastConfig::new());

        // Create a channel with a subscriber
        let _rx = manager.subscribe("active").await.unwrap();
        // Create a channel with no subscribers
        manager.publish("orphan", "e".into(), serde_json::json!({})).await.unwrap();

        assert_eq!(manager.channel_count().await, 2);

        let removed = manager.gc_empty_channels().await;
        assert_eq!(removed, 1);
        assert_eq!(manager.channel_count().await, 1);
    }

    #[tokio::test]
    async fn test_stats() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

        let _rx = manager.subscribe("ch1").await.unwrap();
        manager.publish("ch1", "e".into(), serde_json::json!({})).await.unwrap();
        manager.publish("ch2", "e".into(), serde_json::json!({})).await.unwrap();

        let stats = manager.stats().await;
        assert_eq!(stats.messages_published, 2);
        assert_eq!(stats.active_channels, 2);
        assert_eq!(stats.active_receivers, 1);
    }

    #[tokio::test]
    async fn test_channel_isolation() {
        let manager = Arc::new(BroadcastManager::new(BroadcastConfig::new()));

        let mut rx_a = manager.subscribe("channel_a").await.unwrap();
        let _rx_b = manager.subscribe("channel_b").await.unwrap();

        manager
            .publish("channel_a", "event".into(), serde_json::json!({"for": "a"}))
            .await
            .unwrap();

        let msg = rx_a.recv().await.unwrap();
        assert_eq!(msg.payload["for"], "a");

        // channel_b subscriber should not have received anything
        // (try_recv would return Empty, not a message)
    }
}
