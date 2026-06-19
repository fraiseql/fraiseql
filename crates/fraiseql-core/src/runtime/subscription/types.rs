use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::SubscriptionDefinition;

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
    pub const fn from_uuid(uuid: Uuid) -> Self {
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
///
/// Marked `#[non_exhaustive]` to allow future CDC operations (e.g., `Truncate`)
/// to be added without breaking downstream `match` expressions.
#[non_exhaustive]
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

/// Change-Spine envelope metadata delivered to subscription clients alongside the
/// resolved entity data (#425).
///
/// Carries the audit / provenance context the Change-Spine records for every
/// change — who made it (`actor_type`), who a delegated agent acted for
/// (`acting_for`), the producer `schema_version`, the `tenant_id`, the mutation
/// `duration_ms`, and the durable `seq`. Serialized camelCase into the subscription
/// `next` payload's `extensions.changeSpine` slot; `None` fields are omitted, so a
/// producer that stamped nothing yields an empty object (see [`Self::is_empty`],
/// which callers use to skip emitting it entirely).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSpineEnvelope {
    /// Actor classification of the request that produced the change:
    /// `"human_user"`, `"service_account"`, `"ai_agent"`, or `"system_job"` (#390).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,

    /// For a delegated-agent request (RFC 8693 `act` claim), the public-facing UUID
    /// of the underlying human the agent acts for (#390).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acting_for: Option<String>,

    /// Schema version of the producer that wrote the change (#377).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,

    /// Public-facing tenant UUID stamp (a display copy of the event's `tenant_id`;
    /// server-side tenant filtering uses the event's own field, not this).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Wall-clock duration of the originating mutation, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i32>,

    /// Monotonic Change-Spine sequence for durable ordering / dedup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
}

impl ChangeSpineEnvelope {
    /// True when no envelope field is set — callers skip emitting an empty object.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.actor_type.is_none()
            && self.acting_for.is_none()
            && self.schema_version.is_none()
            && self.tenant_id.is_none()
            && self.duration_ms.is_none()
            && self.seq.is_none()
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

    /// Tenant identifier for multi-tenant isolation (from `fk_customer_org`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Change-Spine envelope metadata for client delivery (#425). Carried for the
    /// subscription `next` payload's `extensions.changeSpine`; not used for
    /// server-side filtering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_spine: Option<ChangeSpineEnvelope>,
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
            tenant_id: None,
            change_spine: None,
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
    pub const fn with_sequence(mut self, seq: u64) -> Self {
        self.sequence_number = seq;
        self
    }

    /// Set the tenant identifier for multi-tenant filtering.
    #[must_use]
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Attach the Change-Spine envelope for client delivery (#425).
    #[must_use]
    pub fn with_change_spine(mut self, envelope: ChangeSpineEnvelope) -> Self {
        self.change_spine = Some(envelope);
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

    /// Row-level security conditions evaluated at subscribe time.
    ///
    /// Each entry is `(field_path, expected_value)`. An event is only delivered
    /// when **every** condition matches the event data (AND semantics).
    /// An empty list means no RLS filtering (admin or no RLS policy).
    pub rls_conditions: Vec<(String, serde_json::Value)>,

    /// Tenant identifier for multi-tenant isolation.
    ///
    /// When set, only events with a matching `tenant_id` are delivered.
    /// Extracted from the subscriber's JWT `fk_customer_org` claim at subscribe time.
    pub tenant_id: Option<String>,
}

impl ActiveSubscription {
    /// Create a new active subscription.
    ///
    /// # Arguments
    ///
    /// * `subscription_name` - Schema subscription name
    /// * `definition` - Subscription definition from compiled schema
    /// * `user_context` - Raw user context JSON from `WebSocket` `connection_init`
    /// * `variables` - Runtime variables from client
    /// * `connection_id` - Client connection identifier
    #[must_use]
    pub fn new(
        subscription_name: impl Into<String>,
        definition: SubscriptionDefinition,
        user_context: serde_json::Value,
        variables: serde_json::Value,
        connection_id: impl Into<String>,
    ) -> Self {
        // Extract tenant_id from user_context if present
        let tenant_id = user_context.get("tenant_id").and_then(|v| v.as_str()).map(str::to_string);

        Self {
            id: SubscriptionId::new(),
            subscription_name: subscription_name.into(),
            definition,
            user_context,
            variables,
            created_at: chrono::Utc::now(),
            connection_id: connection_id.into(),
            rls_conditions: Vec::new(),
            tenant_id,
        }
    }

    /// Set row-level security conditions for event filtering.
    ///
    /// The caller evaluates the RLS policy against the user's `SecurityContext`
    /// at subscribe time and converts the resulting `WhereClause` into
    /// `(field, value)` equality conditions. During event delivery,
    /// `matches_subscription` checks every condition against the event data.
    #[must_use]
    pub fn with_rls_conditions(mut self, conditions: Vec<(String, serde_json::Value)>) -> Self {
        self.rls_conditions = conditions;
        self
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
