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
    ActiveSubscription, ChangeSpineEnvelope, SubscriptionEvent, SubscriptionId,
    SubscriptionOperation, SubscriptionPayload,
};
pub use webhook::{WebhookAdapter, WebhookPayload, WebhookTransportConfig};
/// Backward-compatible type alias — use [`WebhookTransportConfig`] in new code.
pub type WebhookConfig = WebhookTransportConfig;

/// Extract `(field, value)` equality conditions from an RLS `WhereClause`,
/// **fail-closed**.
///
/// Walks the clause tree and collects all `Field { op: Eq }` nodes; `And` nodes are
/// recursively flattened. Any shape that **cannot** be represented as a simple
/// `field == value` equality — a non-`Eq` operator, `Or`, `Not`, or a `NativeField`
/// column condition — is a hard **error**, not a silent skip.
///
/// This is a security property (#596). Subscription event delivery enforces each
/// extracted condition against the event data (AND semantics); a condition that is
/// *dropped* rather than *enforced* silently widens visibility — the subscriber
/// receives rows the RLS clause was meant to hide. When these conditions derive from a
/// row-visibility policy or an RLS clause, "deliver more, never fewer" is exactly the
/// wrong default. So the caller evaluates the policy at subscribe time, passes the
/// result here, and **refuses the subscription** (rather than delivering unfiltered)
/// if any part cannot be enforced.
///
/// # Errors
///
/// Returns a description of the first unenforceable clause shape encountered.
pub fn extract_rls_conditions(
    clause: &crate::db::WhereClause,
) -> Result<Vec<(String, serde_json::Value)>, String> {
    let mut conditions = Vec::new();
    collect_eq_conditions(clause, &mut conditions)?;
    Ok(conditions)
}

fn collect_eq_conditions(
    clause: &crate::db::WhereClause,
    out: &mut Vec<(String, serde_json::Value)>,
) -> Result<(), String> {
    use crate::db::{WhereClause, WhereOperator};
    match clause {
        WhereClause::Field {
            path,
            operator: WhereOperator::Eq,
            value,
        } => {
            // Use the last path component as the field name (e.g., ["tenant_id"] → "tenant_id")
            if let Some(field) = path.last() {
                out.push((field.clone(), value.clone()));
            }
            Ok(())
        },
        WhereClause::And(clauses) => {
            for c in clauses {
                collect_eq_conditions(c, out)?;
            }
            Ok(())
        },
        WhereClause::Field { path, operator, .. } => Err(format!(
            "row-visibility condition on `{}` uses operator `{operator:?}`; only `Eq` can be \
             enforced on the pushed event stream — refusing the subscription rather than \
             delivering unfiltered rows (#596)",
            path.last().map_or("<field>", String::as_str),
        )),
        // `Or`, `Not`, and `NativeField` (a native-column condition that does not map to
        // a JSONB event-data key) cannot be enforced as event-stream equality — refuse
        // rather than silently widen.
        _ => Err("row-visibility clause uses an `Or`/`Not`/native-column shape that cannot be \
                  enforced as event-stream equality — refusing the subscription rather than \
                  delivering unfiltered rows (#596)"
            .to_string()),
    }
}

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during subscription operations.
#[non_exhaustive]
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
