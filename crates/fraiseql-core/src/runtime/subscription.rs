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
//! ```ignore
//! use fraiseql_core::runtime::subscription::{
//!     SubscriptionManager, SubscriptionEvent, SubscriptionId,
//! };
//! use tokio::sync::broadcast;
//!
//! // Create subscription manager
//! let manager = SubscriptionManager::new(schema);
//!
//! // Subscribe to events
//! let subscription_id = manager.subscribe(
//!     "OrderCreated",
//!     user_context,
//!     variables,
//! ).await?;
//!
//! // Receive events
//! let mut receiver = manager.receiver();
//! while let Ok(event) = receiver.recv().await {
//!     if event.matches_subscription(subscription_id) {
//!         // Deliver to client
//!     }
//! }
//!
//! // Unsubscribe
//! manager.unsubscribe(subscription_id).await?;
//! ```

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::schema::{CompiledSchema, SubscriptionDefinition};

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

// =============================================================================
// Subscription Types
// =============================================================================

/// Unique identifier for a subscription instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(pub Uuid);

impl SubscriptionId {
    /// Generate a new random subscription ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from a UUID.
    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Database operation that triggered the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SubscriptionOperation {
    /// Row was inserted.
    Create,
    /// Row was updated.
    Update,
    /// Row was deleted.
    Delete,
}

impl std::fmt::Display for SubscriptionOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create => write!(f, "CREATE"),
            Self::Update => write!(f, "UPDATE"),
            Self::Delete => write!(f, "DELETE"),
        }
    }
}

/// An event from the database that may trigger subscriptions.
///
/// This is the internal event format, captured from LISTEN/NOTIFY or CDC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionEvent {
    /// Unique event identifier.
    pub event_id: String,

    /// Entity type name (e.g., "Order", "User").
    pub entity_type: String,

    /// Entity primary key.
    pub entity_id: String,

    /// Database operation that created this event.
    pub operation: SubscriptionOperation,

    /// Event timestamp (from database).
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Monotonic sequence number for ordering.
    pub sequence_number: u64,

    /// Event payload data (the row data as JSON).
    pub data: serde_json::Value,

    /// Optional old data (for UPDATE operations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_data: Option<serde_json::Value>,
}

impl SubscriptionEvent {
    /// Create a new subscription event.
    #[must_use]
    pub fn new(
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        operation: SubscriptionOperation,
        data: serde_json::Value,
    ) -> Self {
        Self {
            event_id: format!("evt_{}", Uuid::new_v4()),
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            operation,
            timestamp: chrono::Utc::now(),
            sequence_number: 0, // Set by manager
            data,
            old_data: None,
        }
    }

    /// Add old data for UPDATE operations.
    #[must_use]
    pub fn with_old_data(mut self, old_data: serde_json::Value) -> Self {
        self.old_data = Some(old_data);
        self
    }

    /// Set the sequence number.
    #[must_use]
    pub fn with_sequence(mut self, seq: u64) -> Self {
        self.sequence_number = seq;
        self
    }
}

/// A client's active subscription.
#[derive(Debug, Clone)]
pub struct ActiveSubscription {
    /// Unique subscription ID.
    pub id: SubscriptionId,

    /// Subscription type name from schema.
    pub subscription_name: String,

    /// Reference to subscription definition.
    pub definition: SubscriptionDefinition,

    /// User context for authorization filtering.
    pub user_context: serde_json::Value,

    /// Runtime variables provided by client.
    pub variables: serde_json::Value,

    /// When the subscription was created.
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Connection/client identifier (for routing).
    pub connection_id: String,
}

impl ActiveSubscription {
    /// Create a new active subscription.
    #[must_use]
    pub fn new(
        subscription_name: impl Into<String>,
        definition: SubscriptionDefinition,
        user_context: serde_json::Value,
        variables: serde_json::Value,
        connection_id: impl Into<String>,
    ) -> Self {
        Self {
            id: SubscriptionId::new(),
            subscription_name: subscription_name.into(),
            definition,
            user_context,
            variables,
            created_at: chrono::Utc::now(),
            connection_id: connection_id.into(),
        }
    }
}

/// Delivery payload sent to transport adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionPayload {
    /// The subscription ID this payload is for.
    pub subscription_id: SubscriptionId,

    /// Subscription type name.
    pub subscription_name: String,

    /// The event that triggered this payload.
    pub event: SubscriptionEvent,

    /// Projected data (filtered/transformed for this subscription).
    pub data: serde_json::Value,
}

// =============================================================================
// Subscription Manager
// =============================================================================

/// Manages active subscriptions and event routing.
///
/// The `SubscriptionManager` is the central hub for:
/// - Tracking active subscriptions per connection
/// - Receiving events from database listeners
/// - Matching events to subscriptions
/// - Broadcasting to transport adapters
pub struct SubscriptionManager {
    /// Compiled schema for subscription definitions.
    schema: Arc<CompiledSchema>,

    /// Active subscriptions indexed by ID.
    subscriptions: DashMap<SubscriptionId, ActiveSubscription>,

    /// Subscriptions indexed by connection ID (for cleanup on disconnect).
    subscriptions_by_connection: DashMap<String, Vec<SubscriptionId>>,

    /// Broadcast channel for delivering events to transports.
    event_sender: broadcast::Sender<SubscriptionPayload>,

    /// Monotonic sequence counter for event ordering.
    sequence_counter: AtomicU64,
}

impl SubscriptionManager {
    /// Create a new subscription manager.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema containing subscription definitions
    /// * `channel_capacity` - Broadcast channel capacity (default: 1024)
    #[must_use]
    pub fn new(schema: Arc<CompiledSchema>) -> Self {
        Self::with_capacity(schema, 1024)
    }

    /// Create a new subscription manager with custom channel capacity.
    #[must_use]
    pub fn with_capacity(schema: Arc<CompiledSchema>, channel_capacity: usize) -> Self {
        let (event_sender, _) = broadcast::channel(channel_capacity);

        Self {
            schema,
            subscriptions: DashMap::new(),
            subscriptions_by_connection: DashMap::new(),
            event_sender,
            sequence_counter: AtomicU64::new(1),
        }
    }

    /// Get a receiver for subscription payloads.
    ///
    /// Transport adapters use this to receive events for delivery.
    #[must_use]
    pub fn receiver(&self) -> broadcast::Receiver<SubscriptionPayload> {
        self.event_sender.subscribe()
    }

    /// Subscribe to a subscription type.
    ///
    /// # Arguments
    ///
    /// * `subscription_name` - Name of the subscription type
    /// * `user_context` - User authentication/authorization context
    /// * `variables` - Runtime variables from client
    /// * `connection_id` - Client connection identifier
    ///
    /// # Errors
    ///
    /// Returns error if subscription not found or user not authorized.
    pub fn subscribe(
        &self,
        subscription_name: &str,
        user_context: serde_json::Value,
        variables: serde_json::Value,
        connection_id: &str,
    ) -> Result<SubscriptionId, SubscriptionError> {
        // Find subscription definition
        let definition = self
            .schema
            .find_subscription(subscription_name)
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(subscription_name.to_string()))?
            .clone();

        // Create active subscription
        let active = ActiveSubscription::new(
            subscription_name,
            definition,
            user_context,
            variables,
            connection_id,
        );

        let id = active.id;

        // Store subscription
        self.subscriptions.insert(id, active);

        // Index by connection
        self.subscriptions_by_connection
            .entry(connection_id.to_string())
            .or_default()
            .push(id);

        tracing::info!(
            subscription_id = %id,
            subscription_name = subscription_name,
            connection_id = connection_id,
            "Subscription created"
        );

        Ok(id)
    }

    /// Unsubscribe from a subscription.
    ///
    /// # Errors
    ///
    /// Returns error if subscription not found.
    pub fn unsubscribe(&self, id: SubscriptionId) -> Result<(), SubscriptionError> {
        let removed = self
            .subscriptions
            .remove(&id)
            .ok_or_else(|| SubscriptionError::NotActive(id.to_string()))?;

        // Remove from connection index
        if let Some(mut subs) = self.subscriptions_by_connection.get_mut(&removed.1.connection_id) {
            subs.retain(|s| *s != id);
        }

        tracing::info!(
            subscription_id = %id,
            subscription_name = removed.1.subscription_name,
            "Subscription removed"
        );

        Ok(())
    }

    /// Unsubscribe all subscriptions for a connection.
    ///
    /// Called when a client disconnects.
    pub fn unsubscribe_connection(&self, connection_id: &str) {
        if let Some((_, subscription_ids)) = self.subscriptions_by_connection.remove(connection_id)
        {
            for id in subscription_ids {
                self.subscriptions.remove(&id);
            }

            tracing::info!(
                connection_id = connection_id,
                "All subscriptions removed for connection"
            );
        }
    }

    /// Get an active subscription by ID.
    #[must_use]
    pub fn get_subscription(&self, id: SubscriptionId) -> Option<ActiveSubscription> {
        self.subscriptions.get(&id).map(|r| r.clone())
    }

    /// Get all active subscriptions for a connection.
    #[must_use]
    pub fn get_connection_subscriptions(&self, connection_id: &str) -> Vec<ActiveSubscription> {
        self.subscriptions_by_connection
            .get(connection_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.subscriptions.get(id).map(|r| r.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get total number of active subscriptions.
    #[must_use]
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Get number of active connections with subscriptions.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.subscriptions_by_connection.len()
    }

    /// Publish an event to matching subscriptions.
    ///
    /// This is called by the database listener when an event is received.
    /// The event is matched against all active subscriptions and delivered
    /// to matching ones.
    ///
    /// # Arguments
    ///
    /// * `event` - The database event to publish
    ///
    /// # Returns
    ///
    /// Number of subscriptions that matched the event.
    pub fn publish_event(&self, mut event: SubscriptionEvent) -> usize {
        // Assign sequence number
        event.sequence_number = self.sequence_counter.fetch_add(1, Ordering::SeqCst);

        let mut matched = 0;

        // Find matching subscriptions
        for subscription in self.subscriptions.iter() {
            if self.matches_subscription(&event, &subscription) {
                matched += 1;

                // Project data for this subscription
                let data = self.project_event_data(&event, &subscription);

                let payload = SubscriptionPayload {
                    subscription_id: subscription.id,
                    subscription_name: subscription.subscription_name.clone(),
                    event: event.clone(),
                    data,
                };

                // Send to broadcast channel (may fail if no receivers, that's ok)
                let _ = self.event_sender.send(payload);
            }
        }

        if matched > 0 {
            tracing::debug!(
                event_id = event.event_id,
                entity_type = event.entity_type,
                operation = %event.operation,
                matched = matched,
                "Event matched subscriptions"
            );
        }

        matched
    }

    /// Check if an event matches a subscription's filters.
    fn matches_subscription(
        &self,
        event: &SubscriptionEvent,
        subscription: &ActiveSubscription,
    ) -> bool {
        // Check entity type matches (subscription return_type maps to entity)
        if subscription.definition.return_type != event.entity_type {
            return false;
        }

        // Check operation matches topic (if specified)
        if let Some(ref topic) = subscription.definition.topic {
            let expected_op = match topic.to_lowercase().as_str() {
                t if t.contains("created") || t.contains("insert") => {
                    Some(SubscriptionOperation::Create)
                },
                t if t.contains("updated") || t.contains("update") => {
                    Some(SubscriptionOperation::Update)
                },
                t if t.contains("deleted") || t.contains("delete") => {
                    Some(SubscriptionOperation::Delete)
                },
                _ => None,
            };

            if let Some(expected) = expected_op {
                if event.operation != expected {
                    return false;
                }
            }
        }

        // Evaluate compiled WHERE filters against event.data and subscription variables
        if let Some(ref filter) = subscription.definition.filter {
            // Check argument-based filters (variable values must match event data)
            for (arg_name, path) in &filter.argument_paths {
                // Get the variable value provided by the client
                if let Some(expected_value) = subscription.variables.get(arg_name) {
                    // Get the actual value from event data using JSON pointer
                    let actual_value = get_json_pointer_value(&event.data, path);

                    // Compare values
                    if actual_value != Some(expected_value) {
                        tracing::trace!(
                            subscription_id = %subscription.id,
                            arg_name = arg_name,
                            expected = ?expected_value,
                            actual = ?actual_value,
                            "Filter mismatch on argument"
                        );
                        return false;
                    }
                }
            }

            // Check static filter conditions
            for condition in &filter.static_filters {
                let actual_value = get_json_pointer_value(&event.data, &condition.path);

                if !evaluate_filter_condition(actual_value, condition.operator, &condition.value) {
                    tracing::trace!(
                        subscription_id = %subscription.id,
                        path = condition.path,
                        operator = ?condition.operator,
                        expected = ?condition.value,
                        actual = ?actual_value,
                        "Filter mismatch on static condition"
                    );
                    return false;
                }
            }
        }

        true
    }

    /// Project event data to subscription's field selection.
    fn project_event_data(
        &self,
        event: &SubscriptionEvent,
        subscription: &ActiveSubscription,
    ) -> serde_json::Value {
        // If no fields specified, return full event data
        if subscription.definition.fields.is_empty() {
            return event.data.clone();
        }

        // Project only requested fields
        let mut projected = serde_json::Map::new();

        for field in &subscription.definition.fields {
            // Support both simple field names and JSON pointer paths
            let value = if field.starts_with('/') {
                get_json_pointer_value(&event.data, field).cloned()
            } else {
                event.data.get(field).cloned()
            };

            if let Some(v) = value {
                // Use the field name (without leading slash) as the key
                let key = field.trim_start_matches('/').to_string();
                projected.insert(key, v);
            }
        }

        serde_json::Value::Object(projected)
    }
}

/// Retrieve a value from JSON data using a JSON pointer path.
///
/// # Lifetime Parameter
///
/// The lifetime `'a` is tied to the input `data` reference. The returned reference
/// is guaranteed to live as long as the input data reference, enabling zero-copy
/// access to nested JSON values without allocation.
///
/// # Arguments
///
/// * `data` - The JSON data object to query
/// * `path` - The path to the value, either in JSON pointer format (/a/b/c) or dot notation (a.b.c)
///
/// # Returns
///
/// A reference to the JSON value if found, or `None` if the path doesn't exist.
/// The returned reference has the same lifetime as the input data.
///
/// # Examples
///
/// ```ignore
/// let data = json!({"user": {"id": 123, "name": "Alice"}});
/// let id = get_json_pointer_value(&data, "user/id");  // Some(&123)
/// let alt = get_json_pointer_value(&data, "user.id"); // Some(&123)
/// let missing = get_json_pointer_value(&data, "admin/id"); // None
/// ```
fn get_json_pointer_value<'a>(
    data: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    // Normalize path to JSON pointer format
    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path.replace('.', "/"))
    };

    data.pointer(&normalized)
}

/// Evaluate a filter condition against an actual value.
fn evaluate_filter_condition(
    actual: Option<&serde_json::Value>,
    operator: crate::schema::FilterOperator,
    expected: &serde_json::Value,
) -> bool {
    use crate::schema::FilterOperator;

    match actual {
        None => {
            // Null/missing values only match specific conditions
            matches!(operator, FilterOperator::Eq) && expected.is_null()
        },
        Some(actual_value) => match operator {
            FilterOperator::Eq => actual_value == expected,
            FilterOperator::Ne => actual_value != expected,
            FilterOperator::Gt => {
                compare_values(actual_value, expected) == Some(std::cmp::Ordering::Greater)
            },
            FilterOperator::Gte => {
                matches!(
                    compare_values(actual_value, expected),
                    Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                )
            },
            FilterOperator::Lt => {
                compare_values(actual_value, expected) == Some(std::cmp::Ordering::Less)
            },
            FilterOperator::Lte => {
                matches!(
                    compare_values(actual_value, expected),
                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            },
            FilterOperator::Contains => {
                match (actual_value, expected) {
                    // Array contains value
                    (serde_json::Value::Array(arr), val) => arr.contains(val),
                    // String contains substring
                    (serde_json::Value::String(s), serde_json::Value::String(sub)) => {
                        s.contains(sub.as_str())
                    },
                    _ => false,
                }
            },
            FilterOperator::StartsWith => match (actual_value, expected) {
                (serde_json::Value::String(s), serde_json::Value::String(prefix)) => {
                    s.starts_with(prefix.as_str())
                },
                _ => false,
            },
            FilterOperator::EndsWith => match (actual_value, expected) {
                (serde_json::Value::String(s), serde_json::Value::String(suffix)) => {
                    s.ends_with(suffix.as_str())
                },
                _ => false,
            },
        },
    }
}

/// Compare two JSON values for ordering (numeric and string comparisons).
fn compare_values(a: &serde_json::Value, b: &serde_json::Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        // Numeric comparisons
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let a_f64 = a.as_f64()?;
            let b_f64 = b.as_f64()?;
            a_f64.partial_cmp(&b_f64)
        },
        // String comparisons
        (serde_json::Value::String(a), serde_json::Value::String(b)) => Some(a.cmp(b)),
        // Bool comparisons (false < true)
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => Some(a.cmp(b)),
        // Null comparisons
        (serde_json::Value::Null, serde_json::Value::Null) => Some(std::cmp::Ordering::Equal),
        // Incompatible types
        _ => None,
    }
}

impl std::fmt::Debug for SubscriptionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionManager")
            .field("subscription_count", &self.subscriptions.len())
            .field("connection_count", &self.subscriptions_by_connection.len())
            .finish_non_exhaustive()
    }
}

// =============================================================================
// =============================================================================
// graphql-ws Protocol Messages
// =============================================================================

/// graphql-ws protocol message types.
///
/// Implements the graphql-ws protocol as specified at:
/// <https://github.com/enisdenjo/graphql-ws/blob/master/PROTOCOL.md>
///
/// This is the modern "graphql-transport-ws" protocol, not the legacy
/// "subscriptions-transport-ws" protocol.
pub mod protocol {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    /// Client-to-server message types.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ClientMessageType {
        /// Connection initialization.
        ConnectionInit,
        /// Ping (keepalive).
        Ping,
        /// Pong response.
        Pong,
        /// Subscribe to operation.
        Subscribe,
        /// Complete/unsubscribe from operation.
        Complete,
    }

    impl ClientMessageType {
        /// Parse message type from string.
        #[must_use]
        pub fn from_str(s: &str) -> Option<Self> {
            match s {
                "connection_init" => Some(Self::ConnectionInit),
                "ping" => Some(Self::Ping),
                "pong" => Some(Self::Pong),
                "subscribe" => Some(Self::Subscribe),
                "complete" => Some(Self::Complete),
                _ => None,
            }
        }

        /// Get string representation.
        #[must_use]
        pub fn as_str(&self) -> &'static str {
            match self {
                Self::ConnectionInit => "connection_init",
                Self::Ping => "ping",
                Self::Pong => "pong",
                Self::Subscribe => "subscribe",
                Self::Complete => "complete",
            }
        }
    }

    /// Server-to-client message types.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ServerMessageType {
        /// Connection acknowledged.
        ConnectionAck,
        /// Ping (keepalive).
        Ping,
        /// Pong response.
        Pong,
        /// Subscription data.
        Next,
        /// Operation error.
        Error,
        /// Operation complete.
        Complete,
    }

    impl ServerMessageType {
        /// Get string representation.
        #[must_use]
        pub fn as_str(&self) -> &'static str {
            match self {
                Self::ConnectionAck => "connection_ack",
                Self::Ping => "ping",
                Self::Pong => "pong",
                Self::Next => "next",
                Self::Error => "error",
                Self::Complete => "complete",
            }
        }
    }

    /// Client message (from WebSocket client).
    #[derive(Debug, Clone, Deserialize)]
    pub struct ClientMessage {
        /// Message type.
        #[serde(rename = "type")]
        pub message_type: String,

        /// Operation ID (for subscribe/complete).
        #[serde(default)]
        pub id: Option<String>,

        /// Payload (connection params or subscription query).
        #[serde(default)]
        pub payload: Option<serde_json::Value>,
    }

    impl ClientMessage {
        /// Parse the message type.
        #[must_use]
        pub fn parsed_type(&self) -> Option<ClientMessageType> {
            ClientMessageType::from_str(&self.message_type)
        }

        /// Extract connection parameters from connection_init payload.
        #[must_use]
        pub fn connection_params(&self) -> Option<&serde_json::Value> {
            self.payload.as_ref()
        }

        /// Extract subscription query from subscribe payload.
        #[must_use]
        pub fn subscription_payload(&self) -> Option<SubscribePayload> {
            self.payload.as_ref().and_then(|p| serde_json::from_value(p.clone()).ok())
        }
    }

    /// Subscribe message payload.
    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct SubscribePayload {
        /// GraphQL query string.
        pub query: String,

        /// Optional operation name.
        #[serde(rename = "operationName")]
        #[serde(default)]
        pub operation_name: Option<String>,

        /// Query variables.
        #[serde(default)]
        pub variables: HashMap<String, serde_json::Value>,

        /// Extensions (e.g., persisted query hash).
        #[serde(default)]
        pub extensions: HashMap<String, serde_json::Value>,
    }

    /// Server message (to WebSocket client).
    #[derive(Debug, Clone, Serialize)]
    pub struct ServerMessage {
        /// Message type.
        #[serde(rename = "type")]
        pub message_type: String,

        /// Operation ID (for next/error/complete).
        #[serde(skip_serializing_if = "Option::is_none")]
        pub id: Option<String>,

        /// Payload (data, errors, or ack payload).
        #[serde(skip_serializing_if = "Option::is_none")]
        pub payload: Option<serde_json::Value>,
    }

    impl ServerMessage {
        /// Create connection_ack message.
        #[must_use]
        pub fn connection_ack(payload: Option<serde_json::Value>) -> Self {
            Self {
                message_type: ServerMessageType::ConnectionAck.as_str().to_string(),
                id: None,
                payload,
            }
        }

        /// Create ping message.
        #[must_use]
        pub fn ping(payload: Option<serde_json::Value>) -> Self {
            Self {
                message_type: ServerMessageType::Ping.as_str().to_string(),
                id: None,
                payload,
            }
        }

        /// Create pong message.
        #[must_use]
        pub fn pong(payload: Option<serde_json::Value>) -> Self {
            Self {
                message_type: ServerMessageType::Pong.as_str().to_string(),
                id: None,
                payload,
            }
        }

        /// Create next (data) message.
        #[must_use]
        pub fn next(id: impl Into<String>, data: serde_json::Value) -> Self {
            Self {
                message_type: ServerMessageType::Next.as_str().to_string(),
                id:           Some(id.into()),
                payload:      Some(serde_json::json!({ "data": data })),
            }
        }

        /// Create error message.
        #[must_use]
        pub fn error(id: impl Into<String>, errors: Vec<GraphQLError>) -> Self {
            Self {
                message_type: ServerMessageType::Error.as_str().to_string(),
                id:           Some(id.into()),
                payload:      Some(serde_json::to_value(errors).unwrap_or_default()),
            }
        }

        /// Create complete message.
        #[must_use]
        pub fn complete(id: impl Into<String>) -> Self {
            Self {
                message_type: ServerMessageType::Complete.as_str().to_string(),
                id:           Some(id.into()),
                payload:      None,
            }
        }

        /// Serialize to JSON string.
        ///
        /// # Errors
        ///
        /// Returns error if serialization fails.
        pub fn to_json(&self) -> Result<String, serde_json::Error> {
            serde_json::to_string(self)
        }
    }

    /// GraphQL error format.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GraphQLError {
        /// Error message.
        pub message: String,

        /// Error locations in query.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub locations: Option<Vec<ErrorLocation>>,

        /// Error path.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub path: Option<Vec<serde_json::Value>>,

        /// Extensions (error codes, etc.).
        #[serde(skip_serializing_if = "Option::is_none")]
        pub extensions: Option<HashMap<String, serde_json::Value>>,
    }

    impl GraphQLError {
        /// Create a simple error message.
        #[must_use]
        pub fn new(message: impl Into<String>) -> Self {
            Self {
                message:    message.into(),
                locations:  None,
                path:       None,
                extensions: None,
            }
        }

        /// Create an error with code extension.
        #[must_use]
        pub fn with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
            let mut extensions = HashMap::new();
            extensions.insert("code".to_string(), serde_json::json!(code.into()));

            Self {
                message:    message.into(),
                locations:  None,
                path:       None,
                extensions: Some(extensions),
            }
        }
    }

    /// Error location in query.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ErrorLocation {
        /// Line number (1-indexed).
        pub line:   u32,
        /// Column number (1-indexed).
        pub column: u32,
    }

    /// Close codes for WebSocket connection.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CloseCode {
        /// Normal closure.
        Normal               = 1000,
        /// Client violated protocol.
        ProtocolError        = 1002,
        /// Internal server error.
        InternalError        = 1011,
        /// Connection initialization timeout.
        ConnectionInitTimeout = 4408,
        /// Too many initialization requests.
        TooManyInitRequests  = 4429,
        /// Subscriber already exists (duplicate ID).
        SubscriberAlreadyExists = 4409,
        /// Unauthorized.
        Unauthorized         = 4401,
        /// Subscription not found (invalid ID on complete).
        SubscriptionNotFound = 4404,
    }

    impl CloseCode {
        /// Get the close code value.
        #[must_use]
        pub fn code(self) -> u16 {
            self as u16
        }

        /// Get the close reason message.
        #[must_use]
        pub fn reason(self) -> &'static str {
            match self {
                Self::Normal => "Normal closure",
                Self::ProtocolError => "Protocol error",
                Self::InternalError => "Internal server error",
                Self::ConnectionInitTimeout => "Connection initialization timeout",
                Self::TooManyInitRequests => "Too many initialization requests",
                Self::SubscriberAlreadyExists => "Subscriber already exists",
                Self::Unauthorized => "Unauthorized",
                Self::SubscriptionNotFound => "Subscription not found",
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_client_message_type_parsing() {
            assert_eq!(
                ClientMessageType::from_str("connection_init"),
                Some(ClientMessageType::ConnectionInit)
            );
            assert_eq!(
                ClientMessageType::from_str("subscribe"),
                Some(ClientMessageType::Subscribe)
            );
            assert_eq!(ClientMessageType::from_str("invalid"), None);
        }

        #[test]
        fn test_server_message_connection_ack() {
            let msg = ServerMessage::connection_ack(None);
            assert_eq!(msg.message_type, "connection_ack");
            assert!(msg.id.is_none());

            let json = msg.to_json().unwrap();
            assert!(json.contains("connection_ack"));
        }

        #[test]
        fn test_server_message_next() {
            let data = serde_json::json!({"orderCreated": {"id": "ord_123"}});
            let msg = ServerMessage::next("op_1", data);

            assert_eq!(msg.message_type, "next");
            assert_eq!(msg.id, Some("op_1".to_string()));

            let json = msg.to_json().unwrap();
            assert!(json.contains("next"));
            assert!(json.contains("op_1"));
            assert!(json.contains("orderCreated"));
        }

        #[test]
        fn test_server_message_error() {
            let errors = vec![GraphQLError::with_code(
                "Subscription not found",
                "SUBSCRIPTION_NOT_FOUND",
            )];
            let msg = ServerMessage::error("op_1", errors);

            assert_eq!(msg.message_type, "error");
            let json = msg.to_json().unwrap();
            assert!(json.contains("Subscription not found"));
        }

        #[test]
        fn test_server_message_complete() {
            let msg = ServerMessage::complete("op_1");

            assert_eq!(msg.message_type, "complete");
            assert_eq!(msg.id, Some("op_1".to_string()));
            assert!(msg.payload.is_none());
        }

        #[test]
        fn test_client_message_parsing() {
            let json = r#"{
                "type": "subscribe",
                "id": "op_1",
                "payload": {
                    "query": "subscription { orderCreated { id } }"
                }
            }"#;

            let msg: ClientMessage = serde_json::from_str(json).unwrap();
            assert_eq!(msg.parsed_type(), Some(ClientMessageType::Subscribe));
            assert_eq!(msg.id, Some("op_1".to_string()));

            let payload = msg.subscription_payload().unwrap();
            assert!(payload.query.contains("orderCreated"));
        }

        #[test]
        fn test_close_codes() {
            assert_eq!(CloseCode::Normal.code(), 1000);
            assert_eq!(CloseCode::Unauthorized.code(), 4401);
            assert_eq!(CloseCode::SubscriberAlreadyExists.code(), 4409);
        }

        #[test]
        fn test_graphql_error() {
            let error = GraphQLError::with_code("Test error", "TEST_ERROR");
            assert_eq!(error.message, "Test error");
            assert!(error.extensions.is_some());

            let json = serde_json::to_string(&error).unwrap();
            assert!(json.contains("TEST_ERROR"));
        }
    }
}

// =============================================================================
// Transport Adapters
// =============================================================================

/// Transport adapter trait for delivering subscription events.
///
/// Transport adapters are responsible for delivering events to external systems.
/// Each adapter implements a specific delivery mechanism (HTTP, Kafka, etc.).
///
/// # Implementors
///
/// - [`WebhookAdapter`] - HTTP POST delivery with retry logic
/// - [`KafkaAdapter`] - Apache Kafka event streaming
#[async_trait::async_trait]
pub trait TransportAdapter: Send + Sync {
    /// Deliver an event to the transport.
    ///
    /// # Arguments
    ///
    /// * `event` - The subscription event to deliver
    /// * `subscription_name` - Name of the subscription
    ///
    /// # Returns
    ///
    /// `Ok(())` on successful delivery, `Err` on failure.
    async fn deliver(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<(), SubscriptionError>;

    /// Get the adapter name for logging/metrics.
    fn name(&self) -> &'static str;

    /// Check if the adapter is healthy/connected.
    async fn health_check(&self) -> bool;
}

/// Webhook transport adapter configuration.
#[derive(Debug, Clone)]
pub struct WebhookConfig {
    /// Target URL for webhook delivery.
    pub url: String,

    /// Secret key for HMAC-SHA256 signature.
    pub secret: Option<String>,

    /// Request timeout in milliseconds.
    pub timeout_ms: u64,

    /// Maximum retry attempts.
    pub max_retries: u32,

    /// Initial retry delay in milliseconds (exponential backoff).
    pub retry_delay_ms: u64,

    /// Custom headers to include in requests.
    pub headers: std::collections::HashMap<String, String>,
}

impl WebhookConfig {
    /// Create a new webhook configuration.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url:            url.into(),
            secret:         None,
            timeout_ms:     30_000,
            max_retries:    3,
            retry_delay_ms: 1000,
            headers:        std::collections::HashMap::new(),
        }
    }

    /// Set the signing secret for HMAC-SHA256 signatures.
    #[must_use]
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        self.secret = Some(secret.into());
        self
    }

    /// Set the request timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set maximum retry attempts.
    #[must_use]
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set initial retry delay.
    #[must_use]
    pub fn with_retry_delay(mut self, delay_ms: u64) -> Self {
        self.retry_delay_ms = delay_ms;
        self
    }

    /// Add a custom header.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }
}

/// Webhook payload format for event delivery.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    /// Unique event identifier.
    pub event_id: String,

    /// Subscription name that triggered the event.
    pub subscription_name: String,

    /// Entity type (e.g., "Order").
    pub entity_type: String,

    /// Entity primary key.
    pub entity_id: String,

    /// Operation type.
    pub operation: String,

    /// Event data.
    pub data: serde_json::Value,

    /// Previous data (for UPDATE operations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_data: Option<serde_json::Value>,

    /// Event timestamp (ISO 8601).
    pub timestamp: String,

    /// Sequence number for ordering.
    pub sequence_number: u64,
}

impl WebhookPayload {
    /// Create a webhook payload from a subscription event.
    #[must_use]
    pub fn from_event(event: &SubscriptionEvent, subscription_name: &str) -> Self {
        Self {
            event_id:          event.event_id.clone(),
            subscription_name: subscription_name.to_string(),
            entity_type:       event.entity_type.clone(),
            entity_id:         event.entity_id.clone(),
            operation:         format!("{:?}", event.operation),
            data:              event.data.clone(),
            old_data:          event.old_data.clone(),
            timestamp:         event.timestamp.to_rfc3339(),
            sequence_number:   event.sequence_number,
        }
    }
}

/// Webhook transport adapter for HTTP POST delivery.
///
/// Delivers subscription events via HTTP POST with:
/// - HMAC-SHA256 signature (X-FraiseQL-Signature header)
/// - Exponential backoff retry logic
/// - Configurable timeouts
///
/// # Example
///
/// ```ignore
/// use fraiseql_core::runtime::subscription::{WebhookAdapter, WebhookConfig};
///
/// let config = WebhookConfig::new("https://api.example.com/webhooks")
///     .with_secret("my_secret_key")
///     .with_max_retries(3);
///
/// let adapter = WebhookAdapter::new(config);
/// adapter.deliver(&event, "orderCreated").await?;
/// ```
pub struct WebhookAdapter {
    config: WebhookConfig,
    client: reqwest::Client,
}

impl WebhookAdapter {
    /// Create a new webhook adapter.
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be built (should not happen in practice).
    #[must_use]
    pub fn new(config: WebhookConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .expect("Failed to build HTTP client");

        Self { config, client }
    }

    /// Compute HMAC-SHA256 signature for payload.
    fn compute_signature(&self, payload: &str) -> Option<String> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let secret = self.config.secret.as_ref()?;

        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take any size key");
        mac.update(payload.as_bytes());

        let result = mac.finalize();
        Some(hex::encode(result.into_bytes()))
    }
}

#[async_trait::async_trait]
impl TransportAdapter for WebhookAdapter {
    async fn deliver(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<(), SubscriptionError> {
        let payload = WebhookPayload::from_event(event, subscription_name);
        let payload_json = serde_json::to_string(&payload).map_err(|e| {
            SubscriptionError::Internal(format!("Failed to serialize payload: {e}"))
        })?;

        let mut attempt = 0;
        let mut delay = self.config.retry_delay_ms;

        loop {
            attempt += 1;

            let mut request = self
                .client
                .post(&self.config.url)
                .header("Content-Type", "application/json")
                .header("X-FraiseQL-Event-Id", &event.event_id)
                .header("X-FraiseQL-Event-Type", subscription_name);

            // Add signature if secret is configured
            if let Some(signature) = self.compute_signature(&payload_json) {
                request = request.header("X-FraiseQL-Signature", format!("sha256={signature}"));
            }

            // Add custom headers
            for (name, value) in &self.config.headers {
                request = request.header(name, value);
            }

            let result = request.body(payload_json.clone()).send().await;

            match result {
                Ok(response) if response.status().is_success() => {
                    tracing::debug!(
                        url = %self.config.url,
                        event_id = %event.event_id,
                        attempt = attempt,
                        "Webhook delivered successfully"
                    );
                    return Ok(());
                },
                Ok(response) => {
                    let status = response.status();
                    tracing::warn!(
                        url = %self.config.url,
                        event_id = %event.event_id,
                        status = %status,
                        attempt = attempt,
                        "Webhook delivery failed with status"
                    );

                    // Don't retry on client errors (4xx) except 429
                    if status.is_client_error() && status.as_u16() != 429 {
                        return Err(SubscriptionError::Internal(format!(
                            "Webhook delivery failed: {status}"
                        )));
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        url = %self.config.url,
                        event_id = %event.event_id,
                        error = %e,
                        attempt = attempt,
                        "Webhook delivery error"
                    );
                },
            }

            // Check if we should retry
            if attempt >= self.config.max_retries {
                return Err(SubscriptionError::Internal(format!(
                    "Webhook delivery failed after {} attempts",
                    attempt
                )));
            }

            // Exponential backoff
            tracing::debug!(delay_ms = delay, "Retrying webhook delivery");
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            delay *= 2;
        }
    }

    fn name(&self) -> &'static str {
        "webhook"
    }

    async fn health_check(&self) -> bool {
        // Simple health check - verify URL is reachable
        match self.client.head(&self.config.url).send().await {
            Ok(response) => response.status().is_success() || response.status().as_u16() == 405,
            Err(_) => false,
        }
    }
}

impl std::fmt::Debug for WebhookAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookAdapter")
            .field("url", &self.config.url)
            .field("has_secret", &self.config.secret.is_some())
            .finish_non_exhaustive()
    }
}

/// Kafka transport adapter configuration.
#[derive(Debug, Clone)]
pub struct KafkaConfig {
    /// Kafka broker addresses (comma-separated).
    pub brokers: String,

    /// Default topic for events (can be overridden per subscription).
    pub default_topic: String,

    /// Client ID for Kafka producer.
    pub client_id: String,

    /// Message acknowledgment mode ("all", "1", "0").
    pub acks: String,

    /// Message timeout in milliseconds.
    pub timeout_ms: u64,

    /// Enable message compression.
    pub compression: Option<String>,
}

impl KafkaConfig {
    /// Create a new Kafka configuration.
    #[must_use]
    pub fn new(brokers: impl Into<String>, default_topic: impl Into<String>) -> Self {
        Self {
            brokers:       brokers.into(),
            default_topic: default_topic.into(),
            client_id:     "fraiseql".to_string(),
            acks:          "all".to_string(),
            timeout_ms:    30_000,
            compression:   None,
        }
    }

    /// Set the client ID.
    #[must_use]
    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = client_id.into();
        self
    }

    /// Set acknowledgment mode.
    #[must_use]
    pub fn with_acks(mut self, acks: impl Into<String>) -> Self {
        self.acks = acks.into();
        self
    }

    /// Set message timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Enable compression (e.g., "gzip", "snappy", "lz4").
    #[must_use]
    pub fn with_compression(mut self, compression: impl Into<String>) -> Self {
        self.compression = Some(compression.into());
        self
    }
}

/// Kafka message format for event delivery.
#[derive(Debug, Clone, Serialize)]
pub struct KafkaMessage {
    /// Unique event identifier.
    pub event_id: String,

    /// Subscription name.
    pub subscription_name: String,

    /// Entity type.
    pub entity_type: String,

    /// Entity primary key (used as message key).
    pub entity_id: String,

    /// Operation type.
    pub operation: String,

    /// Event data.
    pub data: serde_json::Value,

    /// Previous data (for UPDATE operations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_data: Option<serde_json::Value>,

    /// Event timestamp.
    pub timestamp: String,

    /// Sequence number.
    pub sequence_number: u64,
}

impl KafkaMessage {
    /// Create a Kafka message from a subscription event.
    #[must_use]
    pub fn from_event(event: &SubscriptionEvent, subscription_name: &str) -> Self {
        Self {
            event_id:          event.event_id.clone(),
            subscription_name: subscription_name.to_string(),
            entity_type:       event.entity_type.clone(),
            entity_id:         event.entity_id.clone(),
            operation:         format!("{:?}", event.operation),
            data:              event.data.clone(),
            old_data:          event.old_data.clone(),
            timestamp:         event.timestamp.to_rfc3339(),
            sequence_number:   event.sequence_number,
        }
    }

    /// Get the message key (entity_id for partitioning).
    #[must_use]
    pub fn key(&self) -> &str {
        &self.entity_id
    }
}

// =============================================================================
// Kafka Adapter - Full Implementation (with `kafka` feature)
// =============================================================================

/// Kafka transport adapter for event streaming.
///
/// Delivers subscription events to Apache Kafka topics.
/// Uses the entity_id as the message key for consistent partitioning.
///
/// # Feature Flag
///
/// This adapter has two implementations:
/// - **With `kafka` feature**: Full rdkafka-based producer with actual Kafka delivery
/// - **Without `kafka` feature**: Stub that logs events (for development/testing)
///
/// # Example
///
/// ```ignore
/// use fraiseql_core::runtime::subscription::{KafkaAdapter, KafkaConfig};
///
/// let config = KafkaConfig::new("localhost:9092", "fraiseql-events")
///     .with_client_id("my-service")
///     .with_compression("lz4");
///
/// let adapter = KafkaAdapter::new(config)?;
/// adapter.deliver(&event, "orderCreated").await?;
/// ```
#[cfg(feature = "kafka")]
pub struct KafkaAdapter {
    config:   KafkaConfig,
    producer: rdkafka::producer::FutureProducer,
}

#[cfg(feature = "kafka")]
impl std::fmt::Debug for KafkaAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KafkaAdapter")
            .field("brokers", &self.config.brokers)
            .field("default_topic", &self.config.default_topic)
            .field("client_id", &self.config.client_id)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "kafka")]
impl KafkaAdapter {
    /// Create a new Kafka adapter with a producer connection.
    ///
    /// # Errors
    ///
    /// Returns error if the Kafka producer cannot be created (e.g., invalid config).
    pub fn new(config: KafkaConfig) -> Result<Self, SubscriptionError> {
        use rdkafka::{config::ClientConfig, producer::FutureProducer};

        let mut client_config = ClientConfig::new();
        client_config
            .set("bootstrap.servers", &config.brokers)
            .set("client.id", &config.client_id)
            .set("acks", &config.acks)
            .set("message.timeout.ms", config.timeout_ms.to_string());

        if let Some(ref compression) = config.compression {
            client_config.set("compression.type", compression);
        }

        let producer: FutureProducer = client_config.create().map_err(|e| {
            SubscriptionError::Internal(format!("Failed to create Kafka producer: {e}"))
        })?;

        tracing::info!(
            brokers = %config.brokers,
            topic = %config.default_topic,
            client_id = %config.client_id,
            "KafkaAdapter created with rdkafka producer"
        );

        Ok(Self { config, producer })
    }

    /// Get the topic for a subscription (uses default if not specified).
    fn get_topic(&self, _subscription_name: &str) -> &str {
        // Could be extended to support per-subscription topic mapping
        &self.config.default_topic
    }

    /// Get reference to the underlying producer for direct Kafka operations.
    #[must_use = "the producer reference should be used for Kafka operations"]
    pub fn producer(&self) -> &rdkafka::producer::FutureProducer {
        &self.producer
    }
}

#[cfg(feature = "kafka")]
#[async_trait::async_trait]
impl TransportAdapter for KafkaAdapter {
    async fn deliver(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<(), SubscriptionError> {
        use std::time::Duration;

        use rdkafka::producer::FutureRecord;

        let message = KafkaMessage::from_event(event, subscription_name);
        let topic = self.get_topic(subscription_name);

        let payload = serde_json::to_string(&message).map_err(|e| {
            SubscriptionError::Internal(format!("Failed to serialize message: {e}"))
        })?;

        let record = FutureRecord::to(topic).key(message.key()).payload(&payload);

        let timeout = Duration::from_millis(self.config.timeout_ms);

        match self.producer.send(record, timeout).await {
            Ok((partition, offset)) => {
                tracing::debug!(
                    topic = topic,
                    partition = partition,
                    offset = offset,
                    key = message.key(),
                    event_id = %event.event_id,
                    "Kafka message delivered successfully"
                );
                Ok(())
            },
            Err((kafka_error, _)) => {
                tracing::error!(
                    topic = topic,
                    key = message.key(),
                    event_id = %event.event_id,
                    error = %kafka_error,
                    "Failed to deliver Kafka message"
                );
                Err(SubscriptionError::DeliveryFailed {
                    transport: "kafka".to_string(),
                    reason:    kafka_error.to_string(),
                })
            },
        }
    }

    fn name(&self) -> &'static str {
        "kafka"
    }

    async fn health_check(&self) -> bool {
        // Check if we can fetch cluster metadata as a health check
        use std::time::Duration;

        use rdkafka::producer::Producer;

        match self.producer.client().fetch_metadata(
            None, // All topics
            Duration::from_secs(5),
        ) {
            Ok(metadata) => {
                tracing::debug!(
                    broker_count = metadata.brokers().len(),
                    topic_count = metadata.topics().len(),
                    "Kafka health check passed"
                );
                true
            },
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Kafka health check failed"
                );
                false
            },
        }
    }
}

// =============================================================================
// Kafka Adapter - Stub Implementation (without `kafka` feature)
// =============================================================================

/// Kafka transport adapter stub (without `kafka` feature).
///
/// This is a stub implementation for development and testing.
/// Enable the `kafka` feature for actual Kafka delivery.
#[cfg(not(feature = "kafka"))]
#[derive(Debug)]
pub struct KafkaAdapter {
    config: KafkaConfig,
}

#[cfg(not(feature = "kafka"))]
impl KafkaAdapter {
    /// Create a new Kafka adapter stub.
    ///
    /// # Note
    ///
    /// This is a stub implementation. Enable the `kafka` feature for actual delivery.
    ///
    /// # Errors
    ///
    /// This stub implementation never fails, but returns `Result` for API compatibility.
    pub fn new(config: KafkaConfig) -> Result<Self, SubscriptionError> {
        tracing::warn!(
            brokers = %config.brokers,
            topic = %config.default_topic,
            "KafkaAdapter created (STUB - enable 'kafka' feature for real Kafka support)"
        );
        Ok(Self { config })
    }

    /// Get the topic for a subscription (uses default if not specified).
    fn get_topic(&self, _subscription_name: &str) -> &str {
        &self.config.default_topic
    }
}

#[cfg(not(feature = "kafka"))]
#[async_trait::async_trait]
impl TransportAdapter for KafkaAdapter {
    async fn deliver(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<(), SubscriptionError> {
        let message = KafkaMessage::from_event(event, subscription_name);
        let topic = self.get_topic(subscription_name);

        let _payload = serde_json::to_string(&message).map_err(|e| {
            SubscriptionError::Internal(format!("Failed to serialize message: {e}"))
        })?;

        // Stub implementation - log the event
        tracing::info!(
            topic = topic,
            key = message.key(),
            event_id = %event.event_id,
            "Kafka delivery (STUB) - enable 'kafka' feature for actual delivery"
        );

        Ok(())
    }

    fn name(&self) -> &'static str {
        "kafka"
    }

    async fn health_check(&self) -> bool {
        // Stub always returns true
        tracing::debug!("Kafka health check (STUB) - always returns true");
        true
    }
}

/// Multi-transport delivery manager.
///
/// Manages multiple transport adapters and delivers events to all configured
/// destinations in parallel.
///
/// # Example
///
/// ```ignore
/// use fraiseql_core::runtime::subscription::{
///     TransportManager, WebhookAdapter, WebhookConfig,
/// };
///
/// let mut manager = TransportManager::new();
///
/// // Add webhook adapter
/// let webhook = WebhookAdapter::new(WebhookConfig::new("https://api.example.com/events"));
/// manager.add_adapter(Box::new(webhook));
///
/// // Deliver to all transports
/// manager.deliver_all(&event, "orderCreated").await?;
/// ```
pub struct TransportManager {
    adapters: Vec<Box<dyn TransportAdapter>>,
}

impl TransportManager {
    /// Create a new transport manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Add a transport adapter.
    pub fn add_adapter(&mut self, adapter: Box<dyn TransportAdapter>) {
        tracing::info!(adapter = adapter.name(), "Added transport adapter");
        self.adapters.push(adapter);
    }

    /// Get the number of configured adapters.
    #[must_use]
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }

    /// Check if there are no adapters configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Deliver an event to all configured transports.
    ///
    /// Delivers in parallel and collects results. Returns Ok if at least one
    /// delivery succeeded, or the last error if all failed.
    pub async fn deliver_all(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<DeliveryResult, SubscriptionError> {
        if self.adapters.is_empty() {
            return Ok(DeliveryResult {
                successful: 0,
                failed:     0,
                errors:     Vec::new(),
            });
        }

        let futures: Vec<_> = self
            .adapters
            .iter()
            .map(|adapter| {
                let name = adapter.name().to_string();
                async move {
                    let result = adapter.deliver(event, subscription_name).await;
                    (name, result)
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        let mut successful = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        for (name, result) in results {
            match result {
                Ok(()) => successful += 1,
                Err(e) => {
                    failed += 1;
                    errors.push((name, e.to_string()));
                },
            }
        }

        Ok(DeliveryResult {
            successful,
            failed,
            errors,
        })
    }

    /// Check health of all adapters.
    pub async fn health_check_all(&self) -> Vec<(String, bool)> {
        let futures: Vec<_> = self
            .adapters
            .iter()
            .map(|adapter| {
                let name = adapter.name().to_string();
                async move {
                    let healthy = adapter.health_check().await;
                    (name, healthy)
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TransportManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportManager")
            .field("adapter_count", &self.adapters.len())
            .finish()
    }
}

/// Result of delivering an event to multiple transports.
#[derive(Debug, Clone)]
pub struct DeliveryResult {
    /// Number of successful deliveries.
    pub successful: usize,
    /// Number of failed deliveries.
    pub failed:     usize,
    /// Errors from failed deliveries (adapter name, error message).
    pub errors:     Vec<(String, String)>,
}

impl DeliveryResult {
    /// Check if all deliveries succeeded.
    #[must_use]
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0
    }

    /// Check if at least one delivery succeeded.
    #[must_use]
    pub fn any_succeeded(&self) -> bool {
        self.successful > 0
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SubscriptionDefinition;

    fn create_test_schema() -> CompiledSchema {
        CompiledSchema {
            subscriptions: vec![
                SubscriptionDefinition::new("OrderCreated", "Order").with_topic("order_created"),
                SubscriptionDefinition::new("OrderUpdated", "Order").with_topic("order_updated"),
                SubscriptionDefinition::new("UserDeleted", "User").with_topic("user_deleted"),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn test_subscription_id() {
        let id1 = SubscriptionId::new();
        let id2 = SubscriptionId::new();
        assert_ne!(id1, id2);

        let uuid = Uuid::new_v4();
        let id3 = SubscriptionId::from_uuid(uuid);
        assert_eq!(id3.0, uuid);
    }

    #[test]
    fn test_subscription_event_creation() {
        let event = SubscriptionEvent::new(
            "Order",
            "ord_123",
            SubscriptionOperation::Create,
            serde_json::json!({"id": "ord_123", "amount": 99.99}),
        );

        assert!(event.event_id.starts_with("evt_"));
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.entity_id, "ord_123");
        assert_eq!(event.operation, SubscriptionOperation::Create);
    }

    #[test]
    fn test_subscription_manager_subscribe() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        let id = manager
            .subscribe(
                "OrderCreated",
                serde_json::json!({"user_id": "usr_123"}),
                serde_json::json!({}),
                "conn_1",
            )
            .unwrap();

        assert_eq!(manager.subscription_count(), 1);
        assert_eq!(manager.connection_count(), 1);

        let sub = manager.get_subscription(id).unwrap();
        assert_eq!(sub.subscription_name, "OrderCreated");
        assert_eq!(sub.connection_id, "conn_1");
    }

    #[test]
    fn test_subscription_manager_subscribe_not_found() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        let result = manager.subscribe(
            "NonExistent",
            serde_json::json!({}),
            serde_json::json!({}),
            "conn_1",
        );

        assert!(matches!(result, Err(SubscriptionError::SubscriptionNotFound(_))));
    }

    #[test]
    fn test_subscription_manager_unsubscribe() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        let id = manager
            .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
            .unwrap();

        assert_eq!(manager.subscription_count(), 1);

        manager.unsubscribe(id).unwrap();

        assert_eq!(manager.subscription_count(), 0);
    }

    #[test]
    fn test_subscription_manager_unsubscribe_connection() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        // Create multiple subscriptions for same connection
        manager
            .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
            .unwrap();

        manager
            .subscribe("OrderUpdated", serde_json::json!({}), serde_json::json!({}), "conn_1")
            .unwrap();

        assert_eq!(manager.subscription_count(), 2);

        manager.unsubscribe_connection("conn_1");

        assert_eq!(manager.subscription_count(), 0);
        assert_eq!(manager.connection_count(), 0);
    }

    #[test]
    fn test_subscription_event_matching() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        // Subscribe to OrderCreated
        manager
            .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
            .unwrap();

        // Create event should match
        let create_event = SubscriptionEvent::new(
            "Order",
            "ord_123",
            SubscriptionOperation::Create,
            serde_json::json!({"id": "ord_123"}),
        );

        let delivered = manager.publish_event(create_event);
        assert_eq!(delivered, 1);

        // Update event should not match (wrong operation)
        let update_event = SubscriptionEvent::new(
            "Order",
            "ord_123",
            SubscriptionOperation::Update,
            serde_json::json!({"id": "ord_123"}),
        );

        let delivered = manager.publish_event(update_event);
        assert_eq!(delivered, 0);
    }

    #[test]
    fn test_subscription_event_wrong_entity() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        // Subscribe to OrderCreated
        manager
            .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
            .unwrap();

        // User event should not match (wrong entity)
        let user_event = SubscriptionEvent::new(
            "User",
            "usr_123",
            SubscriptionOperation::Create,
            serde_json::json!({"id": "usr_123"}),
        );

        let delivered = manager.publish_event(user_event);
        assert_eq!(delivered, 0);
    }

    #[test]
    fn test_subscription_sequence_numbers() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        manager
            .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
            .unwrap();

        let mut receiver = manager.receiver();

        // Publish multiple events
        for i in 1..=3 {
            let event = SubscriptionEvent::new(
                "Order",
                format!("ord_{i}"),
                SubscriptionOperation::Create,
                serde_json::json!({"id": format!("ord_{}", i)}),
            );
            manager.publish_event(event);
        }

        // Check sequence numbers are monotonic
        let mut last_seq = 0;
        for _ in 0..3 {
            if let Ok(payload) = receiver.try_recv() {
                assert!(payload.event.sequence_number > last_seq);
                last_seq = payload.event.sequence_number;
            }
        }
    }

    // =========================================================================

    // =========================================================================
    // Transport Adapter Tests
    // =========================================================================

    #[test]
    fn test_webhook_config_builder() {
        let config = WebhookConfig::new("https://api.example.com/webhooks")
            .with_secret("my-secret")
            .with_timeout(10_000)
            .with_max_retries(5)
            .with_retry_delay(500)
            .with_header("X-Custom-Header", "custom-value");

        assert_eq!(config.url, "https://api.example.com/webhooks");
        assert_eq!(config.secret, Some("my-secret".to_string()));
        assert_eq!(config.timeout_ms, 10_000);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_ms, 500);
        assert_eq!(config.headers.get("X-Custom-Header"), Some(&"custom-value".to_string()));
    }

    #[test]
    fn test_webhook_config_defaults() {
        let config = WebhookConfig::new("https://api.example.com/webhooks");

        assert_eq!(config.url, "https://api.example.com/webhooks");
        assert!(config.secret.is_none());
        assert_eq!(config.timeout_ms, 30_000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert!(config.headers.is_empty());
    }

    #[test]
    fn test_webhook_payload_from_event() {
        let event = SubscriptionEvent {
            event_id:        "evt_123".to_string(),
            entity_type:     "Order".to_string(),
            entity_id:       "ord_456".to_string(),
            operation:       SubscriptionOperation::Create,
            data:            serde_json::json!({"id": "ord_456", "total": 99.99}),
            old_data:        None,
            timestamp:       chrono::Utc::now(),
            sequence_number: 42,
        };

        let payload = WebhookPayload::from_event(&event, "order_created");

        assert_eq!(payload.event_id, "evt_123");
        assert_eq!(payload.subscription_name, "order_created");
        assert_eq!(payload.entity_type, "Order");
        assert_eq!(payload.entity_id, "ord_456");
        assert_eq!(payload.operation, "Create");
        assert_eq!(payload.data["total"], 99.99);
        assert!(payload.old_data.is_none());
        assert_eq!(payload.sequence_number, 42);
    }

    #[test]
    fn test_webhook_adapter_debug() {
        let config =
            WebhookConfig::new("https://api.example.com/webhooks").with_secret("secret-key");
        let adapter = WebhookAdapter::new(config);

        let debug = format!("{:?}", adapter);
        assert!(debug.contains("WebhookAdapter"));
        assert!(debug.contains("https://api.example.com/webhooks"));
        assert!(debug.contains("has_secret: true"));
    }

    #[test]
    fn test_webhook_adapter_name() {
        let config = WebhookConfig::new("https://api.example.com/webhooks");
        let adapter = WebhookAdapter::new(config);

        assert_eq!(adapter.name(), "webhook");
    }

    #[test]
    fn test_kafka_config_builder() {
        let config = KafkaConfig::new("localhost:9092", "events")
            .with_client_id("test-client")
            .with_acks("all")
            .with_timeout(5_000)
            .with_compression("gzip");

        assert_eq!(config.brokers, "localhost:9092");
        assert_eq!(config.default_topic, "events");
        assert_eq!(config.client_id, "test-client");
        assert_eq!(config.acks, "all");
        assert_eq!(config.timeout_ms, 5_000);
        assert_eq!(config.compression, Some("gzip".to_string()));
    }

    #[test]
    fn test_kafka_config_defaults() {
        let config = KafkaConfig::new("localhost:9092", "events");

        assert_eq!(config.brokers, "localhost:9092");
        assert_eq!(config.default_topic, "events");
        assert_eq!(config.client_id, "fraiseql");
        assert_eq!(config.acks, "all"); // Default: wait for all replicas
        assert_eq!(config.timeout_ms, 30_000); // 30 seconds default
        assert!(config.compression.is_none());
    }

    #[test]
    fn test_kafka_message_from_event() {
        let event = SubscriptionEvent {
            event_id:        "evt_789".to_string(),
            entity_type:     "User".to_string(),
            entity_id:       "usr_123".to_string(),
            operation:       SubscriptionOperation::Update,
            data:            serde_json::json!({"id": "usr_123", "name": "John"}),
            old_data:        Some(serde_json::json!({"id": "usr_123", "name": "Jane"})),
            timestamp:       chrono::Utc::now(),
            sequence_number: 100,
        };

        let message = KafkaMessage::from_event(&event, "user_updated");

        assert_eq!(message.event_id, "evt_789");
        assert_eq!(message.subscription_name, "user_updated");
        assert_eq!(message.entity_type, "User");
        assert_eq!(message.entity_id, "usr_123");
        assert_eq!(message.operation, "Update");
        assert_eq!(message.data["name"], "John");
        assert_eq!(message.old_data.as_ref().unwrap()["name"], "Jane");
        assert_eq!(message.sequence_number, 100);
    }

    #[test]
    fn test_kafka_message_key() {
        let event = SubscriptionEvent {
            event_id:        "evt_1".to_string(),
            entity_type:     "Order".to_string(),
            entity_id:       "ord_partition_key".to_string(),
            operation:       SubscriptionOperation::Create,
            data:            serde_json::json!({}),
            old_data:        None,
            timestamp:       chrono::Utc::now(),
            sequence_number: 1,
        };

        let message = KafkaMessage::from_event(&event, "test_sub");

        // Key should be entity_id for consistent partitioning
        assert_eq!(message.key(), "ord_partition_key");
    }

    #[test]
    fn test_kafka_adapter_name() {
        let config = KafkaConfig::new("localhost:9092", "events");
        let adapter = KafkaAdapter::new(config).unwrap();

        assert_eq!(adapter.name(), "kafka");
    }

    #[test]
    fn test_transport_manager_new() {
        let manager = TransportManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.adapter_count(), 0);
    }

    #[test]
    fn test_transport_manager_add_adapter() {
        let mut manager = TransportManager::new();

        let webhook = WebhookAdapter::new(WebhookConfig::new("https://api.example.com/webhooks"));
        manager.add_adapter(Box::new(webhook));

        assert!(!manager.is_empty());
        assert_eq!(manager.adapter_count(), 1);
    }

    #[test]
    fn test_transport_manager_debug() {
        let mut manager = TransportManager::new();
        let webhook = WebhookAdapter::new(WebhookConfig::new("https://api.example.com/webhooks"));
        manager.add_adapter(Box::new(webhook));

        let debug = format!("{:?}", manager);
        assert!(debug.contains("TransportManager"));
        assert!(debug.contains("adapter_count: 1"));
    }

    #[test]
    fn test_delivery_result_all_succeeded() {
        let result = DeliveryResult {
            successful: 3,
            failed:     0,
            errors:     vec![],
        };

        assert!(result.all_succeeded());
        assert!(result.any_succeeded());
    }

    #[test]
    fn test_delivery_result_partial_failure() {
        let result = DeliveryResult {
            successful: 2,
            failed:     1,
            errors:     vec![("webhook".to_string(), "Connection refused".to_string())],
        };

        assert!(!result.all_succeeded());
        assert!(result.any_succeeded());
    }

    #[test]
    fn test_delivery_result_all_failed() {
        let result = DeliveryResult {
            successful: 0,
            failed:     2,
            errors:     vec![
                ("webhook".to_string(), "Connection refused".to_string()),
                ("kafka".to_string(), "Broker unavailable".to_string()),
            ],
        };

        assert!(!result.all_succeeded());
        assert!(!result.any_succeeded());
    }

    // =========================================================================
    // Filter Evaluation Tests
    // =========================================================================

    #[test]
    fn test_get_json_pointer_value_simple() {
        let data = serde_json::json!({"id": "123", "name": "Test"});

        assert_eq!(get_json_pointer_value(&data, "/id"), Some(&serde_json::json!("123")));
        assert_eq!(get_json_pointer_value(&data, "/name"), Some(&serde_json::json!("Test")));
        assert_eq!(get_json_pointer_value(&data, "/missing"), None);
    }

    #[test]
    fn test_get_json_pointer_value_nested() {
        let data = serde_json::json!({
            "user": {
                "profile": {
                    "name": "Alice"
                }
            }
        });

        assert_eq!(
            get_json_pointer_value(&data, "/user/profile/name"),
            Some(&serde_json::json!("Alice"))
        );
    }

    #[test]
    fn test_get_json_pointer_value_dot_notation() {
        let data = serde_json::json!({"user": {"name": "Bob"}});

        // Dot notation should be converted to JSON pointer
        assert_eq!(get_json_pointer_value(&data, "user.name"), Some(&serde_json::json!("Bob")));
    }

    #[test]
    fn test_filter_condition_eq() {
        use crate::schema::FilterOperator;

        assert!(evaluate_filter_condition(
            Some(&serde_json::json!("active")),
            FilterOperator::Eq,
            &serde_json::json!("active")
        ));

        assert!(!evaluate_filter_condition(
            Some(&serde_json::json!("active")),
            FilterOperator::Eq,
            &serde_json::json!("inactive")
        ));
    }

    #[test]
    fn test_filter_condition_ne() {
        use crate::schema::FilterOperator;

        assert!(evaluate_filter_condition(
            Some(&serde_json::json!("active")),
            FilterOperator::Ne,
            &serde_json::json!("inactive")
        ));

        assert!(!evaluate_filter_condition(
            Some(&serde_json::json!("active")),
            FilterOperator::Ne,
            &serde_json::json!("active")
        ));
    }

    #[test]
    fn test_filter_condition_numeric_comparisons() {
        use crate::schema::FilterOperator;

        // Greater than
        assert!(evaluate_filter_condition(
            Some(&serde_json::json!(100)),
            FilterOperator::Gt,
            &serde_json::json!(50)
        ));
        assert!(!evaluate_filter_condition(
            Some(&serde_json::json!(50)),
            FilterOperator::Gt,
            &serde_json::json!(100)
        ));

        // Greater than or equal
        assert!(evaluate_filter_condition(
            Some(&serde_json::json!(100)),
            FilterOperator::Gte,
            &serde_json::json!(100)
        ));

        // Less than
        assert!(evaluate_filter_condition(
            Some(&serde_json::json!(50)),
            FilterOperator::Lt,
            &serde_json::json!(100)
        ));

        // Less than or equal
        assert!(evaluate_filter_condition(
            Some(&serde_json::json!(100)),
            FilterOperator::Lte,
            &serde_json::json!(100)
        ));
    }

    #[test]
    fn test_filter_condition_string_comparisons() {
        use crate::schema::FilterOperator;

        // Contains
        assert!(evaluate_filter_condition(
            Some(&serde_json::json!("hello world")),
            FilterOperator::Contains,
            &serde_json::json!("world")
        ));

        // StartsWith
        assert!(evaluate_filter_condition(
            Some(&serde_json::json!("hello world")),
            FilterOperator::StartsWith,
            &serde_json::json!("hello")
        ));

        // EndsWith
        assert!(evaluate_filter_condition(
            Some(&serde_json::json!("hello world")),
            FilterOperator::EndsWith,
            &serde_json::json!("world")
        ));
    }

    #[test]
    fn test_filter_condition_array_contains() {
        use crate::schema::FilterOperator;

        assert!(evaluate_filter_condition(
            Some(&serde_json::json!(["a", "b", "c"])),
            FilterOperator::Contains,
            &serde_json::json!("b")
        ));

        assert!(!evaluate_filter_condition(
            Some(&serde_json::json!(["a", "b", "c"])),
            FilterOperator::Contains,
            &serde_json::json!("d")
        ));
    }

    #[test]
    fn test_filter_condition_null_handling() {
        use crate::schema::FilterOperator;

        // Missing value equals null
        assert!(evaluate_filter_condition(None, FilterOperator::Eq, &serde_json::Value::Null));

        // Missing value does not equal non-null
        assert!(!evaluate_filter_condition(
            None,
            FilterOperator::Eq,
            &serde_json::json!("value")
        ));
    }

    #[test]
    fn test_subscription_filter_matching() {
        use std::collections::HashMap;

        use crate::schema::{FilterOperator, StaticFilterCondition, SubscriptionFilter};

        let mut argument_paths = HashMap::new();
        argument_paths.insert("orderId".to_string(), "/id".to_string());

        let filter = SubscriptionFilter {
            argument_paths,
            static_filters: vec![StaticFilterCondition {
                path:     "/status".to_string(),
                operator: FilterOperator::Eq,
                value:    serde_json::json!("active"),
            }],
        };

        let schema = Arc::new(CompiledSchema {
            subscriptions: vec![
                SubscriptionDefinition::new("OrderUpdated", "Order")
                    .with_topic("order_updated")
                    .with_filter(filter),
            ],
            ..Default::default()
        });

        let manager = SubscriptionManager::new(schema);

        // Subscribe with a specific orderId
        manager
            .subscribe(
                "OrderUpdated",
                serde_json::json!({}),
                serde_json::json!({"orderId": "ord_123"}),
                "conn_1",
            )
            .unwrap();

        // Event matching the filter
        let matching_event = SubscriptionEvent::new(
            "Order",
            "ord_123",
            SubscriptionOperation::Update,
            serde_json::json!({"id": "ord_123", "status": "active"}),
        );
        assert_eq!(manager.publish_event(matching_event), 1);

        // Event with wrong orderId
        let wrong_id_event = SubscriptionEvent::new(
            "Order",
            "ord_456",
            SubscriptionOperation::Update,
            serde_json::json!({"id": "ord_456", "status": "active"}),
        );
        assert_eq!(manager.publish_event(wrong_id_event), 0);

        // Event with wrong status
        let wrong_status_event = SubscriptionEvent::new(
            "Order",
            "ord_123",
            SubscriptionOperation::Update,
            serde_json::json!({"id": "ord_123", "status": "inactive"}),
        );
        assert_eq!(manager.publish_event(wrong_status_event), 0);
    }

    #[test]
    fn test_subscription_field_projection() {
        let schema = Arc::new(CompiledSchema {
            subscriptions: vec![
                SubscriptionDefinition::new("OrderCreated", "Order")
                    .with_topic("order_created")
                    .with_fields(vec!["id".to_string(), "total".to_string()]),
            ],
            ..Default::default()
        });

        let manager = SubscriptionManager::new(schema);

        manager
            .subscribe("OrderCreated", serde_json::json!({}), serde_json::json!({}), "conn_1")
            .unwrap();

        let mut receiver = manager.receiver();

        let event = SubscriptionEvent::new(
            "Order",
            "ord_123",
            SubscriptionOperation::Create,
            serde_json::json!({
                "id": "ord_123",
                "total": 99.99,
                "secret_field": "should_not_appear",
                "customer": "John"
            }),
        );

        manager.publish_event(event);

        if let Ok(payload) = receiver.try_recv() {
            // Only projected fields should be present
            assert_eq!(payload.data.get("id"), Some(&serde_json::json!("ord_123")));
            assert_eq!(payload.data.get("total"), Some(&serde_json::json!(99.99)));
            assert!(payload.data.get("secret_field").is_none());
            assert!(payload.data.get("customer").is_none());
        }
    }
}
