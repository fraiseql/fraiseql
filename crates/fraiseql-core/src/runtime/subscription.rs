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

use crate::schema::{CompiledSchema, SubscriptionDefinition};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;
use futures::future::poll_fn;
use tokio_postgres::{AsyncMessage, NoTls};
use uuid::Uuid;

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
        if let Some(mut subs) = self
            .subscriptions_by_connection
            .get_mut(&removed.1.connection_id)
        {
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
        if let Some((_, subscription_ids)) =
            self.subscriptions_by_connection.remove(connection_id)
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
                }
                t if t.contains("updated") || t.contains("update") => {
                    Some(SubscriptionOperation::Update)
                }
                t if t.contains("deleted") || t.contains("delete") => {
                    Some(SubscriptionOperation::Delete)
                }
                _ => None,
            };

            if let Some(expected) = expected_op {
                if event.operation != expected {
                    return false;
                }
            }
        }

        // TODO: Evaluate compiled WHERE filters against event.data and user_context
        // For now, we match on entity type and topic only

        true
    }

    /// Project event data to subscription's field selection.
    fn project_event_data(
        &self,
        event: &SubscriptionEvent,
        _subscription: &ActiveSubscription,
    ) -> serde_json::Value {
        // TODO: Project only requested fields from subscription definition
        // For now, return full event data
        event.data.clone()
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
// PostgreSQL LISTEN/NOTIFY Listener
// =============================================================================

/// Configuration for the PostgreSQL notification listener.
#[derive(Debug, Clone)]
pub struct ListenerConfig {
    /// PostgreSQL connection string.
    pub connection_string: String,

    /// Channel name to listen on (default: "fraiseql_events").
    pub channel_name: String,

    /// Whether to reconnect on connection loss.
    pub auto_reconnect: bool,

    /// Reconnection delay in milliseconds.
    pub reconnect_delay_ms: u64,

    /// Maximum reconnection attempts (0 = unlimited).
    pub max_reconnect_attempts: u32,
}

impl ListenerConfig {
    /// Create a new listener configuration.
    #[must_use]
    pub fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
            channel_name: "fraiseql_events".to_string(),
            auto_reconnect: true,
            reconnect_delay_ms: 1000,
            max_reconnect_attempts: 0,
        }
    }

    /// Set the channel name to listen on.
    #[must_use]
    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channel_name = channel.into();
        self
    }

    /// Disable automatic reconnection.
    #[must_use]
    pub fn without_auto_reconnect(mut self) -> Self {
        self.auto_reconnect = false;
        self
    }

    /// Set reconnection delay.
    #[must_use]
    pub fn with_reconnect_delay(mut self, delay_ms: u64) -> Self {
        self.reconnect_delay_ms = delay_ms;
        self
    }

    /// Set maximum reconnection attempts.
    #[must_use]
    pub fn with_max_reconnect_attempts(mut self, max_attempts: u32) -> Self {
        self.max_reconnect_attempts = max_attempts;
        self
    }
}

/// PostgreSQL LISTEN/NOTIFY listener for subscription events.
///
/// This listener connects to PostgreSQL and subscribes to database notifications
/// on a configured channel. When notifications are received, they are parsed
/// as subscription events and published to the `SubscriptionManager`.
///
/// # Expected Notification Format
///
/// Notifications should be JSON with the following structure:
///
/// ```json
/// {
///   "entity_type": "Order",
///   "entity_id": "ord_123",
///   "operation": "CREATE",
///   "data": { "id": "ord_123", "amount": 99.99 }
/// }
/// ```
///
/// # Example
///
/// ```ignore
/// use fraiseql_core::runtime::subscription::{
///     PostgresListener, ListenerConfig, SubscriptionManager,
/// };
///
/// let config = ListenerConfig::new("postgresql://localhost/mydb")
///     .with_channel("fraiseql_events");
///
/// let manager = Arc::new(SubscriptionManager::new(schema));
/// let listener = PostgresListener::new(config, manager);
///
/// // Start listening (runs in background)
/// let handle = listener.start().await?;
///
/// // Stop listening
/// handle.stop().await;
/// ```
pub struct PostgresListener {
    /// Listener configuration.
    config: ListenerConfig,

    /// Reference to subscription manager for event publishing.
    manager: Arc<SubscriptionManager>,

    /// Shutdown signal sender.
    shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,

    /// Whether the listener is currently running.
    running: std::sync::atomic::AtomicBool,
}

impl PostgresListener {
    /// Create a new PostgreSQL listener.
    #[must_use]
    pub fn new(config: ListenerConfig, manager: Arc<SubscriptionManager>) -> Self {
        Self {
            config,
            manager,
            shutdown_tx: None,
            running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Start the listener in the background.
    ///
    /// Returns a handle that can be used to stop the listener.
    ///
    /// # Errors
    ///
    /// Returns error if listener is already running or connection fails.
    pub async fn start(mut self) -> Result<ListenerHandle, SubscriptionError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(SubscriptionError::ListenerAlreadyRunning);
        }

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx.clone());
        self.running.store(true, Ordering::SeqCst);

        let config = self.config.clone();
        let manager = self.manager.clone();
        let running = Arc::new(self.running);

        // Spawn the listener task
        let task_handle = tokio::spawn(async move {
            Self::listen_loop(config, manager, shutdown_rx, running).await;
        });

        Ok(ListenerHandle {
            shutdown_tx,
            task_handle,
        })
    }

    /// Main listening loop with reconnection logic.
    async fn listen_loop(
        config: ListenerConfig,
        manager: Arc<SubscriptionManager>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
        running: Arc<std::sync::atomic::AtomicBool>,
    ) {
        let mut reconnect_attempts = 0;

        loop {
            // Check for shutdown
            if *shutdown_rx.borrow() {
                tracing::info!("Listener shutdown requested");
                break;
            }

            // Connect to PostgreSQL
            match Self::connect_and_listen(&config, &manager, &mut shutdown_rx).await {
                Ok(()) => {
                    // Clean exit (shutdown requested)
                    break;
                }
                Err(e) => {
                    tracing::error!(error = %e, "Listener connection error");

                    if !config.auto_reconnect {
                        tracing::info!("Auto-reconnect disabled, stopping listener");
                        break;
                    }

                    reconnect_attempts += 1;
                    if config.max_reconnect_attempts > 0
                        && reconnect_attempts >= config.max_reconnect_attempts
                    {
                        tracing::error!(
                            attempts = reconnect_attempts,
                            "Max reconnection attempts reached, stopping listener"
                        );
                        break;
                    }

                    tracing::info!(
                        delay_ms = config.reconnect_delay_ms,
                        attempt = reconnect_attempts,
                        "Reconnecting in {}ms",
                        config.reconnect_delay_ms
                    );

                    // Wait before reconnecting (or shutdown)
                    tokio::select! {
                        () = tokio::time::sleep(std::time::Duration::from_millis(config.reconnect_delay_ms)) => {},
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                break;
                            }
                        }
                    }
                }
            }
        }

        running.store(false, Ordering::SeqCst);
        tracing::info!("Listener stopped");
    }

    /// Connect to PostgreSQL and listen for notifications.
    async fn connect_and_listen(
        config: &ListenerConfig,
        manager: &SubscriptionManager,
        shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), SubscriptionError> {
        // Connect to PostgreSQL
        let (client, mut connection) =
            tokio_postgres::connect(&config.connection_string, NoTls)
                .await
                .map_err(|e| SubscriptionError::DatabaseConnection(e.to_string()))?;

        tracing::info!(
            channel = config.channel_name,
            "Connected to PostgreSQL, subscribing to channel"
        );

        // Create message stream from connection
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        // Spawn connection driver that forwards messages
        let conn_handle = tokio::spawn(async move {
            loop {
                match poll_fn(|cx| connection.poll_message(cx)).await {
                    Some(Ok(msg)) => {
                        if tx.send(msg).is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        tracing::error!(error = %e, "Connection error");
                        break;
                    }
                    None => break,
                }
            }
        });

        // Subscribe to channel
        let listen_cmd = format!("LISTEN {}", config.channel_name);
        client.batch_execute(&listen_cmd).await.map_err(|e| {
            SubscriptionError::DatabaseConnection(format!("Failed to execute LISTEN: {e}"))
        })?;

        tracing::info!(channel = config.channel_name, "Listening for notifications");

        // Main message loop
        loop {
            tokio::select! {
                // Check for shutdown
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::info!("Shutdown signal received");
                        // Gracefully disconnect
                        let _ = client.batch_execute(&format!("UNLISTEN {}", config.channel_name)).await;
                        conn_handle.abort();
                        return Ok(());
                    }
                }

                // Process messages from connection
                msg = rx.recv() => {
                    match msg {
                        Some(AsyncMessage::Notification(notification)) => {
                            tracing::debug!(
                                channel = notification.channel(),
                                payload_len = notification.payload().len(),
                                "Received notification"
                            );

                            // Parse and publish event
                            if let Err(e) = Self::process_notification(notification.payload(), manager) {
                                tracing::warn!(
                                    error = %e,
                                    payload = notification.payload(),
                                    "Failed to process notification"
                                );
                            }
                        }
                        Some(AsyncMessage::Notice(notice)) => {
                            tracing::debug!(
                                message = %notice.message(),
                                severity = %notice.severity(),
                                "Received notice"
                            );
                        }
                        Some(_) => {
                            // Other message types (we don't expect these)
                        }
                        None => {
                            // Channel closed, connection lost
                            tracing::warn!("Connection to PostgreSQL lost");
                            conn_handle.abort();
                            return Err(SubscriptionError::DatabaseConnection(
                                "Connection lost".to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    /// Parse a notification payload and publish to the subscription manager.
    fn process_notification(
        payload: &str,
        manager: &SubscriptionManager,
    ) -> Result<(), SubscriptionError> {
        // Parse the JSON payload
        let notification: NotificationPayload = serde_json::from_str(payload)
            .map_err(|e| SubscriptionError::InvalidNotification(e.to_string()))?;

        // Convert to subscription event
        let operation = match notification.operation.to_uppercase().as_str() {
            "CREATE" | "INSERT" => SubscriptionOperation::Create,
            "UPDATE" => SubscriptionOperation::Update,
            "DELETE" => SubscriptionOperation::Delete,
            op => {
                return Err(SubscriptionError::InvalidNotification(format!(
                    "Unknown operation: {op}"
                )));
            }
        };

        let mut event = SubscriptionEvent::new(
            notification.entity_type,
            notification.entity_id,
            operation,
            notification.data,
        );

        // Add old_data if present (for UPDATE operations)
        if let Some(old_data) = notification.old_data {
            event = event.with_old_data(old_data);
        }

        // Publish to manager
        let matched = manager.publish_event(event);
        tracing::debug!(matched = matched, "Published event to subscribers");

        Ok(())
    }
}

impl std::fmt::Debug for PostgresListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresListener")
            .field("channel", &self.config.channel_name)
            .field("running", &self.running.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

/// Handle for controlling a running listener.
pub struct ListenerHandle {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    task_handle: tokio::task::JoinHandle<()>,
}

impl ListenerHandle {
    /// Stop the listener gracefully.
    pub async fn stop(self) {
        // Send shutdown signal
        let _ = self.shutdown_tx.send(true);

        // Wait for task to complete (with timeout)
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.task_handle,
        )
        .await
        {
            Ok(Ok(())) => tracing::info!("Listener stopped gracefully"),
            Ok(Err(e)) => tracing::error!(error = %e, "Listener task panicked"),
            Err(_) => {
                tracing::warn!("Listener stop timed out, aborting task");
                // Task will be aborted when handle is dropped
            }
        }
    }

    /// Check if the listener is still running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        !self.task_handle.is_finished()
    }
}

/// Internal payload format for PostgreSQL notifications.
#[derive(Debug, Deserialize)]
struct NotificationPayload {
    /// Entity type name (e.g., "Order").
    entity_type: String,

    /// Entity primary key.
    entity_id: String,

    /// Operation type ("CREATE", "UPDATE", "DELETE").
    operation: String,

    /// Current row data.
    data: serde_json::Value,

    /// Previous row data (for UPDATE operations).
    #[serde(default)]
    old_data: Option<serde_json::Value>,
}

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
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

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
            self.payload
                .as_ref()
                .and_then(|p| serde_json::from_value(p.clone()).ok())
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
                id: Some(id.into()),
                payload: Some(serde_json::json!({ "data": data })),
            }
        }

        /// Create error message.
        #[must_use]
        pub fn error(id: impl Into<String>, errors: Vec<GraphQLError>) -> Self {
            Self {
                message_type: ServerMessageType::Error.as_str().to_string(),
                id: Some(id.into()),
                payload: Some(serde_json::to_value(errors).unwrap_or_default()),
            }
        }

        /// Create complete message.
        #[must_use]
        pub fn complete(id: impl Into<String>) -> Self {
            Self {
                message_type: ServerMessageType::Complete.as_str().to_string(),
                id: Some(id.into()),
                payload: None,
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
                message: message.into(),
                locations: None,
                path: None,
                extensions: None,
            }
        }

        /// Create an error with code extension.
        #[must_use]
        pub fn with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
            let mut extensions = HashMap::new();
            extensions.insert("code".to_string(), serde_json::json!(code.into()));

            Self {
                message: message.into(),
                locations: None,
                path: None,
                extensions: Some(extensions),
            }
        }
    }

    /// Error location in query.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ErrorLocation {
        /// Line number (1-indexed).
        pub line: u32,
        /// Column number (1-indexed).
        pub column: u32,
    }

    /// Close codes for WebSocket connection.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CloseCode {
        /// Normal closure.
        Normal = 1000,
        /// Client violated protocol.
        ProtocolError = 1002,
        /// Internal server error.
        InternalError = 1011,
        /// Connection initialization timeout.
        ConnectionInitTimeout = 4408,
        /// Too many initialization requests.
        TooManyInitRequests = 4429,
        /// Subscriber already exists (duplicate ID).
        SubscriberAlreadyExists = 4409,
        /// Unauthorized.
        Unauthorized = 4401,
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
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SubscriptionDefinition;

    fn create_test_schema() -> CompiledSchema {
        CompiledSchema {
            subscriptions: vec![
                SubscriptionDefinition::new("OrderCreated", "Order")
                    .with_topic("order_created"),
                SubscriptionDefinition::new("OrderUpdated", "Order")
                    .with_topic("order_updated"),
                SubscriptionDefinition::new("UserDeleted", "User")
                    .with_topic("user_deleted"),
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

        assert!(matches!(
            result,
            Err(SubscriptionError::SubscriptionNotFound(_))
        ));
    }

    #[test]
    fn test_subscription_manager_unsubscribe() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        let id = manager
            .subscribe(
                "OrderCreated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
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
            .subscribe(
                "OrderCreated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
            .unwrap();

        manager
            .subscribe(
                "OrderUpdated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
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
            .subscribe(
                "OrderCreated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
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
            .subscribe(
                "OrderCreated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
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
            .subscribe(
                "OrderCreated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
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
    // Listener Configuration Tests
    // =========================================================================

    #[test]
    fn test_listener_config_defaults() {
        let config = ListenerConfig::new("postgresql://localhost/mydb");

        assert_eq!(config.connection_string, "postgresql://localhost/mydb");
        assert_eq!(config.channel_name, "fraiseql_events");
        assert!(config.auto_reconnect);
        assert_eq!(config.reconnect_delay_ms, 1000);
        assert_eq!(config.max_reconnect_attempts, 0);
    }

    #[test]
    fn test_listener_config_builder() {
        let config = ListenerConfig::new("postgresql://localhost/mydb")
            .with_channel("custom_channel")
            .without_auto_reconnect()
            .with_reconnect_delay(5000)
            .with_max_reconnect_attempts(3);

        assert_eq!(config.channel_name, "custom_channel");
        assert!(!config.auto_reconnect);
        assert_eq!(config.reconnect_delay_ms, 5000);
        assert_eq!(config.max_reconnect_attempts, 3);
    }

    // =========================================================================
    // Notification Processing Tests
    // =========================================================================

    #[test]
    fn test_process_notification_create() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        manager
            .subscribe(
                "OrderCreated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
            .unwrap();

        let payload = r#"{
            "entity_type": "Order",
            "entity_id": "ord_123",
            "operation": "CREATE",
            "data": {"id": "ord_123", "amount": 99.99}
        }"#;

        let result = PostgresListener::process_notification(payload, &manager);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_notification_update_with_old_data() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        manager
            .subscribe(
                "OrderUpdated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
            .unwrap();

        let payload = r#"{
            "entity_type": "Order",
            "entity_id": "ord_123",
            "operation": "UPDATE",
            "data": {"id": "ord_123", "amount": 199.99},
            "old_data": {"id": "ord_123", "amount": 99.99}
        }"#;

        let result = PostgresListener::process_notification(payload, &manager);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_notification_insert_operation() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        manager
            .subscribe(
                "OrderCreated",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
            .unwrap();

        // "INSERT" should be treated as CREATE
        let payload = r#"{
            "entity_type": "Order",
            "entity_id": "ord_123",
            "operation": "INSERT",
            "data": {"id": "ord_123"}
        }"#;

        let result = PostgresListener::process_notification(payload, &manager);
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_notification_invalid_json() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        let result = PostgresListener::process_notification("not valid json", &manager);
        assert!(matches!(
            result,
            Err(SubscriptionError::InvalidNotification(_))
        ));
    }

    #[test]
    fn test_process_notification_unknown_operation() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        let payload = r#"{
            "entity_type": "Order",
            "entity_id": "ord_123",
            "operation": "UNKNOWN",
            "data": {"id": "ord_123"}
        }"#;

        let result = PostgresListener::process_notification(payload, &manager);
        assert!(matches!(
            result,
            Err(SubscriptionError::InvalidNotification(_))
        ));
    }

    #[test]
    fn test_process_notification_delete() {
        let schema = Arc::new(create_test_schema());
        let manager = SubscriptionManager::new(schema);

        manager
            .subscribe(
                "UserDeleted",
                serde_json::json!({}),
                serde_json::json!({}),
                "conn_1",
            )
            .unwrap();

        let payload = r#"{
            "entity_type": "User",
            "entity_id": "usr_123",
            "operation": "DELETE",
            "data": {"id": "usr_123"}
        }"#;

        let result = PostgresListener::process_notification(payload, &manager);
        assert!(result.is_ok());
    }
}
