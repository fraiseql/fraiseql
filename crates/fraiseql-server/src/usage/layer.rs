//! Tracing subscriber layer that captures `fraiseql::mutation_audit` events.
//!
//! Install this layer alongside your normal subscriber to automatically feed
//! the [`UsageAggregator`] from every mutation executed by the runtime:
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use tracing_subscriber::{layer::SubscriberExt, Registry};
//! use fraiseql_server::usage::{aggregator::UsageAggregator, layer::MutationAuditLayer};
//!
//! let aggregator = Arc::new(UsageAggregator::new());
//! let subscriber = Registry::default()
//!     .with(MutationAuditLayer::new(Arc::clone(&aggregator)));
//! tracing::subscriber::set_global_default(subscriber).unwrap();
//! ```

use std::sync::Arc;

use chrono::Utc;
use tracing::{Event, Subscriber, field::{Field, Visit}};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use super::{aggregator::UsageAggregator, events::MutationAuditEvent};

/// The tracing target emitted by the FraiseQL mutation executor.
const MUTATION_AUDIT_TARGET: &str = "fraiseql::mutation_audit";

// ── Field visitor ──────────────────────────────────────────────────────────

/// Extracts the four string fields from a `fraiseql::mutation_audit` event.
struct AuditFieldVisitor {
    mutation_name: String,
    entity_type:   String,
    operation:     String,
    tenant_id:     String,
}

impl AuditFieldVisitor {
    const fn new() -> Self {
        Self {
            mutation_name: String::new(),
            entity_type:   String::new(),
            operation:     String::new(),
            tenant_id:     String::new(),
        }
    }
}

impl Visit for AuditFieldVisitor {
    /// Called for fields recorded with a plain `&str` value (e.g. `mutation_name = name`).
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "mutation_name" => value.clone_into(&mut self.mutation_name),
            "entity_type"   => value.clone_into(&mut self.entity_type),
            "operation"     => value.clone_into(&mut self.operation),
            "tenant_id"     => value.clone_into(&mut self.tenant_id),
            _               => {}
        }
    }

    /// Called for fields recorded with `%value` (Display-formatted) or `?value`
    /// (Debug-formatted).
    ///
    /// `tracing` wraps `%value` in a `DisplayValue` whose `Debug` impl delegates
    /// to `Display`, so `format!("{value:?}")` yields the raw Display string.
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let s = format!("{value:?}");
        match field.name() {
            "mutation_name" => self.mutation_name = s,
            "entity_type"   => self.entity_type   = s,
            "operation"     => self.operation      = s,
            "tenant_id"     => self.tenant_id      = s,
            _               => {}
        }
    }
}

// ── MutationAuditLayer ─────────────────────────────────────────────────────

/// Tracing [`Layer`] that feeds [`UsageAggregator`] from mutation audit events.
///
/// Only events whose target is exactly `"fraiseql::mutation_audit"` are
/// processed; all other events are ignored with no overhead.
///
/// The layer holds an [`Arc<UsageAggregator>`] so it can be cloned cheaply
/// and the aggregator can be shared with other components (e.g. an HTTP query
/// endpoint).
#[derive(Clone)]
pub struct MutationAuditLayer {
    aggregator: Arc<UsageAggregator>,
}

impl MutationAuditLayer {
    /// Create a new layer backed by the given aggregator.
    #[must_use]
    pub const fn new(aggregator: Arc<UsageAggregator>) -> Self {
        Self { aggregator }
    }

    /// Return a reference to the underlying aggregator.
    #[must_use]
    pub const fn aggregator(&self) -> &Arc<UsageAggregator> {
        &self.aggregator
    }
}

impl<S: Subscriber> Layer<S> for MutationAuditLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != MUTATION_AUDIT_TARGET {
            return;
        }

        let mut visitor = AuditFieldVisitor::new();
        event.record(&mut visitor);

        let period = Utc::now().format("%Y-%m").to_string();

        let audit_event = MutationAuditEvent {
            mutation_name: visitor.mutation_name,
            entity_type:   visitor.entity_type,
            operation:     visitor.operation,
            tenant_id:     visitor.tenant_id,
            period,
        };

        self.aggregator.record(&audit_event);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use tracing_subscriber::{Registry, layer::SubscriberExt as _};

    use super::*;
    use crate::usage::aggregator::UsageAggregator;

    fn current_period() -> String {
        Utc::now().format("%Y-%m").to_string()
    }

    /// Emit a synthetic `fraiseql::mutation_audit` event and verify the aggregator
    /// captures it correctly.
    #[test]
    fn test_layer_captures_mutation_audit_event() {
        let aggregator = Arc::new(UsageAggregator::new());
        let layer = MutationAuditLayer::new(Arc::clone(&aggregator));
        let subscriber = Registry::default().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(
            target: "fraiseql::mutation_audit",
            mutation_name = "create_user",
            entity_type   = %"User",
            operation      = %"create",
            tenant_id      = %"acme",
            "mutation.executed"
        );

        let period  = current_period();
        let summary = aggregator.query("acme", &period);
        assert_eq!(summary.mutations.get("User"), Some(&1));
    }

    /// Events from other targets must not be counted.
    #[test]
    fn test_layer_ignores_other_targets() {
        let aggregator = Arc::new(UsageAggregator::new());
        let layer = MutationAuditLayer::new(Arc::clone(&aggregator));
        let subscriber = Registry::default().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(
            target: "fraiseql::other",
            mutation_name = "create_user",
            entity_type   = %"User",
            operation      = %"create",
            tenant_id      = %"acme",
            "not an audit event"
        );

        let summary = aggregator.query("acme", &current_period());
        assert!(summary.mutations.is_empty());
    }

    /// Multiple events across two tenants aggregate independently.
    #[test]
    fn test_layer_aggregates_multiple_events_across_tenants() {
        let aggregator = Arc::new(UsageAggregator::new());
        let layer = MutationAuditLayer::new(Arc::clone(&aggregator));
        let subscriber = Registry::default().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let period = current_period();

        // 3 × User mutations for tenant_x; 2 × Order for tenant_y
        for _ in 0..3 {
            tracing::info!(
                target: "fraiseql::mutation_audit",
                mutation_name = "create_user",
                entity_type   = %"User",
                operation      = %"create",
                tenant_id      = %"tenant_x",
                "mutation.executed"
            );
        }
        for _ in 0..2 {
            tracing::info!(
                target: "fraiseql::mutation_audit",
                mutation_name = "delete_order",
                entity_type   = %"Order",
                operation      = %"delete",
                tenant_id      = %"tenant_y",
                "mutation.executed"
            );
        }

        let x = aggregator.query("tenant_x", &period);
        assert_eq!(x.mutations.get("User"), Some(&3));
        assert_eq!(x.mutations.get("Order"), None);

        let y = aggregator.query("tenant_y", &period);
        assert_eq!(y.mutations.get("Order"), Some(&2));
        assert_eq!(y.mutations.get("User"), None);
    }

    /// Empty-string `tenant_id` (single-tenant scenario) is handled gracefully.
    #[test]
    fn test_layer_handles_empty_tenant_id() {
        let aggregator = Arc::new(UsageAggregator::new());
        let layer = MutationAuditLayer::new(Arc::clone(&aggregator));
        let subscriber = Registry::default().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(
            target: "fraiseql::mutation_audit",
            mutation_name = "update_product",
            entity_type   = %"Product",
            operation      = %"update",
            tenant_id      = %"",
            "mutation.executed"
        );

        let summary = aggregator.query("", &current_period());
        assert_eq!(summary.mutations.get("Product"), Some(&1));
    }

    /// `aggregator()` accessor returns the same `Arc`.
    #[test]
    fn test_aggregator_accessor() {
        let aggregator = Arc::new(UsageAggregator::new());
        let layer = MutationAuditLayer::new(Arc::clone(&aggregator));
        assert!(Arc::ptr_eq(&aggregator, layer.aggregator()));
    }
}
