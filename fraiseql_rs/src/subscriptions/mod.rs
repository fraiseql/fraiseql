//! GraphQL Subscriptions Support
//!
//! Real-time GraphQL subscriptions with WebSocket support.
//!
//! Features:
//! - WebSocket protocol handler (graphql-ws spec)
//! - Subscription executor
//! - Event bus with Redis (primary) and PostgreSQL (fallback)
//! - Connection management
//! - Rate limiting
//! - Resource limits
//! - Prometheus metrics
//!
//! Phase 15b: Real-time & Caching (Subscriptions)

pub mod config;
pub mod connection_manager;
pub mod event_bus;
pub mod executor;
pub mod protocol;
pub mod rate_limiter;
pub mod websocket;

// Metrics module will be created in Week 2

pub use config::{SubscriptionConfig, SubscriptionLimits};
pub use connection_manager::ConnectionManager;
pub use event_bus::{EventBus, Event, EventStream, InMemoryEventBus};
pub use executor::SubscriptionExecutor;
pub use protocol::{GraphQLMessage, SubscriptionMessage};
pub use rate_limiter::SubscriptionRateLimiter;
pub use websocket::{WebSocketConnection, WebSocketServer};

/// Subscription error type
#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    /// Connection not found
    #[error("Connection not found")]
    ConnectionNotFound,

    /// Subscription not found
    #[error("Subscription not found")]
    SubscriptionNotFound,

    /// Invalid message format
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    /// Authentication failed
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Authorization failed
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),

    /// Subscription rejected
    #[error("Subscription rejected: {0}")]
    SubscriptionRejected(String),

    /// Too many subscriptions
    #[error("Too many subscriptions for this connection")]
    TooManySubscriptions,

    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    /// Event bus error
    #[error("Event bus error: {0}")]
    EventBusError(String),

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Internal error
    #[error("Internal error: {0}")]
    InternalError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_error_display() {
        let err = SubscriptionError::ConnectionNotFound;
        assert_eq!(err.to_string(), "Connection not found");

        let err = SubscriptionError::TooManySubscriptions;
        assert_eq!(
            err.to_string(),
            "Too many subscriptions for this connection"
        );
    }
}
