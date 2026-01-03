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
    /// `PostgreSQL` event bus (fallback)
    PostgreSQL {
        /// `PostgreSQL` connection string
        connection_string: String,
        /// Listen channel prefix
        channel_prefix: String,
    },
    /// In-memory event bus (for testing)
    InMemory,
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
