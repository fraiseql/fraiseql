//! Subscription runtime for event-driven GraphQL subscriptions.
//!
//! FraiseQL subscriptions are **compiled projections of database events**, not
//! traditional resolver-based subscriptions. Events originate from database
//! transactions (via LISTEN/NOTIFY or CDC) and are delivered through transport
//! adapters.
//!
//! # Architecture
//!
//! ```text
//! Database Transaction (INSERT/UPDATE/DELETE)
//!     ↓ (commits)
//! LISTEN/NOTIFY (PostgreSQL)
//!     ↓
//! SubscriptionManager (event routing)
//!     ↓
//! SubscriptionMatcher (filter evaluation)
//!     ↓ (parallel delivery)
//! ├─ graphql-ws Adapter (WebSocket)
//! ├─ Webhook Adapter (HTTP POST)
//! └─ Kafka Adapter (event streaming)
//! ```
//!
//! # Example
//!
//! ```text
//! // Illustrative — subscription infrastructure requires a live schema + transport.
//! // Use SubscriptionManager::new(Arc::new(schema)) for the full API.
//!
//! // Create subscription manager
//! let manager = SubscriptionManager::new(Arc::new(schema));
//!
//! // Subscribe to events (synchronous, not async)
//! let subscription_id = manager.subscribe(
//!     "OrderCreated",
//!     user_context_json,
//!     variables_json,
//!     "connection-id",
//! )?;
//!
//! // Receive events via broadcast channel
//! let mut receiver = manager.receiver();
//! while let Ok(payload) = receiver.recv().await {
//!     if payload.subscription_id == subscription_id {
//!         // Deliver to client
//!     }
//! }
//!
//! // Unsubscribe (synchronous)
//! manager.unsubscribe(subscription_id)?;
//! ```

use thiserror::Error;

mod kafka;
mod manager;
pub mod protocol;
#[cfg(test)]
mod tests;
mod transport;
mod types;
mod webhook;

pub use kafka::{KafkaAdapter, KafkaConfig, KafkaMessage};
pub use manager::SubscriptionManager;
pub use transport::{BoxDynTransportAdapter, DeliveryResult, TransportAdapter, TransportManager};
pub use types::{
    ActiveSubscription, SubscriptionEvent, SubscriptionId, SubscriptionOperation,
    SubscriptionPayload,
};
pub use webhook::{WebhookAdapter, WebhookPayload, WebhookTransportConfig};
/// Backward-compatible type alias — use [`WebhookTransportConfig`] in new code.
pub type WebhookConfig = WebhookTransportConfig;

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during subscription operations.
#[derive(Debug, Error)]
pub enum SubscriptionError {
    /// Subscription type not found in schema.
    #[error("Subscription not found: {0}")]
    SubscriptionNotFound(String),

    /// Authentication required for subscription.
    #[error("Authentication required for subscription: {0}")]
    AuthenticationRequired(String),

    /// User not authorized for subscription.
    #[error("Not authorized for subscription: {0}")]
    Forbidden(String),

    /// Invalid subscription variables.
    #[error("Invalid subscription variables: {0}")]
    InvalidVariables(String),

    /// Subscription already exists.
    #[error("Subscription already exists: {0}")]
    AlreadyExists(String),

    /// Subscription not active.
    #[error("Subscription not active: {0}")]
    NotActive(String),

    /// Internal subscription error.
    #[error("Subscription error: {0}")]
    Internal(String),

    /// Channel send error.
    #[error("Failed to send event: {0}")]
    SendError(String),

    /// Database connection error.
    #[error("Database connection error: {0}")]
    DatabaseConnection(String),

    /// Listener already running.
    #[error("Listener already running")]
    ListenerAlreadyRunning,

    /// Listener not running.
    #[error("Listener not running")]
    ListenerNotRunning,

    /// Failed to parse notification payload.
    #[error("Failed to parse notification: {0}")]
    InvalidNotification(String),

    /// Failed to deliver event to transport.
    #[error("Failed to deliver to {transport}: {reason}")]
    DeliveryFailed {
        /// Transport that failed.
        transport: String,
        /// Reason for failure.
        reason:    String,
    },
}
