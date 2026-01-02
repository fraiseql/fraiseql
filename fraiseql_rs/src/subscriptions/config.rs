//! Configuration for GraphQL Subscriptions
//!
//! Defines limits, timeouts, and settings for subscription server.

use std::time::Duration;

/// Subscription server configuration
#[derive(Debug, Clone)]
pub struct SubscriptionConfig {
    /// WebSocket server host
    pub host: String,

    /// WebSocket server port
    pub port: u16,

    /// Connection limits
    pub limits: SubscriptionLimits,

    /// Rate limiter configuration
    pub rate_limiter: RateLimiterConfig,

    /// WebSocket configuration
    pub websocket: WebSocketConfig,

    /// Event bus configuration
    pub event_bus: EventBusConfig,
}

/// Resource limits for subscriptions
#[derive(Debug, Clone)]
pub struct SubscriptionLimits {
    /// Maximum subscriptions per connection
    pub max_subscriptions_per_connection: usize,

    /// Maximum concurrent connections
    pub max_concurrent_connections: usize,

    /// Maximum filter complexity (nested levels)
    pub max_filter_complexity: usize,

    /// Maximum event payload size (bytes)
    pub max_event_payload_size: usize,

    /// Maximum query size (bytes)
    pub max_query_size: usize,
}

/// WebSocket configuration
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Connection initialization timeout
    pub init_timeout: Duration,

    /// Ping interval
    pub ping_interval: Duration,

    /// Pong timeout (how long to wait for pong after ping)
    pub pong_timeout: Duration,

    /// Graceful shutdown timeout
    pub shutdown_grace: Duration,

    /// Maximum message size (bytes)
    pub max_message_size: usize,

    /// Buffer capacity for message queue
    pub message_buffer_capacity: usize,
}

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Maximum subscriptions per user (per minute)
    pub max_subscriptions_per_user: usize,

    /// Maximum events per subscription per second
    pub max_events_per_subscription: usize,

    /// Maximum connections per user
    pub max_connections_per_user: usize,

    /// Token bucket refill rate (tokens per second)
    pub token_refill_rate: f64,

    /// Token bucket capacity
    pub token_capacity: u32,
}

/// Event bus configuration
#[derive(Debug, Clone)]
pub enum EventBusConfig {
    /// Redis event bus (primary)
    Redis {
        /// Redis connection URL
        url: String,
        /// Consumer group for subscriptions
        consumer_group: String,
        /// Message TTL (seconds)
        message_ttl: u64,
    },
    /// PostgreSQL event bus (fallback)
    PostgreSQL {
        /// PostgreSQL connection string
        connection_string: String,
        /// Listen channel prefix
        channel_prefix: String,
    },
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8001,
            limits: SubscriptionLimits::default(),
            rate_limiter: RateLimiterConfig::default(),
            websocket: WebSocketConfig::default(),
            event_bus: EventBusConfig::Redis {
                url: "redis://localhost:6379".to_string(),
                consumer_group: "fraiseql-subscriptions".to_string(),
                message_ttl: 3600, // 1 hour
            },
        }
    }
}

impl Default for SubscriptionLimits {
    fn default() -> Self {
        Self {
            max_subscriptions_per_connection: 100,
            max_concurrent_connections: 10_000,
            max_filter_complexity: 10,
            max_event_payload_size: 1_024 * 1_024, // 1MB
            max_query_size: 100 * 1_024, // 100KB
        }
    }
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            init_timeout: Duration::from_secs(5),
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(10),
            shutdown_grace: Duration::from_secs(5),
            max_message_size: 64 * 1_024, // 64KB
            message_buffer_capacity: 1_000,
        }
    }
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            max_subscriptions_per_user: 100,
            max_events_per_subscription: 100, // per second
            max_connections_per_user: 10,
            token_refill_rate: 100.0,
            token_capacity: 1_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_config_defaults() {
        let config = SubscriptionConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8001);
        assert_eq!(config.limits.max_subscriptions_per_connection, 100);
    }

    #[test]
    fn test_websocket_config_defaults() {
        let config = WebSocketConfig::default();
        assert_eq!(config.ping_interval, Duration::from_secs(30));
        assert_eq!(config.pong_timeout, Duration::from_secs(10));
        assert_eq!(config.max_message_size, 64 * 1_024);
    }

    #[test]
    fn test_rate_limiter_config_defaults() {
        let config = RateLimiterConfig::default();
        assert_eq!(config.max_subscriptions_per_user, 100);
        assert_eq!(config.max_events_per_subscription, 100);
    }
}
