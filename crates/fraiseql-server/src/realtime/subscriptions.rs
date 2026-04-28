//! Subscription manager for tracking entity change subscriptions.
//!
//! Uses a two-level index for O(1) fan-out (entity → connections) and
//! O(1) per-connection cleanup (connection → subscriptions).

use std::collections::HashMap;

use dashmap::{DashMap, DashSet};
use serde_json::Value;

use super::connections::ConnectionId;

/// Event kind for filtering subscription events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// `INSERT` — new row created.
    Insert,
    /// `UPDATE` — existing row modified.
    Update,
    /// `DELETE` — row removed.
    Delete,
}

impl EventKind {
    /// Parse an event kind from a string.
    ///
    /// # Errors
    ///
    /// Returns an error message if the string is not a recognized event kind.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_uppercase().as_str() {
            "INSERT" => Ok(Self::Insert),
            "UPDATE" => Ok(Self::Update),
            "DELETE" => Ok(Self::Delete),
            other => Err(format!("unknown event kind: {other}")),
        }
    }
}

/// Comparison operator for field filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterOperator {
    /// Equal (`eq`).
    Eq,
    /// Not equal (`neq`).
    Neq,
    /// Greater than (`gt`).
    Gt,
    /// Less than (`lt`).
    Lt,
    /// Greater than or equal (`gte`).
    Gte,
    /// Less than or equal (`lte`).
    Lte,
    /// In a set of values (`in`).
    In,
}

/// A single field-level filter on subscription events.
#[derive(Debug, Clone)]
pub struct FieldFilter {
    /// Field name to compare.
    pub field: String,
    /// Comparison operator.
    pub operator: FilterOperator,
    /// Value to compare against.
    pub value: Value,
}

impl FilterOperator {
    /// Parse an operator from its string representation.
    ///
    /// # Errors
    ///
    /// Returns an error message if the string is not a recognized operator.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "eq" => Ok(Self::Eq),
            "neq" => Ok(Self::Neq),
            "gt" => Ok(Self::Gt),
            "lt" => Ok(Self::Lt),
            "gte" => Ok(Self::Gte),
            "lte" => Ok(Self::Lte),
            "in" => Ok(Self::In),
            other => Err(format!("unknown filter operator: {other}")),
        }
    }
}

/// Parse a filter value, coercing to number when possible.
fn parse_filter_value(s: &str) -> Value {
    if let Ok(n) = s.parse::<i64>() {
        Value::Number(n.into())
    } else if let Ok(f) = s.parse::<f64>() {
        serde_json::Number::from_f64(f)
            .map_or_else(|| Value::String(s.to_owned()), Value::Number)
    } else {
        Value::String(s.to_owned())
    }
}

/// Parse a filter string in `field=op.value` format.
///
/// Multiple filters can be comma-separated: `"author_id=eq.123,status=neq.draft"`.
///
/// # Errors
///
/// Returns an error message if the filter string is malformed.
pub fn parse_filter(filter_str: &str) -> Result<Vec<FieldFilter>, String> {
    filter_str
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|part| {
            let (field, rest) = part
                .split_once('=')
                .ok_or_else(|| format!("invalid filter syntax: {part}"))?;
            let (op_str, value_str) = rest
                .split_once('.')
                .ok_or_else(|| format!("invalid filter operator: {rest}"))?;
            Ok(FieldFilter {
                field: field.to_owned(),
                operator: FilterOperator::parse(op_str)?,
                value: parse_filter_value(value_str),
            })
        })
        .collect()
}

/// Details for a single subscription held by a connection.
#[derive(Debug, Clone)]
pub struct SubscriptionDetails {
    /// Optional event type filter (None = all events).
    pub event_filter: Option<EventKind>,
    /// Field-level filters applied to event payloads.
    pub field_filters: Vec<FieldFilter>,
    /// Security context hash for RLS grouping.
    pub security_context_hash: u64,
}

/// Thread-safe subscription manager with two-level indexing.
///
/// Level 1: entity → set of connection IDs (for fan-out).
/// Level 2: connection → map of entity → subscription details (for per-connection state).
pub struct SubscriptionManager {
    /// entity → set of connection IDs subscribed to it.
    entity_subscribers: DashMap<String, DashSet<ConnectionId>>,
    /// `connection_id` → (entity → subscription details).
    connection_subscriptions: DashMap<ConnectionId, HashMap<String, SubscriptionDetails>>,
    /// Maximum subscriptions per entity (fan-out limit).
    max_per_entity: usize,
}

impl SubscriptionManager {
    /// Create a new subscription manager with the given fan-out limit.
    #[must_use]
    pub fn new(max_per_entity: usize) -> Self {
        Self {
            entity_subscribers: DashMap::new(),
            connection_subscriptions: DashMap::new(),
            max_per_entity,
        }
    }

    /// Subscribe a connection to an entity.
    ///
    /// Returns `Ok(true)` if this is a new subscription, `Ok(false)` if the
    /// connection was already subscribed (idempotent).
    ///
    /// # Errors
    ///
    /// Returns an error if the fan-out limit for this entity is reached.
    pub fn subscribe(
        &self,
        connection_id: &str,
        entity: &str,
        details: SubscriptionDetails,
    ) -> Result<bool, String> {
        // Check if already subscribed (idempotent)
        if let Some(subs) = self.connection_subscriptions.get(connection_id) {
            if subs.contains_key(entity) {
                return Ok(false);
            }
        }

        // Check fan-out limit
        let current_count = self
            .entity_subscribers
            .get(entity)
            .map_or(0, |set| set.len());
        if current_count >= self.max_per_entity {
            return Err(format!(
                "subscription limit reached for entity {entity} ({} max)",
                self.max_per_entity
            ));
        }

        // Add to entity → connections index
        self.entity_subscribers
            .entry(entity.to_owned())
            .or_default()
            .insert(connection_id.to_owned());

        // Add to connection → subscriptions index
        self.connection_subscriptions
            .entry(connection_id.to_owned())
            .or_default()
            .insert(entity.to_owned(), details);

        Ok(true)
    }

    /// Unsubscribe a connection from an entity.
    ///
    /// Returns `true` if the subscription existed and was removed.
    pub fn unsubscribe(&self, connection_id: &str, entity: &str) -> bool {
        // Remove from connection → subscriptions
        let had_sub = self
            .connection_subscriptions
            .get_mut(connection_id)
            .is_some_and(|mut subs| subs.remove(entity).is_some());

        if had_sub {
            // Remove from entity → connections
            if let Some(set) = self.entity_subscribers.get(entity) {
                set.remove(connection_id);
            }
        }

        had_sub
    }

    /// Remove all subscriptions for a connection (called on disconnect).
    pub fn unsubscribe_all(&self, connection_id: &str) {
        if let Some((_, subs)) = self.connection_subscriptions.remove(connection_id) {
            for entity in subs.keys() {
                if let Some(set) = self.entity_subscribers.get(entity) {
                    set.remove(connection_id);
                }
            }
        }
    }

    /// Number of subscriptions for a given entity.
    #[must_use]
    pub fn count_for_entity(&self, entity: &str) -> usize {
        self.entity_subscribers
            .get(entity)
            .map_or(0, |set| set.len())
    }

    /// Number of entities a connection is subscribed to.
    #[must_use]
    pub fn count_for_connection(&self, connection_id: &str) -> usize {
        self.connection_subscriptions
            .get(connection_id)
            .map_or(0, |subs| subs.len())
    }
}
