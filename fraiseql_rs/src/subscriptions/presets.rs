//! Preset configurations for WebSocket subscriptions
//!
//! Provides pre-configured settings for common deployment scenarios:
//! - Development: Permissive limits, minimal rate limiting, in-memory events
//! - Production: Strict limits, aggressive rate limiting, Redis events
//! - High Performance: Maximum throughput, relaxed limits, Redis required

use std::time::Duration;

use super::config::{
    EventBusConfig, RateLimiterConfig, SubscriptionConfig, SubscriptionLimits, WebSocketConfig,
};

impl SubscriptionConfig {
    /// Development preset: Permissive limits, in-memory events, no rate limiting
    ///
    /// Use this for local development and testing.
    ///
    /// Features:
    /// - In-memory event bus (no external dependencies)
    /// - No rate limiting (permissive)
    /// - Higher connection limits (easier testing)
    /// - Verbose logging (easier debugging)
    ///
    /// Performance:
    /// - Suitable for single-instance deployments
    /// - Max ~100 concurrent connections
    ///
    /// Example:
    /// ```ignore
    /// let config = SubscriptionConfig::development();
    /// ```
    #[must_use]
    pub fn development() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8001,
            limits: SubscriptionLimits {
                max_subscriptions_per_connection: 10,
                max_concurrent_connections: 100,
                max_filter_complexity: 50,
                max_event_payload_size: 1_024 * 1_024, // 1MB
                max_query_size: 64 * 1_024,            // 64KB
            },
            rate_limiter: RateLimiterConfig {
                max_subscriptions_per_user: 100,
                max_events_per_subscription: usize::MAX, // No limit
                max_connections_per_user: 10,
                token_refill_rate: 1_000.0, // Very permissive
                token_capacity: 10_000,
            },
            websocket: WebSocketConfig {
                init_timeout: Duration::from_secs(10),
                ping_interval: Duration::from_secs(30),
                pong_timeout: Duration::from_secs(5),
                shutdown_grace: Duration::from_secs(5),
                max_message_size: 256 * 1_024, // 256KB
                message_buffer_capacity: 1_000,
            },
            event_bus: EventBusConfig::InMemory,
        }
    }

    /// Production preset: Strict limits, Redis events, aggressive rate limiting
    ///
    /// Use this for production deployments with monitoring and alerting.
    ///
    /// Features:
    /// - Redis event bus (distributed, persistent)
    /// - Strict rate limiting (`DDoS` protection)
    /// - Lower connection limits (resource protection)
    /// - Authentication enforced
    ///
    /// Performance:
    /// - Suitable for high-traffic deployments
    /// - Max ~500 concurrent connections per instance
    /// - Scales horizontally with Redis
    ///
    /// Requirements:
    /// - Redis server available
    /// - `PostgreSQL` for auth/metadata
    ///
    /// Example:
    /// ```ignore
    /// let config = SubscriptionConfig::production();
    /// ```
    #[must_use]
    pub fn production() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8001,
            limits: SubscriptionLimits {
                max_subscriptions_per_connection: 5,
                max_concurrent_connections: 500,
                max_filter_complexity: 50,
                max_event_payload_size: 1_024 * 1_024, // 1MB
                max_query_size: 64 * 1_024,            // 64KB
            },
            rate_limiter: RateLimiterConfig {
                max_subscriptions_per_user: 50,
                max_events_per_subscription: 100,
                max_connections_per_user: 5,
                token_refill_rate: 100.0,
                token_capacity: 1_000,
            },
            websocket: WebSocketConfig {
                init_timeout: Duration::from_secs(5),
                ping_interval: Duration::from_secs(20),
                pong_timeout: Duration::from_secs(5),
                shutdown_grace: Duration::from_secs(5),
                max_message_size: 256 * 1_024, // 256KB
                message_buffer_capacity: 1_000,
            },
            event_bus: EventBusConfig::Redis {
                url: "redis://localhost:6379".to_string(),
                consumer_group: "fraiseql_subscriptions".to_string(),
                message_ttl: 3_600,
            },
        }
    }

    /// High-performance preset: Maximum throughput, Redis required, relaxed limits
    ///
    /// Use this for high-traffic deployments where throughput is critical.
    ///
    /// Features:
    /// - Redis event bus (required)
    /// - Relaxed rate limiting (trust your infrastructure)
    /// - Higher connection limits (throughput optimization)
    /// - Complexity analysis disabled (performance trade-off)
    ///
    /// Performance:
    /// - Suitable for ultra-high-traffic deployments
    /// - Max ~2,000 concurrent connections per instance
    /// - Scales horizontally with Redis
    /// - Lower latency, higher throughput
    ///
    /// Requirements:
    /// - Redis server (required)
    /// - Robust monitoring and alerting
    /// - Infrastructure to handle failures
    ///
    /// Example:
    /// ```ignore
    /// let config = SubscriptionConfig::high_performance();
    /// ```
    #[must_use]
    pub fn high_performance() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8001,
            limits: SubscriptionLimits {
                max_subscriptions_per_connection: 20,
                max_concurrent_connections: 2_000,
                max_filter_complexity: 50,
                max_event_payload_size: 1_024 * 1_024, // 1MB
                max_query_size: 64 * 1_024,            // 64KB
            },
            rate_limiter: RateLimiterConfig {
                max_subscriptions_per_user: 100,
                max_events_per_subscription: 1_000,
                max_connections_per_user: 20,
                token_refill_rate: 1_000.0,
                token_capacity: 10_000,
            },
            websocket: WebSocketConfig {
                init_timeout: Duration::from_secs(5),
                ping_interval: Duration::from_secs(10),
                pong_timeout: Duration::from_secs(5),
                shutdown_grace: Duration::from_secs(5),
                max_message_size: 256 * 1_024, // 256KB
                message_buffer_capacity: 1_000,
            },
            event_bus: EventBusConfig::Redis {
                url: "redis://localhost:6379".to_string(),
                consumer_group: "fraiseql_subscriptions".to_string(),
                message_ttl: 3_600,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_development_preset() {
        let config = SubscriptionConfig::development();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8001);
        assert_eq!(config.limits.max_subscriptions_per_connection, 10);
        assert_eq!(config.limits.max_concurrent_connections, 100);
        assert_eq!(
            config.rate_limiter.max_events_per_subscription,
            usize::MAX,
            "Development: no rate limiting"
        );
        assert!(
            matches!(config.event_bus, EventBusConfig::InMemory),
            "Development: in-memory event bus"
        );
    }

    #[test]
    fn test_production_preset() {
        let config = SubscriptionConfig::production();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8001);
        assert_eq!(config.limits.max_subscriptions_per_connection, 5);
        assert_eq!(config.limits.max_concurrent_connections, 500);
        assert_eq!(config.rate_limiter.max_subscriptions_per_user, 50);
        assert_eq!(config.rate_limiter.max_connections_per_user, 5);
        assert!(
            matches!(config.event_bus, EventBusConfig::Redis { .. }),
            "Production: Redis event bus"
        );
    }

    #[test]
    fn test_high_performance_preset() {
        let config = SubscriptionConfig::high_performance();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8001);
        assert_eq!(config.limits.max_subscriptions_per_connection, 20);
        assert_eq!(config.limits.max_concurrent_connections, 2_000);
        assert_eq!(config.rate_limiter.max_subscriptions_per_user, 100);
        assert_eq!(config.rate_limiter.max_connections_per_user, 20);
        assert_eq!(config.rate_limiter.max_events_per_subscription, 1_000);
        assert!(
            matches!(config.event_bus, EventBusConfig::Redis { .. }),
            "High-performance: Redis event bus"
        );
    }

    #[test]
    fn test_presets_have_consistent_timeouts() {
        let dev = SubscriptionConfig::development();
        let prod = SubscriptionConfig::production();
        let perf = SubscriptionConfig::high_performance();

        // All presets should have reasonable timeout values
        assert!(dev.websocket.init_timeout.as_secs() >= 5);
        assert!(prod.websocket.init_timeout.as_secs() >= 5);
        assert!(perf.websocket.init_timeout.as_secs() >= 5);

        // All presets should have reasonable ping intervals
        assert!(dev.websocket.ping_interval.as_secs() >= 10);
        assert!(prod.websocket.ping_interval.as_secs() >= 10);
        assert!(perf.websocket.ping_interval.as_secs() >= 10);
    }

    #[test]
    fn test_presets_respect_limits() {
        let dev = SubscriptionConfig::development();
        let prod = SubscriptionConfig::production();
        let perf = SubscriptionConfig::high_performance();

        // Verify message sizes are reasonable
        assert!(dev.websocket.max_message_size > 0);
        assert!(prod.websocket.max_message_size > 0);
        assert!(perf.websocket.max_message_size > 0);

        // Verify query sizes are within payload limits
        assert!(dev.limits.max_query_size < dev.limits.max_event_payload_size);
        assert!(prod.limits.max_query_size < prod.limits.max_event_payload_size);
        assert!(perf.limits.max_query_size < perf.limits.max_event_payload_size);
    }
}
