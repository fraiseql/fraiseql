//! Subscription/observer structs: `IntermediateSubscription`,
//! `IntermediateSubscriptionFilter`, `IntermediateFilterCondition`,
//! `IntermediateObserver`, `IntermediateRetryConfig`.

use serde::{Deserialize, Serialize};

use super::{operations::IntermediateArgument, types::IntermediateDeprecation};

// =============================================================================
// Subscription Definitions
// =============================================================================

/// Subscription definition in intermediate format.
///
/// Subscriptions provide real-time event streams for GraphQL clients.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "orderUpdated",
///   "return_type": "Order",
///   "arguments": [
///     {"name": "orderId", "type": "ID", "nullable": true}
///   ],
///   "topic": "order_events",
///   "filter": {
///     "conditions": [
///       {"argument": "orderId", "path": "$.id"}
///     ]
///   },
///   "description": "Stream of order update events"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateSubscription {
    /// Subscription name (e.g., "orderUpdated")
    pub name: String,

    /// Return type name (e.g., "Order")
    pub return_type: String,

    /// Subscription arguments (for filtering events)
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Subscription description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Event topic to subscribe to (e.g., "order_events")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,

    /// Filter configuration for event matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<IntermediateSubscriptionFilter>,

    /// Fields to project from event data
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

/// Subscription filter definition for event matching.
///
/// Maps subscription arguments to JSONB paths in event data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateSubscriptionFilter {
    /// Filter conditions mapping arguments to event data paths
    pub conditions: Vec<IntermediateFilterCondition>,
}

/// A single filter condition for subscription event matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFilterCondition {
    /// Argument name from subscription arguments
    pub argument: String,

    /// JSON path to the value in event data (e.g., "$.id", "$.order_status")
    pub path: String,
}

// =============================================================================
// Observer Definitions
// =============================================================================

/// Observer definition in intermediate format.
///
/// Observers listen to database change events (INSERT/UPDATE/DELETE) and execute
/// actions (webhooks, Slack, email) when conditions are met.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "onHighValueOrder",
///   "entity": "Order",
///   "event": "INSERT",
///   "condition": "total > 1000",
///   "actions": [
///     {
///       "type": "webhook",
///       "url": "https://api.example.com/orders",
///       "headers": {"Content-Type": "application/json"}
///     },
///     {
///       "type": "slack",
///       "channel": "#sales",
///       "message": "New order: {id}",
///       "webhook_url_env": "SLACK_WEBHOOK_URL"
///     }
///   ],
///   "retry": {
///     "max_attempts": 3,
///     "backoff_strategy": "exponential",
///     "initial_delay_ms": 100,
///     "max_delay_ms": 60000
///   }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateObserver {
    /// Observer name (unique identifier)
    pub name: String,

    /// Entity type to observe (e.g., "Order", "User")
    pub entity: String,

    /// Event type: INSERT, UPDATE, or DELETE
    pub event: String,

    /// Actions to execute when observer triggers
    pub actions: Vec<IntermediateObserverAction>,

    /// Optional condition expression in FraiseQL DSL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,

    /// Retry configuration for action execution
    pub retry: IntermediateRetryConfig,
}

/// Observer action (webhook, Slack, email, etc.).
///
/// Actions are stored as flexible JSON objects since they have different
/// structures based on action type.
pub type IntermediateObserverAction = serde_json::Value;

/// Retry configuration for observer actions.
///
/// # Example JSON
///
/// ```json
/// {
///   "max_attempts": 5,
///   "backoff_strategy": "exponential",
///   "initial_delay_ms": 100,
///   "max_delay_ms": 60000
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateRetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,

    /// Backoff strategy: exponential, linear, or fixed
    pub backoff_strategy: String,

    /// Initial delay in milliseconds
    pub initial_delay_ms: u32,

    /// Maximum delay in milliseconds
    pub max_delay_ms: u32,
}
