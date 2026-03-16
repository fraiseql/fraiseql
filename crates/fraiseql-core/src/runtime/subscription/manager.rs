use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use dashmap::DashMap;
use tokio::sync::broadcast;

#[allow(clippy::wildcard_imports)]
// Reason: types::* re-exports the subscription type vocabulary used throughout this module
use super::{SubscriptionError, types::*};
use crate::schema::CompiledSchema;

// =============================================================================
// Subscription Manager
// =============================================================================

/// Maximum number of active subscriptions a single connection may hold.
///
/// Prevents a single authenticated connection from exhausting server memory by
/// calling `subscribe()` in a loop.
const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 100;

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
        let mut definition = self
            .schema
            .find_subscription(subscription_name)
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(subscription_name.to_string()))?
            .clone();

        // Expand filter_fields into argument_paths on the filter.
        // Each filter_field name becomes an argument_path entry mapping
        // the field name to a JSON pointer path (e.g., "user_id" → "/user_id").
        if !definition.filter_fields.is_empty() {
            let filter =
                definition.filter.get_or_insert_with(|| crate::schema::SubscriptionFilter {
                    argument_paths: std::collections::HashMap::new(),
                    static_filters: Vec::new(),
                });
            for field in &definition.filter_fields {
                filter
                    .argument_paths
                    .entry(field.clone())
                    .or_insert_with(|| format!("/{field}"));
            }
        }

        // Create active subscription
        let active = ActiveSubscription::new(
            subscription_name,
            definition,
            user_context,
            variables,
            connection_id,
        );

        let id = active.id;

        // Enforce per-connection subscription cap before inserting.
        {
            let mut conn_subs =
                self.subscriptions_by_connection.entry(connection_id.to_string()).or_default();
            if conn_subs.len() >= MAX_SUBSCRIPTIONS_PER_CONNECTION {
                return Err(SubscriptionError::Internal(format!(
                    "Connection '{connection_id}' has reached the maximum of \
                     {MAX_SUBSCRIPTIONS_PER_CONNECTION} concurrent subscriptions"
                )));
            }
            conn_subs.push(id);
        }

        // Store subscription
        self.subscriptions.insert(id, active);

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
    ///
    /// # Concurrency note
    ///
    /// A concurrent `subscribe` call that runs between the DashMap entry removal and the
    /// per-subscription cleanup loop would create a new connection entry that is not cleaned
    /// up by this call. A second-pass removal after the first loop closes this window for
    /// all but the most extreme concurrent races. Any subscription that slips through is
    /// benign: it will receive events until the broadcast receiver is dropped (which happens
    /// on disconnect), and will be removed on the next disconnect or subscription-not-found
    /// event for that ID.
    pub fn unsubscribe_connection(&self, connection_id: &str) {
        // First pass: remove the connection index atomically and clean up known subscriptions.
        let first_pass_count = if let Some((_, subscription_ids)) =
            self.subscriptions_by_connection.remove(connection_id)
        {
            let count = subscription_ids.len();
            for id in subscription_ids {
                self.subscriptions.remove(&id);
            }
            count
        } else {
            0
        };

        // Second pass: clean up any subscriptions added by a concurrent `subscribe` call that
        // ran between the `remove()` above and the loop.  A concurrent `subscribe` that saw
        // the connection entry absent would have inserted a *new* entry; removing it here
        // closes the TOCTOU window to a negligible two-CAS race.
        let second_pass_count = if let Some((_, subscription_ids)) =
            self.subscriptions_by_connection.remove(connection_id)
        {
            let count = subscription_ids.len();
            for id in subscription_ids {
                self.subscriptions.remove(&id);
                tracing::warn!(
                    subscription_id = %id,
                    connection_id = connection_id,
                    "Cleaned up subscription added concurrently during disconnect"
                );
            }
            count
        } else {
            0
        };

        tracing::info!(
            connection_id = connection_id,
            subscriptions_removed = first_pass_count + second_pass_count,
            "All subscriptions removed for connection"
        );
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
        for subscription in &self.subscriptions {
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
/// ```rust
/// # use serde_json::json;
/// # fn get_json_pointer_value<'a>(data: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
/// #     let normalized = if path.starts_with('/') { path.to_string() } else { format!("/{}", path.replace('.', "/")) };
/// #     data.pointer(&normalized)
/// # }
/// let data = json!({"user": {"id": 123, "name": "Alice"}});
/// let id = get_json_pointer_value(&data, "user/id");  // Some(&123)
/// let alt = get_json_pointer_value(&data, "user.id"); // Some(&123)
/// let missing = get_json_pointer_value(&data, "admin/id"); // None
/// ```
pub fn get_json_pointer_value<'a>(
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
pub fn evaluate_filter_condition(
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
