//! In-memory usage counter store.
//!
//! Counters are keyed by `(tenant_id, period_yyyy_mm, entity_type)` and stored
//! as lock-free [`AtomicU64`] values inside a [`DashMap`].
//!
//! # Memory growth
//!
//! This is a **v1, unbounded** store: entries are never evicted. Growth is
//! proportional to the product of `#tenants × #periods × #entity_types`.
//! For a deployment with 100 tenants, 12 months retention, and 50 entity types
//! that is at most 60 000 entries — approximately 5 MB.  Eviction policies and
//! persistent storage are out of scope for v1.
//!
//! # Restarts
//!
//! Counters are **in-memory only**; they reset to zero on process restart.
//! The aggregator is wired into `AppState` and exposed via `GET /api/v1/admin/usage`.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, atomic::{AtomicU64, Ordering}},
};

use dashmap::DashMap;
use serde::Serialize;

use super::events::MutationAuditEvent;

// ── Global aggregator ──────────────────────────────────────────────────────

static GLOBAL_USAGE_AGGREGATOR: OnceLock<Arc<UsageAggregator>> = OnceLock::new();

/// Return a reference to the process-wide [`UsageAggregator`].
///
/// Initialised on first call and shared for the lifetime of the process.
/// Both [`MutationAuditLayer`](super::layer::MutationAuditLayer) (tracing
/// subscriber) and the HTTP query endpoint use the same `Arc`, so counters
/// written by the layer are immediately visible to the endpoint.
///
/// [`MutationAuditLayer`]: crate::usage::layer::MutationAuditLayer
#[must_use]
pub fn global_aggregator() -> &'static Arc<UsageAggregator> {
    GLOBAL_USAGE_AGGREGATOR.get_or_init(|| Arc::new(UsageAggregator::new()))
}

// ── Period validation ──────────────────────────────────────────────────────

/// Validate a usage period string in `"YYYY-MM"` format.
///
/// Returns `true` when the period is exactly seven ASCII characters with a
/// `-` separator at index 4, a four-digit year, and a month in `01..=12`.
///
/// # Examples
///
/// ```
/// use fraiseql_server::usage::aggregator::validate_period;
///
/// assert!(validate_period("2026-04"));
/// assert!(!validate_period("2026-13")); // invalid month
/// assert!(!validate_period("2026"));    // missing month
/// assert!(!validate_period("26-04"));   // short year
/// ```
#[must_use]
pub fn validate_period(period: &str) -> bool {
    let bytes = period.as_bytes();
    if bytes.len() != 7 || bytes[4] != b'-' {
        return false;
    }
    let year_str = &period[..4];
    let month_str = &period[5..];
    if !year_str.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if !month_str.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let month: u8 = month_str.parse().unwrap_or(0);
    (1..=12).contains(&month)
}

// ── UsageSummary ───────────────────────────────────────────────────────────

/// Per-period mutation counts for a single tenant.
///
/// The `mutations` map has entity-type names as keys and the total mutation
/// count for that entity type in the queried period as values.
///
/// Serialises to:
/// ```json
/// { "mutations": { "User": 42, "Order": 7 } }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    /// Mutation counts keyed by entity type.
    pub mutations: HashMap<String, u64>,
}

// ── UsageAggregator ────────────────────────────────────────────────────────

/// Thread-safe, in-memory usage counter store.
///
/// Cheaply cloneable via [`Arc`] — all clones share the same underlying map.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use fraiseql_server::usage::aggregator::UsageAggregator;
/// use fraiseql_server::usage::events::MutationAuditEvent;
///
/// let agg = Arc::new(UsageAggregator::new());
/// let event = MutationAuditEvent {
///     mutation_name: "create_user".to_owned(),
///     entity_type:   "User".to_owned(),
///     operation:     "create".to_owned(),
///     tenant_id:     "acme".to_owned(),
///     period:        "2026-05".to_owned(),
/// };
/// agg.record(&event);
/// let summary = agg.query("acme", "2026-05");
/// assert_eq!(summary.mutations["User"], 1);
/// ```
#[derive(Debug)]
pub struct UsageAggregator {
    /// Key: `(tenant_id, period_yyyy_mm, entity_type)`.
    counters: DashMap<(String, String, String), AtomicU64>,
}

impl UsageAggregator {
    /// Create an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
        }
    }

    /// Record one mutation audit event, incrementing the appropriate counter.
    ///
    /// This method is lock-free on the hot path: it uses [`AtomicU64::fetch_add`]
    /// after the initial shard lock in [`DashMap::entry`].
    pub fn record(&self, event: &MutationAuditEvent) {
        let key = (
            event.tenant_id.clone(),
            event.period.clone(),
            event.entity_type.clone(),
        );
        self.counters
            .entry(key)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Return the usage summary for a tenant and period.
    ///
    /// Returns `UsageSummary { mutations: {} }` (never an error) when no events
    /// have been recorded for the given `(tenant_id, period)` pair.
    pub fn query(&self, tenant_id: &str, period: &str) -> UsageSummary {
        let mut mutations: HashMap<String, u64> = HashMap::new();
        for entry in &self.counters {
            let (t, p, e) = entry.key();
            if t == tenant_id && p == period {
                mutations.insert(e.clone(), entry.value().load(Ordering::Relaxed));
            }
        }
        UsageSummary { mutations }
    }

    /// Return the total number of distinct counter entries (for monitoring).
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.counters.len()
    }
}

impl Default for UsageAggregator {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
}
