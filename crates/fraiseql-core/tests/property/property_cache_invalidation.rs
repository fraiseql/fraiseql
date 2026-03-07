//! Property-based tests for `CascadeInvalidator`.
//!
//! Invariants verified:
//! - `cascade_invalidate` always includes the target view itself.
//! - `cascade_invalidate` always includes all direct dependents of the target.
//! - `cascade_invalidate` never includes views that are not in the dependency graph.
//! - `cascade_invalidate` is idempotent with respect to the returned set
//!   (calling again on a separate invalidator with the same graph gives the same set).

use std::collections::HashSet;

use fraiseql_core::cache::CascadeInvalidator;
use proptest::prelude::*;

/// Generate a list of (dependent, dependency) string pairs.
/// Using short names to keep the graph small and cycles manageable.
fn arb_dependency_pairs() -> impl Strategy<Value = Vec<(String, String)>> {
    prop::collection::vec(
        ("[a-e]{1,3}", "[a-e]{1,3}"),
        0..8,
    )
}

/// Build a `CascadeInvalidator` from dependency pairs, silently skipping
/// self-edges and edges that would create cycles (both are detected by the
/// `add_dependency` method).
fn build_invalidator(pairs: &[(String, String)]) -> (CascadeInvalidator, HashSet<String>) {
    let mut inv = CascadeInvalidator::new();
    let mut all_views = HashSet::new();

    for (dependent, dependency) in pairs {
        all_views.insert(dependent.clone());
        all_views.insert(dependency.clone());
        // Cycles and self-edges are rejected — that's fine; we skip them.
        let _ = inv.add_dependency(dependent, dependency);
    }

    (inv, all_views)
}

proptest! {
    /// `cascade_invalidate` must always include the target view itself,
    /// even when the target has no dependents.
    #[test]
    fn invalidation_always_includes_target(
        pairs in arb_dependency_pairs(),
        target in "[a-e]{1,3}",
    ) {
        let (mut inv, _) = build_invalidator(&pairs);
        let invalidated = inv.cascade_invalidate(&target)
            .expect("cascade_invalidate must not fail");
        prop_assert!(
            invalidated.contains(&target),
            "target {target:?} must be in the invalidation set"
        );
    }

    /// Every direct dependent of the target must appear in the invalidation set.
    #[test]
    fn invalidation_includes_all_direct_dependents(
        pairs in arb_dependency_pairs(),
        target in "[a-e]{1,3}",
    ) {
        let (mut inv, _) = build_invalidator(&pairs);
        let direct = inv.get_direct_dependents(&target);
        let invalidated = inv.cascade_invalidate(&target)
            .expect("cascade_invalidate must not fail");

        for view in &direct {
            prop_assert!(
                invalidated.contains(view),
                "direct dependent {view:?} must be in the invalidation set for target {target:?}"
            );
        }
    }

    /// The invalidation set must be a subset of all known views plus the target
    /// itself — it must never invent views not present in the graph.
    #[test]
    fn invalidation_never_produces_unknown_views(
        pairs in arb_dependency_pairs(),
        target in "[a-e]{1,3}",
    ) {
        let (mut inv, mut all_views) = build_invalidator(&pairs);
        all_views.insert(target.clone());

        let invalidated = inv.cascade_invalidate(&target)
            .expect("cascade_invalidate must not fail");

        for view in &invalidated {
            prop_assert!(
                all_views.contains(view),
                "view {view:?} appeared in the invalidation set but is not in the dependency graph"
            );
        }
    }

    /// `cascade_invalidate` must be deterministic: two invalidators built from
    /// the same dependency pairs must produce equal invalidation sets.
    #[test]
    fn invalidation_is_deterministic(
        pairs in arb_dependency_pairs(),
        target in "[a-e]{1,3}",
    ) {
        let (mut inv1, _) = build_invalidator(&pairs);
        let (mut inv2, _) = build_invalidator(&pairs);

        let set1 = inv1.cascade_invalidate(&target).expect("first call");
        let set2 = inv2.cascade_invalidate(&target).expect("second call");

        prop_assert_eq!(set1, set2, "invalidation sets must be equal for the same graph");
    }

    /// Direct dependents returned by `get_direct_dependents` must be a subset
    /// of the full invalidation set (transitively reachable views include at least
    /// the direct ones).
    #[test]
    fn direct_dependents_are_subset_of_full_invalidation(
        pairs in arb_dependency_pairs(),
        target in "[a-e]{1,3}",
    ) {
        let (mut inv, _) = build_invalidator(&pairs);
        let direct = inv.get_direct_dependents(&target);
        let full = inv.cascade_invalidate(&target).expect("cascade_invalidate");

        for view in &direct {
            prop_assert!(
                full.contains(view),
                "direct dependent {view:?} must be ⊆ full invalidation set"
            );
        }
    }
}
