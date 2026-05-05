mod aggregator_tests {
    use std::collections::HashMap;

    use super::super::aggregator::*;
    use super::super::events::MutationAuditEvent;

    fn event(tenant: &str, period: &str, entity: &str) -> MutationAuditEvent {
        MutationAuditEvent {
            mutation_name: format!("create_{entity}"),
            entity_type:   entity.to_owned(),
            operation:     "create".to_owned(),
            tenant_id:     tenant.to_owned(),
            period:        period.to_owned(),
        }
    }

    // ── record / query ─────────────────────────────────────────────────────

    #[test]
    fn test_record_and_query_single_tenant() {
        let agg = UsageAggregator::new();

        // 4 × User, 3 × Order for tenant_a in 2026-05
        for _ in 0..4 {
            agg.record(&event("tenant_a", "2026-05", "User"));
        }
        for _ in 0..3 {
            agg.record(&event("tenant_a", "2026-05", "Order"));
        }

        let summary = agg.query("tenant_a", "2026-05");
        assert_eq!(summary.mutations.get("User"), Some(&4));
        assert_eq!(summary.mutations.get("Order"), Some(&3));
    }

    #[test]
    fn test_record_and_query_two_tenants() {
        let agg = UsageAggregator::new();

        // tenant_a: 5 × User; tenant_b: 2 × User, 3 × Product
        for _ in 0..5 {
            agg.record(&event("tenant_a", "2026-05", "User"));
        }
        for _ in 0..2 {
            agg.record(&event("tenant_b", "2026-05", "User"));
        }
        for _ in 0..3 {
            agg.record(&event("tenant_b", "2026-05", "Product"));
        }

        let a = agg.query("tenant_a", "2026-05");
        assert_eq!(a.mutations.get("User"), Some(&5));
        assert_eq!(a.mutations.get("Product"), None);

        let b = agg.query("tenant_b", "2026-05");
        assert_eq!(b.mutations.get("User"), Some(&2));
        assert_eq!(b.mutations.get("Product"), Some(&3));
    }

    #[test]
    fn test_record_across_periods_does_not_bleed() {
        let agg = UsageAggregator::new();

        // 10 events in 2026-04, 3 in 2026-05 — same tenant and entity
        for _ in 0..10 {
            agg.record(&event("t1", "2026-04", "Widget"));
        }
        for _ in 0..3 {
            agg.record(&event("t1", "2026-05", "Widget"));
        }

        assert_eq!(agg.query("t1", "2026-04").mutations.get("Widget"), Some(&10));
        assert_eq!(agg.query("t1", "2026-05").mutations.get("Widget"), Some(&3));
    }

    #[test]
    fn test_record_10_events_across_2_tenants_3_entities() {
        let agg = UsageAggregator::new();

        // 10 events: tenant_a gets 4+3=7, tenant_b gets 3
        let events = [
            ("tenant_a", "Alpha"),
            ("tenant_a", "Beta"),
            ("tenant_a", "Alpha"),
            ("tenant_b", "Gamma"),
            ("tenant_a", "Alpha"),
            ("tenant_b", "Gamma"),
            ("tenant_a", "Beta"),
            ("tenant_b", "Gamma"),
            ("tenant_a", "Alpha"),
            ("tenant_a", "Beta"),
        ];
        for (tenant, entity) in events {
            agg.record(&event(tenant, "2026-05", entity));
        }

        let a = agg.query("tenant_a", "2026-05");
        assert_eq!(a.mutations.get("Alpha"), Some(&4));
        assert_eq!(a.mutations.get("Beta"), Some(&3));
        assert_eq!(a.mutations.get("Gamma"), None);

        let b = agg.query("tenant_b", "2026-05");
        assert_eq!(b.mutations.get("Gamma"), Some(&3));
        assert_eq!(b.mutations.len(), 1);
    }

    // ── empty result ───────────────────────────────────────────────────────

    #[test]
    fn test_empty_result_for_unknown_tenant() {
        let agg = UsageAggregator::new();
        let summary = agg.query("nobody", "2026-05");
        assert!(summary.mutations.is_empty());
    }

    #[test]
    fn test_empty_result_for_unknown_period() {
        let agg = UsageAggregator::new();
        agg.record(&event("tenant_a", "2026-05", "User"));

        let summary = agg.query("tenant_a", "2026-06");
        assert!(summary.mutations.is_empty());
    }

    // ── period validation ──────────────────────────────────────────────────

    #[test]
    fn test_validate_period_valid() {
        assert!(validate_period("2026-04"));
        assert!(validate_period("2026-01"));
        assert!(validate_period("2026-12"));
        assert!(validate_period("1000-06"));
        assert!(validate_period("9999-11"));
    }

    #[test]
    fn test_validate_period_invalid_month() {
        assert!(!validate_period("2026-00")); // month 0
        assert!(!validate_period("2026-13")); // month 13
        assert!(!validate_period("2026-99"));
    }

    #[test]
    fn test_validate_period_invalid_format() {
        assert!(!validate_period("2026"));        // missing month
        assert!(!validate_period("26-04"));       // short year
        assert!(!validate_period("2026/04"));     // wrong separator
        assert!(!validate_period("2026-4"));      // single-digit month
        assert!(!validate_period("2026-04-01"));  // too long
        assert!(!validate_period(""));            // empty
    }

    // ── persistence backend ────────────────────────────────────────────────

    #[test]
    fn test_counters_reset_on_new_aggregator_without_persistence() {
        // Documents existing behaviour: in-memory counters are lost when a new
        // aggregator is created.  This is the behaviour the backend feature fixes.
        let agg = UsageAggregator::new();
        agg.record(&event("tenant_a", "2026-05", "User"));
        assert_eq!(agg.query("tenant_a", "2026-05").mutations["User"], 1);

        let new_agg = UsageAggregator::new();
        assert_eq!(new_agg.query("tenant_a", "2026-05").mutations.get("User"), None);
    }

    /// In-memory persistence backend used only in tests.
    ///
    /// Stores flushed counters in a `Mutex<HashMap>` so they survive across
    /// `UsageAggregator` instances within the same process (simulating a restart).
    struct InMemoryPersistenceBackend {
        store: std::sync::Mutex<HashMap<(String, String, String), u64>>,
    }

    impl InMemoryPersistenceBackend {
        fn new() -> Self {
            Self { store: std::sync::Mutex::new(HashMap::new()) }
        }
    }

    #[async_trait::async_trait]
    impl UsageBackend for InMemoryPersistenceBackend {
        async fn flush(
            &self,
            counters: &HashMap<(String, String, String), u64>,
        ) -> Result<(), String> {
            let mut store = self.store.lock().map_err(|e| e.to_string())?;
            for (key, &count) in counters {
                *store.entry(key.clone()).or_insert(0) = count;
            }
            Ok(())
        }

        async fn load(&self) -> Result<HashMap<(String, String, String), u64>, String> {
            let store = self.store.lock().map_err(|e| e.to_string())?;
            Ok(store.clone())
        }
    }

    #[tokio::test]
    async fn test_flush_and_load_round_trip() {
        let backend = std::sync::Arc::new(InMemoryPersistenceBackend::new());

        // Record events and flush
        let agg = UsageAggregator::new_with_backend(backend.clone());
        agg.record(&event("tenant_a", "2026-05", "User"));
        agg.record(&event("tenant_a", "2026-05", "User"));
        agg.record(&event("tenant_b", "2026-05", "Order"));
        agg.flush_to_backend().await.expect("flush");

        // Simulate restart: create a new aggregator with the same backend
        let new_agg = UsageAggregator::new_with_backend(backend.clone());
        assert_eq!(new_agg.query("tenant_a", "2026-05").mutations.get("User"), None); // not yet loaded

        new_agg.load_from_backend().await.expect("load");
        assert_eq!(new_agg.query("tenant_a", "2026-05").mutations["User"], 2);
        assert_eq!(new_agg.query("tenant_b", "2026-05").mutations["Order"], 1);
    }

    #[tokio::test]
    async fn test_load_merges_with_inflight_events() {
        // Events recorded between flush and load should not be lost
        let backend = std::sync::Arc::new(InMemoryPersistenceBackend::new());

        let agg = UsageAggregator::new_with_backend(backend.clone());
        agg.record(&event("t1", "2026-05", "User"));
        agg.flush_to_backend().await.expect("flush"); // persists count=1

        // Restart: new aggregator picks up 2 in-flight events before loading
        let new_agg = UsageAggregator::new_with_backend(backend.clone());
        new_agg.record(&event("t1", "2026-05", "User"));
        new_agg.record(&event("t1", "2026-05", "User"));
        new_agg.load_from_backend().await.expect("load"); // adds persisted 1 → total 3

        assert_eq!(new_agg.query("t1", "2026-05").mutations["User"], 3);
    }

    #[tokio::test]
    async fn test_noop_backend_flush_and_load_are_harmless() {
        let agg = UsageAggregator::new(); // uses NoopBackend
        agg.record(&event("t1", "2026-05", "User"));
        agg.flush_to_backend().await.expect("flush ok");

        let new_agg = UsageAggregator::new();
        new_agg.load_from_backend().await.expect("load ok");
        assert_eq!(new_agg.query("t1", "2026-05").mutations.get("User"), None); // noop
    }
}

mod layer_tests {
    use std::sync::Arc;

    use chrono::Utc;
    use tracing_subscriber::{Registry, layer::SubscriberExt as _};

    use super::super::layer::*;
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
