//! Event delivery pipeline for the realtime broadcast system.
//!
//! Receives entity change events, groups subscriptions by security context hash,
//! evaluates RLS once per group, applies field filters, and delivers events to
//! authorized connections.

use std::{collections::HashMap, future::Future, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::{
    connections::{ConnectionId, ConnectionManager},
    subscriptions::{EventKind, FieldFilter, FilterOperator, SubscriptionManager},
};

/// An entity change event to be broadcast to subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityEvent {
    /// Entity name (e.g., `"Post"`).
    pub entity: String,
    /// Type of change.
    pub event_kind: EventKindSerde,
    /// New row data (present for INSERT and UPDATE).
    pub new: Option<Value>,
    /// Old row data (present for UPDATE and DELETE).
    pub old: Option<Value>,
    /// Event timestamp (ISO 8601).
    pub timestamp: String,
}

/// Serializable event kind for JSON wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventKindSerde {
    /// Row inserted.
    Insert,
    /// Row updated.
    Update,
    /// Row deleted.
    Delete,
}

impl EventKindSerde {
    /// Convert to the internal `EventKind` type.
    #[must_use]
    pub const fn to_event_kind(self) -> EventKind {
        match self {
            Self::Insert => EventKind::Insert,
            Self::Update => EventKind::Update,
            Self::Delete => EventKind::Delete,
        }
    }
}

impl From<EventKind> for EventKindSerde {
    fn from(kind: EventKind) -> Self {
        match kind {
            EventKind::Insert => Self::Insert,
            EventKind::Update => Self::Update,
            EventKind::Delete => Self::Delete,
        }
    }
}

/// Trait for evaluating row-level security on event delivery.
///
/// Implementations check whether a given security context hash is authorized
/// to see a specific row of a given entity.
pub trait RlsEvaluator: Send + Sync + 'static {
    /// Check if the given security context can access the row.
    ///
    /// Returns `true` if the row should be delivered, `false` if it should
    /// be silently dropped.
    fn can_access(
        &self,
        context_hash: u64,
        entity: &str,
        row: &Value,
    ) -> impl Future<Output = bool> + Send;
}

/// A change event formatted for delivery to a client.
#[derive(Debug, Clone, Serialize)]
pub struct ChangeMessage {
    /// Always `"change"`.
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    /// Entity name.
    pub entity: String,
    /// Event type (`INSERT`, `UPDATE`, `DELETE`).
    pub event: EventKindSerde,
    /// New row data.
    pub new: Option<Value>,
    /// Old row data.
    pub old: Option<Value>,
    /// Timestamp (ISO 8601).
    pub timestamp: String,
}

impl ChangeMessage {
    /// Create from an `EntityEvent`.
    #[must_use]
    pub fn from_event(event: &EntityEvent) -> Self {
        Self {
            msg_type: "change",
            entity: event.entity.clone(),
            event: event.event_kind,
            new: event.new.clone(),
            old: event.old.clone(),
            timestamp: event.timestamp.clone(),
        }
    }
}

/// Event delivery pipeline that processes entity events and delivers to subscribers.
pub struct EventDeliveryPipeline<R: RlsEvaluator> {
    /// Subscription manager for looking up who receives what.
    subscriptions: Arc<SubscriptionManager>,
    /// Connection manager for sending events to connections.
    connections: Arc<ConnectionManager>,
    /// RLS evaluator for access control.
    rls_evaluator: Arc<R>,
    /// Receiver for incoming entity events.
    event_rx: mpsc::Receiver<EntityEvent>,
}

impl<R: RlsEvaluator> EventDeliveryPipeline<R> {
    /// Create a new event delivery pipeline.
    pub const fn new(
        subscriptions: Arc<SubscriptionManager>,
        connections: Arc<ConnectionManager>,
        rls_evaluator: Arc<R>,
        event_rx: mpsc::Receiver<EntityEvent>,
    ) -> Self {
        Self {
            subscriptions,
            connections,
            rls_evaluator,
            event_rx,
        }
    }

    /// Run the delivery loop. Processes events until the channel is closed.
    pub async fn run(mut self) {
        while let Some(event) = self.event_rx.recv().await {
            self.deliver_event(&event).await;
        }
        debug!("Event delivery pipeline shutting down");
    }

    /// Deliver a single event to all authorized subscribers.
    async fn deliver_event(&self, event: &EntityEvent) {
        let event_kind = event.event_kind.to_event_kind();

        // Get all subscribers for this entity
        let Some(subscriber_details) = self.subscriptions.get_subscribers(&event.entity) else {
            return;
        };

        // Group subscribers by security context hash for RLS coalescing
        let mut groups: HashMap<u64, Vec<(ConnectionId, Vec<FieldFilter>)>> = HashMap::new();
        for (conn_id, details) in &subscriber_details {
            // Apply event type filter
            if let Some(filter_kind) = details.event_filter {
                if filter_kind != event_kind {
                    continue;
                }
            }
            groups
                .entry(details.security_context_hash)
                .or_default()
                .push((conn_id.clone(), details.field_filters.clone()));
        }

        // Determine the row to check for RLS (prefer `new`, fall back to `old`)
        let row = event.new.as_ref().or(event.old.as_ref());

        // Serialize the change message once (shared across all connections)
        let Ok(json) = serde_json::to_string(&ChangeMessage::from_event(event)) else {
            return;
        };

        // Evaluate RLS once per group, then deliver
        for (context_hash, connections) in &groups {
            // RLS check: can this security context see this row?
            if let Some(row) = row {
                if !self
                    .rls_evaluator
                    .can_access(*context_hash, &event.entity, row)
                    .await
                {
                    debug!(
                        entity = %event.entity,
                        context_hash = context_hash,
                        "RLS denied event delivery"
                    );
                    continue;
                }
            }

            // Deliver to each connection in this group
            for (conn_id, field_filters) in connections {
                // Apply field filters
                if !evaluate_field_filters(field_filters, row) {
                    continue;
                }

                if !self.connections.send_event(conn_id, json.clone()) {
                    warn!(
                        connection_id = %conn_id,
                        "Failed to send event to connection (channel full or closed)"
                    );
                }
            }
        }
    }
}

/// Evaluate field filters against a row.
///
/// Returns `true` if the row passes all filters (or if there are no filters).
pub fn evaluate_field_filters(filters: &[FieldFilter], row: Option<&Value>) -> bool {
    if filters.is_empty() {
        return true;
    }
    let Some(row) = row else {
        // No row data to filter against — pass through
        return true;
    };
    for filter in filters {
        let field_value = row.get(&filter.field);
        if !evaluate_single_filter(field_value, &filter.operator, &filter.value) {
            return false;
        }
    }
    true
}

/// Evaluate a single filter comparison.
fn evaluate_single_filter(
    field_value: Option<&Value>,
    operator: &FilterOperator,
    filter_value: &Value,
) -> bool {
    let Some(field_value) = field_value else {
        // Field not present in row — filter fails (except for Neq)
        return matches!(operator, FilterOperator::Neq);
    };

    match operator {
        FilterOperator::Eq => field_value == filter_value,
        FilterOperator::Neq => field_value != filter_value,
        FilterOperator::Gt => compare_values(field_value, filter_value).is_some_and(|o| o.is_gt()),
        FilterOperator::Lt => compare_values(field_value, filter_value).is_some_and(|o| o.is_lt()),
        FilterOperator::Gte => {
            compare_values(field_value, filter_value).is_some_and(|o| o.is_ge())
        }
        FilterOperator::Lte => {
            compare_values(field_value, filter_value).is_some_and(|o| o.is_le())
        }
        FilterOperator::In => {
            if let Value::Array(arr) = filter_value {
                arr.contains(field_value)
            } else {
                field_value == filter_value
            }
        }
    }
}

/// Compare two JSON values numerically if possible, otherwise as strings.
fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    // Try numeric comparison
    let a_num = value_as_f64(a);
    let b_num = value_as_f64(b);
    if let (Some(a_f), Some(b_f)) = (a_num, b_num) {
        return a_f.partial_cmp(&b_f);
    }

    // Fall back to string comparison
    let a_str = a.as_str().or_else(|| if a.is_number() { None } else { Some("") });
    let b_str = b.as_str().or_else(|| if b.is_number() { None } else { Some("") });
    match (a_str, b_str) {
        (Some(a_s), Some(b_s)) => Some(a_s.cmp(b_s)),
        _ => None,
    }
}

/// Try to extract a float from a JSON value.
fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}
