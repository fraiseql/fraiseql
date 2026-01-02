//! Default configuration values for WebSocket subscriptions
//!
//! Provides sensible default values for each configuration component.
//! Used as fallback when specific values are not provided.

use std::time::Duration;

use super::config::{RateLimiterConfig, SubscriptionLimits, WebSocketConfig};

/// Default subscription limits
impl Default for SubscriptionLimits {
    fn default() -> Self {
        Self {
            // Allow reasonable number of subscriptions per connection
            max_subscriptions_per_connection: 10,
            // Support many concurrent connections
            max_concurrent_connections: 500,
            // Reasonable filter complexity
            max_filter_complexity: 50,
            // 1MB max payload
            max_event_payload_size: 1_024 * 1_024,
            // 64KB max query
            max_query_size: 64 * 1_024,
        }
    }
}

/// Default WebSocket configuration
impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            // Wait up to 10 seconds for connection init
            init_timeout: Duration::from_secs(10),
            // Ping every 30 seconds to keep connection alive
            ping_interval: Duration::from_secs(30),
            // Wait up to 5 seconds for pong response
            pong_timeout: Duration::from_secs(5),
            // 5 seconds to gracefully shutdown
            shutdown_grace: Duration::from_secs(5),
            // 256KB max message size
            max_message_size: 256 * 1_024,
            // Buffer up to 1000 messages per connection
            message_buffer_capacity: 1_000,
        }
    }
}

/// Default rate limiter configuration
impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            // 50 subscriptions per user per minute
            max_subscriptions_per_user: 50,
            // 100 events per subscription per second
            max_events_per_subscription: 100,
            // 5 concurrent connections per user
            max_connections_per_user: 5,
            // Token bucket: 100 tokens per second refill rate
            token_refill_rate: 100.0,
            // Token bucket: capacity of 1000 tokens
            token_capacity: 1_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_limits_defaults() {
        let limits = SubscriptionLimits::default();
        assert_eq!(limits.max_subscriptions_per_connection, 10);
        assert_eq!(limits.max_concurrent_connections, 500);
        assert_eq!(limits.max_filter_complexity, 50);
        assert_eq!(limits.max_event_payload_size, 1_024 * 1_024);
        assert_eq!(limits.max_query_size, 64 * 1_024);
    }

    #[test]
    fn test_websocket_config_defaults() {
        let config = WebSocketConfig::default();
        assert_eq!(config.init_timeout, Duration::from_secs(10));
        assert_eq!(config.ping_interval, Duration::from_secs(30));
        assert_eq!(config.pong_timeout, Duration::from_secs(5));
        assert_eq!(config.shutdown_grace, Duration::from_secs(5));
        assert_eq!(config.max_message_size, 256 * 1_024);
        assert_eq!(config.message_buffer_capacity, 1_000);
    }

    #[test]
    fn test_rate_limiter_config_defaults() {
        let config = RateLimiterConfig::default();
        assert_eq!(config.max_subscriptions_per_user, 50);
        assert_eq!(config.max_events_per_subscription, 100);
        assert_eq!(config.max_connections_per_user, 5);
        assert_eq!(config.token_refill_rate, 100.0);
        assert_eq!(config.token_capacity, 1_000);
    }

    #[test]
    fn test_defaults_are_reasonable() {
        let limits = SubscriptionLimits::default();
        let websocket = WebSocketConfig::default();
        let rate_limit = RateLimiterConfig::default();

        // Query size should fit within payload size
        assert!(limits.max_query_size < limits.max_event_payload_size);

        // Message size should fit within payload size
        assert!(websocket.max_message_size < limits.max_event_payload_size);

        // Buffer capacity should be positive
        assert!(websocket.message_buffer_capacity > 0);

        // Rate limits should be positive
        assert!(rate_limit.max_subscriptions_per_user > 0);
        assert!(rate_limit.max_events_per_subscription > 0);
        assert!(rate_limit.max_connections_per_user > 0);
        assert!(rate_limit.token_refill_rate > 0.0);
        assert!(rate_limit.token_capacity > 0);
    }

    #[test]
    fn test_timeout_durations_are_reasonable() {
        let config = WebSocketConfig::default();

        // All timeouts should be positive and reasonable
        assert!(config.init_timeout.as_secs() > 0);
        assert!(config.ping_interval.as_secs() > 0);
        assert!(config.pong_timeout.as_secs() > 0);
        assert!(config.shutdown_grace.as_secs() > 0);

        // Pong timeout should be less than ping interval
        assert!(config.pong_timeout < config.ping_interval);

        // Ping interval should be less than init timeout
        assert!(config.ping_interval < config.init_timeout);
    }
}
